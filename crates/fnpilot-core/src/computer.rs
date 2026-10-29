//! One screenshot, one bounded input, then a fresh screenshot. No desktop access here.
use crate::{Result, runner::Target, session::Session};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Screen {
    pub id: u64,
    pub target: Target,
    pub app_name: String,
    pub width: u32,
    pub height: u32,
    /// PNG, base64 encoded. Kept in memory, never included in diagnostics.
    pub image: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Key {
    Return,
    Tab,
    Backspace,
    Delete,
    Space,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    #[serde(rename = "primary+a")]
    SelectAll,
    #[serde(rename = "primary+c")]
    Copy,
    #[serde(rename = "primary+v")]
    Paste,
    #[serde(rename = "primary+z")]
    Undo,
    #[serde(rename = "primary+shift+z")]
    Redo,
    #[serde(rename = "primary+n")]
    New,
    #[serde(rename = "primary+s")]
    Save,
    #[serde(rename = "primary+f")]
    Find,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    Move {
        x: u32,
        y: u32,
    },
    Click {
        x: u32,
        y: u32,
    },
    DoubleClick {
        x: u32,
        y: u32,
    },
    RightClick {
        x: u32,
        y: u32,
    },
    Type {
        text: String,
    },
    Key {
        key: Key,
    },
    Scroll {
        x: u32,
        y: u32,
        direction: Direction,
    },
    Wait {
        milliseconds: u32,
    },
    Done {
        message: String,
    },
    Clarify {
        message: String,
    },
    Unsupported {
        message: String,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Step {
    pub fn point(&self) -> Option<(u32, u32)> {
        match *self {
            Self::Move { x, y }
            | Self::Click { x, y }
            | Self::DoubleClick { x, y }
            | Self::RightClick { x, y }
            | Self::Scroll { x, y, .. } => Some((x, y)),
            _ => None,
        }
    }
    pub fn terminal(&self) -> bool {
        matches!(
            self,
            Self::Done { .. } | Self::Clarify { .. } | Self::Unsupported { .. }
        )
    }
    pub fn validate(&self, screen: &Screen) -> Result<()> {
        if screen.width == 0
            || screen.height == 0
            || screen.width > 4096
            || screen.height > 4096
            || self
                .point()
                .is_some_and(|(x, y)| x >= screen.width || y >= screen.height)
            || matches!(self, Self::Wait { milliseconds } if !(1..=2000).contains(milliseconds))
            || matches!(self, Self::Type { text } if text.is_empty() || text.chars().count() > 4096)
            || matches!(self, Self::Done { message } | Self::Clarify { message } | Self::Unsupported { message } if message.trim().is_empty() || message.chars().count() > 500)
        {
            return Err(
                "Invalid computer-use step or screenshot coordinates. Input stopped.".into(),
            );
        }
        Ok(())
    }
    pub fn decode(input: &str, screen: &Screen) -> Result<Self> {
        let step: Self = serde_json::from_str(input)
            .map_err(|_| "Invalid computer-use response. Input stopped.")?;
        step.validate(screen)?;
        Ok(step)
    }
}

pub fn schema() -> Value {
    let point = json!({"type":"integer","minimum":0,"maximum":4095});
    let mut variants = vec![];
    for kind in [
        "move",
        "click",
        "double_click",
        "right_click",
        "scroll",
        "type",
        "key",
        "wait",
        "done",
        "clarify",
        "unsupported",
    ] {
        let mut properties = json!({"kind":{"const":kind}});
        let mut required = vec!["kind"];
        if ["move", "click", "double_click", "right_click", "scroll"].contains(&kind) {
            properties["x"] = point.clone();
            properties["y"] = point.clone();
            required.extend(["x", "y"]);
        }
        match kind {
            "scroll" => {
                properties["direction"] = json!({"enum":["up","down","left","right"]});
                required.push("direction");
            }
            "type" => {
                properties["text"] = json!({"type":"string","minLength":1,"maxLength":4096});
                required.push("text");
            }
            "key" => {
                properties["key"] = json!({"enum":["Return","Tab","Backspace","Delete","Space","ArrowUp","ArrowDown","ArrowLeft","ArrowRight","primary+a","primary+c","primary+v","primary+z","primary+shift+z","primary+n","primary+s","primary+f"]});
                required.push("key");
            }
            "wait" => {
                properties["milliseconds"] = json!({"type":"integer","minimum":1,"maximum":2000});
                required.push("milliseconds");
            }
            "done" | "clarify" | "unsupported" => {
                properties["message"] = json!({"type":"string","minLength":1,"maxLength":500});
                required.push("message");
            }
            _ => {}
        }
        variants.push(json!({"type":"object","additionalProperties":false,"properties":properties,"required":required}));
    }
    json!({"oneOf":variants})
}

pub trait Computer {
    fn capture(&mut self) -> Result<Screen>;
    fn act(&mut self, step: &Step, expected: &Screen) -> Result<()>;
}

pub fn same_app(a: &Target, b: &Target) -> bool {
    a.platform == b.platform
        && a.app_identity == b.app_identity
        && a.process_id == b.process_id
        && a.process_start == b.process_start
        && a.generation == b.generation
}

pub fn run(
    computer: &mut impl Computer,
    session: &Session,
    token: u64,
    origin: &Target,
    mut decide: impl FnMut(&Screen, &[Step]) -> Result<Step>,
) -> Result<Step> {
    let mut history = vec![];
    loop {
        session.check(token)?;
        let screen = computer.capture()?;
        session.check(token)?;
        if !same_app(origin, &screen.target)
            || (history.is_empty() && !origin.same_window(&screen.target))
        {
            return Err("The computer-use target changed. Retry in the intended app.".into());
        }
        let step = decide(&screen, &history)?;
        session.check(token)?;
        step.validate(&screen)?;
        if step.terminal() {
            return Ok(step);
        }
        session.reserve_action(token)?;
        session.check(token)?;
        computer.act(&step, &screen)?;
        session.check(token)?;
        history.push(step);
    }
}
