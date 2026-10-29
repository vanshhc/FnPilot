fn main() {
    if let Ok(paths) = std::env::var("DEP_FNPILOTSPEECH_SWIFT_RUNTIME_PATHS") {
        for path in paths.split(';') {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{path}");
        }
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
    }
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "snapshot",
            "submit_instruction",
            "cancel_session",
            "set_paused",
            "read_settings",
            "write_settings",
            "diagnose",
            "preview_legacy_settings",
            "import_legacy_settings",
            "configure_autostart",
            "install_speech_model",
            "cancel_setup",
            "start_runtime",
            "stop_runtime",
            "check_runtime",
            "request_microphone",
            "unload_models",
            "request_accessibility",
            "request_screen_recording",
        ]),
    ))
    .expect("Tauri build configuration");
}
