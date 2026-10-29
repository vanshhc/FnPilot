mod commands;
pub mod diagnostics;
pub mod migration;
pub mod settings;
mod setup;

use commands::Backend;
use tauri::{
    Manager, WebviewUrl, WebviewWindowBuilder,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};

fn show_settings(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
    } else if let Ok(window) =
        WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html".into()))
            .title("FnPilot Settings")
            .inner_size(700.0, 800.0)
            .min_inner_size(520.0, 620.0)
            .build()
    {
        let _ = window.set_focus();
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _working_directory| {
            // Reopening shows Settings; external arguments never become instructions.
            show_settings(app);
        }))
        .invoke_handler(tauri::generate_handler![
            commands::snapshot,
            commands::submit_instruction,
            commands::cancel_session,
            commands::set_paused,
            commands::read_settings,
            commands::write_settings,
            commands::diagnose,
            commands::preview_legacy_settings,
            commands::import_legacy_settings,
            commands::configure_autostart,
            setup::install_speech_model, setup::cancel_setup, setup::start_runtime, setup::stop_runtime, setup::check_runtime, setup::request_microphone, setup::unload_models, setup::request_accessibility, setup::request_screen_recording
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let path = app.path().app_config_dir()?.join("settings.json");
            let first_run = !path.exists();
            let backend = Backend::new(path).map_err(std::io::Error::other)?;
            #[cfg(windows)]
            fnpilot_platform::request(serde_json::json!({"operation":"configureTalkKey", "key":backend.settings.lock().unwrap().talk_key})).map_err(std::io::Error::other)?;
            app.manage(backend.clone());
            let show = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let pause = MenuItem::with_id(app, "pause", "Pause / resume", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &pause, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().expect("Bundled tray icon").clone())
                .tooltip("FnPilot Preview")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => {
                        show_settings(app);
                    }
                    "pause" => {
                        let state = app.state::<Backend>();
                        if state.session.snapshot().phase == fnpilot_core::session::Phase::Paused {
                            state.session.resume();
                            state.publish(app);
                        } else {
                            state.cancel(app, true);
                        }
                    }
                    "quit" => {
                        let state = app.state::<Backend>();
                        state.cancel(app, false);
                        state.runtime_session.cancel(false);
                        state.runtime.lock().unwrap().take();
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;
            if first_run { show_settings(app.handle()); }
            let handle = app.handle().clone();
            if let Ok(events) = fnpilot_platform::key_events() {
                std::thread::spawn(move || {
                    let mut key = fnpilot_core::hotkey::HotkeyState::default();
                    for (kind, a, b) in events {
                        let signal = match kind {
                            0 => key.flags(a != 0, b != 0),
                            1 => key.key_down(a != 0),
                            _ => {
                                key.reset();
                                Some(fnpilot_core::hotkey::Signal::Cancel)
                            }
                        };
                        if let Some(signal) = signal {
                            backend.key_signal(&handle, signal);
                        }
                    }
                });
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("FnPilot desktop startup failed")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event
                && let Some(state) = app.try_state::<Backend>()
            {
                state.cancel(app, false);
                state.runtime_session.cancel(false);
                state.runtime.lock().unwrap().take();
            }
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { has_visible_windows: false, .. } = event {
                show_settings(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = app;
            if let tauri::RunEvent::ExitRequested { code: None, api, .. } = event {
                // Closing Settings releases its WebView. The tray remains until explicit Quit.
                api.prevent_exit();
            }
        });
}
