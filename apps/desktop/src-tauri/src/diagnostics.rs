use crate::settings::Settings;
use serde_json::{Value, json};

pub fn report(settings: &Settings, permissions: Value) -> Value {
    json!({"schemaVersion":1,"host":"desktop-app","platform":std::env::consts::OS,
        "permissions":permissions,"modelActionsEnabled":settings.allow_model_actions,
        "computerUseEnabled":settings.allow_computer_use,
        "runtime":settings.configuration.runtime,"speechBackend":settings.speech_backend,
        "speechModelSelected":match settings.speech_backend { crate::settings::SpeechBackend::Portable=>settings.speech_model_path.is_some(),crate::settings::SpeechBackend::WhisperKit=>settings.legacy_speech_cache.is_some() && settings.tokenizer_cache.is_some() },
        "privacy":"No transcript, control labels, home paths or inference payloads are included.",
        "nativeQualification":"pending"})
}
