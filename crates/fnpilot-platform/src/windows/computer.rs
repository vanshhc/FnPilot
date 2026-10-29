use super::{State, targets};
use base64::{Engine, engine::general_purpose::STANDARD};
use fnpilot_core::{
    Action, Result,
    computer::{Direction, Key, Screen, Step},
    runner::Target,
};
use serde_json::{Value, json};
use std::{
    ffi::c_void,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::*,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

#[link(name = "user32")]
unsafe extern "system" {
    fn SetThreadDpiAwarenessContext(context: *mut c_void) -> *mut c_void;
}
pub(super) fn initialize() -> Result<()> {
    // Keep screenshots and input in physical pixels, including mixed-DPI monitors.
    if unsafe { SetThreadDpiAwarenessContext(-4isize as *mut c_void) }.is_null() {
        return Err("Could not establish physical screenshot coordinates.".into());
    }
    Ok(())
}

pub(super) struct Frame {
    screen: Screen,
    png: Vec<u8>,
    bounds: RECT,
    captured: Instant,
}
struct Surface {
    desktop: HDC,
    memory: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
}
impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            if !self.previous.is_invalid() {
                SelectObject(self.memory, self.previous);
            }
            if !self.bitmap.is_invalid() {
                let _ = DeleteObject(self.bitmap.into());
            }
            if !self.memory.is_invalid() {
                let _ = DeleteDC(self.memory);
            }
            if !self.desktop.is_invalid() {
                ReleaseDC(None, self.desktop);
            }
        }
    }
}
fn capture(bounds: RECT) -> Result<(Vec<u8>, u32, u32)> {
    let (width, height) = (bounds.right - bounds.left, bounds.bottom - bounds.top);
    if width <= 0 || height <= 0 {
        return Err("Invalid target monitor.".into());
    }
    let scale = (1280.0 / width.max(height) as f64).min(1.0);
    let (w, h) = (
        (width as f64 * scale).round() as i32,
        (height as f64 * scale).round() as i32,
    );
    unsafe {
        let mut surface = Surface {
            desktop: GetDC(None),
            memory: HDC::default(),
            bitmap: HBITMAP::default(),
            previous: HGDIOBJ::default(),
        };
        if surface.desktop.is_invalid() {
            return Err("Could not capture the display.".into());
        }
        surface.memory = CreateCompatibleDC(Some(surface.desktop));
        if surface.memory.is_invalid() {
            return Err("Could not allocate capture context.".into());
        }
        surface.bitmap = CreateCompatibleBitmap(surface.desktop, w, h);
        if surface.bitmap.is_invalid() {
            return Err("Could not allocate capture bitmap.".into());
        }
        surface.previous = SelectObject(surface.memory, surface.bitmap.into());
        if surface.previous.is_invalid() {
            return Err("Could not select capture bitmap.".into());
        }
        if !StretchBlt(
            surface.memory,
            0,
            0,
            w,
            h,
            Some(surface.desktop),
            bounds.left,
            bounds.top,
            width,
            height,
            SRCCOPY | CAPTUREBLT,
        )
        .as_bool()
        {
            return Err("The display capture failed.".into());
        }
        SelectObject(surface.memory, surface.previous);
        surface.previous = HGDIOBJ::default();
        let mut data = vec![0u8; (w * h * 4) as usize];
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        if GetDIBits(
            surface.desktop,
            surface.bitmap,
            0,
            h as u32,
            Some(data.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        ) != h
        {
            return Err("Could not read screenshot pixels.".into());
        }
        for pixel in data.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        let mut png = vec![];
        {
            let mut encoder = png::Encoder::new(&mut png, w as u32, h as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .map_err(|_| "Could not encode screenshot.")?
                .write_image_data(&data)
                .map_err(|_| "Could not encode screenshot pixels.")?;
        }
        Ok((png, w as u32, h as u32))
    }
}
fn foreground(target: &Target, deadline: Instant, epoch: u64) -> Result<HWND> {
    super::check(deadline, epoch)?;
    let hwnd = unsafe { GetForegroundWindow() };
    let mut fresh = targets::target(hwnd)?.0;
    fresh.generation = epoch;
    if &fresh != target {
        return Err("The target app or window changed. Computer use stopped.".into());
    }
    Ok(hwnd)
}
fn pause(deadline: Instant, epoch: u64, milliseconds: u32) -> Result<()> {
    for _ in 0..milliseconds.div_ceil(10) {
        super::check(deadline, epoch)?;
        std::thread::sleep(Duration::from_millis(10));
    }
    super::check(deadline, epoch)
}
fn send(events: &[INPUT]) -> Result<()> {
    if unsafe { SendInput(events, std::mem::size_of::<INPUT>() as i32) } != events.len() as u32 {
        return Err("Windows refused input. Check the app before retrying.".into());
    }
    Ok(())
}
fn mouse(flags: MOUSE_EVENT_FLAGS, data: u32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flags,
                mouseData: data,
                ..Default::default()
            },
        },
    }
}
fn keyboard(key: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                ..Default::default()
            },
        },
    }
}

impl State {
    pub(super) fn computer_prepare(
        &mut self,
        value: &Value,
        deadline: Instant,
        epoch: u64,
    ) -> Result<Value> {
        let expected: Target = serde_json::from_value(value["target"].clone())
            .map_err(|_| "Invalid computer-use target.")?;
        if self.observe(deadline)?.target != expected {
            return Err("The target changed before computer use.".into());
        }
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid));
        }
        if pid == std::process::id() {
            unsafe {
                SetForegroundWindow(HWND(expected.window_id as *mut _))
                    .ok()
                    .map_err(|_| "Focus the intended app and retry.")?;
            }
        }
        foreground(&expected, deadline, epoch)?;
        Ok(json!(true))
    }
    pub(super) fn computer_capture(&mut self, deadline: Instant, epoch: u64) -> Result<Screen> {
        initialize()?;
        self.computer_frame = None;
        let hwnd = unsafe { GetForegroundWindow() };
        let (mut target, app_name) = targets::target(hwnd)?;
        target.generation = epoch;
        foreground(&target, deadline, epoch)?;
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !unsafe { GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut info) }
            .as_bool()
        {
            return Err("Cannot inspect target monitor.".into());
        }
        let (png, width, height) = capture(info.rcMonitor)?;
        foreground(&target, deadline, epoch)?;
        self.computer_sequence = self
            .computer_sequence
            .checked_add(1)
            .ok_or("Screenshot sequence exhausted.")?;
        let screen = Screen {
            id: self.computer_sequence,
            target,
            app_name,
            width,
            height,
            image: STANDARD.encode(&png),
        };
        self.computer_frame = Some(Frame {
            screen: screen.clone(),
            png,
            bounds: info.rcMonitor,
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
            .ok_or("Screenshot expired. Capture again before acting.")?;
        let target: Target = serde_json::from_value(value["target"].clone())
            .map_err(|_| "Invalid computer-use target.")?;
        if value["frameID"].as_u64() != Some(frame.screen.id)
            || target != frame.screen.target
            || frame.captured.elapsed() > Duration::from_secs(75)
        {
            return Err("The screenshot expired or its target changed.".into());
        }
        let step: Step = serde_json::from_value(value["step"].clone())
            .map_err(|_| "Invalid computer-use input.")?;
        step.validate(&frame.screen)?;
        foreground(&target, deadline, epoch)?;
        if let Step::Wait { milliseconds } = step {
            pause(deadline, epoch, milliseconds)?;
            return Ok(json!(true));
        }
        if step.terminal() {
            return Err("Terminal model responses cannot dispatch input.".into());
        }
        let (fresh, _, _) = capture(frame.bounds)?;
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
                    frame.bounds.left as f64,
                    frame.bounds.top as f64,
                    (frame.bounds.right - frame.bounds.left) as f64,
                    (frame.bounds.bottom - frame.bounds.top) as f64,
                ),
            )?;
            let point = POINT {
                x: desktop_x.round() as i32,
                y: desktop_y.round() as i32,
            };
            self.controls.validate_cursor(point, target.process_id)?;
            unsafe {
                SetCursorPos(point.x, point.y).map_err(|_| "Could not move the cursor.")?;
            }
            pause(deadline, epoch, 120)?;
            foreground(&target, deadline, epoch)?;
            self.controls.validate_cursor(point, target.process_id)?;
            match step {
                Step::Click { .. } | Step::DoubleClick { .. } | Step::RightClick { .. } => {
                    let right = matches!(step, Step::RightClick { .. });
                    for _ in 0..if matches!(step, Step::DoubleClick { .. }) {
                        2
                    } else {
                        1
                    } {
                        foreground(&target, deadline, epoch)?;
                        send(&[
                            mouse(
                                if right {
                                    MOUSEEVENTF_RIGHTDOWN
                                } else {
                                    MOUSEEVENTF_LEFTDOWN
                                },
                                0,
                            ),
                            mouse(
                                if right {
                                    MOUSEEVENTF_RIGHTUP
                                } else {
                                    MOUSEEVENTF_LEFTUP
                                },
                                0,
                            ),
                        ])?;
                    }
                }
                Step::Scroll { direction, .. } => {
                    let (flags, delta) = match direction {
                        Direction::Up => (MOUSEEVENTF_WHEEL, 360i32),
                        Direction::Down => (MOUSEEVENTF_WHEEL, -360),
                        Direction::Left => (MOUSEEVENTF_HWHEEL, -360),
                        Direction::Right => (MOUSEEVENTF_HWHEEL, 360),
                    };
                    send(&[mouse(flags, delta as u32)])?;
                }
                _ => {}
            }
        } else {
            self.controls.safe_focus()?;
            match step {
                Step::Type { text } => {
                    let observation = self.observe(deadline)?;
                    if observation.target != target {
                        return Err(
                            "The focused window changed before typing. Input stopped.".into()
                        );
                    }
                    let mut action = Action::new("type_text");
                    action.text = Some(text);
                    super::executor::execute(self, &action, &observation, deadline, epoch)?;
                }
                Step::Key { key } => {
                    let (key, primary, shift) = match key {
                        Key::Return => (VK_RETURN, false, false),
                        Key::Tab => (VK_TAB, false, false),
                        Key::Backspace => (VK_BACK, false, false),
                        Key::Delete => (VK_DELETE, false, false),
                        Key::Space => (VK_SPACE, false, false),
                        Key::ArrowUp => (VK_UP, false, false),
                        Key::ArrowDown => (VK_DOWN, false, false),
                        Key::ArrowLeft => (VK_LEFT, false, false),
                        Key::ArrowRight => (VK_RIGHT, false, false),
                        Key::SelectAll => (VK_A, true, false),
                        Key::Copy => (VK_C, true, false),
                        Key::Paste => (VK_V, true, false),
                        Key::Undo => (VK_Z, true, false),
                        Key::Redo => (VK_Z, true, true),
                        Key::New => (VK_N, true, false),
                        Key::Save => (VK_S, true, false),
                        Key::Find => (VK_F, true, false),
                    };
                    let mut events = vec![];
                    if primary {
                        events.push(keyboard(VK_CONTROL, false));
                    }
                    if shift {
                        events.push(keyboard(VK_SHIFT, false));
                    }
                    events.extend([keyboard(key, false), keyboard(key, true)]);
                    if shift {
                        events.push(keyboard(VK_SHIFT, true));
                    }
                    if primary {
                        events.push(keyboard(VK_CONTROL, true));
                    }
                    foreground(&target, deadline, epoch)?;
                    if let Err(error) = send(&events) {
                        let _ = send(&[
                            keyboard(key, true),
                            keyboard(VK_SHIFT, true),
                            keyboard(VK_CONTROL, true),
                        ]);
                        return Err(error);
                    }
                }
                _ => return Err("Unsupported computer-use input.".into()),
            }
        }
        pause(deadline, epoch, 350)?;
        Ok(json!(true))
    }
}
