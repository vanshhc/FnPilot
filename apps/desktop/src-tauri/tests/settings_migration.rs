use fnpilot_desktop::{
    diagnostics, migration,
    settings::{self, Settings},
};
use serde_json::json;

#[test]
fn old_settings_keep_computer_use_off_and_enabling_requires_a_vision_model() {
    let old: Settings =
        serde_json::from_value(json!({"schemaVersion":1,"allowModelActions":true})).unwrap();
    assert!(!old.allow_computer_use);
    let mut enabled = Settings {
        allow_computer_use: true,
        ..old
    };
    assert!(enabled.validate().is_err());
    enabled.computer_use_model = "local-vision-model".into();
    enabled.validate().unwrap();
    let (imported, _) = migration::import(
        &Settings::default(),
        &json!({"allowComputerUse":true,"computerUseModel":"unreviewed"}),
    );
    assert!(!imported.allow_computer_use);
}

#[test]
fn opt_in_is_explicit_and_import_is_idempotent() {
    for value in [
        json!({}),
        json!({"allowModelActions":false}),
        json!({"allowModelActions":"true"}),
    ] {
        let (imported, _) = migration::import(&Settings::default(), &value);
        assert!(!imported.allow_model_actions);
    }
    let (mut imported, _) = migration::import(
        &Settings::default(),
        &json!({"allowModelActions":true,"speechModelFolder":"/private/old/CoreML"}),
    );
    assert!(imported.allow_model_actions);
    assert!(imported.speech_model_path.is_none());
    imported.allow_model_actions = false;
    assert_eq!(
        migration::import(&imported, &json!({"allowModelActions":true})).0,
        imported
    );
}

#[test]
fn missing_model_files_remain_visible_as_setup_errors_without_substitution() {
    let path =
        std::env::temp_dir().join(format!("fnpilot-missing-model-{}.json", std::process::id()));
    let settings = Settings {
        speech_model_path: Some(std::env::temp_dir().join("fnpilot-model-does-not-exist.bin")),
        ..Default::default()
    };
    std::fs::write(&path, serde_json::to_vec(&settings).unwrap()).unwrap();
    let loaded = settings::load(&path).unwrap();
    assert_eq!(loaded.speech_model_path, settings.speech_model_path);
    assert!(loaded.validate().is_err());
    std::fs::remove_file(path).unwrap();
}
#[test]
fn interrupted_write_preserves_valid_settings_and_diagnostics_omit_content() {
    let folder = std::env::temp_dir().join(format!("fnpilot-settings-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("settings.json");
    let original = Settings::default();
    settings::save(&path, &original).unwrap();
    std::fs::write(path.with_extension("partial"), b"interrupted").unwrap();
    let changed = Settings {
        allow_model_actions: true,
        ..original.clone()
    };
    assert!(settings::save(&path, &changed).is_err());
    assert_eq!(settings::load(&path).unwrap(), original);
    let diagnostic = diagnostics::report(&original, json!({"accessibility":false})).to_string();
    assert!(!diagnostic.contains("hf.co"));
    assert!(!diagnostic.contains("127.0.0.1"));
    std::fs::remove_dir_all(folder).unwrap();
}
