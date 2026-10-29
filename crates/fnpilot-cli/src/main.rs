use fnpilot_core::{Interpretation, context::CommandContext, direct_ui, parser};
use fnpilot_runtime::{Configuration, Interpreter};
use std::{collections::BTreeMap, env, fs};

const USAGE: &str = "FnPilot CLI — interprets commands without controlling the desktop.\n\n  fnpilot-cli parse TEXT\n  fnpilot-cli interpret TEXT [--runtime ollama|openAICompatible] [--endpoint URL] [--model ID] [--context FILE] [--format jsonSchema|jsonObject|promptJSON|toolCalls] [--model-only]\n  fnpilot-cli validate FILE\n  fnpilot-cli health [options]\n\nExit codes: 0 actions, 2 clarification, 3 unsupported, 1 validation/runtime error, 64 usage. No desktop actions execute.";

fn run(args: &[String]) -> Result<i32, (i32, String)> {
    let Some(verb) = args.first().map(String::as_str) else {
        println!("{USAGE}");
        return Ok(0);
    };
    if verb == "--help" {
        println!("{USAGE}");
        return Ok(0);
    }
    if !["parse", "interpret", "validate", "health"].contains(&verb) {
        return Err((64, USAGE.into()));
    }
    if verb != "health" && (args.len() < 2 || args[1].starts_with("--")) {
        return Err((64, USAGE.into()));
    }
    let mut options = BTreeMap::new();
    let mut cursor = if verb == "health" { 1 } else { 2 };
    while cursor < args.len() {
        let key = args[cursor].as_str();
        if options.contains_key(key) {
            return Err((64, format!("Duplicate option: {key}")));
        }
        if key == "--model-only" {
            options.insert(key, "true");
            cursor += 1;
            continue;
        }
        if ![
            "--runtime",
            "--endpoint",
            "--model",
            "--context",
            "--format",
        ]
        .contains(&key)
            || cursor + 1 >= args.len()
            || args[cursor + 1].starts_with("--")
        {
            return Err((64, format!("Unknown option or missing value: {key}")));
        }
        options.insert(key, args[cursor + 1].as_str());
        cursor += 2;
    }
    let error = |e: String| (1, e);
    let read = |file: &str| {
        fs::read_to_string(file).map_err(|_| (1, "Could not read the requested file.".into()))
    };
    let result = if verb == "validate" {
        if args.len() != 2 {
            return Err((64, USAGE.into()));
        }
        Interpretation::decode(&read(&args[1])?).map_err(error)?
    } else {
        let context: CommandContext = if let Some(path) = options.get("--context") {
            serde_json::from_str(&read(path)?)
                .map_err(|_| (1, "Invalid fixture context.".into()))?
        } else {
            CommandContext::default()
        };
        let text = if verb == "health" {
            "volume up"
        } else {
            &args[1]
        };
        let direct = if verb == "health" || verb != "parse" && options.contains_key("--model-only")
        {
            None
        } else {
            parser::parse(text).or_else(|| {
                if verb == "interpret" {
                    direct_ui::parse(text, &context)
                } else {
                    None
                }
            })
        };
        if let Some(result) = direct {
            result
        } else if verb == "parse" {
            Interpretation::message(
                "unsupported",
                "The direct parser does not recognize this instruction. Try interpret with a local model.",
            )
        } else {
            let mut config = Configuration::default();
            if let Some(runtime) = options.get("--runtime") {
                config.runtime = serde_json::from_value(serde_json::json!(runtime))
                    .map_err(|_| (64, format!("Unknown runtime: {runtime}")))?;
            }
            if let Some(endpoint) = options.get("--endpoint") {
                config.endpoint = (*endpoint).into();
            } else if config.runtime == fnpilot_runtime::Runtime::OpenAICompatible {
                config.endpoint = "http://127.0.0.1:18114/v1".into();
            }
            if let Some(model) = options.get("--model") {
                config.model = (*model).into();
            }
            if let Some(format) = options.get("--format") {
                config.output_mode = serde_json::from_value(serde_json::json!(format))
                    .map_err(|_| (64, format!("Unknown output format: {format}")))?;
            }
            Interpreter::new(config)
                .map_err(error)?
                .interpret(text, &context)
                .map_err(error)?
        }
    };
    result.validate().map_err(error)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&result).map_err(|_| (1, "JSON encoding failed.".into()))?
    );
    Ok(result.exit_code())
}
fn main() {
    match run(&env::args().skip(1).collect::<Vec<_>>()) {
        Ok(code) => std::process::exit(code),
        Err((code, message)) => {
            eprintln!("{message}");
            std::process::exit(code);
        }
    }
}
