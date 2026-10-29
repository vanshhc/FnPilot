use crate::{
    action,
    parser::{parts, regex},
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub fn kinds(input: &str) -> Vec<String> {
    let sequence = parts(input);
    if sequence.len() > 1 {
        return sequence
            .iter()
            .flat_map(|s| kinds(s))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
    }
    let text = input.to_lowercase();
    let matches = |r: &str| regex(r).is_match(&text);
    if matches(r"^(?:please\s+)?(?:type|write|enter text)\s") {
        return vec!["type_text".into()];
    }
    let mut choices = BTreeSet::new();
    if matches(r"\b(open|launch|start|switch|focus|quit|bring|active app)\b")
        || matches(r"^exit\s+") && !matches(r"full.?screen")
    {
        choices.extend(["open_app", "focus_app", "quit_app"]);
    }
    let video = matches(r"\b(video|play|pause|playback|resume|media)\b");
    let full = matches(r"full\s?screen");
    if video {
        choices.extend([
            "play",
            "pause",
            "toggle_playback",
            "fullscreen_video",
            "exit_fullscreen_video",
        ]);
    }
    if matches(r"\b(window|dock|close|minimi[sz]e|maximi[sz]e|restore|shrink)\b") || full && !video
    {
        choices.extend([
            "close_window",
            "minimize_window",
            "restore_window",
            "maximize_window",
            "fullscreen_window",
            "exit_fullscreen_window",
        ]);
    }
    if matches(
        r"\b(volume|sound|audio|speaker|speakers|louder|loud|quieter|quiet|silence|mute|unmute)\b",
    ) {
        choices.extend(["volume_up", "volume_down", "set_volume", "mute", "unmute"]);
    }
    if matches(r"\b(brightness|brighter|dimmer|dim|display|light)\b")
        || matches(r"\bscreen\b") && !video && !full
    {
        choices.extend(["brightness_up", "brightness_down", "set_brightness"]);
    }
    if matches(r"\b(click|press|activate|hit|button|field|search|save|scroll|put|change)\b") {
        choices.extend(["press_control", "set_control_value", "scroll"]);
    }
    choices.into_iter().map(str::to_owned).collect()
}
pub fn tools(input: &str) -> Vec<Value> {
    tools_for(kinds(input).iter().map(String::as_str))
}

/// Model intent resolution must not depend on command keywords.
pub fn model_tools() -> Vec<Value> {
    tools_for(action::KINDS.iter().copied())
}

/// Scope app-specific tools by observed capabilities, never by words in the request.
pub fn model_kinds(context: &crate::context::CommandContext) -> Vec<&'static str> {
    action::KINDS
        .iter()
        .copied()
        .filter(|kind| {
            !crate::browser::KINDS.contains(kind)
                || context
                    .browser_actions
                    .iter()
                    .any(|available| available == kind)
        })
        .collect()
}

pub fn model_tools_for(context: &crate::context::CommandContext) -> Vec<Value> {
    tools_for(model_kinds(context).into_iter())
}

fn tools_for<'a>(kinds: impl Iterator<Item = &'a str>) -> Vec<Value> {
    let mut tools: Vec<_> = kinds.map(|kind| json!({"type":"function","function":{"name":kind,"description":kind.replace('_', " "),"parameters":action::parameters(kind, false)}})).collect();
    for status in ["clarify", "unsupported"] {
        tools.push(json!({"type":"function","function":{"name":status,"description":status,"parameters":{"type":"object","additionalProperties":false,"required":["message"],"properties":{"message":{"type":"string","minLength":1}}}}}));
    }
    tools
}
