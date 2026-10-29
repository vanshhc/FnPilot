use crate::{commands::Backend, settings};
use fnpilot_core::{Result, session::Phase};
use fnpilot_runtime::{Runtime, managed::ManagedRuntime};
use fnpilot_speech::model_store::{self, ModelManifest};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
use tauri::{Manager, State};

#[tauri::command]
pub async fn install_speech_model(
    app: tauri::AppHandle,
    source: PathBuf,
    manifest_path: PathBuf,
) -> Result<Value> {
    let token = app
        .state::<Backend>()
        .setup_session
        .begin(Phase::Interpreting)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Backend>();
        let result = (|| {
            if !source.is_absolute()
                || !manifest_path.is_absolute()
                || fs::metadata(&manifest_path)
                    .map_err(|_| "Select a model manifest.")?
                    .len()
                    > 65536
            {
                return Err(
                    "Choose absolute model and manifest paths; the manifest must be under 64 KiB."
                        .into(),
                );
            }
            let manifest: ModelManifest = serde_json::from_slice(
                &fs::read(manifest_path).map_err(|_| "Could not read manifest.")?,
            )
            .map_err(|_| "Invalid model provenance manifest.")?;
            // Verify before using the digest as an application-owned destination name.
            manifest.verify(&source)?;
            state.setup_session.check(token)?;
            let directory = app
                .path()
                .app_data_dir()
                .map_err(|_| "Model directory is unavailable.")?
                .join("models");
            fs::create_dir_all(&directory).map_err(|_| "Could not create model directory.")?;
            let destination = directory.join(format!("{}.bin", manifest.sha256.to_lowercase()));
            if destination.exists() {
                manifest.verify(&destination)?;
            } else {
                model_store::install(&source, &destination, &manifest, || {
                    state.setup_session.check(token).is_err()
                })?;
            }
            state.setup_session.check(token)?;
            let sidecar = destination.with_extension("manifest.json");
            if !sidecar.exists() {
                use std::io::Write;
                let mut file = fs::File::options()
                    .write(true)
                    .create_new(true)
                    .open(sidecar)
                    .map_err(|_| "Could not save installed model provenance.")?;
                file.write_all(
                    &serde_json::to_vec_pretty(&manifest)
                        .map_err(|_| "Could not encode provenance.")?,
                )
                .map_err(|_| "Could not save model provenance.")?;
                file.sync_all()
                    .map_err(|_| "Could not flush model provenance.")?;
            }
            let mut current = state.settings.lock().unwrap();
            let mut next = current.clone();
            next.speech_model_path = Some(destination);
            next.speech_backend = settings::SpeechBackend::Portable;
            state.setup_session.check(token)?;
            settings::save(&state.settings_path, &next)?;
            *current = next.clone();
            Ok(json!({"settings":next,"installed":true}))
        })();
        let _ = state.setup_session.update(
            token,
            if result.is_ok() {
                Phase::Ready
            } else {
                Phase::Error
            },
            "Model setup finished",
        );
        result
    })
    .await
    .map_err(|_| "Model setup worker failed.".to_string())?
}

#[tauri::command]
pub fn cancel_setup(state: State<'_, Backend>) {
    state.setup_session.cancel(false);
}

#[tauri::command]
pub async fn start_runtime(app: tauri::AppHandle) -> Result<Value> {
    // A separate lifetime token lets Stop interrupt readiness without touching a user's server.
    let state = app.state::<Backend>();
    let token = state.runtime_session.begin(Phase::Interpreting)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Backend>();
        let mut owned = state.runtime.lock().unwrap();
        state.runtime_session.check(token)?;
        if let Some(runtime) = owned.as_mut() {
            if runtime.running()? {
                return Ok(json!({"running":true,"port":runtime.port(),"owned":true}));
            }
            owned.take();
        }
        let settings = state.settings.lock().unwrap().clone();
        let model = settings
            .gguf_path
            .ok_or("Select a local GGUF file, save Settings, then start the helper.")?;
        use std::io::Read;
        let mut magic = [0; 4];
        fs::File::open(&model)
            .and_then(|mut f| f.read_exact(&mut magic))
            .map_err(|_| "Could not read GGUF file.")?;
        if &magic != b"GGUF" {
            return Err("Select a GGUF model file.".into());
        }
        let helper = app
            .path()
            .resource_dir()
            .map_err(|_| "Resource directory unavailable.")?
            .join("runtime")
            .join(if cfg!(windows) {
                "llama-server.exe"
            } else {
                "llama-server"
            });
        #[cfg(debug_assertions)]
        let helper = if helper.is_file() {
            helper
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("resources/runtime")
                .join(if cfg!(windows) {
                    "llama-server.exe"
                } else {
                    "llama-server"
                })
        };
        let runtime =
            ManagedRuntime::launch(&helper, &model, 18089, &state.runtime_session, token)?;
        state.runtime_session.check(token)?;
        let mut current = state.settings.lock().unwrap();
        let mut next = current.clone();
        next.configuration.runtime = Runtime::OpenAICompatible;
        next.configuration.endpoint = "http://127.0.0.1:18089/v1".into();
        next.configuration.model = "local-gguf".into();
        next.prefer_bundled_runtime = true;
        settings::save(&state.settings_path, &next)?;
        *current = next.clone();
        *owned = Some(runtime);
        Ok(json!({"running":true,"port":18089,"owned":true,"settings":next}))
    })
    .await
    .map_err(|_| "Runtime worker failed.".to_string())?
}

#[tauri::command]
pub async fn stop_runtime(app: tauri::AppHandle) -> Result<Value> {
    app.state::<Backend>().runtime_session.cancel(false);
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Backend>().runtime.lock().unwrap().take();
        Ok(json!({"running":false,"owned":true}))
    })
    .await
    .map_err(|_| "Runtime stop failed.".to_string())?
}

#[tauri::command]
pub async fn check_runtime(app: tauri::AppHandle) -> Result<Value> {
    tauri::async_runtime::spawn_blocking(move || {
        let config = app.state::<Backend>().settings.lock().unwrap().configuration.clone();
        fnpilot_runtime::Interpreter::new(config)?.health()?;
        Ok(json!({"available":true,"modelActionsEnabled":app.state::<Backend>().settings.lock().unwrap().allow_model_actions}))
    }).await.map_err(|_| "Runtime health worker failed.".to_string())?
}

#[tauri::command]
pub async fn request_microphone(app: tauri::AppHandle) -> Result<Value> {
    // macOS delays privacy prompts for background applications. This explicit
    // setup action can focus Settings; ordinary recording never takes focus.
    if let Some(window) = app.get_webview_window("settings") {
        window
            .set_focus()
            .map_err(|_| "Bring FnPilot Settings to the front and retry.")?;
    }
    tauri::async_runtime::spawn_blocking(|| {
        fnpilot_platform::request(json!({"operation":"requestMicrophone"}))
    })
    .await
    .map_err(|_| "Microphone permission request failed.".to_string())?
}

#[tauri::command]
pub async fn request_accessibility() -> Result<Value> {
    tauri::async_runtime::spawn_blocking(|| {
        fnpilot_platform::request(json!({"operation":"requestAccessibility"}))
    })
    .await
    .map_err(|_| "Permission request failed.".to_string())?
}

#[tauri::command]
pub async fn unload_models(app: tauri::AppHandle) -> Result<Value> {
    let state = app.state::<Backend>();
    state.cancel(&app, false);
    state.runtime_session.cancel(false);
    let state = state.inner().clone();
    let (reply, received) = std::sync::mpsc::channel();
    state.queue_unload(reply)?;
    tauri::async_runtime::spawn_blocking(move || {
        received
            .recv_timeout(std::time::Duration::from_secs(120))
            .map_err(|_| "Model unload timed out.".to_string())?
    })
    .await
    .map_err(|_| "Model unload worker failed.".to_string())?
}

#[tauri::command]
pub async fn request_screen_recording() -> Result<Value> {
    tauri::async_runtime::spawn_blocking(|| {
        fnpilot_platform::request(json!({"operation":"requestScreenRecording"}))
    })
    .await
    .map_err(|_| "Screen Recording permission request failed.".to_string())?
}
