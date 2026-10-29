use crate::{Action, Interpretation, context::CommandContext, parser::regex};

fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}
fn request(input: &str) -> Option<(String, Option<String>)> {
    let text = input.trim();
    if let Some(g) =
        regex(r"^(?:please\s+)?(?:click|press|activate)\s+(?:the\s+)?(.+?)(?:\s+button)?[.!]?$")
            .captures(text)
    {
        return Some((g[1].into(), None));
    }
    if let Some(g) = regex(
        r"^(?:please\s+)?(?:set|change)\s+(?:the\s+)?(.+?)(?:\s+(?:text\s+)?field)?\s+to\s+(.+)$",
    )
    .captures(text)
    {
        return Some((g[1].into(), Some(g[2].into())));
    }
    if let Some(g) = regex(
        r"^(?:please\s+)?put\s+(.+?)\s+in(?:to)?\s+(?:the\s+)?(.+?)(?:\s+(?:text\s+)?field)?$",
    )
    .captures(text)
    {
        return Some((g[2].into(), Some(g[1].into())));
    }
    None
}
fn field(role: &str) -> bool {
    ["AXTextField", "AXTextArea", "AXComboBox"].contains(&role)
}
fn resolve(name: &str, context: &CommandContext, value: Option<String>) -> Interpretation {
    if context.controls_complete == Some(false) {
        return Interpretation::message(
            "unsupported",
            "The control scan is incomplete. Narrow the target window and retry.",
        );
    }
    let matches: Vec<_> = context
        .controls
        .iter()
        .filter(|c| normalize(&c.label) == normalize(name) && (value.is_none() || field(&c.role)))
        .collect();
    if matches.len() != 1 {
        return if matches.is_empty() {
            Interpretation::message(
                "unsupported",
                format!("No accessible control named ‘{name}’ is available in the target app."),
            )
        } else {
            Interpretation::message(
                "clarify",
                format!("Several controls are named ‘{name}’. Which one do you mean?"),
            )
        };
    }
    let mut a = Action::new(if value.is_none() {
        "press_control"
    } else {
        "set_control_value"
    });
    a.control_id = Some(matches[0].id.clone());
    a.text = value;
    Interpretation::actions(vec![a])
}
pub fn parse(input: &str, context: &CommandContext) -> Option<Interpretation> {
    let (name, value) = request(input)?;
    if [
        "it",
        "that",
        "this",
        "thebutton",
        "button",
        "field",
        "thefield",
        "textfield",
    ]
    .contains(&normalize(&name).as_str())
    {
        return Some(Interpretation::message(
            "clarify",
            if value.is_none() {
                "Which control should I press? Say its label."
            } else {
                "Which text field should I fill? Say its label."
            },
        ));
    }
    if value.is_some()
        && !regex(r"\bfield\b").is_match(input)
        && !context
            .controls
            .iter()
            .any(|c| normalize(&c.label) == normalize(&name) && field(&c.role))
    {
        return None;
    }
    Some(resolve(&name, context, value))
}
pub fn answer(original: &str, reply: &str, context: &CommandContext) -> Option<Interpretation> {
    let (_, value) = request(original)?;
    Some(resolve(reply.trim(), context, value))
}
