use fnpilot_core::{
    Action, Interpretation, Result,
    context::{CommandContext, UIControl},
    direct_ui, parser,
    runner::{self, Observation, Outcome, Platform, Target},
    session::{Phase, Session},
};

fn context() -> CommandContext {
    CommandContext {
        controls: vec![
            UIControl {
                id: "save".into(),
                role: "AXButton".into(),
                label: "Save".into(),
                value: None,
                focused: false,
            },
            UIControl {
                id: "search".into(),
                role: "AXTextField".into(),
                label: "Search".into(),
                value: None,
                focused: false,
            },
        ],
        controls_complete: Some(true),
        ..Default::default()
    }
}
#[test]
fn exact_controls_literal_field_values_and_clarification_match_legacy_contract() {
    let ctx = context();
    assert_eq!(
        direct_ui::parse("Please click the Save button", &ctx)
            .unwrap()
            .actions[0]
            .control_id
            .as_deref(),
        Some("save")
    );
    assert_eq!(
        direct_ui::parse("Click Export", &ctx).unwrap().status,
        "unsupported"
    );
    let mut duplicate = ctx.clone();
    duplicate.controls.push(UIControl {
        id: "other".into(),
        ..ctx.controls[0].clone()
    });
    let reply = direct_ui::parse("Click Save", &duplicate).unwrap();
    assert_eq!(reply.status, "clarify");
    assert!(reply.actions.is_empty());
    for (input, literal) in [
        (
            "Change the Search field to Open Chrome then quit nothing!",
            "Open Chrome then quit nothing!",
        ),
        ("Put cats in the Search field", "cats"),
    ] {
        let action = direct_ui::parse(input, &ctx).unwrap().actions.remove(0);
        assert_eq!(action.kind, "set_control_value");
        assert_eq!(action.control_id.as_deref(), Some("search"));
        assert_eq!(action.text.as_deref(), Some(literal));
    }
    assert_eq!(
        direct_ui::answer("click it", "Save", &ctx).unwrap().actions[0]
            .control_id
            .as_deref(),
        Some("save")
    );
    assert_eq!(
        direct_ui::answer("Change the field to Keep my words!", "Search", &ctx)
            .unwrap()
            .actions[0]
            .text
            .as_deref(),
        Some("Keep my words!")
    );
    assert!(direct_ui::parse("Put this window into full screen mode", &ctx).is_none());
    for input in ["Click it", "Change the field to hello"] {
        assert_eq!(direct_ui::parse(input, &ctx).unwrap().status, "clarify");
    }
    let incomplete = CommandContext {
        controls_complete: Some(false),
        ..ctx
    };
    assert_eq!(
        direct_ui::parse("click Save", &incomplete).unwrap().status,
        "unsupported"
    );
    assert_eq!(
        direct_ui::answer("click it", "Save", &incomplete)
            .unwrap()
            .status,
        "unsupported"
    );
}
#[test]
fn each_dictation_verb_and_compound_clarification_preserves_legacy_semantics() {
    for input in [
        "open the app and increase volume",
        "volume up and close the app",
    ] {
        let result = parser::parse(input).unwrap();
        assert_eq!(result.status, "clarify");
        assert!(result.actions.is_empty());
    }
    for verb in ["type", "write", "enter text"] {
        let input = format!("open Notes and {verb} Buy milk! Then open Chrome!");
        let parts = parser::parts(&input);
        assert_eq!(parts.len(), 2);
        assert_eq!(
            parser::parse(&parts[1]).unwrap().actions[0].text.as_deref(),
            Some("Buy milk! Then open Chrome!")
        );
    }
}

struct Fake {
    observed: Observation,
    calls: usize,
}
impl Platform for Fake {
    fn observe(&mut self) -> Result<Observation> {
        Ok(self.observed.clone())
    }
    fn execute(&mut self, action: &Action, _: &Observation) -> Result<Outcome> {
        self.calls += 1;
        if action.kind == "open_app" {
            self.observed.target.app_identity = "notes".into();
            self.observed.target.process_id = 2;
            self.observed.context = context();
            self.observed.context.controls[0].label = "New Note".into();
        }
        Ok(Outcome {
            message: "sent".into(),
            verified: false,
        })
    }
}
fn observed() -> Observation {
    Observation {
        target: Target {
            platform: "test".into(),
            app_identity: "browser".into(),
            process_id: 1,
            process_start: 1,
            window_id: 1,
            generation: 1,
        },
        context: context(),
        capabilities: ["open_app", "press_control", "type_text"]
            .map(str::to_string)
            .to_vec(),
    }
}
#[test]
fn sequence_refreshes_controls_after_explicit_target_change() {
    let session = Session::default();
    let token = session.begin(Phase::Interpreting).unwrap();
    let mut fake = Fake {
        observed: observed(),
        calls: 0,
    };
    for part in parser::parts("open Notes then click New Note then type Hello!") {
        let mut observation = fake.observe().unwrap();
        let result = runner::interpret(&part, &observation.context, |_, _| {
            panic!("Unexpected model fallback")
        })
        .unwrap();
        runner::execute(&mut fake, &session, token, &result, &mut observation).unwrap();
    }
    assert_eq!(fake.calls, 3);
}
#[test]
fn changed_labels_targets_capabilities_and_incomplete_scans_stop_before_dispatch() {
    let mut before = observed();
    let result = Interpretation::decode(
        r#"{"status":"actions","actions":[{"kind":"press_control","controlID":"save"}]}"#,
    )
    .unwrap();
    for mode in 0..6 {
        let mut fresh = before.clone();
        match mode {
            0 => fresh.context.controls[0].label = "Delete".into(),
            1 => fresh.target.process_start += 1,
            2 => fresh.capabilities.clear(),
            3 => fresh.context.controls_complete = Some(false),
            4 => fresh.context.controls_complete = None,
            _ => fresh.context.controls.clear(),
        }
        let mut fake = Fake {
            observed: fresh,
            calls: 0,
        };
        let session = Session::default();
        let token = session.begin(Phase::Acting).unwrap();
        assert!(runner::execute(&mut fake, &session, token, &result, &mut before).is_err());
        assert_eq!(fake.calls, 0);
    }
}

#[test]
fn explicit_focus_carries_the_target_to_later_parser_parts_and_rejects_a_user_switch() {
    let session = Session::default();
    let token = session.begin(Phase::Acting).unwrap();
    let mut fake = Fake {
        observed: observed(),
        calls: 0,
    };
    let mut continuation = fake.observe().unwrap();
    let open = parser::parse("open Notes").unwrap();
    runner::execute(&mut fake, &session, token, &open, &mut continuation).unwrap();
    assert_eq!(continuation.target.process_id, 2);
    fake.observed.target.process_id = 3;
    let typing = parser::parse("type Hello!").unwrap();
    assert!(runner::execute(&mut fake, &session, token, &typing, &mut continuation).is_err());
    assert_eq!(fake.calls, 1);
}
