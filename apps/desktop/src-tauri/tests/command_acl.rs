use serde_json::Value;
#[test]
fn privilege_is_limited_to_the_bundled_settings_window() {
    let capability: Value =
        serde_json::from_str(include_str!("../capabilities/settings.json")).unwrap();
    assert_eq!(capability["windows"], serde_json::json!(["settings"]));
    assert!(capability.get("remote").is_none());
    let config: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    let csp = config["app"]["security"]["csp"].as_str().unwrap();
    assert!(!csp.contains("unsafe-eval"));
    assert!(!csp.contains("unsafe-inline"));
    let permission = include_str!("../permissions/app.toml");
    assert!(!permission.contains("execute_action"));
    assert!(!permission.contains("shell"));
    let manifest = include_str!("../build.rs");
    for name in [
        "submit_instruction",
        "cancel_session",
        "write_settings",
        "request_screen_recording",
    ] {
        assert!(manifest.contains(name));
        assert!(permission.contains(name));
    }
}
