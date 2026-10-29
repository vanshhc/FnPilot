use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes};
use core_graphics::event::{
    CGEventFlags as Flags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType, CallbackResult,
};
use fnpilot_core::Result;
use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
use objc2_foundation::NSString;
use std::{
    cell::RefCell,
    sync::{OnceLock, atomic::Ordering, mpsc},
    time::{Duration, Instant},
};
static LISTENER: OnceLock<()> = OnceLock::new();
static SENDER: OnceLock<mpsc::Sender<(i32, i32, i32)>> = OnceLock::new();
thread_local! {static OBSERVERS:RefCell<Vec<Retained<AnyObject>>>=const{RefCell::new(Vec::new())};}
fn interrupt() {
    super::HELD.store(false, Ordering::SeqCst);
    super::invalidate();
    if let Some(sender) = SENDER.get() {
        let _ = sender.send((3, 0, 0));
    }
    crate::capture::notify("cancel");
}
pub fn events() -> Result<mpsc::Receiver<(i32, i32, i32)>> {
    LISTENER
        .set(())
        .map_err(|_| "The macOS key listener is already attached.")?;
    let (tx, rx) = mpsc::channel();
    let _ = SENDER.set(tx.clone());
    dispatch2::DispatchQueue::main().exec_async(install_observers);
    std::thread::spawn(move || {
        loop {
            // Permission changes do not require creating a second listener/session.
            if !unsafe { super::permissions::CGPreflightListenEventAccess() } {
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
            let result = CGEventTap::new(
                CGEventTapLocation::Session,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::ListenOnly,
                vec![CGEventType::FlagsChanged, CGEventType::KeyDown],
                {
                    let tx = tx.clone();
                    move |_, kind, event| {
                        match kind {
                            CGEventType::FlagsChanged => {
                                let flags = event.get_flags();
                                let held = flags.contains(Flags::CGEventFlagSecondaryFn);
                                let chord = flags.intersects(
                                    Flags::CGEventFlagCommand
                                        | Flags::CGEventFlagControl
                                        | Flags::CGEventFlagAlternate
                                        | Flags::CGEventFlagShift,
                                );
                                super::HELD.store(held && !chord, Ordering::SeqCst);
                                if held && chord {
                                    super::invalidate();
                                }
                                if !held || chord {
                                    crate::capture::notify("endCapture");
                                }
                                let _ = tx.send((0, i32::from(held), i32::from(chord)));
                            }
                            CGEventType::KeyDown => {
                                super::HELD.store(false, Ordering::SeqCst);
                                // kCGKeyboardEventKeycode = 9. Escape must interrupt in-flight native writes.
                                let escape = event.get_integer_value_field(9) == 53;
                                if escape {
                                    super::invalidate();
                                }
                                let _ = tx.send((1, i32::from(escape), 0));
                                crate::capture::notify("endCapture");
                            }
                            CGEventType::TapDisabledByTimeout
                            | CGEventType::TapDisabledByUserInput => interrupt(),
                            _ => {}
                        }
                        CallbackResult::Keep
                    }
                },
            );
            if let Ok(tap) = result {
                let Ok(source) = tap.mach_port().create_runloop_source(0) else {
                    break;
                };
                let loop_ = CFRunLoop::get_current();
                loop_.add_source(&source, unsafe { kCFRunLoopCommonModes });
                tap.enable();
                let mut last = Instant::now();
                let mut was_locked = false;
                loop {
                    CFRunLoop::run_in_mode(
                        unsafe { core_foundation::runloop::kCFRunLoopDefaultMode },
                        Duration::from_millis(100),
                        true,
                    );
                    let locked = super::locked();
                    if last.elapsed() > Duration::from_secs(1) || locked && !was_locked {
                        interrupt();
                    }
                    was_locked = locked;
                    last = Instant::now();
                    tap.enable();
                    if !unsafe { super::permissions::CGPreflightListenEventAccess() } {
                        interrupt();
                        break;
                    }
                }
                loop_.remove_source(&source, unsafe { kCFRunLoopCommonModes });
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    });
    Ok(rx)
}
fn install_observers() {
    unsafe {
        if let Some(app) = super::apps::App::foreground() {
            super::remember(&app);
        }
        let workspace: Retained<AnyObject> = msg_send![class!(NSWorkspace), sharedWorkspace];
        let center: Retained<AnyObject> = msg_send![&*workspace, notificationCenter];
        for name in [
            "NSWorkspaceWillSleepNotification",
            "NSWorkspaceScreensDidSleepNotification",
            "NSWorkspaceSessionDidResignActiveNotification",
        ] {
            observe(&center, name, move |_| {
                super::LOCKED.store(true, Ordering::SeqCst);
                interrupt();
            });
        }
        for name in [
            "NSWorkspaceDidWakeNotification",
            "NSWorkspaceScreensDidWakeNotification",
            "NSWorkspaceSessionDidBecomeActiveNotification",
        ] {
            observe(&center, name, move |_| {
                super::LOCKED.store(false, Ordering::SeqCst);
            });
        }
        observe(
            &center,
            "NSWorkspaceDidActivateApplicationNotification",
            |notification| {
                if notification.is_null() {
                    return;
                }
                let info: Option<Retained<AnyObject>> = msg_send![notification, userInfo];
                if let Some(info) = info {
                    let app: Option<Retained<AnyObject>> = msg_send![&*info,objectForKey:&*NSString::from_str("NSWorkspaceApplicationKey")];
                    if let Some(app) = app {
                        let pid: i32 = msg_send![&*app, processIdentifier];
                        if pid > 0
                            && let Some(app) = super::apps::App::for_pid(pid as u32)
                        {
                            super::remember(&app);
                        }
                    }
                }
            },
        );
        let distributed: Retained<AnyObject> =
            msg_send![class!(NSDistributedNotificationCenter), defaultCenter];
        observe(&distributed, "com.apple.screenIsLocked", |_| {
            super::LOCKED.store(true, Ordering::SeqCst);
            interrupt();
        });
        observe(&distributed, "com.apple.screenIsUnlocked", |_| {
            super::LOCKED.store(false, Ordering::SeqCst);
        });
    }
}
unsafe fn observe(center: &AnyObject, name: &str, callback: impl Fn(*mut AnyObject) + 'static) {
    let block = block2::RcBlock::new(callback);
    let observer: Retained<AnyObject> = unsafe {
        msg_send![center,addObserverForName:&*NSString::from_str(name), object:std::ptr::null::<AnyObject>(), queue:std::ptr::null::<AnyObject>(), usingBlock:&*block]
    };
    OBSERVERS.with(|tokens| tokens.borrow_mut().push(observer));
}
