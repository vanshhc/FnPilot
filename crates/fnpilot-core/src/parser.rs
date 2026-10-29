use crate::{Action, Interpretation};
use regex::{Regex, RegexBuilder};
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
};

pub(crate) fn regex(pattern: &str) -> Regex {
    static CACHE: LazyLock<Mutex<HashMap<String, Regex>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    CACHE
        .lock()
        .unwrap()
        .entry(pattern.into())
        .or_insert_with(|| {
            RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .expect("static pattern")
        })
        .clone()
}

pub fn parse(input: &str) -> Option<Interpretation> {
    parse_depth(input, 0)
}
fn parse_depth(input: &str, depth: usize) -> Option<Interpretation> {
    if depth >= 20 {
        return None;
    }
    let original = input.trim();
    if original.is_empty() {
        return None;
    }
    if let Some(g) = regex(r"^(?:please\s+)?(?:type|write|enter text)\s+(.+)$").captures(original) {
        let mut a = Action::new("type_text");
        a.text = Some(g[1].into());
        return Some(Interpretation::actions(vec![a]));
    }
    let trimmed = original.trim_matches(['.', '!', '?', ' ']);
    let clean = regex(r"^please\s+").replace(trimmed, "");
    if let Some(separator) = regex(r"\s+(?:and then|then|and)\s+").find(original) {
        let mut first = parse_depth(&original[..separator.start()], depth + 1)?;
        let rest = parse_depth(&original[separator.end()..], depth + 1)?;
        if first.actions.len() + rest.actions.len() > 20 {
            return None;
        }
        if first.status != "actions" {
            return Some(first);
        }
        if rest.status != "actions" {
            return Some(rest);
        }
        first.actions.extend(rest.actions);
        return Some(first);
    }
    if let Some(g) =
        regex(r"^(open|launch|start|switch to|focus|quit|exit|close)\s+(.+)$").captures(&clean)
        && [
            "app",
            "the app",
            "an app",
            "this app",
            "that app",
            "the application",
            "it",
        ]
        .contains(&g[2].to_lowercase().as_str())
    {
        return Some(Interpretation::message(
            "clarify",
            "Which app do you mean? Say its full name.",
        ));
    }
    let lower = clean.to_lowercase();
    let kind = match lower.as_str() {
        "volume up" => "volume_up",
        "turn volume up" => "volume_up",
        "turn the volume up" => "volume_up",
        "increase volume" => "volume_up",
        "increase the volume" => "volume_up",
        "volume down" => "volume_down",
        "turn volume down" => "volume_down",
        "turn the volume down" => "volume_down",
        "decrease volume" => "volume_down",
        "lower the volume" => "volume_down",
        "mute" => "mute",
        "mute audio" => "mute",
        "unmute" => "unmute",
        "unmute audio" => "unmute",
        "brightness up" => "brightness_up",
        "increase brightness" => "brightness_up",
        "turn brightness up" => "brightness_up",
        "brightness down" => "brightness_down",
        "decrease brightness" => "brightness_down",
        "lower brightness" => "brightness_down",
        "play" => "play",
        "play the video" => "play",
        "resume playback" => "play",
        "pause" => "pause",
        "pause the video" => "pause",
        "pause playback" => "pause",
        "play pause" => "toggle_playback",
        "toggle playback" => "toggle_playback",
        "full screen the video" => "fullscreen_video",
        "fullscreen the video" => "fullscreen_video",
        "video full screen" => "fullscreen_video",
        "exit video full screen" => "exit_fullscreen_video",
        "exit video fullscreen" => "exit_fullscreen_video",
        "full screen the window" => "fullscreen_window",
        "fullscreen the window" => "fullscreen_window",
        "full screen" => "fullscreen_window",
        "fullscreen" => "fullscreen_window",
        "exit full screen" => "exit_fullscreen_window",
        "exit fullscreen" => "exit_fullscreen_window",
        "minimize" => "minimize_window",
        "minimise" => "minimize_window",
        "minimize the window" => "minimize_window",
        "minimise the window" => "minimize_window",
        "minimize window" => "minimize_window",
        "maximize" => "maximize_window",
        "maximise" => "maximize_window",
        "maximize the window" => "maximize_window",
        "maximise the window" => "maximize_window",
        "maximize window" => "maximize_window",
        "restore" => "restore_window",
        "restore the window" => "restore_window",
        "restore window" => "restore_window",
        "close" => "close_window",
        "close the window" => "close_window",
        "close window" => "close_window",
        _ => "",
    };
    if !kind.is_empty() {
        return Some(Interpretation::actions(vec![Action::new(kind)]));
    }
    if let Some(g) = regex(
        r"^(?:set\s+)?(?:the\s+)?(volume|brightness)\s+(?:to\s+)?(\d+(?:\.\d+)?)\s*(?:%|percent)?$",
    )
    .captures(&clean)
        && let Ok(number) = g[2].parse::<f64>()
        && (0.0..=100.0).contains(&number)
    {
        let mut a = Action::new(if g[1].eq_ignore_ascii_case("volume") {
            "set_volume"
        } else {
            "set_brightness"
        });
        a.value = Some(number);
        return Some(Interpretation::actions(vec![a]));
    }
    if let Some(g) = regex(r"^scroll\s+(up|down|left|right)$").captures(&clean) {
        let mut a = Action::new("scroll");
        a.direction = Some(g[1].to_lowercase());
        return Some(Interpretation::actions(vec![a]));
    }
    if let Some(g)=regex(r"^(open|launch|start|switch to|focus|quit|exit|close|minimize|minimise|maximize|maximise|restore)\s+(.+)$").captures(&clean) {
        let app=&g[2];
        if app.chars().count()>=120 || app.contains('/') || regex(r"\b(and|then|that|it|window|video)\b").is_match(app) {return None;}
        let kind=match g[1].to_lowercase().as_str() {"open"|"launch"|"start"=>"open_app","switch to"|"focus"=>"focus_app","quit"|"exit"=>"quit_app","close"=>"close_window","minimize"|"minimise"=>"minimize_window","maximize"|"maximise"=>"maximize_window",_=>"restore_window"};
        let mut a=Action::new(kind);a.app=Some(app.into());return Some(Interpretation::actions(vec![a]));
    }
    None
}

pub fn parts(input: &str) -> Vec<String> {
    let clean = input.trim();
    if regex(r"^(?:please\s+)?(?:type|write|enter text)\s").is_match(clean) {
        return vec![clean.into()];
    }
    // Field values are literal, including command words and separators.
    if regex(r"^(?:please\s+)?put\s+").is_match(clean) {
        return vec![clean.into()];
    }
    if regex(r"^(?:please\s+)?(?:set|change)\s+.+\s+to\s+").is_match(clean)
        && !regex(r"^(?:please\s+)?(?:set|change)\s+(?:the\s+)?(?:volume|brightness)\b")
            .is_match(clean)
    {
        return vec![clean.into()];
    }
    let verbs = regex(
        r"^(?:open|launch|start|switch|focus|quit|close|minimi[sz]e|maximi[sz]e|restore|type|write|enter text|click|press|scroll|mute|unmute|volume|brightness|set|play|pause|exit|full\s?screen)\b",
    );
    for separator in regex(r"\s+(?:and then|then|and)\s+").find_iter(clean) {
        if verbs.is_match(&clean[separator.end()..]) {
            let mut result = vec![clean[..separator.start()].into()];
            result.extend(parts(&clean[separator.end()..]));
            return result;
        }
    }
    vec![clean.into()]
}
