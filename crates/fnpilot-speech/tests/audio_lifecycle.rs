use fnpilot_speech::backend::{Capture, validate_audio};
#[test]
fn short_audible_audio_is_retained_but_silence_and_late_callbacks_are_rejected() {
    for n in [1600, 8000, 16000] {
        assert!(validate_audio(&vec![0.1; n]).is_ok());
    }
    for samples in [
        vec![],
        vec![0.1; 1599],
        vec![0.0; 16000],
        vec![f32::NAN; 1600],
        vec![0.1; 960001],
    ] {
        assert!(validate_audio(&samples).is_err());
    }
    let mut capture = Capture::default();
    capture.start(1);
    capture.append(1, &vec![0.1; 1600]);
    capture.cancel();
    capture.start(2);
    capture.append(1, &vec![0.9; 1600]);
    assert!(capture.finish(1).is_err());
    capture.append(2, &vec![0.1; 1600]);
    assert_eq!(capture.finish(2).unwrap(), vec![0.1; 1600]);
}
