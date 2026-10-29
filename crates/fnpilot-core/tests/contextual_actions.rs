use fnpilot_core::{
    Action, Interpretation, Result,
    context::{CommandContext, UIControl},
    runner::{self, Observation, Outcome, Platform, Target},
    session::{Phase, Session},
};

fn context() -> CommandContext {
    CommandContext {
        app_name: "Browser".into(),
        controls_complete: Some(true),
        controls: vec![UIControl {
            id: "address".into(),
            role: "AXTextField".into(),
            label: "Address and search bar".into(),
            focused: true,
            value: Some("https://example.org".into()),
        }],
        ..Default::default()
    }
}
fn submit() -> Action {
    let mut action = Action::new("submit_control");
    action.control_id = Some("address".into());
    action
}

#[test]
fn unfamiliar_phrasing_keeps_focused_drafts_and_unmatched_controls() {
    let mut ctx = context();
    ctx.controls.push(UIControl {
        id: "send".into(),
        role: "AXButton".into(),
        label: "Send".into(),
        ..Default::default()
    });
    for phrase in [
        "go there",
        "let them know",
        "that's ready",
        "please do it now",
    ] {
        let compact = ctx.compacted(phrase);
        assert_eq!(compact.controls, ctx.controls);
        assert_eq!(compact.controls_complete, Some(true));
    }
    ctx.controls.resize(91, UIControl::default());
    assert_eq!(ctx.compacted("go there").controls_complete, Some(false));
}

#[test]
fn submit_requires_one_known_nonempty_focused_text_field() {
    let action = submit();
    action.validate().unwrap();
    runner::validate_context_action(&action, &context()).unwrap();
    for mode in 0..7 {
        let mut ctx = context();
        match mode {
            0 => ctx.controls[0].focused = false,
            1 => ctx.controls[0].value = None,
            2 => ctx.controls[0].value = Some("  \n".into()),
            3 => ctx.controls[0].role = "AXButton".into(),
            4 => ctx.controls_complete = Some(false),
            5 => ctx.controls.clear(),
            _ => ctx.controls.push(UIControl {
                id: "other".into(),
                ..ctx.controls[0].clone()
            }),
        }
        assert!(
            runner::validate_context_action(&action, &ctx).is_err(),
            "mode {mode}"
        );
    }
    assert!(Interpretation::decode(r#"{"status":"actions","actions":[{"kind":"submit_control","controlID":"address","text":"replacement"}]}"#).is_err());
}

struct Fake {
    observed: Observation,
    calls: usize,
}
impl Platform for Fake {
    fn observe(&mut self) -> Result<Observation> {
        Ok(self.observed.clone())
    }
    fn execute(&mut self, _: &Action, _: &Observation) -> Result<Outcome> {
        self.calls += 1;
        Ok(Outcome {
            message: "requested".into(),
            verified: false,
        })
    }
}

#[test]
fn changed_draft_or_focus_stops_submission_and_send_buttons() {
    let before = Observation {
        target: Target {
            platform: "test".into(),
            app_identity: "browser".into(),
            process_id: 1,
            process_start: 1,
            window_id: 1,
            generation: 1,
        },
        context: context(),
        capabilities: vec!["submit_control".into(), "press_control".into()],
    };
    for kind in ["submit_control", "press_control"] {
        for mode in 0..3 {
            let mut expected = before.clone();
            if kind == "press_control" {
                expected.context.controls.push(UIControl {
                    id: "send".into(),
                    role: "AXButton".into(),
                    label: "Send".into(),
                    ..Default::default()
                });
            }
            let mut fresh = expected.clone();
            match mode {
                0 => fresh.context.controls[0].value = Some("changed".into()),
                1 => fresh.context.controls[0].focused = false,
                _ => fresh.target.window_id += 1,
            }
            let mut action = Action::new(kind);
            action.control_id = Some(
                if kind == "press_control" {
                    "send"
                } else {
                    "address"
                }
                .into(),
            );
            let mut fake = Fake {
                observed: fresh,
                calls: 0,
            };
            let session = Session::default();
            let token = session.begin(Phase::Acting).unwrap();
            assert!(
                runner::execute(
                    &mut fake,
                    &session,
                    token,
                    &Interpretation::actions(vec![action]),
                    &mut expected
                )
                .is_err()
            );
            assert_eq!(fake.calls, 0);
        }
    }
}
