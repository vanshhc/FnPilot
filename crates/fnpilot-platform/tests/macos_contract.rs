#![cfg(target_os = "macos")]
use fnpilot_platform::{current_epoch, request};
use serde_json::json;

#[test]
fn native_read_only_contracts_and_stale_requests_need_no_os_grants() {
    // These calls neither prompt for grants nor operate on any desktop target.
    let report = request(json!({"operation":"permissions"})).unwrap();
    for permission in ["accessibility", "inputMonitoring", "microphone"] {
        assert!(report[permission].is_boolean());
    }
    let legacy = request(json!({"operation":"legacySettings"})).unwrap();
    assert!(legacy.is_object());
    let stale = current_epoch().wrapping_sub(1);
    for operation in [
        "execute",
        "observe",
        "startCapture",
        "autostart",
        "requestAccessibility",
        "requestMicrophone",
        "transcribeWhisperKit",
    ] {
        assert!(request(json!({"operation":operation,"cancellationEpoch":stale})).is_err());
    }
    assert!(request(json!({"operation":"unknown"})).is_err());
    assert!(request(json!({"operation":"configureTalkKey","key":"F8"})).is_err());
    assert!(
        request(json!({"operation":"transcribeWhisperKit"}))
            .unwrap_err()
            .contains("Invalid speech input")
    );
    assert_eq!(
        request(json!({"operation":"unloadSpeech"})).unwrap(),
        json!(true)
    );
    assert_eq!(current_epoch(), stale.wrapping_add(1));
}

#[test]
#[ignore = "Requires explicitly provisioned local speech weights and synthetic samples; never downloads models"]
fn actual_offline_decoder_abi_transcribes_prepared_synthetic_audio() {
    let folder = std::env::var("FNPILOT_TEST_SPEECH_FOLDER").expect("Set explicit model folder");
    let tokenizer = std::env::var("FNPILOT_TEST_TOKENIZER").expect("Set explicit tokenizer cache");
    let audio =
        std::env::var("FNPILOT_TEST_SPEECH_SAMPLES").expect("Set prepared synthetic samples JSON");
    let samples: Vec<f32> = serde_json::from_slice(&std::fs::read(audio).unwrap()).unwrap();
    fnpilot_speech::backend::validate_audio(&samples).unwrap();
    let text=request(json!({"operation":"transcribeWhisperKit","folder":folder,"tokenizerBase":tokenizer,"samples":samples})).unwrap();
    assert!(text.as_str().unwrap().to_lowercase().contains("calculator"));
    assert_eq!(
        request(json!({"operation":"unloadSpeech"})).unwrap(),
        json!(true)
    );
}
