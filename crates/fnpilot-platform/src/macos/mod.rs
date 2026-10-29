//! macOS integration is Rust. Swift is confined to the WhisperKit decoder ABI.
mod apps;
mod ax;
mod browser;
mod computer;
mod executor;
mod keys;
mod overlay;
mod permissions;
mod speech;
mod system;

use fnpilot_core::Result;
use serde_json::{Value, json};
use std::{
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
pub(super) static EPOCH: AtomicU64 = AtomicU64::new(1);
static HELD: AtomicBool = AtomicBool::new(false);
static LOCKED: AtomicBool = AtomicBool::new(false);
// Remember process birth as well as PID: Settings must never revive a recycled PID.
static LAST_EXTERNAL: Mutex<Option<(u32, u64)>> = Mutex::new(None);
static LAST_SCAN: Mutex<Option<Value>> = Mutex::new(None);
fn remember(app: &apps::App) {
    if app.pid() != std::process::id() && app.start() != 0 && !app.id().is_empty() {
        *LAST_EXTERNAL.lock().unwrap() = Some((app.pid(), app.start()));
    }
}
pub fn current_epoch() -> u64 {
    EPOCH.load(Ordering::SeqCst)
}
pub fn invalidate() -> u64 {
    EPOCH.fetch_add(1, Ordering::SeqCst) + 1
}
pub(crate) fn held() -> bool {
    HELD.load(Ordering::SeqCst)
}
fn expire(epoch: u64) {
    let _ = EPOCH.compare_exchange(
        epoch,
        epoch.wrapping_add(1),
        Ordering::SeqCst,
        Ordering::SeqCst,
    );
}
#[unsafe(no_mangle)]
pub extern "C" fn fnpilot_speech_is_current(epoch: u64) -> bool {
    current_epoch() == epoch
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGSessionCopyCurrentDictionary() -> core_foundation::base::CFTypeRef;
}
#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn IsSecureEventInputEnabled() -> bool;
}
fn locked() -> bool {
    use core_foundation::{
        base::{CFType, TCFType},
        boolean::CFBoolean,
        dictionary::CFDictionary,
        string::CFString,
    };
    if LOCKED.load(Ordering::SeqCst) {
        return true;
    }
    unsafe {
        let raw = CGSessionCopyCurrentDictionary();
        if raw.is_null() {
            return true;
        }
        let dict = CFType::wrap_under_create_rule(raw).downcast::<CFDictionary>();
        dict.and_then(|dict| {
            dict.find(CFString::new("CGSSessionScreenIsLocked").as_CFTypeRef())
                .map(|v| CFType::wrap_under_get_rule(*v))
        })
        .and_then(|v| v.downcast::<CFBoolean>())
        .map(bool::from)
        .unwrap_or(false)
    }
}
fn check_lifetime(deadline: Instant, epoch: u64) -> Result<()> {
    if current_epoch() != epoch || Instant::now() > deadline {
        Err("Native request expired or was cancelled.".into())
    } else {
        Ok(())
    }
}
fn check(deadline: Instant, epoch: u64) -> Result<()> {
    check_lifetime(deadline, epoch)?;
    if locked() || unsafe { IsSecureEventInputEnabled() } {
        Err("Protected or locked input is active. Leave the secure prompt and retry.".into())
    } else {
        Ok(())
    }
}
fn wait<T>(rx: mpsc::Receiver<Result<T>>, deadline: Instant, epoch: u64) -> Result<T> {
    loop {
        if current_epoch() != epoch {
            return Err("Native operation was cancelled.".into());
        }
        match rx.recv_timeout(Duration::from_millis(25)) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() < deadline => {}
            Err(_) => {
                expire(epoch);
                return Err("Native operation timed out. Session stopped.".into());
            }
        }
    }
}
fn on_main<T: Send + 'static>(
    deadline: Instant,
    epoch: u64,
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    main_dispatch(deadline, epoch, true, operation)
}
fn main_dispatch<T: Send + 'static>(
    deadline: Instant,
    epoch: u64,
    interactive: bool,
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    let (tx, rx) = mpsc::channel();
    dispatch2::DispatchQueue::main().exec_async(move || {
        let allowed = if interactive {
            check(deadline, epoch)
        } else {
            check_lifetime(deadline, epoch)
        };
        let result = allowed.and_then(|_| objc2::rc::autoreleasepool(|_| operation()));
        let _ = tx.send(result);
    });
    wait(rx, deadline, epoch)
}
struct Job {
    value: Value,
    deadline: Instant,
    epoch: u64,
    reply: mpsc::Sender<Result<Value>>,
}
static WORKER: OnceLock<mpsc::SyncSender<Job>> = OnceLock::new();
pub fn request(value: Value) -> Result<Value> {
    let operation = value["operation"].as_str().unwrap_or("");
    let epoch = value["cancellationEpoch"]
        .as_u64()
        .unwrap_or_else(current_epoch);
    if epoch != current_epoch() {
        return Err("Stale native operation.".into());
    }
    let deadline = Instant::now()
        + Duration::from_secs(if operation == "requestMicrophone" {
            60
        } else {
            8
        });
    match operation {
        "configureTalkKey" => {
            return if value["key"] == "fn" {
                Ok(json!(true))
            } else {
                Err("macOS uses the physical Fn key.".into())
            };
        }
        "transcribeWhisperKit" | "unloadSpeech" => return speech::request(value),
        "permissions" => return Ok(objc2::rc::autoreleasepool(|_| permissions::report())),
        "legacySettings" => {
            return Ok(objc2::rc::autoreleasepool(|_| {
                permissions::legacy_settings()
            }));
        }
        "requestMicrophone" => return permissions::request_microphone(deadline, epoch),
        "requestAccessibility" => return permissions::request_accessibility(deadline, epoch),
        "requestScreenRecording" => return computer::request_permission(deadline, epoch),
        "autostart" => {
            return permissions::autostart(
                value["enabled"].as_bool().ok_or("Invalid login request.")?,
                deadline,
                epoch,
            );
        }
        "startCapture" => {
            check(deadline, epoch)?;
            if !permissions::microphone_authorized() {
                return Err("Request microphone access in FnPilot Settings first.".into());
            }
            return crate::capture::request(operation);
        }
        "stopCapture" => return crate::capture::request(operation),
        "cancel" => {
            let _ = crate::capture::request("cancel");
            overlay::show(None, deadline, epoch)?;
            return Ok(json!(true));
        }
        "overlay" => {
            overlay::show(
                Some(value["message"].as_str().unwrap_or("Working").into()),
                deadline,
                epoch,
            )?;
            return Ok(json!(true));
        }
        "observe" | "execute" | "computerCapture" | "computerAct" | "computerPrepare"
        | "computerRelease" => {}
        _ => return Err("Unknown macOS operation.".into()),
    }
    let sender = WORKER.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel::<Job>(8);
        std::thread::spawn(move || {
            let mut state = executor::State::default();
            for job in rx {
                let result = objc2::rc::autoreleasepool(|_| {
                    if job.value["operation"] == "computerRelease" {
                        state.computer_frame = None;
                        return Ok(json!(true));
                    }
                    check(job.deadline, job.epoch)?;
                    match job.value["operation"].as_str() {
                        Some("computerPrepare") => {
                            state.computer_prepare(&job.value, job.deadline, job.epoch)
                        }
                        Some("computerCapture") => {
                            serde_json::to_value(state.computer_capture(job.deadline, job.epoch)?)
                                .map_err(|_| "Screenshot encoding failed.".into())
                        }
                        Some("computerAct") => {
                            state.computer_act(&job.value, job.deadline, job.epoch)
                        }
                        Some("observe") => {
                            serde_json::to_value(state.observe(job.deadline, job.epoch)?)
                                .map_err(|_| "Observation encoding failed.".into())
                        }
                        Some("execute") => {
                            let action = serde_json::from_value(job.value["action"].clone())
                                .map_err(|_| "Invalid native action.")?;
                            let expected = serde_json::from_value(job.value["expected"].clone())
                                .map_err(|_| "Invalid native target.")?;
                            serde_json::to_value(state.execute(
                                action,
                                expected,
                                job.deadline,
                                job.epoch,
                            )?)
                            .map_err(|_| "Outcome encoding failed.".into())
                        }
                        _ => Err("Unknown serialized macOS operation.".into()),
                    }
                });
                let _ = job.reply.send(result);
            }
        });
        tx
    });
    let (reply, rx) = mpsc::channel();
    sender
        .try_send(Job {
            value,
            deadline,
            epoch,
            reply,
        })
        .map_err(|_| "The native worker is busy; retry shortly.")?;
    wait(rx, deadline, epoch)
}
pub fn key_events() -> Result<mpsc::Receiver<(i32, i32, i32)>> {
    keys::events()
}

fn work_area(
    window: Option<core_graphics::geometry::CGRect>,
    deadline: Instant,
    epoch: u64,
) -> Result<core_graphics::geometry::CGRect> {
    on_main(deadline, epoch, move || unsafe {
        use core_graphics::{
            display::CGDisplay,
            geometry::{CGPoint, CGRect, CGSize},
        };
        use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
        use objc2_foundation::NSRect;
        let screens: Retained<AnyObject> = msg_send![class!(NSScreen), screens];
        let count: usize = msg_send![&*screens, count];
        let mut chosen: Option<Retained<AnyObject>> = msg_send![class!(NSScreen), mainScreen];
        for i in 0..count.min(32) {
            let screen: Retained<AnyObject> = msg_send![&*screens,objectAtIndex:i];
            let frame: NSRect = msg_send![&*screen, frame];
            let main_height = CGDisplay::main().bounds().size.height;
            let bounds = CGRect::new(
                &CGPoint::new(
                    frame.origin.x,
                    main_height - frame.origin.y - frame.size.height,
                ),
                &CGSize::new(frame.size.width, frame.size.height),
            );
            if window.is_some_and(|w| w.is_intersects(&bounds)) {
                chosen = Some(screen);
                break;
            }
        }
        let screen = chosen.ok_or("No display is available.")?;
        let visible: NSRect = msg_send![&*screen, visibleFrame];
        Ok(CGRect::new(
            &CGPoint::new(
                visible.origin.x,
                CGDisplay::main().bounds().size.height - visible.origin.y - visible.size.height,
            ),
            &CGSize::new(visible.size.width, visible.size.height),
        ))
    })
}
