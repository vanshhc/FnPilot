use super::{State, apps, targets};
use fnpilot_core::{
    Action, Result,
    runner::{Observation, Outcome},
};
use std::time::{Duration, Instant};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        Media::Audio::{
            Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator, MMDeviceEnumerator, eConsole,
            eRender,
        },
        System::Com::{CLSCTX_ALL, CoCreateInstance},
        UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::GUID,
};

pub fn execute(
    state: &mut State,
    action: &Action,
    expected: &Observation,
    deadline: Instant,
    epoch: u64,
) -> Result<Outcome> {
    action.validate()?;
    if !expected.capabilities.contains(&action.kind) {
        return Err("That action is unavailable on Windows.".into());
    }
    let mut hwnd = HWND(expected.target.window_id as *mut _);
    let mut current = targets::target(hwnd)?.0;
    current.generation = epoch;
    if current != expected.target {
        return Err("The target process or window changed.".into());
    }
    let check = |hwnd: HWND| -> Result<()> {
        super::check(deadline, epoch)?;
        if unsafe { GetForegroundWindow() } != hwnd {
            return Err("The intended target is no longer foreground. No input was sent.".into());
        }
        Ok(())
    };
    let kind = action.kind.as_str();
    if ["open_app", "focus_app"].contains(&kind) {
        let name = action.app.as_deref().ok_or("Name the app.")?;
        if kind == "open_app" && apps::resolve(name).is_err() {
            apps::launch(name)?;
        }
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            super::check(deadline, epoch)?;
            if let Ok(window) = apps::resolve(name) {
                hwnd = window;
                unsafe {
                    let _ = SetForegroundWindow(hwnd);
                }
                if unsafe { GetForegroundWindow() } == hwnd {
                    state.target = Some(hwnd);
                    return Ok(Outcome {
                        message: "Target app is foreground".into(),
                        verified: true,
                    });
                }
            }
            if Instant::now() > until {
                return Err("Could not verify foreground activation of the requested app.".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    if let Some(app) = &action.app {
        let named = apps::resolve(app)?;
        if named != hwnd {
            return Err("Focus the named app before controlling it.".into());
        }
    }
    if fnpilot_core::browser::KINDS.contains(&kind) {
        return super::browser::execute(state, action, expected, deadline, epoch);
    }
    if kind.contains("volume") || ["mute", "unmute"].contains(&kind) {
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .map_err(|_| "Core Audio is unavailable.")?;
            let endpoint = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|_| "No supported audio output.")?;
            let volume: IAudioEndpointVolume = endpoint
                .Activate(CLSCTX_ALL, None)
                .map_err(|_| "Output volume is unsupported.")?;
            let before = volume
                .GetMasterVolumeLevelScalar()
                .map_err(|_| "Could not read output volume.")?;
            super::check(deadline, epoch)?;
            if ["mute", "unmute"].contains(&kind) {
                let muted = kind == "mute";
                volume
                    .SetMute(muted, &GUID::zeroed())
                    .map_err(|_| "Could not set mute.")?;
                return Ok(Outcome {
                    message: "Mute updated".into(),
                    verified: volume
                        .GetMute()
                        .map_err(|_| "Could not verify mute.")?
                        .as_bool()
                        == muted,
                });
            }
            let desired = match kind {
                "volume_up" => (before + 0.0625).min(1.0),
                "volume_down" => (before - 0.0625).max(0.0),
                _ => action.value.unwrap_or(0.0) as f32 / 100.0,
            };
            volume
                .SetMasterVolumeLevelScalar(desired, &GUID::zeroed())
                .map_err(|_| "Could not set volume.")?;
            return Ok(Outcome {
                message: "Volume updated".into(),
                verified: (volume
                    .GetMasterVolumeLevelScalar()
                    .map_err(|_| "Could not verify volume.")?
                    - desired)
                    .abs()
                    < 0.01,
            });
        }
    }
    check(hwnd)?;
    match kind {
        "type_text" => {
            state.controls.safe_focus()?;
            for chunk in action
                .text
                .as_deref()
                .unwrap_or_default()
                .encode_utf16()
                .collect::<Vec<_>>()
                .chunks(64)
            {
                check(hwnd)?;
                let events = chunk
                    .iter()
                    .flat_map(|unit| {
                        [
                            INPUT {
                                r#type: INPUT_KEYBOARD,
                                Anonymous: INPUT_0 {
                                    ki: KEYBDINPUT {
                                        wVk: VIRTUAL_KEY(0),
                                        wScan: *unit,
                                        dwFlags: KEYEVENTF_UNICODE,
                                        ..Default::default()
                                    },
                                },
                            },
                            INPUT {
                                r#type: INPUT_KEYBOARD,
                                Anonymous: INPUT_0 {
                                    ki: KEYBDINPUT {
                                        wVk: VIRTUAL_KEY(0),
                                        wScan: *unit,
                                        dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                                        ..Default::default()
                                    },
                                },
                            },
                        ]
                    })
                    .collect::<Vec<_>>();
                if unsafe { SendInput(&events, std::mem::size_of::<INPUT>() as i32) }
                    != events.len() as u32
                {
                    return Err(
                        "Windows refused input injection. No elevation retry is performed.".into(),
                    );
                }
            }
            Ok(Outcome {
                message: "Literal text sent".into(),
                verified: false,
            })
        }
        "submit_control" => {
            fnpilot_core::runner::validate_context_action(action, &expected.context)?;
            let id = action
                .control_id
                .as_deref()
                .ok_or("Missing field identity.")?;
            let value = expected
                .context
                .controls
                .iter()
                .find(|c| c.id == id)
                .and_then(|c| c.value.as_deref())
                .ok_or("Missing draft.")?;
            let events = [KEYBD_EVENT_FLAGS(0), KEYEVENTF_KEYUP].map(|flags| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_RETURN,
                        dwFlags: flags,
                        ..Default::default()
                    },
                },
            });
            state.controls.validate_submission(id, value)?;
            check(hwnd)?;
            if unsafe { SendInput(&events, std::mem::size_of::<INPUT>() as i32) }
                != events.len() as u32
            {
                return Err(
                    "Windows refused submission input; check the target app before retrying."
                        .into(),
                );
            }
            Ok(Outcome {
                message: "Submission requested; check the target app".into(),
                verified: false,
            })
        }
        "press_control" | "set_control_value" => {
            state.controls.invoke(
                action
                    .control_id
                    .as_deref()
                    .ok_or("Missing control identity.")?,
                action.text.as_deref(),
                deadline,
                epoch,
            )?;
            Ok(Outcome {
                message: "Control operation sent".into(),
                verified: false,
            })
        }
        "close_window" | "quit_app" => {
            if kind == "quit_app" {
                for window in apps::windows_for_process(current.process_id) {
                    let other = targets::target(window)?.0;
                    if other.process_id == current.process_id
                        && other.process_start == current.process_start
                    {
                        check(hwnd)?;
                        unsafe { PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0)) }
                            .map_err(|_| "Graceful close request failed.")?;
                    }
                }
            } else {
                unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) }
                    .map_err(|_| "Window close request failed.")?;
            }
            Ok(Outcome {
                message: "Graceful close requested; save dialogs are preserved".into(),
                verified: false,
            })
        }
        "minimize_window" | "maximize_window" | "restore_window" => unsafe {
            if kind == "maximize_window" {
                let mut placement = WINDOWPLACEMENT {
                    length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                    ..Default::default()
                };
                GetWindowPlacement(hwnd, &mut placement)
                    .map_err(|_| "Could not read original placement.")?;
                state.placements.entry(hwnd.0 as usize).or_insert(placement);
                let _ = ShowWindow(hwnd, SW_MAXIMIZE);
            } else if kind == "minimize_window" {
                let _ = ShowWindow(hwnd, SW_MINIMIZE);
            } else if let Some(placement) = state.placements.remove(&(hwnd.0 as usize)) {
                SetWindowPlacement(hwnd, &placement)
                    .map_err(|_| "Could not restore original placement.")?;
            } else {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            Ok(Outcome {
                message: "Window state sent".into(),
                verified: match kind {
                    "maximize_window" => IsZoomed(hwnd).as_bool(),
                    "minimize_window" => IsIconic(hwnd).as_bool(),
                    _ => !IsIconic(hwnd).as_bool(),
                },
            })
        },
        "scroll" => {
            state.controls.scroll(
                action
                    .direction
                    .as_deref()
                    .ok_or("Missing scroll direction.")?,
                current.process_id,
                deadline,
                epoch,
            )?;
            Ok(Outcome {
                message: "Scroll sent".into(),
                verified: false,
            })
        }
        _ => Err("That Windows capability is unsupported; no substitute action was sent.".into()),
    }
}
