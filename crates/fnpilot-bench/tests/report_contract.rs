use std::process::Command;
#[test]
fn absent_model_speech_and_desktop_measurements_are_not_passes() {
    let output = std::env::temp_dir().join(format!("fnpilot-report-{}.json", std::process::id()));
    let corpus =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Resources/commands100.json");
    let status = Command::new(env!("CARGO_BIN_EXE_fnpilot-bench"))
        .arg("--parser-only")
        .arg(corpus)
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    assert_eq!(report["count"], 100);
    assert_eq!(report["nativeAcceptance"], false);
    assert!(report["modelCorrect"].is_null());
    assert!(report["speechSeconds"].is_null());
    assert!(report["executionSeconds"].is_null());
    assert_eq!(report["timingStage"], "routing");
    std::fs::remove_file(output).unwrap();
}
