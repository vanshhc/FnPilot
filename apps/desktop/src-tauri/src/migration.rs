use crate::settings::Settings;
use serde_json::Value;
use std::path::PathBuf;

/// Import only named legacy settings. Never write to UserDefaults or enable OS autostart.
pub fn import(current: &Settings, legacy: &Value) -> (Settings, Vec<String>) {
    if current.import_version >= 1 {
        return (current.clone(), vec![]);
    }
    let mut next = current.clone();
    let mut warnings = vec![];
    if let Some(config) = legacy.get("modelConfiguration") {
        match serde_json::from_value::<fnpilot_runtime::Configuration>(config.clone()) {
            Ok(config) if config.validate().is_ok() => next.configuration = config,
            _ => warnings.push(
                "The legacy runtime configuration was rejected; choose a local endpoint.".into(),
            ),
        }
    }
    next.allow_model_actions = legacy
        .get("allowModelActions")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    next.launch_at_login_intent = legacy
        .get("launchAtLogin")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    next.prefer_bundled_runtime = legacy
        .get("useBundledRuntime")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if let Some(path) = legacy.get("ggufPath").and_then(Value::as_str) {
        let path = PathBuf::from(path);
        if path.is_absolute() && path.is_file() {
            next.gguf_path = Some(path);
        } else {
            warnings.push("The legacy GGUF file is missing; select it again.".into());
        }
    }
    if let Some(path) = legacy.get("speechModelFolder").and_then(Value::as_str) {
        if PathBuf::from(path).is_absolute() && PathBuf::from(path).is_dir() {
            next.legacy_speech_cache = Some(path.into());
        } else {
            warnings.push("The legacy speech model directory is missing; select it again.".into());
        }
    }
    if let Some(path) = legacy.get("tokenizerCache").and_then(Value::as_str)
        && PathBuf::from(path).is_absolute()
        && PathBuf::from(path).is_dir()
    {
        next.tokenizer_cache = Some(path.into());
    }
    // Core ML folders are references only and never selected as whisper.cpp weights.
    next.import_version = 1;
    (next, warnings)
}
