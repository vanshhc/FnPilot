#![allow(non_upper_case_globals)]
mod apps;
mod browser;
mod computer;
use crate::capture as audio;
mod executor;
mod overlay;
mod targets;
mod uia;

use fnpilot_core::{
    Action, Result,
    context::{AppInfo, CommandContext},
    runner::Observation,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::HWND,
    System::Com::{COINIT_MULTITHREADED, CoInitializeEx},
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

pub(super) static EPOCH: AtomicU64 = AtomicU64::new(1);
pub(super) static HELD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static TALK_KEY: AtomicU64 = AtomicU64::new(VK_RCONTROL.0 as u64);
pub(crate) fn held() -> bool {
    HELD.load(Ordering::SeqCst)
}
pub fn invalidate() -> u64 {
    EPOCH.fetch_add(1, Ordering::SeqCst) + 1
}
pub fn current_epoch() -> u64 {
    EPOCH.load(Ordering::SeqCst)
}
struct Job {
    value: Value,
    deadline: Instant,
    epoch: u64,
    reply: mpsc::Sender<Result<Value>>,
}
static WORKER: OnceLock<mpsc::Sender<Job>> = OnceLock::new();
static LISTENING: OnceLock<()> = OnceLock::new();

struct State {
    computer_frame: Option<computer::Frame>,
    computer_sequence: u64,
    target: Option<HWND>,
    controls: uia::Controls,
    placements: HashMap<usize, WINDOWPLACEMENT>,
}

fn check(deadline: Instant, epoch: u64) -> Result<()> {
    if Instant::now() > deadline || EPOCH.load(Ordering::SeqCst) != epoch || !interactive() {
        Err("Native request expired or was cancelled.".into())
    } else {
        Ok(())
    }
}
fn interactive() -> bool {
    use windows::Win32::{Foundation::HANDLE, System::StationsAndDesktops::*};
    unsafe {
        let Ok(desktop) = OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS)
        else {
            return false;
        };
        let mut name = [0u16; 128];
        let mut needed = 0;
        let available = GetUserObjectInformationW(
            HANDLE(desktop.0),
            UOI_NAME,
            Some(name.as_mut_ptr().cast()),
            (name.len() * 2) as u32,
            Some(&mut needed),
        )
        .is_ok();
        let _ = CloseDesktop(desktop);
        available
            && String::from_utf16_lossy(
                &name[..name.iter().position(|c| *c == 0).unwrap_or(name.len())],
            )
            .eq_ignore_ascii_case("default")
    }
}
impl State {
    fn observe(&mut self, deadline: Instant) -> Result<Observation> {
        let foreground = unsafe { GetForegroundWindow() };
        let (target, title) = targets::target(foreground)?;
        if target.process_id != std::process::id() {
            self.target = Some(foreground);
        }
        let hwnd = self
            .target
            .ok_or("Focus an ordinary application before opening Settings.")?;
        let (mut target, name) = if hwnd == foreground {
            (target, title)
        } else {
            targets::target(hwnd)?
        };
        target.generation = EPOCH.load(Ordering::SeqCst);
        let (controls, complete) = self.controls.scan(hwnd, deadline)?;
        let mut apps = apps::context_apps();
        apps.extend(apps::running().into_iter().map(|(name, window)| AppInfo {
            name,
            bundle_id: format!("windows-window:{}", window.0 as usize),
        }));
        let browser_actions: Vec<String> =
            if fnpilot_core::browser::supported("windows", &target.app_identity) {
                fnpilot_core::browser::KINDS
                    .iter()
                    .map(|k| (*k).into())
                    .collect()
            } else {
                vec![]
            };
        Ok(Observation {
            target,
            context: CommandContext {
                browser_actions: browser_actions.clone(),
                app_name: name,
                apps,
                controls,
                controls_complete: Some(complete),
                ..Default::default()
            },
            capabilities: [
                "open_app",
                "focus_app",
                "quit_app",
                "type_text",
                "close_window",
                "minimize_window",
                "maximize_window",
                "restore_window",
                "press_control",
                "set_control_value",
                "submit_control",
                "scroll",
                "volume_up",
                "volume_down",
                "set_volume",
                "mute",
                "unmute",
            ]
            .map(str::to_string)
            .into_iter()
            .chain(browser_actions)
            .collect(),
        })
    }
    fn request(&mut self, value: Value, deadline: Instant, epoch: u64) -> Result<Value> {
        if value["operation"] == "computerRelease" {
            self.computer_frame = None;
            return Ok(json!(true));
        }
        check(deadline, epoch)?;
        match value["operation"].as_str() {
            Some("computerPrepare") => self.computer_prepare(&value, deadline, epoch),
            Some("computerCapture") => {
                serde_json::to_value(self.computer_capture(deadline, epoch)?)
                    .map_err(|_| "Screenshot encoding failed.".into())
            }
            Some("computerAct") => self.computer_act(&value, deadline, epoch),
            Some("observe") => serde_json::to_value(self.observe(deadline)?)
                .map_err(|_| "Observation encoding failed.".into()),
            Some("execute") => {
                let action: Action = serde_json::from_value(value["action"].clone())
                    .map_err(|_| "Invalid native action.")?;
                let expected: Observation = serde_json::from_value(value["expected"].clone())
                    .map_err(|_| "Invalid native target.")?;
                let fresh = self.observe(deadline)?;
                if expected.target != fresh.target {
                    return Err("The native target changed before dispatch.".into());
                }
                if let Some(id) = &action.control_id
                    && (fresh.context.controls_complete != Some(true)
                        || expected.context.controls.iter().find(|c| &c.id == id)
                            != fresh.context.controls.iter().find(|c| &c.id == id))
                {
                    return Err("The control changed before native dispatch.".into());
                }
                fnpilot_core::runner::validate_context_action(&action, &fresh.context)?;
                if action.control_id.is_some()
                    && expected
                        .context
                        .controls
                        .iter()
                        .filter(|c| c.focused)
                        .collect::<Vec<_>>()
                        != fresh
                            .context
                            .controls
                            .iter()
                            .filter(|c| c.focused)
                            .collect::<Vec<_>>()
                {
                    return Err(
                        "The focused field or draft changed. Repeat your instruction.".into(),
                    );
                }
                check(deadline, epoch)?;
                serde_json::to_value(executor::execute(
                    self, &action, &expected, deadline, epoch,
                )?)
                .map_err(|_| "Outcome encoding failed.".into())
            }
            Some("permissions") => Ok(
                json!({"accessibility":"ordinary-user UI Automation","inputMonitoring":"Right Ctrl polling","microphone":"checked on explicit capture","elevatedTargets":false}),
            ),
            Some("legacySettings") => Ok(json!({})),
            Some("autostart") => {
                let enabled = value["enabled"]
                    .as_bool()
                    .ok_or("Invalid launch-at-login request.")?;
                let user = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
                let (key, _) = user
                    .create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
                    .map_err(|_| "Could not open the per-user login configuration.")?;
                if enabled {
                    let executable =
                        std::env::current_exe().map_err(|_| "Could not locate this app.")?;
                    key.set_value("FnPilotPreview", &format!("\"{}\"", executable.display()))
                        .map_err(|_| "Could not enable launch at login.")?;
                } else {
                    match key.delete_value("FnPilotPreview") {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(_) => return Err("Could not disable launch at login.".into()),
                    }
                }
                Ok(json!({"enabled":enabled,"approvalRequired":false}))
            }
            _ => Err("Unknown Windows native operation.".into()),
        }
    }
}

pub fn request(value: Value) -> Result<Value> {
    if value
        .get("cancellationEpoch")
        .and_then(Value::as_u64)
        .is_some_and(|epoch| epoch != current_epoch())
    {
        return Err("Stale native request.".into());
    }
    let operation = value["operation"].as_str().unwrap_or("").to_string();
    if operation == "configureTalkKey" {
        let key = match value["key"].as_str() {
            Some("rightCtrl") => VK_RCONTROL,
            Some("rightAlt") => VK_RMENU,
            Some("f8") => VK_F8,
            Some("f9") => VK_F9,
            _ => return Err("Choose Right Ctrl, Right Alt, F8 or F9.".into()),
        };
        if unsafe { GetAsyncKeyState(key.0 as i32) } < 0 {
            return Err("Release the selected talk key before saving its configuration.".into());
        }
        if [VK_F8, VK_F9].contains(&key) {
            unsafe {
                RegisterHotKey(None,0x464e,MOD_NOREPEAT,key.0 as u32).map_err(|_| "That function key is reserved by another global shortcut. Choose a different talk key.")?;
                let _ = UnregisterHotKey(None, 0x464e);
            }
        }
        TALK_KEY.store(key.0 as u64, Ordering::SeqCst);
        return Ok(json!(true));
    }
    if operation == "requestMicrophone" {
        return Ok(
            json!({"instructions":"Enable microphone access for desktop apps in Windows Settings > Privacy & security > Microphone. Recording starts only while the selected key is held."}),
        );
    }
    if operation == "requestAccessibility" {
        return Ok(
            json!({"accessibility":"ordinary-user UI Automation is available without an elevation prompt", "instructions":"Focus an ordinary-user desktop app. Elevated, protected and secure-desktop targets are unsupported."}),
        );
    }
    if operation == "cancel" {
        if value
            .get("cancellationEpoch")
            .and_then(Value::as_u64)
            .is_some_and(|epoch| epoch != EPOCH.load(Ordering::SeqCst))
        {
            return Err("Stale cancellation cleanup.".into());
        }
        let _ = audio::request("cancel");
        return overlay::show(None, EPOCH.load(Ordering::SeqCst)).map(|_| json!(true));
    }
    if operation == "overlay" {
        return overlay::show(
            Some(value["message"].as_str().unwrap_or("Working").into()),
            value
                .get("cancellationEpoch")
                .and_then(Value::as_u64)
                .unwrap_or_else(current_epoch),
        )
        .map(|_| json!(true));
    }
    if ["startCapture", "stopCapture"].contains(&operation.as_str()) {
        return audio::request(&operation);
    }
    let sender = WORKER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::spawn(move || {
            if unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_err() {
                return;
            }
            let Ok(controls) = uia::Controls::new() else {
                return;
            };
            let mut state = State {
                computer_frame: None,
                computer_sequence: 0,
                target: None,
                controls,
                placements: HashMap::new(),
            };
            for job in rx {
                let result = state.request(job.value, job.deadline, job.epoch);
                let _ = job.reply.send(result);
            }
        });
        tx
    });
    let (tx, rx) = mpsc::channel();
    let deadline = Instant::now() + Duration::from_secs(3);
    sender
        .send(Job {
            value,
            deadline,
            epoch: EPOCH.load(Ordering::SeqCst),
            reply: tx,
        })
        .map_err(|_| "The Windows native worker is unavailable.")?;
    rx.recv_timeout(Duration::from_secs(3))
        .map_err(|_| "Windows provider timed out. No subsequent action will run.".to_string())?
}

pub fn key_events() -> Result<mpsc::Receiver<(i32, i32, i32)>> {
    LISTENING
        .set(())
        .map_err(|_| "The Windows key listener is already attached.")?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut held = false;
        let mut escape = false;
        let mut chord = false;
        let mut armed = true;
        let mut last = Instant::now();
        let mut selected = TALK_KEY.load(Ordering::SeqCst) as u16;
        loop {
            let down = |vk: VIRTUAL_KEY| unsafe { GetAsyncKeyState(vk.0 as i32) } < 0;
            let configured = TALK_KEY.load(Ordering::SeqCst) as u16;
            if selected != configured {
                selected = configured;
                HELD.store(false, Ordering::SeqCst);
                let _ = audio::request("cancel");
                if tx.send((3, 0, 0)).is_err() {
                    break;
                }
                held = false;
                chord = false;
                armed = false;
            }
            let next = down(VIRTUAL_KEY(selected));
            let now = Instant::now();
            let interrupted = !interactive() || now.duration_since(last) > Duration::from_secs(1);
            last = now;
            if interrupted {
                HELD.store(false, Ordering::SeqCst);
                EPOCH.fetch_add(1, Ordering::SeqCst);
                let _ = audio::request("cancel");
                if tx.send((3, 0, 0)).is_err() {
                    break;
                }
                held = false;
                escape = false;
                chord = false;
                armed = false;
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            if !armed {
                if !next {
                    armed = true;
                }
                std::thread::sleep(Duration::from_millis(8));
                continue;
            }
            let esc = down(VK_ESCAPE);
            let other = next
                && (8..=254).any(|key| {
                    key != selected as i32
                        && !(selected == VK_RCONTROL.0 && key == VK_CONTROL.0 as i32)
                        && !(selected == VK_RMENU.0 && key == VK_MENU.0 as i32)
                        && unsafe { GetAsyncKeyState(key) } < 0
                });
            HELD.store(next && !other, Ordering::SeqCst);
            if held && (!next || other) {
                let _ = audio::request("endCapture");
            }
            if (next != held || other != chord) && tx.send((0, next as i32, other as i32)).is_err()
            {
                break;
            }
            if esc && !escape && tx.send((1, 1, 0)).is_err() {
                break;
            }
            if other && !chord && tx.send((1, 0, 0)).is_err() {
                break;
            }
            held = next;
            escape = esc;
            chord = other;
            std::thread::sleep(Duration::from_millis(8));
        }
    });
    Ok(rx)
}
