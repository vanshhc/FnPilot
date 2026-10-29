//! Screenshot-guided cursor input. Uses ScreenCaptureKit (macOS 14+), never menu paths.
use super::{apps::App, ax::Element, executor::State};
use base64::{Engine, engine::general_purpose::STANDARD};
use core_foundation::{
    base::{CFType, CFTypeRef, TCFType},
    data::CFData,
    string::{CFString, CFStringRef},
};
use core_graphics::{
    display::CGDisplay,
    event::{CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, CGMouseButton, EventField},
    event_source::{CGEventSource, CGEventSourceStateID},
    geometry::{CGPoint, CGRect},
};
use fnpilot_core::{
    Result,
    computer::{Direction, Key, Screen, Step},
    runner::Target,
};
use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
use serde_json::{Value, json};
use std::{
    ffi::c_void,
    sync::mpsc,
    time::{Duration, Instant},
};

#[link(name = "ScreenCaptureKit", kind = "framework")]
unsafe extern "C" {}
#[link(name = "ImageIO", kind = "framework")]
unsafe extern "C" {
    fn CGImageDestinationCreateWithData(
        data: *mut c_void,
        kind: CFStringRef,
        count: usize,
        options: CFTypeRef,
    ) -> CFTypeRef;
    fn CGImageDestinationAddImage(
        destination: CFTypeRef,
        image: *const c_void,
        properties: CFTypeRef,
    );
    fn CGImageDestinationFinalize(destination: CFTypeRef) -> bool;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFDataCreateMutable(allocator: CFTypeRef, capacity: isize) -> *mut c_void;
}
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGImageGetWidth(image: *const c_void) -> usize;
    fn CGImageGetHeight(image: *const c_void) -> usize;
    pub(super) fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

pub(super) struct Frame {
    screen: Screen,
    png: Vec<u8>,
    bounds: CGRect,
    captured: Instant,
}

pub(super) fn request_permission(deadline: Instant, epoch: u64) -> Result<Value> {
    super::on_main(deadline, epoch, || {
        Ok(json!(unsafe { CGRequestScreenCaptureAccess() }))
    })
}

unsafe fn encode(image: *const c_void) -> Result<Vec<u8>> {
    unsafe {
        let raw = CFDataCreateMutable(std::ptr::null(), 0);
        if raw.is_null() {
            return Err("Could not allocate screenshot data.".into());
        }
        let data = CFType::wrap_under_create_rule(raw)
            .downcast::<CFData>()
            .ok_or("Invalid screenshot data.")?;
        let raw_destination = CGImageDestinationCreateWithData(
            raw,
            CFString::new("public.png").as_concrete_TypeRef(),
            1,
            std::ptr::null(),
        );
        if raw_destination.is_null() {
            return Err("Could not encode screenshot.".into());
        }
        let destination = CFType::wrap_under_create_rule(raw_destination);
        CGImageDestinationAddImage(destination.as_CFTypeRef(), image, std::ptr::null());
        if !CGImageDestinationFinalize(destination.as_CFTypeRef()) || data.len() > 8_000_000 {
            return Err("Screenshot encoding failed or exceeded the size limit.".into());
        }
        Ok(data.bytes().to_vec())
    }
}

fn capture(bounds: CGRect, deadline: Instant, epoch: u64) -> Result<(Vec<u8>, u32, u32)> {
    super::check(deadline, epoch)?;
    if !unsafe { CGPreflightScreenCaptureAccess() } {
        return Err(
            "Enable Screen Recording for FnPilot in System Settings, then retry computer use."
                .into(),
        );
    }
    let display_id = CGDisplay::active_displays()
        .map_err(|_| "Could not inspect displays.")?
        .into_iter()
        .find(|id| {
            let rect = CGDisplay::new(*id).bounds();
            rect.origin.x == bounds.origin.x
                && rect.origin.y == bounds.origin.y
                && rect.size.width == bounds.size.width
                && rect.size.height == bounds.size.height
        })
        .ok_or("The target display changed.")?;
    let scale = (1280.0 / bounds.size.width.max(bounds.size.height)).min(1.0);
    let width = (bounds.size.width * scale).round().max(1.0) as usize;
    let height = (bounds.size.height * scale).round().max(1.0) as usize;
    let (tx, rx) = mpsc::channel();
    let callback = block2::RcBlock::new(
        move |content: *mut AnyObject, error: *mut AnyObject| unsafe {
            let result: Result<()> = (|| {
                super::check(deadline, epoch)?;
                if content.is_null() || !error.is_null() {
                    return Err("ScreenCaptureKit could not capture the target display.".into());
                }
                let displays: Retained<AnyObject> = msg_send![content, displays];
                let count: usize = msg_send![&*displays, count];
                let mut display = None;
                for index in 0..count.min(64) {
                    let item: Retained<AnyObject> = msg_send![&*displays, objectAtIndex:index];
                    let id: u32 = msg_send![&*item, displayID];
                    if id == display_id {
                        display = Some(item);
                        break;
                    }
                }
                let display = display.ok_or("The target display is no longer shareable.")?;
                let empty: Retained<AnyObject> = msg_send![class!(NSArray), array];
                let allocated: objc2::rc::Allocated<AnyObject> =
                    msg_send![class!(SCContentFilter), alloc];
                let filter: Retained<AnyObject> =
                    msg_send![allocated, initWithDisplay:&*display, excludingWindows:&*empty];
                let supported: bool =
                    msg_send![&*filter, respondsToSelector:objc2::sel!(setIncludeMenuBar:)];
                if supported {
                    let _: () = msg_send![&*filter, setIncludeMenuBar:true];
                }
                let config: Retained<AnyObject> = msg_send![class!(SCStreamConfiguration), new];
                let _: () = msg_send![&*config, setWidth:width];
                let _: () = msg_send![&*config, setHeight:height];
                let _: () = msg_send![&*config, setShowsCursor:false];
                let output = tx.clone();
                let image_callback =
                    block2::RcBlock::new(move |image: *const c_void, error: *mut AnyObject| {
                        let result = (|| {
                            super::check(deadline, epoch)?;
                            if image.is_null() || !error.is_null() {
                                return Err("ScreenCaptureKit did not return an image.".into());
                            }
                            let (w, h) = (CGImageGetWidth(image), CGImageGetHeight(image));
                            if w != width || h != height {
                                return Err("The screenshot dimensions changed.".into());
                            }
                            Ok((encode(image)?, w as u32, h as u32))
                        })();
                        let _ = output.send(result);
                    });
                let _: () = msg_send![class!(SCScreenshotManager), captureImageWithFilter:&*filter, configuration:&*config, completionHandler:&*image_callback];
                Ok(())
            })();
            if let Err(error) = result {
                let _ = tx.send(Err(error));
            }
        },
    );
    unsafe {
        let _: () = msg_send![class!(SCShareableContent), getShareableContentExcludingDesktopWindows:true, onScreenWindowsOnly:true, completionHandler:&*callback];
    }
    super::wait(rx, deadline, epoch)
}

fn source() -> Result<CGEventSource> {
    CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "Could not create cursor input source.".into())
}
fn foreground(target: &Target, deadline: Instant, epoch: u64) -> Result<App> {
    super::check(deadline, epoch)?;
    let app = App::foreground()
        .filter(|app| {
            app.pid() == target.process_id
                && app.start() == target.process_start
                && app.id() == target.app_identity
        })
        .ok_or("The target app is no longer foreground. Computer use stopped.")?;
    let root = Element::application(app.pid());
    let window = root
        .element("AXFocusedWindow")
        .or_else(|| root.elements("AXWindows").first().cloned());
    if window.map_or(0, |window| window.hash()) != target.window_id {
        return Err("The focused window changed. Computer use stopped.".into());
    }
    Ok(app)
}
fn pause(deadline: Instant, epoch: u64, milliseconds: u32) -> Result<()> {
    for _ in 0..milliseconds.div_ceil(10) {
        super::check(deadline, epoch)?;
        std::thread::sleep(Duration::from_millis(10));
    }
    super::check(deadline, epoch)
}
fn hit(point: CGPoint, target: &Target) -> Result<()> {
    let node = Element::at_position(point.x, point.y)
        .ok_or("Cannot verify the app under the cursor. Input stopped.")?;
    if node.pid() != Some(target.process_id) || node.string("AXSubrole") == "AXSecureTextField" {
        return Err("The cursor target is outside the app or is protected. Input stopped.".into());
    }
    Ok(())
}

impl State {
    fn computer_target(&mut self, deadline: Instant, epoch: u64) -> Result<(App, Target)> {
        super::check(deadline, epoch)?;
        if !super::ax::trusted() {
            return Err("Grant Accessibility access to FnPilot in System Settings.".into());
        }
        let app = Self::current()?;
        let target = Target {
            platform: "macos".into(),
            app_identity: app.id(),
            process_id: app.pid(),
            process_start: app.start(),
            window_id: self.window(&app, false).map_or(0, |window| window.hash()),
            generation: epoch,
        };
        Ok((app, target))
    }
    pub(super) fn computer_prepare(
        &mut self,
        value: &Value,
        deadline: Instant,
        epoch: u64,
    ) -> Result<Value> {
        let target: Target = serde_json::from_value(value["target"].clone())
            .map_err(|_| "Invalid computer-use target.")?;
        if self.computer_target(deadline, epoch)?.1 != target {
            return Err("The computer-use target changed before starting.".into());
        }
        if App::foreground().is_some_and(|app| app.pid() == std::process::id()) {
            let app = App::for_identity(target.process_id, target.process_start)
                .ok_or("The target app exited.")?;
            super::executor::activate(&app, deadline, epoch)?;
        }
        foreground(&target, deadline, epoch)?;
        Ok(json!(true))
    }
    pub(super) fn computer_capture(&mut self, deadline: Instant, epoch: u64) -> Result<Screen> {
        // A frame ID is a single-use lease owned by this worker, not a model-supplied authority.
        self.computer_frame = None;
        let (app, target) = self.computer_target(deadline, epoch)?;
        foreground(&target, deadline, epoch)?;
        let window = self
            .window(&app, false)
            .and_then(|window| window.frame())
            .ok_or("Show the target app's window before using computer use.")?;
        let center = CGPoint::new(
            window.origin.x + window.size.width / 2.0,
            window.origin.y + window.size.height / 2.0,
        );
        let bounds = CGDisplay::active_displays()
            .map_err(|_| "Could not inspect displays.")?
            .into_iter()
            .map(|id| CGDisplay::new(id).bounds())
            .find(|rect| {
                center.x >= rect.origin.x
                    && center.x < rect.origin.x + rect.size.width
                    && center.y >= rect.origin.y
                    && center.y < rect.origin.y + rect.size.height
            })
            .ok_or("The target window is not on an active display.")?;
        let (png, width, height) = capture(bounds, deadline, epoch)?;
        foreground(&target, deadline, epoch)?;
        if self.window(&app, false).map_or(0, |w| w.hash()) != target.window_id {
            return Err("The target window changed during capture.".into());
        }
        self.computer_sequence = self
            .computer_sequence
            .checked_add(1)
            .ok_or("Screenshot sequence exhausted.")?;
        let screen = Screen {
            id: self.computer_sequence,
            target,
            app_name: app.name(),
            width,
            height,
            image: STANDARD.encode(&png),
        };
        self.computer_frame = Some(Frame {
            screen: screen.clone(),
            png,
            bounds,
            captured: Instant::now(),
        });
        Ok(screen)
    }

    pub(super) fn computer_act(
        &mut self,
        value: &Value,
        deadline: Instant,
        epoch: u64,
    ) -> Result<Value> {
        let frame = self
            .computer_frame
            .take()
            .ok_or("The screenshot expired. Capture again before acting.")?;
        let target: Target = serde_json::from_value(value["target"].clone())
            .map_err(|_| "Invalid computer-use target.")?;
        if value["frameID"].as_u64() != Some(frame.screen.id)
            || target != frame.screen.target
            || frame.captured.elapsed() > Duration::from_secs(75)
        {
            return Err("The screenshot expired or the target changed. Input stopped.".into());
        }
        let step: Step =
            serde_json::from_value(value["step"].clone()).map_err(|_| "Invalid cursor action.")?;
        step.validate(&frame.screen)?;
        let app = foreground(&target, deadline, epoch)?;
        if self.window(&app, false).map_or(0, |w| w.hash()) != target.window_id {
            return Err("The focused window changed. Input stopped.".into());
        }
        if let Step::Wait { milliseconds } = step {
            pause(deadline, epoch, milliseconds)?;
            return Ok(json!(true));
        }
        if step.terminal() {
            return Err("Terminal model responses cannot dispatch input.".into());
        }
        let (fresh, _, _) = capture(frame.bounds, deadline, epoch)?;
        if !crate::screen::unchanged(&frame.png, &fresh, step.point())? {
            return Err(
                "The screen changed while the model was deciding. Repeat the instruction.".into(),
            );
        }
        foreground(&target, deadline, epoch)?;
        if let Some((x, y)) = step.point() {
            let (desktop_x, desktop_y) = crate::screen::desktop_point(
                (x, y),
                (frame.screen.width, frame.screen.height),
                (
                    frame.bounds.origin.x,
                    frame.bounds.origin.y,
                    frame.bounds.size.width,
                    frame.bounds.size.height,
                ),
            )?;
            let point = CGPoint::new(desktop_x, desktop_y);
            hit(point, &target)?;
            let event = CGEvent::new_mouse_event(
                source()?,
                CGEventType::MouseMoved,
                point,
                CGMouseButton::Left,
            )
            .map_err(|_| "Could not move the cursor.")?;
            foreground(&target, deadline, epoch)?;
            event.post(CGEventTapLocation::HID);
            pause(deadline, epoch, 120)?;
            foreground(&target, deadline, epoch)?;
            hit(point, &target)?;
            match step {
                Step::Click { .. } | Step::DoubleClick { .. } | Step::RightClick { .. } => {
                    let right = matches!(step, Step::RightClick { .. });
                    let button = if right {
                        CGMouseButton::Right
                    } else {
                        CGMouseButton::Left
                    };
                    for count in 1..=if matches!(step, Step::DoubleClick { .. }) {
                        2
                    } else {
                        1
                    } {
                        let down = CGEvent::new_mouse_event(
                            source()?,
                            if right {
                                CGEventType::RightMouseDown
                            } else {
                                CGEventType::LeftMouseDown
                            },
                            point,
                            button,
                        )
                        .map_err(|_| "Could not create click.")?;
                        let up = CGEvent::new_mouse_event(
                            source()?,
                            if right {
                                CGEventType::RightMouseUp
                            } else {
                                CGEventType::LeftMouseUp
                            },
                            point,
                            button,
                        )
                        .map_err(|_| "Could not release click.")?;
                        down.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, count);
                        up.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, count);
                        foreground(&target, deadline, epoch)?;
                        hit(point, &target)?;
                        down.post(CGEventTapLocation::HID);
                        up.post(CGEventTapLocation::HID);
                    }
                }
                Step::Scroll { direction, .. } => {
                    let (vertical, horizontal) = match direction {
                        Direction::Up => (5, 0),
                        Direction::Down => (-5, 0),
                        Direction::Left => (0, 5),
                        Direction::Right => (0, -5),
                    };
                    let event = CGEvent::new_scroll_event(source()?, 1, 2, vertical, horizontal, 0)
                        .map_err(|_| "Could not scroll.")?;
                    event.set_location(point);
                    foreground(&target, deadline, epoch)?;
                    event.post(CGEventTapLocation::HID);
                }
                _ => {}
            }
        } else {
            let field = Element::application(app.pid())
                .element("AXFocusedUIElement")
                .ok_or("Cannot inspect focused input.")?;
            if field.string("AXSubrole") == "AXSecureTextField" {
                return Err("Password input is disabled.".into());
            }
            match step {
                Step::Type { text } => {
                    self.type_text(&app, &text, deadline, epoch)?;
                }
                Step::Key { key } => {
                    let (code, primary, shift) = match key {
                        Key::Return => (36, false, false),
                        Key::Tab => (48, false, false),
                        Key::Backspace => (51, false, false),
                        Key::Delete => (117, false, false),
                        Key::Space => (49, false, false),
                        Key::ArrowUp => (126, false, false),
                        Key::ArrowDown => (125, false, false),
                        Key::ArrowLeft => (123, false, false),
                        Key::ArrowRight => (124, false, false),
                        Key::SelectAll => (0, true, false),
                        Key::Copy => (8, true, false),
                        Key::Paste => (9, true, false),
                        Key::Undo => (6, true, false),
                        Key::Redo => (6, true, true),
                        Key::New => (45, true, false),
                        Key::Save => (1, true, false),
                        Key::Find => (3, true, false),
                    };
                    let down = CGEvent::new_keyboard_event(source()?, code, true)
                        .map_err(|_| "Could not create key input.")?;
                    let up = CGEvent::new_keyboard_event(source()?, code, false)
                        .map_err(|_| "Could not release key input.")?;
                    let mut flags = CGEventFlags::empty();
                    if primary {
                        flags |= CGEventFlags::CGEventFlagCommand;
                    }
                    if shift {
                        flags |= CGEventFlags::CGEventFlagShift;
                    }
                    down.set_flags(flags);
                    up.set_flags(flags);
                    foreground(&target, deadline, epoch)?;
                    down.post(CGEventTapLocation::HID);
                    up.post(CGEventTapLocation::HID);
                }
                _ => return Err("Unsupported computer-use input.".into()),
            }
        }
        pause(deadline, epoch, 350)?;
        Ok(json!(true))
    }
}
