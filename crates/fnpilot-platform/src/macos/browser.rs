use super::{apps::App, ax::Element, executor::source};
use core_graphics::event::{CGEvent, CGEventFlags as Flags, CGEventTapLocation};
use fnpilot_core::{
    Action, Result, browser,
    runner::{Observation, Outcome},
};
use std::time::{Duration, Instant};

fn check(app: &App, expected: &Observation, deadline: Instant, epoch: u64) -> Result<()> {
    super::check(deadline, epoch)?;
    if !App::foreground()
        .is_some_and(|current| current.pid() == app.pid() && current.start() == app.start())
        || Element::application(app.pid())
            .element("AXFocusedWindow")
            .map(|w| w.hash())
            != Some(expected.target.window_id)
    {
        return Err("The browser window changed. Repeat your instruction.".into());
    }
    Ok(())
}

fn shortcut(
    app: &App,
    expected: &Observation,
    key: u16,
    flags: Flags,
    deadline: Instant,
    epoch: u64,
) -> Result<()> {
    let down = CGEvent::new_keyboard_event(source()?, key, true)
        .map_err(|_| "Could not create browser input.")?;
    let up = CGEvent::new_keyboard_event(source()?, key, false)
        .map_err(|_| "Could not create browser input.")?;
    down.set_flags(flags);
    up.set_flags(flags);
    check(app, expected, deadline, epoch)?;
    down.post(CGEventTapLocation::HID);
    up.post(CGEventTapLocation::HID);
    Ok(())
}

fn address_field(app: &App) -> Option<Element> {
    let field = Element::application(app.pid()).element("AXFocusedUIElement")?;
    if field.pid() != Some(app.pid())
        || field.string("AXSubrole") == "AXSecureTextField"
        || !matches!(
            field.string("AXRole").as_str(),
            "AXTextField" | "AXComboBox"
        )
        || field.boolean("AXEnabled") == Some(false)
    {
        return None;
    }
    // A page text field must never be mistaken for the browser's address field.
    let mut ancestor = field.clone();
    let mut roles = vec![];
    for _ in 0..16 {
        let role = ancestor.string("AXRole");
        roles.push(role.clone());
        if matches!(
            role.as_str(),
            "AXWebArea" | "AXWindow" | "AXSheet" | "AXDialog"
        ) {
            return browser::address_in_toolbar(roles.iter().map(String::as_str)).then_some(field);
        }
        ancestor = ancestor.element("AXParent")?;
    }
    None
}

pub fn execute(
    app: &App,
    action: &Action,
    expected: &Observation,
    deadline: Instant,
    epoch: u64,
) -> Result<Outcome> {
    if !browser::supported("macos", &app.id()) || app.id() != expected.target.app_identity {
        return Err("Focus a supported browser before using browser actions.".into());
    }
    let command = Flags::CGEventFlagCommand;
    let control = Flags::CGEventFlagControl;
    let (key, flags) = match action.kind.as_str() {
        "browser_new_tab" => (17, command),
        "browser_close_tab" => (13, command),
        "browser_next_tab" => (48, control),
        "browser_previous_tab" => (48, control | Flags::CGEventFlagShift),
        "browser_back" => (33, command),
        "browser_forward" => (30, command),
        "browser_reload" => (15, command),
        "browser_navigate" => {
            let url = browser::navigation_url(action.text.as_deref().ok_or("Missing web URL.")?)?;
            shortcut(app, expected, 37, command, deadline, epoch)?;
            let mut selected = None;
            for _ in 0..20 {
                check(app, expected, deadline, epoch)?;
                if let Some(field) = address_field(app) {
                    selected = Some(field);
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            let field =
                selected.ok_or("The browser address field is unavailable. Nothing was typed.")?;
            check(app, expected, deadline, epoch)?;
            if Element::application(app.pid())
                .element("AXFocusedUIElement")
                .as_ref()
                != Some(&field)
            {
                return Err("The browser address focus changed.".into());
            }
            field.set_text("AXValue", &url)?;
            if field.string("AXValue") != url
                || Element::application(app.pid())
                    .element("AXFocusedUIElement")
                    .as_ref()
                    != Some(&field)
            {
                return Err("The address changed before navigation. Nothing was submitted.".into());
            }
            shortcut(app, expected, 36, Flags::empty(), deadline, epoch)?;
            return Ok(Outcome {
                message: "Browser navigation requested; check the page".into(),
                verified: false,
            });
        }
        _ => return Err("Unsupported browser action.".into()),
    };
    shortcut(app, expected, key, flags, deadline, epoch)?;
    Ok(Outcome {
        message: format!("{} requested", action.kind.replace('_', " ")),
        verified: false,
    })
}
