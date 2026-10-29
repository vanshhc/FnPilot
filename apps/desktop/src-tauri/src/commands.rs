use crate::{
    diagnostics,
    settings::{self, Settings, SpeechBackend},
};
use fnpilot_core::{
    Interpretation, Result, direct_ui, parser,
    runner::{self, Platform},
    session::{Phase, Session, Snapshot},
};
use fnpilot_platform::NativePlatform;
use fnpilot_runtime::Interpreter;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
};
use tauri::{Emitter, Manager, State};

#[derive(Clone)]
pub struct Backend {
    pub session: Session,
    pub settings: Arc<Mutex<Settings>>,
    pub settings_path: PathBuf,
    work: mpsc::SyncSender<Box<dyn FnOnce() + Send>>,
    control: Arc<Mutex<()>>,
    pending: Arc<Mutex<Option<Clarification>>>,
    recording_target: Arc<Mutex<Option<(u64, runner::Target)>>>,
    transcript: Arc<Mutex<Option<(u64, String)>>>,
    pub runtime: Arc<Mutex<Option<fnpilot_runtime::managed::ManagedRuntime>>>,
    pub runtime_session: Session,
    pub setup_session: Session,
    #[cfg(feature = "speech")]
    speech: Arc<Mutex<Option<(PathBuf, fnpilot_speech::whisper::Whisper)>>>,
}
#[derive(Clone)]
struct Clarification {
    original: String,
    observed: runner::Observation,
    remaining: Vec<String>,
    budget_used: usize,
}

impl Backend {
    pub fn queue_unload(&self, reply: mpsc::Sender<Result<Value>>) -> Result<()> {
        let state = self.clone();
        self.work
            .try_send(Box::new(move || {
                #[cfg(feature = "speech")]
                state.speech.lock().unwrap().take();
                state.runtime.lock().unwrap().take();
                #[cfg(target_os = "macos")]
                let result = fnpilot_platform::request(json!({"operation":"unloadSpeech"}));
                #[cfg(not(target_os = "macos"))]
                let result = Ok(json!(true));
                let _ = reply.send(result);
            }))
            .map_err(|_| "The previous operation is still stopping. Retry shortly.".into())
    }
    pub fn new(path: PathBuf) -> Result<Self> {
        let (settings, load_error) = match settings::load(&path) {
            Ok(settings) => (settings, None),
            Err(error) => (
                Settings::default(),
                Some(format!(
                    "{error} The original file is preserved. Saving explicitly creates a backup."
                )),
            ),
        };
        let session = Session::default();
        let (work, jobs) = mpsc::sync_channel::<Box<dyn FnOnce() + Send>>(1);
        std::thread::spawn(move || {
            for job in jobs {
                job();
            }
        });
        if let Some(error) = load_error.or_else(|| settings.validate().err()) {
            let token = session.begin(Phase::Interpreting)?;
            session.update(token, Phase::Error, error)?;
        }
        Ok(Self {
            session,
            settings: Arc::new(Mutex::new(settings)),
            settings_path: path,
            work,
            control: Arc::new(Mutex::new(())),
            pending: Arc::new(Mutex::new(None)),
            recording_target: Arc::new(Mutex::new(None)),
            transcript: Arc::new(Mutex::new(None)),
            runtime: Arc::new(Mutex::new(None)),
            runtime_session: Session::default(),
            setup_session: Session::default(),
            #[cfg(feature = "speech")]
            speech: Arc::new(Mutex::new(None)),
        })
    }
    pub(crate) fn publish(&self, app: &tauri::AppHandle) {
        let _ = app.emit("session-state", self.session.snapshot());
    }
    fn phase(
        &self,
        app: &tauri::AppHandle,
        token: u64,
        phase: Phase,
        message: impl Into<String>,
    ) -> Result<()> {
        let epoch = {
            let _guard = self.control.lock().unwrap();
            self.session.update(token, phase, message)?;
            fnpilot_platform::current_epoch()
        };
        self.publish(app);
        let label = match phase {
            Phase::Recording => "Listening",
            Phase::Transcribing => "Transcribing",
            Phase::Interpreting => "Interpreting",
            Phase::Acting => "Acting",
            Phase::Clarifying => "Clarification needed",
            Phase::Error => "Could not finish",
            Phase::Ready => "Done",
            Phase::Paused => "Paused",
        };
        let transcript = self.transcript.lock().unwrap();
        let label = match transcript.as_ref() {
            Some((generation, text)) if *generation == token => format!("{label}\n{text}"),
            _ => label.to_string(),
        };
        drop(transcript);
        let _ = fnpilot_platform::request(
            json!({"operation":"overlay","message":label,"cancellationEpoch":epoch}),
        );
        Ok(())
    }
    pub fn submit(&self, app: tauri::AppHandle, input: String) -> Result<u64> {
        if input.trim().is_empty() || input.chars().count() > 16_384 {
            return Err("Enter an instruction of 1–16384 characters.".into());
        }
        let (token, epoch) = {
            let _guard = self.control.lock().unwrap();
            (
                self.session.begin(Phase::Interpreting)?,
                fnpilot_platform::invalidate(),
            )
        };
        self.publish(&app);
        let state = self.clone();
        let handle = app.clone();
        if self
            .work
            .try_send(Box::new(move || {
                if state.session.check(token).is_err() {
                    return;
                }
                let _ = fnpilot_platform::request(
                    json!({"operation":"cancel","cancellationEpoch":epoch}),
                );
                if let Err(error) = state.route(&handle, token, &input) {
                    let _ = state.phase(&handle, token, Phase::Error, error);
                }
            }))
            .is_err()
        {
            self.phase(
                &app,
                token,
                Phase::Error,
                "The previous operation is still stopping. Retry shortly.",
            )?;
            return Err("The command worker is busy.".into());
        }
        Ok(token)
    }
    fn route(&self, app: &tauri::AppHandle, token: u64, input: &str) -> Result<()> {
        self.session.check(token)?;
        let settings = self.settings.lock().unwrap().clone();
        let mut platform = NativePlatform;
        let origin = {
            let mut recording = self.recording_target.lock().unwrap();
            if recording
                .as_ref()
                .is_some_and(|(generation, _)| *generation == token)
            {
                recording.take().map(|(_, target)| target)
            } else {
                None
            }
        };
        if let Some(origin) = &origin {
            self.session.check(token)?;
            if &platform.observe()?.target != origin {
                return Err(
                    "The original recording target changed. Retry in the intended app.".into(),
                );
            }
        }
        if settings.allow_computer_use {
            let direct = parser::parse(input);
            let observed = platform.observe()?;
            if origin
                .as_ref()
                .is_some_and(|target| target != &observed.target)
            {
                return Err(
                    "The original recording target changed. Retry in the intended app.".into(),
                );
            }
            // Apparent app names absent from the live catalog may be in-app requests.
            let use_screen = direct.as_ref().is_none_or(|result| {
                result.actions.iter().any(|action| {
                    action.kind == "open_app"
                        && action.app.as_ref().is_some_and(|name| {
                            !observed
                                .context
                                .apps
                                .iter()
                                .any(|app| app.name.eq_ignore_ascii_case(name))
                        })
                })
            });
            if use_screen {
                self.pending.lock().unwrap().take();
                let mut config = settings.configuration.clone();
                config.model = settings.computer_use_model.clone();
                let interpreter = Interpreter::new(config)?;
                self.session.check(token)?;
                fnpilot_platform::request(
                    json!({"operation":"computerPrepare","target":observed.target}),
                )?;
                let result = fnpilot_core::computer::run(
                    &mut platform,
                    &self.session,
                    token,
                    &observed.target,
                    |screen, history| {
                        self.phase(
                            app,
                            token,
                            Phase::Interpreting,
                            format!(
                                "Looking at {} · step {}",
                                screen.app_name,
                                history.len() + 1
                            ),
                        )?;
                        let step = interpreter.computer_step_session(
                            input,
                            screen,
                            history,
                            &self.session,
                            token,
                        )?;
                        if !step.terminal() {
                            self.phase(app, token, Phase::Acting, "Using the cursor")?;
                        }
                        Ok(step)
                    },
                );
                let _ = fnpilot_platform::request(json!({"operation":"computerRelease"}));
                let result = result?;
                use fnpilot_core::computer::Step;
                return match result {
                    Step::Done { message } => {
                        self.phase(app, token, Phase::Ready, format!("Screen agent: {message}"))
                    }
                    Step::Clarify { message } => self.phase(
                        app,
                        token,
                        Phase::Clarifying,
                        format!("{message} Repeat the full instruction with your answer."),
                    ),
                    Step::Unsupported { message } => self.phase(app, token, Phase::Error, message),
                    _ => unreachable!("computer loop returns only terminal steps"),
                };
            }
        }
        let mut parts = if settings.allow_model_actions {
            vec![input.to_string()]
        } else {
            parser::parts(input)
        };
        let pending = self.pending.lock().unwrap().take();
        let mut continuation = None;
        if let Some(pending) = pending {
            let context = platform.observe()?;
            let direct = parser::parse(input).or_else(|| direct_ui::parse(input, &context.context));
            if direct.is_none() {
                self.session.carry_budget(token, pending.budget_used)?;
                if !pending.observed.target.same_window(&context.target) {
                    return Err(
                        "The clarification target changed. Give the full instruction again.".into(),
                    );
                }
                continuation = direct_ui::answer(&pending.original, input, &context.context);
                if continuation.is_none() && settings.allow_model_actions {
                    let mut ctx = context.context.clone();
                    ctx.clarification = Some(pending.original.clone());
                    continuation = Some(
                        Interpreter::new(settings.configuration.clone())?.interpret_session(
                            input,
                            &ctx,
                            &self.session,
                            token,
                        )?,
                    );
                }
                parts = std::iter::once(input.to_string())
                    .chain(pending.remaining)
                    .collect();
            }
        }
        let mut expected_target = origin;
        let mut verified = true;
        for (index, part) in parts.iter().enumerate() {
            self.phase(app, token, Phase::Interpreting, "Interpreting")?;
            let mut observed = platform.observe()?;
            if expected_target
                .as_ref()
                .is_some_and(|target| target != &observed.target)
            {
                return Err("The sequence target changed. Retry in the intended app.".into());
            }
            let result = if let Some(reply) = continuation.take() {
                reply
            } else if settings.allow_model_actions {
                Interpreter::new(settings.configuration.clone())?.interpret_session(
                    part,
                    &observed.context,
                    &self.session,
                    token,
                )?
            } else {
                runner::interpret(part, &observed.context, |_, _| {
                    Ok(Interpretation::message(
                        "unsupported",
                        "The direct parser does not recognize this instruction. Experimental model actions are disabled in Settings.",
                    ))
                })?
            };
            self.session.check(token)?;
            result.validate()?;
            if result.status != "actions" {
                let message = result.message.clone().unwrap_or_default();
                if result.status == "clarify" {
                    *self.pending.lock().unwrap() = Some(Clarification {
                        original: part.clone(),
                        observed,
                        remaining: parts[index + 1..].to_vec(),
                        budget_used: self.session.snapshot().action_count,
                    });
                    let result = self.phase(app, token, Phase::Clarifying, message);
                    if result.is_err() {
                        self.pending.lock().unwrap().take();
                    }
                    return result;
                }
                return self.phase(app, token, Phase::Error, message);
            }
            self.phase(app, token, Phase::Acting, "Acting")?;
            for outcome in
                runner::execute(&mut platform, &self.session, token, &result, &mut observed)?
            {
                verified &= outcome.verified;
            }
            expected_target = Some(observed.target);
        }
        self.phase(
            app,
            token,
            Phase::Ready,
            if verified {
                "Done — observed success"
            } else {
                "Actions sent — effect not fully verified"
            },
        )
    }
    pub fn key_signal(&self, app: &tauri::AppHandle, signal: fnpilot_core::hotkey::Signal) {
        use fnpilot_core::hotkey::Signal;
        match signal {
            Signal::Start => {
                let (token, epoch) = {
                    let _guard = self.control.lock().unwrap();
                    let Ok(token) = self.session.begin(Phase::Recording) else {
                        return;
                    };
                    (token, fnpilot_platform::invalidate())
                };
                let mut platform = NativePlatform;
                match platform.observe() {
                    Ok(observed) => {
                        let _guard = self.control.lock().unwrap();
                        if self.session.check(token).is_err() {
                            return;
                        }
                        *self.recording_target.lock().unwrap() = Some((token, observed.target))
                    }
                    Err(error) => {
                        let _ = self.phase(app, token, Phase::Error, error);
                        return;
                    }
                }
                if let Err(error) = fnpilot_platform::request(
                    json!({"operation":"startCapture","cancellationEpoch":epoch}),
                ) {
                    let _ = self.phase(app, token, Phase::Error, error);
                    return;
                }
                if self.session.check(token).is_err() {
                    let _ = fnpilot_platform::request(
                        json!({"operation":"cancel","cancellationEpoch":epoch}),
                    );
                    return;
                }
                let _ = fnpilot_platform::request(
                    json!({"operation":"overlay","message":"Listening","cancellationEpoch":epoch}),
                );
                self.publish(app);
            }
            Signal::Finish => {
                let (current, epoch) = {
                    let _guard = self.control.lock().unwrap();
                    (self.session.snapshot(), fnpilot_platform::current_epoch())
                };
                if current.phase != Phase::Recording {
                    return;
                }
                let token = current.generation;
                let samples: Result<Vec<f32>> = fnpilot_platform::request(
                    json!({"operation":"stopCapture","cancellationEpoch":epoch}),
                )
                .and_then(|value| {
                    serde_json::from_value(value).map_err(|_| "Invalid captured audio.".into())
                });
                let samples = match samples {
                    Ok(samples) => samples,
                    Err(error) => {
                        let _ = self.phase(app, token, Phase::Error, error);
                        return;
                    }
                };
                let state = self.clone();
                let handle = app.clone();
                if self
                    .work
                    .try_send(Box::new(move || {
                        let result = (|| {
                            state.session.check(token)?;
                            state.phase(&handle, token, Phase::Transcribing, "Transcribing")?;
                            let text = state.transcribe(&samples, token)?;
                            {
                                let _guard = state.control.lock().unwrap();
                                state.session.check(token)?;
                                *state.transcript.lock().unwrap() = Some((token, text.clone()));
                            }
                            state.phase(&handle, token, Phase::Interpreting, "Interpreting")?;
                            state.route(&handle, token, &text)
                        })();
                        if let Err(error) = result {
                            let _ = state.phase(&handle, token, Phase::Error, error);
                        }
                    }))
                    .is_err()
                {
                    let _ = self.phase(
                        app,
                        token,
                        Phase::Error,
                        "The previous operation is still stopping. Retry shortly.",
                    );
                }
            }
            Signal::Cancel => self.cancel(app, false),
        }
    }
    fn transcribe(&self, samples: &[f32], token: u64) -> Result<String> {
        fnpilot_speech::backend::validate_audio(samples)?;
        self.session.check(token)?;
        let selection = self.settings.lock().unwrap().clone();
        if selection.speech_backend == SpeechBackend::WhisperKit {
            #[cfg(feature = "speech")]
            {
                self.speech.lock().unwrap().take();
            }
            self.session.check(token)?;
            let folder = selection
                .legacy_speech_cache
                .ok_or("Select the existing WhisperKit small.en model folder in Settings.")?;
            let tokenizer = selection
                .tokenizer_cache
                .ok_or("Select the existing WhisperKit tokenizer cache in Settings.")?;
            let epoch = {
                let _guard = self.control.lock().unwrap();
                self.session.check(token)?;
                fnpilot_platform::current_epoch()
            };
            let result = fnpilot_platform::request(
                json!({"operation":"transcribeWhisperKit", "folder":folder,"tokenizerBase":tokenizer,"samples":samples,"cancellationEpoch":epoch}),
            )?;
            self.session.check(token)?;
            return result
                .as_str()
                .map(str::to_string)
                .ok_or("Invalid native transcription.".into());
        }
        #[cfg(target_os = "macos")]
        fnpilot_platform::request(json!({"operation":"unloadSpeech"}))?;
        #[cfg(feature = "speech")]
        {
            use fnpilot_speech::{
                backend::SpeechBackend, model_store::ModelManifest, whisper::Whisper,
            };
            let path = selection
                .speech_model_path
                .ok_or("Select verified portable speech weights in Settings.")?;
            let manifest: ModelManifest = serde_json::from_slice(
                &std::fs::read(path.with_extension("manifest.json"))
                    .map_err(|_| "Speech model provenance manifest is missing.")?,
            )
            .map_err(|_| "Speech manifest is invalid.")?;
            let mut cached = self.speech.lock().unwrap();
            if cached
                .as_ref()
                .is_none_or(|(selected, _)| selected != &path)
            {
                *cached = Some((path.clone(), Whisper::load(&path, &manifest)?));
            }
            cached
                .as_mut()
                .unwrap()
                .1
                .transcribe(samples, &self.session, token)
        }
        #[cfg(not(feature = "speech"))]
        {
            Err("This model-free developer build excludes speech decoding. Build with --features speech and select verified weights.".into())
        }
    }
    pub fn cancel(&self, app: &tauri::AppHandle, pause: bool) {
        let epoch = {
            let _guard = self.control.lock().unwrap();
            self.session.cancel(pause);
            let epoch = fnpilot_platform::invalidate();
            self.pending.lock().unwrap().take();
            self.recording_target.lock().unwrap().take();
            epoch
        };
        self.publish(app);
        // Never wait on the execution mutex to invalidate the token or stop recording.
        std::thread::spawn(move || {
            let _ =
                fnpilot_platform::request(json!({"operation":"cancel","cancellationEpoch":epoch}));
        });
    }
}

#[tauri::command]
pub fn snapshot(state: State<'_, Backend>) -> Snapshot {
    state.session.snapshot()
}
#[tauri::command]
pub async fn submit_instruction(app: tauri::AppHandle, input: String) -> Result<u64> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Backend>().submit(app.clone(), input))
        .await
        .map_err(|_| "Command submission failed.".to_string())?
}
#[tauri::command]
pub fn cancel_session(app: tauri::AppHandle, state: State<'_, Backend>) {
    state.cancel(&app, false);
}
#[tauri::command]
pub fn set_paused(app: tauri::AppHandle, state: State<'_, Backend>, paused: bool) {
    if paused {
        state.cancel(&app, true);
    } else {
        state.session.resume();
        state.publish(&app);
    }
}
#[tauri::command]
pub fn read_settings(state: State<'_, Backend>) -> Settings {
    state.settings.lock().unwrap().clone()
}
#[tauri::command]
pub async fn write_settings(app: tauri::AppHandle, settings: Settings) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Backend>();
        let mut runtime = state
            .runtime
            .try_lock()
            .map_err(|_| "The helper is starting or stopping. Retry saving shortly.")?;
        let mut current = state.settings.lock().unwrap();
        if settings.gguf_path != current.gguf_path
            && let Some(helper) = runtime.as_mut()
            && helper.running()?
        {
            return Err("Stop the owned helper before selecting a different GGUF model.".into());
        }
        settings.validate()?;
        fnpilot_platform::request(
            json!({"operation":"configureTalkKey", "key":settings.talk_key}),
        )?;
        if let Err(error) = settings::save(&state.settings_path, &settings) {
            let _ = fnpilot_platform::request(
                json!({"operation":"configureTalkKey", "key":current.talk_key}),
            );
            return Err(error);
        }
        *current = settings;
        drop(current);
        drop(runtime);
        state.cancel(&app, false);
        Ok(())
    })
    .await
    .map_err(|_| "Settings worker failed.".to_string())?
}
#[tauri::command]
pub async fn diagnose(app: tauri::AppHandle) -> Result<Value> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Backend>();
        let permissions = fnpilot_platform::request(json!({"operation":"permissions"}))
            .unwrap_or(json!({"status":"unavailable"}));
        Ok(diagnostics::report(
            &state.settings.lock().unwrap(),
            permissions,
        ))
    })
    .await
    .map_err(|_| "Diagnostics worker failed.".to_string())?
}

#[tauri::command]
pub async fn preview_legacy_settings(app: tauri::AppHandle) -> Result<Value> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Backend>();
        let legacy = fnpilot_platform::request(json!({"operation":"legacySettings"}))?;
        let (settings, warnings) =
            crate::migration::import(&state.settings.lock().unwrap(), &legacy);
        Ok(json!({"settings":settings,"warnings":warnings,"sourceModified":false}))
    })
    .await
    .map_err(|_| "Import preview worker failed.".to_string())?
}
#[tauri::command]
pub async fn import_legacy_settings(app: tauri::AppHandle) -> Result<Value> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Backend>();
        let mut owned = state
            .runtime
            .try_lock()
            .map_err(|_| "The helper is starting or stopping. Retry import shortly.")?;
        if let Some(helper) = owned.as_mut()
            && helper.running()?
        {
            return Err("Stop the owned helper before importing legacy model selections.".into());
        }
        let legacy = fnpilot_platform::request(json!({"operation":"legacySettings"}))?;
        let mut current = state.settings.lock().unwrap();
        let (settings, warnings) = crate::migration::import(&current, &legacy);
        settings::save(&state.settings_path, &settings)?;
        *current = settings.clone();
        Ok(json!({"settings":settings,"warnings":warnings,"sourceModified":false}))
    })
    .await
    .map_err(|_| "Import worker failed.".to_string())?
}
#[tauri::command]
pub async fn configure_autostart(app: tauri::AppHandle, enabled: bool) -> Result<Value> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Backend>();
        let result = fnpilot_platform::request(json!({"operation":"autostart","enabled":enabled}))?;
        let mut current = state.settings.lock().unwrap();
        let mut updated = current.clone();
        updated.launch_at_login_intent = enabled;
        settings::save(&state.settings_path, &updated)?;
        *current = updated;
        Ok(result)
    })
    .await
    .map_err(|_| "Login configuration worker failed.".to_string())?
}
