mod speech;
use fnpilot_core::{
    Interpretation, Result,
    context::{AppInfo, CommandContext},
    direct_ui, parser,
};
use fnpilot_runtime::{Configuration, Interpreter};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Instant};

#[derive(Deserialize)]
struct Example {
    id: u32,
    category: String,
    input: String,
    expected: Interpretation,
    context: CommandContext,
}
fn normalized(
    mut result: Interpretation,
    apps: &[AppInfo],
    active: &str,
    expected: Option<&Interpretation>,
) -> Interpretation {
    result.message = None;
    let action_count = result.actions.len();
    for (i, action) in result.actions.iter_mut().enumerate() {
        if let Some(name) = &action.app {
            let id = apps
                .iter()
                .find(|a| {
                    a.name.eq_ignore_ascii_case(name) || a.bundle_id.eq_ignore_ascii_case(name)
                })
                .map(|a| a.bundle_id.clone())
                .unwrap_or_else(|| match name.to_lowercase().as_str() {
                    "chrome" => "com.google.Chrome".into(),
                    "notes" => "com.apple.Notes".into(),
                    "safari" => "com.apple.Safari".into(),
                    "finder" => "com.apple.finder".into(),
                    "vs code" | "vscode" => "com.microsoft.VSCode".into(),
                    _ => name.to_lowercase(),
                });
            action.app = Some(id);
        }
        if expected.is_some_and(|e| e.actions.len() == action_count && e.actions[i].app.is_none())
            && action.app.as_deref() == Some(active)
            && !["open_app", "focus_app", "quit_app"].contains(&action.kind.as_str())
        {
            action.app = None;
        }
    }
    result
}
fn run(args: &[String]) -> Result<()> {
    if args.first().is_some_and(|arg| arg == "--speech") {
        return speech::run(&args[1..]);
    }
    if args.len() < 3 {
        return Err("Usage: fnpilot-bench --parser-only FIXTURES.json REPORT.json\n       fnpilot-bench MODEL FIXTURES.json REPORT.json [ollama|openAICompatible] [ENDPOINT] [LIMIT] [FORMAT]\nThis benchmark never reads or executes desktop actions.".into());
    }
    let parser_only = args[0] == "--parser-only";
    let bytes = fs::read(&args[1]).map_err(|_| "Could not read fixtures.")?;
    let mut examples: Vec<Example> =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid benchmark corpus.")?;
    let mut config = Configuration {
        model: args[0].clone(),
        ..Default::default()
    };
    if let Some(runtime) = args.get(3) {
        config.runtime = serde_json::from_value(json!(runtime)).map_err(|_| "Unknown runtime.")?;
    }
    if let Some(endpoint) = args.get(4) {
        config.endpoint = endpoint.clone();
    } else if config.runtime == fnpilot_runtime::Runtime::OpenAICompatible {
        config.endpoint = "http://127.0.0.1:18114/v1".into();
    }
    if let Some(limit) = args.get(5) {
        examples.truncate(limit.parse::<usize>().map_err(|_| "Invalid limit.")?);
    }
    if let Some(format) = args.get(6) {
        config.output_mode =
            serde_json::from_value(json!(format)).map_err(|_| "Unknown output format.")?;
    }
    let interpreter = if parser_only {
        None
    } else {
        Some(Interpreter::new(config.clone())?)
    };
    let mut observations = vec![];
    let mut times = vec![];
    for example in examples {
        let start = Instant::now();
        let model = interpreter
            .as_ref()
            .map(|model| model.interpret(&example.input, &example.context));
        let seconds = start.elapsed().as_secs_f64();
        let wanted = normalized(example.expected.clone(), &example.context.apps, "", None);
        let correct = model.as_ref().map(|r| {
            r.as_ref().is_ok_and(|result| {
                normalized(
                    result.clone(),
                    &example.context.apps,
                    &example.context.bundle_id,
                    Some(&wanted),
                ) == wanted
            })
        });
        let direct = parser::parse(&example.input)
            .or_else(|| direct_ui::parse(&example.input, &example.context));
        let route_start = Instant::now();
        let mut context = example.context.clone();
        let mut count = 0;
        let mut hybrid = Interpretation::actions(vec![]);
        let mut failed = false;
        for part in parser::parts(&example.input) {
            let result =
                fnpilot_core::runner::interpret(
                    &part,
                    &context,
                    |part, context| match &interpreter {
                        Some(model) => model.interpret(part, context),
                        None => Err("Model inference not requested.".into()),
                    },
                );
            let Ok(result) = result else {
                failed = true;
                break;
            };
            if result.validate().is_err() {
                failed = true;
                break;
            }
            if result.status != "actions" {
                hybrid = result;
                break;
            }
            for action in result.actions {
                count += 1;
                if count > 20 {
                    failed = true;
                    break;
                }
                if let Some(name) = &action.app
                    && let Some(app) = context
                        .apps
                        .iter()
                        .find(|a| a.name.eq_ignore_ascii_case(name) || a.bundle_id == *name)
                    && app.bundle_id != context.bundle_id
                {
                    context.app_name = app.name.clone();
                    context.bundle_id = app.bundle_id.clone();
                    context.controls.clear();
                    context.controls_complete = Some(false);
                }
                hybrid.actions.push(action);
            }
            if failed {
                break;
            }
        }
        let hybrid_correct = !failed
            && normalized(
                hybrid,
                &example.context.apps,
                &example.context.bundle_id,
                Some(&wanted),
            ) == wanted;
        let routing_seconds = route_start.elapsed().as_secs_f64();
        times.push(if parser_only {
            routing_seconds
        } else {
            seconds
        });
        observations.push(json!({"id":example.id,"category":example.category,"input":example.input,"expected":example.expected,
            "actual":model.as_ref().and_then(|r|r.as_ref().ok()),"correct":correct,"hybridCorrect":hybrid_correct,"direct":direct.is_some(),
            "seconds":if parser_only {Value::Null}else {json!(seconds)},"routingSeconds":routing_seconds,"error":model.as_ref().and_then(|r|r.as_ref().err())}));
    }
    let cold = times.first().copied();
    let mut warm = times.into_iter().skip(1).collect::<Vec<_>>();
    warm.sort_by(f64::total_cmp);
    let quantile = |q: f64| {
        warm.get(((warm.len() as f64 * q).floor() as usize).min(warm.len().saturating_sub(1)))
            .copied()
    };
    let report = json!({"schemaVersion":1,"evidenceScope":if parser_only {"parser-first-fixtures"}else {"model-only-and-parser-first-fixtures"},"nativeAcceptance":false,
        "corpusSha256":format!("{:x}",Sha256::digest(&bytes)),"count":observations.len(),"model":if parser_only {None}else {Some(&config.model)},
        "runtime":if parser_only {Value::Null}else {json!(config.runtime)},"outputMode":if parser_only {Value::Null}else {json!(config.output_mode)},
        "modelCorrect":if parser_only {Value::Null}else {json!(observations.iter().filter(|r|r["correct"]==true).count())},
        "hybridCorrect":observations.iter().filter(|r|r["hybridCorrect"]==true).count(),"invalidResponses":if parser_only {Value::Null}else {json!(observations.iter().filter(|r|!r["error"].is_null()).count())},
        "timingStage":if parser_only {"routing"}else {"model-interpretation"},"coldSeconds":cold,"warmMedianSeconds":quantile(0.5),"warmP95Seconds":quantile(0.95),
        "speechSeconds":null,"executionSeconds":null,"verificationSeconds":null,"observations":observations});
    if let Some(parent) = Path::new(&args[2])
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|_| "Could not create report directory.")?;
    }
    fs::write(
        &args[2],
        serde_json::to_vec_pretty(&report).map_err(|_| "Could not encode benchmark report.")?,
    )
    .map_err(|_| "Could not write benchmark report.")?;
    println!(
        "{}",
        json!({"count":report["count"],"hybridCorrect":report["hybridCorrect"],"nativeAcceptance":false})
    );
    Ok(())
}
fn main() {
    if let Err(error) = run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
