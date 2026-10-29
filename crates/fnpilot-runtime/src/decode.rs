use crate::{Configuration, OutputMode, Runtime};
use fnpilot_core::{Interpretation, Result, action};
use serde_json::{Value, json};

pub fn decode(config: &Configuration, object: &Value) -> Result<Interpretation> {
    let (message, reason) = if config.runtime == Runtime::Ollama {
        if object["done"] != true {
            return Err("Incomplete model response.".into());
        }
        (&object["message"], object["done_reason"].as_str())
    } else {
        if object["choices"].as_array().is_none_or(|c| c.len() != 1) {
            return Err("Expected one complete model response.".into());
        }
        (
            &object["choices"][0]["message"],
            object["choices"][0]["finish_reason"].as_str(),
        )
    };
    if !matches!(reason, Some("stop" | "tool_calls")) {
        return Err("The model response was incomplete. Nothing was executed.".into());
    }
    if config.output_mode != OutputMode::ToolCalls {
        return Interpretation::decode(
            message["content"]
                .as_str()
                .ok_or("Missing model content.")?,
        );
    }
    let Some(calls) = message["tool_calls"].as_array().filter(|c| !c.is_empty()) else {
        if let Some(text) = message["content"].as_str().filter(|s| !s.trim().is_empty()) {
            let result = Interpretation::message(
                if text.contains('?') {
                    "clarify"
                } else {
                    "unsupported"
                },
                text.chars().take(600).collect::<String>(),
            );
            result.validate()?;
            return Ok(result);
        }
        return Err("No tool call returned. Choose a supported output format.".into());
    };
    let mut actions = vec![];
    for call in calls {
        let function = &call["function"];
        let name = function["name"].as_str().ok_or("Invalid tool name.")?;
        let args: Value = if let Some(text) = function["arguments"].as_str() {
            serde_json::from_str(text).map_err(|_| "Invalid tool arguments.")?
        } else {
            function["arguments"].clone()
        };
        let mut args = args.as_object().cloned().ok_or("Invalid tool arguments.")?;
        if ["clarify", "unsupported"].contains(&name) {
            if calls.len() != 1 || args.len() != 1 {
                return Err("Clarification cannot be mixed with actions.".into());
            }
            let result = Interpretation::message(
                name,
                args.get("message")
                    .and_then(Value::as_str)
                    .ok_or("Missing clarification message.")?,
            );
            result.validate()?;
            return Ok(result);
        }
        if !action::KINDS.contains(&name) || args.contains_key("kind") {
            return Err("The model called an unavailable tool.".into());
        }
        args.insert("kind".into(), json!(name));
        actions.push(args);
    }
    Interpretation::decode(&json!({"status":"actions","actions":actions}).to_string())
}
