use super::{
    apps::{App, Catalog},
    ax::{self, Element},
};
use core_graphics::{
    event::{CGEvent, CGEventTapLocation},
    event_source::{CGEventSource, CGEventSourceStateID},
    geometry::CGRect,
};
use fnpilot_core::{
    Action, Result,
    context::{CommandContext, UIControl},
    runner::{Observation, Outcome, Target},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct State {
    pub(super) computer_frame: Option<super::computer::Frame>,
    pub(super) computer_sequence: u64,
    apps: Catalog,
    controls: BTreeMap<String, Element>,
    saved_frames: BTreeMap<(u32, u64, u64), CGRect>,
    minimized: BTreeMap<(u32, u64), Element>,
    scan_problem: Option<String>,
}
const CAPABILITIES: &[&str] = &[
    "open_app",
    "focus_app",
    "quit_app",
    "type_text",
    "close_window",
    "minimize_window",
    "restore_window",
    "maximize_window",
    "fullscreen_window",
    "exit_fullscreen_window",
    "fullscreen_video",
    "exit_fullscreen_video",
    "play",
    "pause",
    "toggle_playback",
    "volume_up",
    "volume_down",
    "set_volume",
    "mute",
    "unmute",
    "brightness_up",
    "brightness_down",
    "set_brightness",
    "press_control",
    "set_control_value",
    "submit_control",
    "scroll",
];
impl State {
    pub(super) fn current() -> Result<App> {
        if let Some(foreground) = App::foreground()
            && foreground.pid() != std::process::id()
        {
            if foreground.start() == 0 || foreground.id().is_empty() {
                return Err("The foreground process identity is unavailable. Focus an accessible app and retry.".into());
            }
            super::remember(&foreground);
        }
        let (pid, start) = super::LAST_EXTERNAL
            .lock()
            .unwrap()
            .ok_or("Focus an accessible target app before opening Settings.")?;
        App::for_identity(pid, start)
            .ok_or("The target app is no longer running. Focus an app and retry.".into())
    }
    pub(super) fn window(&mut self, app: &App, restoring: bool) -> Option<Element> {
        let root = Element::application(app.pid());
        root.timeout();
        let windows = root.elements("AXWindows");
        if restoring
            && let Some(saved) = self.minimized.get(&(app.pid(), app.start()))
            && windows.contains(saved)
            && saved.boolean("AXMinimized") == Some(true)
        {
            return Some(saved.clone());
        }
        root.element("AXFocusedWindow")
            .or_else(|| windows.first().cloned())
    }
    fn context(&mut self, app: &App, deadline: Instant, epoch: u64) -> Result<CommandContext> {
        self.scan_problem = None;
        self.controls.clear();
        let mut controls = Vec::new();
        let mut visited = BTreeSet::new();
        let mut complete = true;
        let root = Element::application(app.pid());
        root.timeout();
        if app.id() == "com.google.Chrome" {
            super::check(deadline, epoch)?;
            let _ = root.set_bool("AXManualAccessibility", true);
        }
        for node in [self.window(app, false), root.element("AXMenuBar")]
            .into_iter()
            .flatten()
        {
            self.scan(
                node,
                0,
                &mut visited,
                &mut controls,
                &mut complete,
                deadline,
                epoch,
            )?;
        }
        // Counts and failure codes only; never retain document text/control labels.
        *super::LAST_SCAN.lock().unwrap() = Some(
            serde_json::json!({"processId":app.pid(),"nodes":visited.len(),"controls":controls.len(),"complete":complete,"problem":self.scan_problem}),
        );
        Ok(CommandContext {
            app_name: app.name(),
            bundle_id: app.id(),
            apps: self.apps.infos(),
            controls,
            controls_complete: Some(complete),
            clarification: None,
            browser_actions: if fnpilot_core::browser::supported("macos", &app.id()) {
                fnpilot_core::browser::KINDS
                    .iter()
                    .map(|k| (*k).into())
                    .collect()
            } else {
                vec![]
            },
        })
    }
    #[allow(clippy::too_many_arguments)]
    fn scan(
        &mut self,
        node: Element,
        depth: usize,
        visited: &mut BTreeSet<u64>,
        controls: &mut Vec<UIControl>,
        complete: &mut bool,
        deadline: Instant,
        epoch: u64,
    ) -> Result<()> {
        super::check(deadline, epoch)?;
        if visited.contains(&node.hash()) {
            return Ok(());
        }
        if depth >= 14 || visited.len() >= 700 || controls.len() >= 90 {
            *complete = false;
            self.scan_problem.get_or_insert_with(|| {
                if depth >= 14 {
                    "depth limit"
                } else if visited.len() >= 700 {
                    "node limit"
                } else {
                    "control limit"
                }
                .into()
            });
            return Ok(());
        }
        visited.insert(node.hash());
        node.timeout();
        let role = node.string("AXRole");
        let subrole = node.string("AXSubrole");
        if subrole == "AXSecureTextField" {
            return Ok(());
        }
        let label = label(&node);
        if [
            "AXButton",
            "AXMenuItem",
            "AXMenuBarItem",
            "AXCheckBox",
            "AXRadioButton",
            "AXTextField",
            "AXTextArea",
            "AXComboBox",
            "AXSlider",
            "AXLink",
            "AXPopUpButton",
        ]
        .contains(&role.as_str())
            && !label.is_empty()
            && node.boolean("AXEnabled") != Some(false)
        {
            let id = format!("c{:x}", node.hash());
            self.controls.insert(id.clone(), node.clone());
            let exposed = if [
                "AXCloseButton",
                "AXMinimizeButton",
                "AXFullScreenButton",
                "AXZoomButton",
            ]
            .contains(&subrole.as_str())
            {
                subrole
            } else {
                role.clone()
            };
            controls.push(UIControl {
                id,
                role: exposed,
                label: label.chars().take(140).collect(),
                focused: node.boolean("AXFocused") == Some(true),
                value: if ["AXCheckBox", "AXRadioButton", "AXSlider"].contains(&role.as_str()) {
                    node.number("AXValue")
                } else if node.boolean("AXFocused") == Some(true)
                    && ["AXTextField", "AXTextArea", "AXComboBox"].contains(&role.as_str())
                {
                    let value = node.string("AXValue");
                    (value.chars().count() <= 2048).then_some(value)
                } else {
                    None
                },
            });
        }
        // Closed menus expose their entire hidden hierarchy through AXChildren.
        // Only the selected menu path is visible and actionable in this context.
        if matches!(role.as_str(), "AXMenuBarItem" | "AXMenuItem")
            && node.boolean("AXSelected") != Some(true)
        {
            return Ok(());
        }
        let (children, children_complete, provider_error) = node.visible_children();
        if !children_complete && self.scan_problem.is_none() {
            self.scan_problem = Some(provider_error.map_or_else(
                || "child array incomplete".into(),
                |code| format!("AXVisibleChildren error {code}"),
            ));
        }
        *complete &= children_complete;
        for child in children {
            self.scan(
                child,
                depth + 1,
                visited,
                controls,
                complete,
                deadline,
                epoch,
            )?;
        }
        Ok(())
    }
    pub fn observe(&mut self, deadline: Instant, epoch: u64) -> Result<Observation> {
        super::check(deadline, epoch)?;
        if !ax::trusted() {
            return Err("Grant Accessibility access to FnPilot in System Settings.".into());
        }
        let app = Self::current()?;
        let context = self.context(&app, deadline, epoch)?;
        let window = self.window(&app, false);
        Ok(Observation {
            target: Target {
                platform: "macos".into(),
                app_identity: app.id(),
                process_id: app.pid(),
                process_start: app.start(),
                window_id: window.map_or(0, |w| w.hash()),
                generation: epoch,
            },
            context,
            capabilities: CAPABILITIES
                .iter()
                .map(|s| s.to_string())
                .chain(
                    fnpilot_core::browser::KINDS
                        .iter()
                        .filter(|_| fnpilot_core::browser::supported("macos", &app.id()))
                        .map(|k| (*k).into()),
                )
                .collect(),
        })
    }
    pub fn execute(
        &mut self,
        action: Action,
        expected: Observation,
        deadline: Instant,
        epoch: u64,
    ) -> Result<Outcome> {
        action.validate()?;
        let fresh = self.observe(deadline, epoch)?;
        if fresh.target != expected.target {
            return Err("The native target changed before dispatch.".into());
        }
        if !fresh.capabilities.contains(&action.kind) {
            return Err("This operation is unavailable on macOS.".into());
        }
        if let Some(id) = &action.control_id
            && (fresh.context.controls_complete != Some(true)
                || expected.context.controls_complete != Some(true)
                || expected.context.controls.iter().find(|c| &c.id == id)
                    != fresh.context.controls.iter().find(|c| &c.id == id))
        {
            return Err("The selected control changed or its scan is incomplete.".into());
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
            return Err("The focused field or draft changed. Repeat your instruction.".into());
        }
        super::check(deadline, epoch)?;
        let kind = action.kind.as_str();
        if kind.contains("volume") || ["mute", "unmute"].contains(&kind) {
            return super::system::audio(&action, deadline, epoch);
        }
        if kind.contains("brightness") {
            return super::system::brightness(&action, deadline, epoch);
        }
        if ["open_app", "focus_app"].contains(&kind) {
            let (info, path) = self
                .apps
                .resolve(action.app.as_deref().ok_or("Name the app.")?)?;
            super::apps::launch_or_activate(
                info.bundle_id.clone(),
                path,
                kind == "open_app",
                deadline,
                epoch,
            )?;
            wait(deadline, epoch, || {
                App::foreground().is_some_and(|a| a.id() == info.bundle_id)
            })?;
            let app = App::foreground().ok_or("The requested app did not become active.")?;
            super::remember(&app);
            return Ok(Outcome {
                message: format!("Opened {}", info.name),
                verified: true,
            });
        }
        let app = if let Some(name) = &action.app {
            let (info, _) = self.apps.resolve(name)?;
            App::running(&info.bundle_id).ok_or("The named app is not running.")?
        } else {
            App::for_pid(expected.target.process_id)
                .filter(|a| a.start() == expected.target.process_start && !a.terminated())
                .ok_or("The original target process changed.")?
        };
        if app.start() == 0 || app.id().is_empty() {
            return Err(
                "The target process identity is unavailable. Focus the intended app and retry."
                    .into(),
            );
        }
        if fnpilot_core::browser::KINDS.contains(&kind) {
            return super::browser::execute(&app, &action, &fresh, deadline, epoch);
        }
        if kind == "quit_app" {
            let pid = app.pid();
            let start = app.start();
            super::on_main(deadline, epoch, move || {
                let selected = App::for_pid(pid)
                    .filter(|a| a.start() == start)
                    .ok_or("The target process changed.")?;
                if selected.terminate() {
                    Ok(())
                } else {
                    Err("The app declined to quit.".into())
                }
            })?;
            pause(deadline, epoch, 300)?;
            return Ok(Outcome {
                message: if app.terminated() {
                    "Quit app"
                } else {
                    "Quit requested; check for a native save dialog"
                }
                .into(),
                verified: app.terminated(),
            });
        }
        if let Some(id) = &action.control_id {
            let node = self
                .controls
                .get(id)
                .cloned()
                .ok_or("That control is no longer visible.")?;
            if node.pid() != Some(app.pid()) {
                return Err("That control belongs to a different app.".into());
            }
            let supplied = fresh
                .context
                .controls
                .iter()
                .find(|c| &c.id == id)
                .ok_or("That control is not in the live scan.")?;
            if label(&node).chars().take(140).collect::<String>() != supplied.label {
                return Err("The selected control label changed.".into());
            }
            if node.string("AXSubrole") == "AXSecureTextField"
                || node.boolean("AXEnabled") == Some(false)
            {
                return Err("The selected control is protected or disabled.".into());
            }
            super::check(deadline, epoch)?;
            if kind == "submit_control" {
                let root = Element::application(app.pid());
                if root.element("AXFocusedUIElement").as_ref() != Some(&node)
                    || supplied.value.as_deref() != Some(node.string("AXValue").as_str())
                {
                    return Err("The focused draft changed. Repeat your instruction.".into());
                }
                let down = CGEvent::new_keyboard_event(source()?, 36, true)
                    .map_err(|_| "Could not create submission event.")?;
                let up = CGEvent::new_keyboard_event(source()?, 36, false)
                    .map_err(|_| "Could not create submission event.")?;
                focused(&app, deadline, epoch)?;
                if root.element("AXFocusedUIElement").as_ref() != Some(&node) {
                    return Err("The focused field changed before submission.".into());
                }
                down.post(CGEventTapLocation::HID);
                up.post(CGEventTapLocation::HID);
                return Ok(Outcome {
                    message: "Submission requested; check the target app".into(),
                    verified: false,
                });
            }
            if kind == "press_control" {
                node.perform("AXPress")?;
                return Ok(Outcome {
                    message: "Control pressed".into(),
                    verified: false,
                });
            }
            let text = action.text.as_deref().ok_or("Missing field text.")?;
            node.set_text("AXValue", text)?;
            return Ok(Outcome {
                message: "Field set".into(),
                verified: node.string("AXValue") == text,
            });
        }
        if kind == "type_text" {
            return self.type_text(
                &app,
                action.text.as_deref().ok_or("Missing text.")?,
                deadline,
                epoch,
            );
        }
        if [
            "play",
            "pause",
            "toggle_playback",
            "fullscreen_video",
            "exit_fullscreen_video",
        ]
        .contains(&kind)
        {
            return self.video(&app, kind, deadline, epoch);
        }
        if kind == "scroll" {
            activate(&app, deadline, epoch)?;
            let rect = self
                .window(&app, false)
                .and_then(|w| w.frame())
                .ok_or("No target window is available to scroll.")?;
            let direction = action
                .direction
                .as_deref()
                .ok_or("Missing scroll direction.")?;
            let event = CGEvent::new_scroll_event(
                source()?,
                1,
                2,
                match direction {
                    "up" => 6,
                    "down" => -6,
                    _ => 0,
                },
                match direction {
                    "left" => 6,
                    "right" => -6,
                    _ => 0,
                },
                0,
            )
            .map_err(|_| "Could not create scroll event.")?;
            event.set_location(core_graphics::geometry::CGPoint::new(
                rect.origin.x + rect.size.width / 2.,
                rect.origin.y + rect.size.height / 2.,
            ));
            focused(&app, deadline, epoch)?;
            event.post(CGEventTapLocation::HID);
            return Ok(Outcome {
                message: "Scroll sent".into(),
                verified: false,
            });
        }
        let window = self
            .window(&app, kind == "restore_window")
            .ok_or("No target window is available.")?;
        let key = (app.pid(), app.start(), window.hash());
        match kind {
            "close_window" => {
                let button = window
                    .element("AXCloseButton")
                    .ok_or("The window has no accessible close button.")?;
                super::check(deadline, epoch)?;
                button.perform("AXPress")?;
                pause(deadline, epoch, 150)?;
                let (windows, complete) =
                    Element::application(app.pid()).elements_bounded("AXWindows");
                let still_present = windows.contains(&window);
                let verified = complete && !still_present;
                return Ok(Outcome {
                    message: if !verified {
                        "Close requested; check for a native save dialog"
                    } else {
                        "Window closed"
                    }
                    .into(),
                    verified,
                });
            }
            "minimize_window" => {
                super::check(deadline, epoch)?;
                window.set_bool("AXMinimized", true)?;
                if self.minimized.len() >= 128 {
                    self.minimized.pop_first();
                }
                self.minimized
                    .insert((app.pid(), app.start()), window.clone());
            }
            "restore_window" => {
                super::check(deadline, epoch)?;
                window.set_bool("AXMinimized", false)?;
                if window.boolean("AXFullScreen") == Some(true) {
                    super::check(deadline, epoch)?;
                    window.set_bool("AXFullScreen", false)?;
                }
                if let Some(frame) = self.saved_frames.get(&key).copied() {
                    set_frame(&window, frame, false, deadline, epoch)?;
                    self.saved_frames.remove(&key);
                }
                activate(&app, deadline, epoch)?;
                super::check(deadline, epoch)?;
                let _ = window.perform("AXRaise");
            }
            "maximize_window" => {
                if window.boolean("AXFullScreen") == Some(true) {
                    return Err("Exit window full screen before maximizing.".into());
                }
                if let Some(frame) = window.frame() {
                    if self.saved_frames.len() >= 128 {
                        self.saved_frames.pop_first();
                    }
                    self.saved_frames.entry(key).or_insert(frame);
                }
                let work = super::work_area(window.frame(), deadline, epoch)?;
                super::check(deadline, epoch)?;
                window.set_bool("AXMinimized", false)?;
                set_frame(&window, work, true, deadline, epoch)?;
            }
            "fullscreen_window" | "exit_fullscreen_window" => {
                let wanted = kind == "fullscreen_window";
                if window.boolean("AXFullScreen") != Some(wanted) {
                    super::check(deadline, epoch)?;
                    window.set_bool("AXFullScreen", wanted)?;
                    wait(deadline, epoch, || {
                        window.boolean("AXFullScreen") == Some(wanted)
                    })?;
                }
            }
            _ => return Err("Unsupported macOS native action.".into()),
        }
        let verified = verify(deadline, epoch, || match kind {
            "minimize_window" => window.boolean("AXMinimized") == Some(true),
            "restore_window" => {
                window.boolean("AXMinimized") == Some(false)
                    && window.boolean("AXFullScreen") != Some(true)
            }
            "fullscreen_window" => window.boolean("AXFullScreen") == Some(true),
            "exit_fullscreen_window" => window.boolean("AXFullScreen") == Some(false),
            "maximize_window" => true,
            _ => false,
        })?;
        Ok(Outcome {
            message: kind.replace('_', " "),
            verified,
        })
    }
    pub(super) fn type_text(
        &mut self,
        app: &App,
        text: &str,
        deadline: Instant,
        epoch: u64,
    ) -> Result<Outcome> {
        activate(app, deadline, epoch)?;
        let root = Element::application(app.pid());
        let field = root
            .element("AXFocusedUIElement")
            .ok_or("Focus an editable non-password field and retry.")?;
        if field.string("AXSubrole") == "AXSecureTextField"
            || (!matches!(
                field.string("AXRole").as_str(),
                "AXTextField" | "AXTextArea" | "AXComboBox"
            ) && !field.editable())
        {
            return Err("The focused control is not an editable non-password field.".into());
        }
        let before = field.string("AXValue");
        let utf16 = text.encode_utf16().collect::<Vec<_>>();
        let mut offset = 0;
        while offset < utf16.len() {
            focused(app, deadline, epoch)?;
            if root.element("AXFocusedUIElement").as_ref() != Some(&field) {
                return Err("The focused field changed. Remaining text stopped.".into());
            }
            let mut end = (offset + 128).min(utf16.len());
            if end < utf16.len() && (0xD800..=0xDBFF).contains(&utf16[end - 1]) {
                end -= 1;
            }
            let down = CGEvent::new_keyboard_event(source()?, 0, true)
                .map_err(|_| "Could not create typing event.")?;
            let up = CGEvent::new_keyboard_event(source()?, 0, false)
                .map_err(|_| "Could not create typing event.")?;
            down.set_string_from_utf16_unchecked(&utf16[offset..end]);
            up.set_string_from_utf16_unchecked(&utf16[offset..end]);
            focused(app, deadline, epoch)?;
            down.post(CGEventTapLocation::HID);
            up.post(CGEventTapLocation::HID);
            offset = end;
            pause(deadline, epoch, 20)?;
        }
        pause(deadline, epoch, 100)?;
        let after = field.string("AXValue");
        Ok(Outcome {
            message: "Text sent".into(),
            verified: after != before && after.contains(text),
        })
    }
    fn video(&mut self, app: &App, kind: &str, deadline: Instant, epoch: u64) -> Result<Outcome> {
        let context = self.context(app, deadline, epoch)?;
        if context.controls_complete != Some(true) {
            return Err("The player control scan is incomplete.".into());
        }
        let matches = |names: &[&str]| {
            context
                .controls
                .iter()
                .filter(|c| {
                    ["AXButton", "AXMenuItem"].contains(&c.role.as_str())
                        && names.iter().any(|name| {
                            let label = c.label.to_lowercase();
                            label == *name
                                || label.strip_prefix(name).is_some_and(|tail| {
                                    tail.starts_with(|c: char| !c.is_alphanumeric())
                                })
                        })
                })
                .collect::<Vec<_>>()
        };
        let play = matches(&["play", "resume"]);
        let pause_controls = matches(&["pause"]);
        let enter = matches(&[
            "enter full screen",
            "full screen",
            "fullscreen",
            "enter fullscreen",
        ]);
        let exit = matches(&[
            "exit full screen",
            "leave full screen",
            "exit fullscreen",
            "leave fullscreen",
        ]);
        let (wanted, already) = match kind {
            "play" => (play.clone(), pause_controls.len() == 1 && play.is_empty()),
            "pause" => (
                pause_controls.clone(),
                play.len() == 1 && pause_controls.is_empty(),
            ),
            "toggle_playback" => (play.into_iter().chain(pause_controls).collect(), false),
            "fullscreen_video" => (enter.clone(), exit.len() == 1 && enter.is_empty()),
            _ => (exit.clone(), enter.len() == 1 && exit.is_empty()),
        };
        if already {
            return Ok(Outcome {
                message: "Player is already in the requested state".into(),
                verified: true,
            });
        }
        if wanted.len() != 1 {
            return Err("No unique accessible player control was found. Show the intended player's controls and retry; window full screen is separate.".into());
        }
        let node = self
            .controls
            .get(&wanted[0].id)
            .ok_or("Player control changed.")?
            .clone();
        super::check(deadline, epoch)?;
        node.perform("AXPress")?;
        pause(deadline, epoch, 250)?;
        let after = self.context(app, deadline, epoch)?;
        let prefixes: &[&str] = match kind {
            "play" => &["pause"],
            "pause" => &["play", "resume"],
            "fullscreen_video" => &["exit full", "leave full"],
            "exit_fullscreen_video" => &["enter full", "full screen", "fullscreen"],
            _ => &[],
        };
        Ok(Outcome {
            message: kind.replace('_', " "),
            verified: !prefixes.is_empty()
                && after.controls.iter().any(|c| {
                    prefixes
                        .iter()
                        .any(|p| c.label.to_lowercase().starts_with(p))
                }),
        })
    }
}
fn label(node: &Element) -> String {
    ["AXTitle", "AXDescription", "AXHelp"]
        .into_iter()
        .map(|k| node.string(k))
        .find(|s| !s.is_empty())
        .unwrap_or_default()
}
pub(super) fn source() -> Result<CGEventSource> {
    CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "Could not create native input source.".into())
}
pub(super) fn activate(app: &App, deadline: Instant, epoch: u64) -> Result<()> {
    let (pid, start) = (app.pid(), app.start());
    let selected = App::for_identity(pid, start).ok_or("The target app changed.")?;
    super::apps::launch_or_activate(
        selected.id(),
        selected
            .bundle_path()
            .ok_or("The target app's bundle path is unavailable.")?,
        false,
        deadline,
        epoch,
    )?;
    wait(deadline, epoch, || {
        App::foreground().is_some_and(|a| a.pid() == pid && a.start() == start)
    })
}
fn focused(app: &App, deadline: Instant, epoch: u64) -> Result<()> {
    super::check(deadline, epoch)?;
    if App::foreground().is_some_and(|a| a.pid() == app.pid() && a.start() == app.start()) {
        Ok(())
    } else {
        Err("The intended app is no longer foreground. Input stopped.".into())
    }
}
fn pause(deadline: Instant, epoch: u64, milliseconds: u64) -> Result<()> {
    for _ in 0..milliseconds.div_ceil(10) {
        super::check(deadline, epoch)?;
        std::thread::sleep(Duration::from_millis(10));
    }
    super::check(deadline, epoch)
}
fn wait(deadline: Instant, epoch: u64, condition: impl Fn() -> bool) -> Result<()> {
    if verify(deadline, epoch, condition)? {
        Ok(())
    } else {
        Err("The app did not finish the requested state transition.".into())
    }
}
fn verify(deadline: Instant, epoch: u64, condition: impl Fn() -> bool) -> Result<bool> {
    for _ in 0..20 {
        super::check(deadline, epoch)?;
        if condition() {
            return Ok(true);
        }
        pause(deadline, epoch, 50)?;
    }
    Ok(false)
}
fn frame_matches(actual: CGRect, requested: CGRect, allow_system_insets: bool) -> bool {
    let exact = (actual.size.width - requested.size.width).abs() < 5.
        && (actual.size.height - requested.size.height).abs() < 5.
        && (actual.origin.x - requested.origin.x).abs() < 5.
        && (actual.origin.y - requested.origin.y).abs() < 5.;
    if exact {
        return true;
    }
    if !allow_system_insets {
        return false;
    }
    // macOS can reserve extra space for system chrome beyond visibleFrame.
    // Accept a nearly full work area, never a small or offscreen window.
    let gaps = [
        actual.origin.x - requested.origin.x,
        actual.origin.y - requested.origin.y,
        requested.origin.x + requested.size.width - actual.origin.x - actual.size.width,
        requested.origin.y + requested.size.height - actual.origin.y - actual.size.height,
    ];
    gaps.iter().all(|gap| (-5.0..=64.0).contains(gap))
        && actual.size.width >= requested.size.width * 0.9
        && actual.size.height >= requested.size.height * 0.9
}
fn set_frame(
    node: &Element,
    rect: CGRect,
    allow_system_insets: bool,
    deadline: Instant,
    epoch: u64,
) -> Result<()> {
    super::check(deadline, epoch)?;
    node.set_size(rect.size)?;
    pause(deadline, epoch, 100)?;
    super::check(deadline, epoch)?;
    node.set_position(rect.origin)?;
    // macOS may clamp the first resize against the old window position.
    // Resize again after moving so maximize/restore can reach the full frame.
    pause(deadline, epoch, 50)?;
    super::check(deadline, epoch)?;
    node.set_size(rect.size)?;
    // Some apps anchor a resize around the window center and shift its origin.
    // Reapply the saved position after the final size has settled.
    pause(deadline, epoch, 50)?;
    super::check(deadline, epoch)?;
    node.set_position(rect.origin)?;
    let result = wait(deadline, epoch, || {
        node.frame()
            .is_some_and(|actual| frame_matches(actual, rect, allow_system_insets))
    });
    if result.is_err() && super::current_epoch() == epoch && Instant::now() < deadline {
        return Err(match node.frame() {
            Some(actual) => format!(
                "The app constrained the window frame. Requested ({:.0}, {:.0}, {:.0} × {:.0}); observed ({:.0}, {:.0}, {:.0} × {:.0}).",
                rect.origin.x,
                rect.origin.y,
                rect.size.width,
                rect.size.height,
                actual.origin.x,
                actual.origin.y,
                actual.size.width,
                actual.size.height
            ),
            None => "The app did not expose its updated window geometry.".into(),
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_graphics::geometry::{CGPoint, CGSize};
    #[test]
    fn maximize_accepts_system_chrome_but_restore_remains_exact() {
        let frame = |x, y, w, h| CGRect::new(&CGPoint::new(x, y), &CGSize::new(w, h));
        let work = frame(0., 33., 1470., 841.);
        let observed = frame(3., 58., 1467., 815.);
        assert!(frame_matches(observed, work, true));
        assert!(!frame_matches(observed, work, false));
        assert!(frame_matches(work, work, false));
        assert!(!frame_matches(frame(0., 58., 800., 500.), work, true));
        assert!(!frame_matches(frame(-100., 33., 1470., 841.), work, true));
        assert!(!frame_matches(frame(0., 133., 1470., 741.), work, true));
    }
}
