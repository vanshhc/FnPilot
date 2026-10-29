use fnpilot_core::{
    Interpretation, Result,
    context::CommandContext,
    runner,
    session::{Phase, Session},
};
use fnpilot_runtime::{Configuration, Interpreter};
use fnpilot_speech::{backend::SpeechBackend, model_store::ModelManifest, whisper::Whisper};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Instant};

#[derive(Deserialize)]
struct Clip {
    id: String,
    path: String,
    reference: String,
    category: String,
    sha256: String,
    #[serde(default)]
    context: CommandContext,
    expected: Option<Interpretation>,
}
fn words(input: &str) -> Vec<String> {
    input
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}
fn distance(reference: &[String], actual: &[String]) -> usize {
    let mut row = (0..=actual.len()).collect::<Vec<_>>();
    for (i, left) in reference.iter().enumerate() {
        let mut next = vec![i + 1];
        for (j, right) in actual.iter().enumerate() {
            next.push(
                (row[j] + usize::from(left != right))
                    .min(row[j + 1] + 1)
                    .min(next[j] + 1),
            );
        }
        row = next;
    }
    row[actual.len()]
}
fn audio(path: &Path) -> Result<Vec<f32>> {
    if fs::metadata(path).map_err(|_| "Audio file missing.")?.len() > 8_000_000 {
        return Err("Use a bounded 16 kHz mono WAV clip.".into());
    }
    let mut wav=hound::WavReader::open(path).map_err(|_| "Use 16 kHz mono PCM16 or float32 WAV; convert other formats explicitly before benchmarking.")?;
    let spec = wav.spec();
    if spec.channels != 1 || spec.sample_rate != 16000 {
        return Err("Speech benchmark WAV must be 16 kHz mono.".into());
    }
    let samples = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Int, 16) => wav
            .samples::<i16>()
            .map(|v| v.map(|x| x as f32 / 32768.0))
            .collect::<std::result::Result<Vec<_>, _>>(),
        (hound::SampleFormat::Float, 32) => wav
            .samples::<f32>()
            .collect::<std::result::Result<Vec<_>, _>>(),
        _ => return Err("Use PCM16 or float32 WAV.".into()),
    }
    .map_err(|_| "Could not decode WAV samples.")?;
    fnpilot_speech::backend::validate_audio(&samples)?;
    Ok(samples)
}
pub fn run(args: &[String]) -> Result<()> {
    if !(4..=5).contains(&args.len()) {
        return Err("Usage: fnpilot-bench --speech MODEL MANIFEST CORPUS_JSON REPORT_JSON [RUNTIME_CONFIG_JSON]. No desktop actions execute.".into());
    }
    let manifest: ModelManifest =
        serde_json::from_slice(&fs::read(&args[1]).map_err(|_| "Model manifest missing.")?)
            .map_err(|_| "Invalid model manifest.")?;
    let corpus = fs::read(&args[2]).map_err(|_| "Speech corpus manifest missing.")?;
    let clips: Vec<Clip> =
        serde_json::from_slice(&corpus).map_err(|_| "Invalid speech corpus manifest.")?;
    let mut ids = std::collections::BTreeSet::new();
    if clips.is_empty() || clips.len() > 1000 || clips.iter().any(|c| !ids.insert(&c.id)) {
        return Err("Use 1–1000 clips with unique IDs.".into());
    }
    let root = Path::new(&args[2]).parent().unwrap_or(Path::new("."));
    for clip in &clips {
        let path = root.join(&clip.path);
        if format!(
            "{:x}",
            Sha256::digest(fs::read(&path).map_err(|_| "Audio file missing.")?)
        ) != clip.sha256
        {
            return Err(format!("Audio checksum mismatch for {}.", clip.id));
        }
        audio(&path)?;
    }
    let interpreter = args
        .get(4)
        .map(|path| -> Result<Interpreter> {
            let config: Configuration = serde_json::from_slice(
                &fs::read(path).map_err(|_| "Runtime configuration missing.")?,
            )
            .map_err(|_| "Invalid runtime configuration.")?;
            Interpreter::new(config)
        })
        .transpose()?;
    let load = Instant::now();
    let mut decoder = Whisper::load(Path::new(&args[0]), &manifest)?;
    let model_load = load.elapsed().as_secs_f64();
    let session = Session::default();
    let token = session.begin(Phase::Transcribing)?;
    let first = audio(&root.join(&clips[0].path))?;
    let cold = Instant::now();
    decoder.transcribe(&first, &session, token)?;
    let first_decode = cold.elapsed().as_secs_f64();
    for _ in 0..2 {
        decoder.transcribe(&first, &session, token)?;
    }
    let mut measurements = Vec::new();
    let mut total_edits = 0;
    let mut total_words = 0;
    let mut exact = 0;
    let mut commands = 0;
    for repetition in 1..=3 {
        for clip in &clips {
            let pipeline = Instant::now();
            let read = Instant::now();
            let samples = audio(&root.join(&clip.path))?;
            let read_seconds = read.elapsed().as_secs_f64();
            let started = Instant::now();
            let transcript = decoder.transcribe(&samples, &session, token)?;
            let speech_seconds = started.elapsed().as_secs_f64();
            let planning = Instant::now();
            let interpretation =
                runner::interpret(
                    &transcript,
                    &clip.context,
                    |text, context| match &interpreter {
                        Some(model) => model.interpret(text, context),
                        None => Ok(Interpretation::message(
                            "unsupported",
                            "Direct parsing only",
                        )),
                    },
                )?;
            let plan_seconds = planning.elapsed().as_secs_f64();
            let pipeline_seconds = pipeline.elapsed().as_secs_f64();
            let reference = words(&clip.reference);
            let actual = words(&transcript);
            let edits = distance(&reference, &actual);
            total_edits += edits;
            total_words += reference.len();
            if clip.category.contains("command") {
                commands += 1;
                if reference == actual {
                    exact += 1;
                }
            }
            let plan_correct = clip.expected.as_ref().map(|expected| {
                super::normalized(
                    interpretation.clone(),
                    &clip.context.apps,
                    &clip.context.bundle_id,
                    Some(expected),
                ) == super::normalized(
                    expected.clone(),
                    &clip.context.apps,
                    &clip.context.bundle_id,
                    Some(expected),
                )
            });
            measurements.push(json!({"id":clip.id,"category":clip.category,"repetition":repetition,"reference":clip.reference,"transcript":transcript,"audioSeconds":samples.len() as f64/16000.0,"audioDecodeSeconds":read_seconds,"transcriptionSeconds":speech_seconds,"planningSeconds":plan_seconds,"fileAudioToPlanSeconds":pipeline_seconds,"wordEdits":edits,"referenceWords":reference.len(),"planCorrect":plan_correct,"interpretation":interpretation}));
        }
    }
    let report = json!({"schemaVersion":1,"backend":"whisper.cpp-cpu","modelSHA256":manifest.sha256,"corpusHash":format!("{:x}",Sha256::digest(corpus)),"platform":std::env::consts::OS,"architecture":std::env::consts::ARCH,"decodingPolicy":{"language":"en","temperature":0,"fallbackCount":0,"threads":2},"modelLoadSeconds":model_load,"firstDecodeSeconds":first_decode,"warmupDecodes":3,"repetitions":3,"wordErrorRate":total_edits as f64/total_words.max(1) as f64,"commandExactMatches":exact,"commandCount":commands,"measurements":measurements,"executedActions":0,"nativeAcceptance":false,"releaseToDispatchSeconds":null,"executionSeconds":null,"verificationSeconds":null});
    fs::write(
        &args[3],
        serde_json::to_vec_pretty(&report).map_err(|_| "Report encoding failed.")?,
    )
    .map_err(|_| "Could not write speech report.")?;
    println!("Speech report written; no desktop actions executed.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn word_error_rate_counts_insertions_deletions_and_substitutions() {
        assert_eq!(distance(&words("open the notes"), &words("OPEN Notes!")), 1);
        assert_eq!(distance(&words("open notes"), &words("open the mail")), 2);
    }
}
