use super::{State, targets};
use fnpilot_core::{
    Action, Result, browser,
    runner::{Observation, Outcome},
};
use std::time::{Duration, Instant};
use windows::{
    Win32::{
        Foundation::HWND,
        UI::{
            Accessibility::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::GetForegroundWindow,
        },
    },
    core::BSTR,
};

fn check(expected: &Observation, deadline: Instant, epoch: u64) -> Result<()> {
    super::check(deadline, epoch)?;
    let window = unsafe { GetForegroundWindow() };
    if window != HWND(expected.target.window_id as *mut _) {
        return Err("The browser window changed. Repeat your instruction.".into());
    }
    let mut target = targets::target(window)?.0;
    target.generation = epoch;
    if target != expected.target {
        return Err("The browser process changed.".into());
    }
    Ok(())
}

fn input(key: VIRTUAL_KEY, release: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                dwFlags: if release {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                ..Default::default()
            },
        },
    }
}

fn shortcut(
    expected: &Observation,
    modifiers: &[VIRTUAL_KEY],
    key: VIRTUAL_KEY,
    deadline: Instant,
    epoch: u64,
) -> Result<()> {
    if [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
        .iter()
        .any(|key| unsafe { GetAsyncKeyState(key.0 as i32) } < 0)
    {
        return Err("Release modifier keys and repeat the browser instruction.".into());
    }
    let events = modifiers
        .iter()
        .map(|key| input(*key, false))
        .chain([input(key, false), input(key, true)])
        .chain(modifiers.iter().rev().map(|key| input(*key, true)))
        .collect::<Vec<_>>();
    check(expected, deadline, epoch)?;
    if unsafe { SendInput(&events, std::mem::size_of::<INPUT>() as i32) } != events.len() as u32 {
        // Release injected keys even if cancellation or partial injection interrupted the chord.
        let releases = std::iter::once(input(key, true))
            .chain(modifiers.iter().rev().map(|key| input(*key, true)))
            .collect::<Vec<_>>();
        unsafe {
            SendInput(&releases, std::mem::size_of::<INPUT>() as i32);
        }
        return Err("Windows refused browser input; check the browser before retrying.".into());
    }
    Ok(())
}

fn address_field(state: &State, process: u32) -> Option<IUIAutomationElement> {
    unsafe {
        let field = state.controls.automation.GetFocusedElement().ok()?;
        if field.CurrentProcessId().ok()? as u32 != process
            || field.CurrentIsPassword().ok()?.as_bool()
            || !field.CurrentIsEnabled().ok()?.as_bool()
            || ![UIA_EditControlTypeId, UIA_ComboBoxControlTypeId]
                .contains(&field.CurrentControlType().ok()?)
        {
            return None;
        }
        let walker = state.controls.automation.ControlViewWalker().ok()?;
        let mut ancestor = field.clone();
        let mut roles = vec![];
        for _ in 0..16 {
            let role = ancestor.CurrentControlType().ok()?;
            roles.push(if role == UIA_ToolBarControlTypeId {
                "AXToolbar"
            } else if role == UIA_DocumentControlTypeId {
                "AXWebArea"
            } else if role == UIA_WindowControlTypeId {
                "AXWindow"
            } else {
                "other"
            });
            if [UIA_DocumentControlTypeId, UIA_WindowControlTypeId].contains(&role) {
                return browser::address_in_toolbar(roles).then_some(field);
            }
            ancestor = walker.GetParentElement(&ancestor).ok()?;
        }
    }
    None
}

pub fn execute(
    state: &State,
    action: &Action,
    expected: &Observation,
    deadline: Instant,
    epoch: u64,
) -> Result<Outcome> {
    if !browser::supported("windows", &expected.target.app_identity) {
        return Err("Focus a supported browser before using browser actions.".into());
    }
    let (key, modifiers): (VIRTUAL_KEY, &[VIRTUAL_KEY]) = match action.kind.as_str() {
        "browser_new_tab" => (VK_T, &[VK_CONTROL]),
        "browser_close_tab" => (VK_W, &[VK_CONTROL]),
        "browser_next_tab" => (VK_TAB, &[VK_CONTROL]),
        "browser_previous_tab" => (VK_TAB, &[VK_CONTROL, VK_SHIFT]),
        "browser_back" => (VK_LEFT, &[VK_MENU]),
        "browser_forward" => (VK_RIGHT, &[VK_MENU]),
        "browser_reload" => (VK_R, &[VK_CONTROL]),
        "browser_navigate" => {
            let url = browser::navigation_url(action.text.as_deref().ok_or("Missing web URL.")?)?;
            shortcut(expected, &[VK_CONTROL], VK_L, deadline, epoch)?;
            let mut selected = None;
            for _ in 0..20 {
                check(expected, deadline, epoch)?;
                if let Some(field) = address_field(state, expected.target.process_id) {
                    selected = Some(field);
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            let field =
                selected.ok_or("The browser address field is unavailable. Nothing was typed.")?;
            unsafe {
                let pattern = field
                    .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                    .map_err(|_| "The browser address field cannot be edited.")?;
                if pattern
                    .CurrentIsReadOnly()
                    .map_err(|_| "Cannot inspect address field access.")?
                    .as_bool()
                {
                    return Err("The browser address field is read-only.".into());
                }
                check(expected, deadline, epoch)?;
                let focused = state
                    .controls
                    .automation
                    .GetFocusedElement()
                    .map_err(|_| "Browser focus unavailable.")?;
                if !state
                    .controls
                    .automation
                    .CompareElements(&field, &focused)
                    .map_err(|_| "Cannot verify browser focus.")?
                    .as_bool()
                {
                    return Err("Browser address focus changed.".into());
                }
                pattern
                    .SetValue(&BSTR::from(url.as_str()))
                    .map_err(|_| "The browser rejected the URL.")?;
                let focused = state
                    .controls
                    .automation
                    .GetFocusedElement()
                    .map_err(|_| "Browser focus unavailable.")?;
                if pattern
                    .CurrentValue()
                    .map_err(|_| "Cannot verify browser address.")?
                    != url.as_str()
                    || !state
                        .controls
                        .automation
                        .CompareElements(&field, &focused)
                        .map_err(|_| "Cannot verify browser focus.")?
                        .as_bool()
                {
                    return Err(
                        "The address changed before navigation. Nothing was submitted.".into(),
                    );
                }
            }
            shortcut(expected, &[], VK_RETURN, deadline, epoch)?;
            return Ok(Outcome {
                message: "Browser navigation requested; check the page".into(),
                verified: false,
            });
        }
        _ => return Err("Unsupported browser action.".into()),
    };
    shortcut(expected, modifiers, key, deadline, epoch)?;
    Ok(Outcome {
        message: format!("{} requested", action.kind.replace('_', " ")),
        verified: false,
    })
}
