use fnpilot_core::{
    Action, Interpretation, Result,
    context::CommandContext,
    hotkey::{HotkeyState, Signal},
    runner::{self, Observation, Outcome, Platform, Target},
    session::{Phase, Session},
};
use serde_json::json;

#[test]
fn cancellation_never_resumes_an_explicitly_paused_session() {
    let session = Session::default();
    session.cancel(true);
    session.cancel(false);
    assert!(matches!(session.snapshot().phase, Phase::Paused));
    assert!(session.begin(Phase::Recording).is_err());
    session.resume();
    assert!(matches!(session.snapshot().phase, Phase::Ready));
}

#[test]
fn strict_validation() {
    for value in [
        json!({"status":"actions","actions":[{"kind":"run_shell","text":"x"}]}),
        json!({"status":"actions","actions":[{"kind":"mute","text":null}]}),
        json!({"status":"actions","actions":[{"kind":"set_volume","value":true}]}),
        json!({"status":"actions","actions":[{"kind":"set_volume","value":101}]}),
        json!({"status":"actions","actions":[],"execute":true}),
        json!({"status":"clarify","actions":[{"kind":"mute"}],"message":"Which?"}),
    ] {
        assert!(
            Interpretation::decode(&value.to_string()).is_err(),
            "{value}"
        );
    }
}

#[test]
fn over_budget_input_is_bounded_before_recursive_parsing() {
    assert_eq!(
        fnpilot_core::parser::parse(&vec!["volume up"; 20].join(" and "))
            .unwrap()
            .actions
            .len(),
        20
    );
    assert!(fnpilot_core::parser::parse(&vec!["volume up"; 2000].join(" and ")).is_none());
}

#[test]
fn generations_cannot_commit_after_cancel_or_new_session() {
    let session = Session::default();
    let old = session.begin(Phase::Recording).unwrap();
    session.cancel(false);
    let new = session.begin(Phase::Recording).unwrap();
    assert!(
        session
            .update(old, Phase::Acting, "late transcript")
            .is_err()
    );
    assert!(session.reserve_action(old).is_err());
    assert_eq!(session.snapshot().generation, new);
    assert_eq!(session.snapshot().phase, Phase::Recording);
}

#[test]
fn clarification_carries_the_aggregate_budget() {
    let session = Session::default();
    let first = session.begin(Phase::Acting).unwrap();
    for _ in 0..19 {
        session.reserve_action(first).unwrap();
    }
    session
        .update(first, Phase::Clarifying, "Which target?")
        .unwrap();
    let continuation = session.begin(Phase::Acting).unwrap();
    session.carry_budget(continuation, 19).unwrap();
    session.reserve_action(continuation).unwrap();
    assert!(session.reserve_action(continuation).is_err());
    assert!(session.carry_budget(first, 0).is_err());
}

struct Fake {
    calls: usize,
    fail: bool,
    session: Session,
    cancel_on_dispatch: bool,
}
impl Platform for Fake {
    fn observe(&mut self) -> Result<Observation> {
        Ok(Observation {
            target: Target {
                platform: "mock".into(),
                app_identity: "fixture".into(),
                process_id: 1,
                process_start: 2,
                window_id: 3,
                generation: 0,
            },
            context: CommandContext::default(),
            capabilities: vec!["mute".into()],
        })
    }
    fn execute(&mut self, _: &Action, _: &Observation) -> Result<Outcome> {
        self.calls += 1;
        if self.cancel_on_dispatch {
            self.session.cancel(false);
        }
        if self.fail {
            Err("native failure".into())
        } else {
            Ok(Outcome {
                message: "sent".into(),
                verified: false,
            })
        }
    }
}

#[test]
fn aggregate_budget_errors_and_cancellation_stop_dispatch() {
    for (fail, cancel) in [(true, false), (false, true), (false, false)] {
        let session = Session::default();
        let token = session.begin(Phase::Acting).unwrap();
        let mut platform = Fake {
            calls: 0,
            fail,
            session: session.clone(),
            cancel_on_dispatch: cancel,
        };
        let mut observed = platform.observe().unwrap();
        let result = Interpretation::actions(vec![Action::new("mute"), Action::new("mute")]);
        if fail || cancel {
            assert!(
                runner::execute(&mut platform, &session, token, &result, &mut observed).is_err()
            );
            assert_eq!(platform.calls, 1);
        } else {
            for _ in 0..10 {
                runner::execute(&mut platform, &session, token, &result, &mut observed).unwrap();
            }
            assert!(
                runner::execute(&mut platform, &session, token, &result, &mut observed).is_err()
            );
            assert_eq!(platform.calls, 20);
        }
    }
}

#[test]
fn physical_hold_repeat_chord_and_escape() {
    let mut key = HotkeyState::default();
    assert_eq!(key.flags(true, false), Some(Signal::Start));
    assert_eq!(key.flags(true, false), None);
    assert_eq!(key.key_down(false), Some(Signal::Cancel));
    assert_eq!(key.flags(false, false), None);
    assert_eq!(key.key_down(true), Some(Signal::Cancel));
    assert_eq!(key.flags(true, true), None);
    key.reset();
    assert_eq!(key.flags(true, false), Some(Signal::Start));
    assert_eq!(key.flags(false, false), Some(Signal::Finish));
}
