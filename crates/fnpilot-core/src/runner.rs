use crate::{Action, Interpretation, Result, context::CommandContext, session::Session};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Target {
    pub platform: String,
    pub app_identity: String,
    pub process_id: u32,
    pub process_start: u64,
    pub window_id: u64,
    pub generation: u64,
}
impl Target {
    pub fn same_window(&self, other: &Self) -> bool {
        self.platform == other.platform
            && self.app_identity == other.app_identity
            && self.process_id == other.process_id
            && self.process_start == other.process_start
            && self.window_id == other.window_id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub target: Target,
    pub context: CommandContext,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    pub message: String,
    pub verified: bool,
}

/// Live adapters own native objects. Context JSON is never a dispatch authority.
pub trait Platform {
    fn observe(&mut self) -> Result<Observation>;
    fn execute(&mut self, action: &Action, expected: &Observation) -> Result<Outcome>;
}

pub fn interpret(
    input: &str,
    context: &CommandContext,
    fallback: impl FnOnce(&str, &CommandContext) -> Result<Interpretation>,
) -> Result<Interpretation> {
    let result = if context.clarification.is_none() {
        if let Some(direct) =
            crate::parser::parse(input).or_else(|| crate::direct_ui::parse(input, context))
        {
            direct
        } else {
            fallback(input, context)?
        }
    } else {
        fallback(input, context)?
    };
    result.validate()?;
    Ok(result)
}

/// A submission is bound to an observed, focused, nonempty text field.
pub fn validate_context_action(action: &Action, context: &CommandContext) -> Result<()> {
    if action.kind == "submit_control" {
        let field = context
            .controls
            .iter()
            .find(|c| Some(&c.id) == action.control_id.as_ref());
        if context.controls_complete != Some(true)
            || context.controls.iter().filter(|c| c.focused).count() != 1
            || field.is_none_or(|c| {
                !c.focused
                    || !["AXTextField", "AXTextArea", "AXComboBox"].contains(&c.role.as_str())
                    || c.value.as_ref().is_none_or(|v| v.trim().is_empty())
            })
        {
            return Err(
                "Focus the intended nonempty text field and repeat your instruction.".into(),
            );
        }
    }
    Ok(())
}

/// Advances the continuation target only after an explicit open/focus action.
pub fn execute(
    platform: &mut impl Platform,
    session: &Session,
    token: u64,
    result: &Interpretation,
    observed: &mut Observation,
) -> Result<Vec<Outcome>> {
    result.validate()?;
    if result.status != "actions" {
        return Ok(vec![]);
    }
    let mut expected = observed.clone();
    let mut outcomes = vec![];
    for action in &result.actions {
        session.check(token)?;
        let fresh = platform.observe()?;
        if fresh.target != expected.target {
            return Err("The target changed while interpreting. Retry in the intended app.".into());
        }
        if !fresh.capabilities.contains(&action.kind) {
            return Err(format!(
                "{} is unavailable for this target on {}.",
                action.kind, fresh.target.platform
            ));
        }
        if crate::browser::KINDS.contains(&action.kind.as_str())
            && !crate::browser::supported(&fresh.target.platform, &fresh.target.app_identity)
        {
            return Err("Focus a supported browser before using browser actions.".into());
        }
        validate_context_action(action, &fresh.context)?;
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
        if let Some(id) = &action.control_id {
            let before = expected.context.controls.iter().find(|c| &c.id == id);
            let after = fresh.context.controls.iter().find(|c| &c.id == id);
            if fresh.context.controls_complete != Some(true)
                || expected.context.controls_complete != Some(true)
                || before.is_none()
                || before != after
            {
                return Err("The selected control changed or its scan is incomplete.".into());
            }
        }
        session.reserve_action(token)?;
        session.check(token)?;
        outcomes.push(platform.execute(action, &fresh)?);
        session.check(token)?;
        let changes_target = ["open_app", "focus_app"].contains(&action.kind.as_str());
        if outcomes.len() == result.actions.len() && !changes_target {
            expected = fresh;
            break;
        }
        let after = platform.observe()?;
        // Only explicit launch/focus operations may change the execution target.
        if !changes_target && after.target != fresh.target && outcomes.len() < result.actions.len()
        {
            return Err("The target changed during execution.".into());
        }
        expected = after;
        if action.control_id.is_some() && outcomes.len() < result.actions.len() {
            // Never reuse control IDs across a mutation, even if an OS recycles them.
            expected.context.controls.clear();
        }
    }
    // Carry the target through parser parts as well as through actions in one result.
    *observed = expected;
    Ok(outcomes)
}
