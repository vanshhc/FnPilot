use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub type Result<T> = std::result::Result<T, String>;

pub const KINDS: &[&str] = &[
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
    "browser_new_tab",
    "browser_close_tab",
    "browser_next_tab",
    "browser_previous_tab",
    "browser_back",
    "browser_forward",
    "browser_reload",
    "browser_navigate",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(default, rename = "controlID", skip_serializing_if = "Option::is_none")]
    pub control_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
}

impl Action {
    pub fn new(kind: &str) -> Self {
        Self {
            kind: kind.into(),
            app: None,
            text: None,
            value: None,
            control_id: None,
            direction: None,
        }
    }
    pub fn validate(&self) -> Result<()> {
        let kind = self.kind.as_str();
        if !KINDS.contains(&kind) || self.app.as_ref().is_some_and(|s| s.trim().is_empty()) {
            return Err("Unknown action or empty app target.".into());
        }
        let system = kind.contains("volume")
            || kind.contains("brightness")
            || ["mute", "unmute"].contains(&kind);
        if ["type_text", "set_control_value", "browser_navigate"].contains(&kind)
            != self.text.is_some()
            || ["set_volume", "set_brightness"].contains(&kind) != self.value.is_some()
            || ["press_control", "set_control_value", "submit_control"].contains(&kind)
                != self.control_id.is_some()
            || (kind == "scroll") != self.direction.is_some()
            || (system && self.app.is_some())
            || (["open_app", "focus_app", "quit_app"].contains(&kind) && self.app.is_none())
            || self
                .value
                .is_some_and(|n| !n.is_finite() || !(0.0..=100.0).contains(&n))
            || self
                .direction
                .as_deref()
                .is_some_and(|d| !["up", "down", "left", "right"].contains(&d))
            || self.text.as_ref().is_some_and(String::is_empty)
            || self.control_id.as_ref().is_some_and(String::is_empty)
        {
            return Err(format!(
                "Invalid arguments for {kind}. Nothing was executed."
            ));
        }
        if kind == "browser_navigate" {
            crate::browser::navigation_url(self.text.as_deref().ok_or("Missing web URL.")?)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interpretation {
    pub status: String,
    pub actions: Vec<Action>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl Interpretation {
    pub fn actions(actions: Vec<Action>) -> Self {
        Self {
            status: "actions".into(),
            actions,
            message: None,
        }
    }
    pub fn message(status: &str, message: impl Into<String>) -> Self {
        Self {
            status: status.into(),
            actions: vec![],
            message: Some(message.into()),
        }
    }
    pub fn validate(&self) -> Result<()> {
        if self.actions.len() > 20 {
            return Err("Stopped after 20 actions. Give a shorter instruction.".into());
        }
        match self.status.as_str() {
            "actions" if !self.actions.is_empty() => {
                self.actions.iter().try_for_each(Action::validate)
            }
            "clarify" | "unsupported"
                if self.actions.is_empty()
                    && self.message.as_ref().is_some_and(|s| !s.trim().is_empty()) =>
            {
                Ok(())
            }
            _ => Err("Invalid command status, actions or message.".into()),
        }
    }
    pub fn decode(data: &str) -> Result<Self> {
        // Validate the field set before deserialization: even null arguments are not allowed
        // on actions whose contract does not use that field.
        let value: Value = serde_json::from_str(data).map_err(|_| "Invalid action JSON.")?;
        for action in value["actions"].as_array().ok_or("Missing actions.")? {
            let object = action.as_object().ok_or("Invalid action.")?;
            let kind = action["kind"].as_str().ok_or("Missing kind.")?;
            let mut allowed = vec!["kind"];
            let system = kind.contains("volume")
                || kind.contains("brightness")
                || ["mute", "unmute"].contains(&kind);
            if !system {
                allowed.push("app");
            }
            if ["type_text", "set_control_value", "browser_navigate"].contains(&kind) {
                allowed.push("text");
            }
            if ["set_volume", "set_brightness"].contains(&kind) {
                allowed.push("value");
            }
            if ["press_control", "set_control_value", "submit_control"].contains(&kind) {
                allowed.push("controlID");
            }
            if kind == "scroll" {
                allowed.push("direction");
            }
            if object.keys().any(|k| !allowed.contains(&k.as_str())) {
                return Err("Unexpected action field.".into());
            }
        }
        let result: Self = serde_json::from_value(value).map_err(|_| "Invalid action object.")?;
        result.validate()?;
        Ok(result)
    }
    pub fn exit_code(&self) -> i32 {
        match self.status.as_str() {
            "actions" => 0,
            "clarify" => 2,
            _ => 3,
        }
    }
}

pub fn parameters(kind: &str, include_kind: bool) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = vec![];
    let string = json!({"type":"string", "minLength":1});
    let system = kind.contains("volume")
        || kind.contains("brightness")
        || ["mute", "unmute"].contains(&kind);
    if include_kind {
        properties.insert("kind".into(), json!({"const":kind}));
        required.push("kind");
    }
    if !system {
        properties.insert("app".into(), string.clone());
        if ["open_app", "focus_app", "quit_app"].contains(&kind) {
            required.push("app");
        }
    }
    for (enabled, name, schema) in [
        (
            ["type_text", "set_control_value", "browser_navigate"].contains(&kind),
            "text",
            string.clone(),
        ),
        (
            ["set_volume", "set_brightness"].contains(&kind),
            "value",
            json!({"type":"number","minimum":0,"maximum":100}),
        ),
        (
            ["press_control", "set_control_value", "submit_control"].contains(&kind),
            "controlID",
            string,
        ),
        (
            kind == "scroll",
            "direction",
            json!({"enum":["up","down","left","right"]}),
        ),
    ] {
        if enabled {
            properties.insert(name.into(), schema);
            required.push(name);
        }
    }
    json!({"type":"object","additionalProperties":false,"properties":properties,"required":required})
}

pub fn schema() -> Value {
    schema_for(KINDS)
}

pub fn schema_for(kinds: &[&str]) -> Value {
    json!({"oneOf":[
        {"type":"object","additionalProperties":false,"required":["status","actions"],"properties":{
            "status":{"const":"actions"},"actions":{"type":"array","minItems":1,"maxItems":20,"items":{"oneOf":kinds.iter().map(|k|parameters(k,true)).collect::<Vec<_>>()}}}},
        {"type":"object","additionalProperties":false,"required":["status","actions","message"],"properties":{
            "status":{"enum":["clarify","unsupported"]},"actions":{"type":"array","maxItems":0},"message":{"type":"string","minLength":1}}}
    ]})
}
