use std::{fs, path::Path};

fn swift_files(directory: &Path, result: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            swift_files(&path, result);
        } else if path.extension().is_some_and(|ext| ext == "swift") {
            result.push(path);
        }
    }
}

#[test]
fn swift_is_a_speech_decoder_binding_not_a_feature_implementation_surface() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native/macos/Sources");
    let mut files = Vec::new();
    swift_files(&root, &mut files);
    for path in files {
        let relative = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if relative.starts_with("FnPilotSpeechBench/") {
            continue;
        }
        assert!(
            [
                "FnPilotSpeech/WhisperSpeech.swift",
                "FnPilotSpeech/SpeechABI.swift"
            ]
            .contains(&relative.as_str()),
            "App/platform feature code must be Rust; unexpected Swift source: {relative}"
        );
        let source = fs::read_to_string(path).unwrap();
        if relative.ends_with("/SpeechABI.swift") {
            let operations = source
                .lines()
                .filter_map(|line| line.trim().strip_prefix("case \""))
                .filter_map(|tail| tail.split('"').next())
                .collect::<Vec<_>>();
            assert_eq!(
                operations,
                vec!["transcribeWhisperKit", "unloadSpeech"],
                "Only decoder operations may cross the Swift ABI"
            );
        }
        for forbidden in [
            "import AppKit",
            "import Cocoa",
            "import SwiftUI",
            "import AVFoundation",
            "NSWorkspace",
            "AXUIElement",
            "CGEvent",
            "import ApplicationServices",
            "import ServiceManagement",
            "struct Action",
            "CommandContext",
            "case \"execute\"",
            "case \"observe\"",
        ] {
            assert!(
                !source.contains(forbidden),
                "Speech-only Swift binding contains app feature code: {forbidden}"
            );
        }
    }
}
