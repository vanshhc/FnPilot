use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UIControl {
    pub id: String,
    pub role: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub focused: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppInfo {
    pub name: String,
    #[serde(rename = "bundleID")]
    pub bundle_id: String,
}

/// Compatibility fixture data, never a trusted live target or Windows identity.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandContext {
    pub app_name: String,
    #[serde(rename = "bundleID")]
    pub bundle_id: String,
    pub apps: Vec<AppInfo>,
    pub controls: Vec<UIControl>,
    pub controls_complete: Option<bool>,
    pub clarification: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub browser_actions: Vec<String>,
}

impl CommandContext {
    pub fn compacted(&self, input: &str) -> Self {
        let ignored = [
            "the", "a", "an", "to", "in", "on", "at", "please", "could", "you", "can", "this",
            "that", "it", "click", "press", "button", "field", "set", "put", "and", "then", "app",
            "window",
        ];
        let words = |text: &str| -> std::collections::BTreeSet<String> {
            text.to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| !w.is_empty() && !ignored.contains(w))
                .map(str::to_owned)
                .collect()
        };
        let query = words(input);
        let score = |s: &str| words(s).intersection(&query).count();
        let mut result = self.clone();
        result
            .apps
            .sort_by_key(|a| std::cmp::Reverse(score(&a.name)));
        result.apps.truncate(12);
        result
            .controls
            .sort_by_key(|c| std::cmp::Reverse(score(&c.label)));
        // Intent is resolved by the model, so unfamiliar phrasing must not hide controls.
        // A focused field and its draft are useful even when no label matches the command.
        result.controls.sort_by_key(|c| !c.focused);
        if result.controls.len() > 90 {
            result.controls.truncate(90);
            result.controls_complete = Some(false);
        }
        result
    }
}
