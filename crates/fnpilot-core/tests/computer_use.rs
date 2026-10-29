use fnpilot_core::{
    Result,
    computer::{self, Computer, Screen, Step},
    runner::Target,
    session::{Phase, Session},
};

fn screen(window: u64) -> Screen {
    Screen {
        id: 1,
        target: Target {
            platform: "test".into(),
            app_identity: "editor".into(),
            process_id: 1,
            process_start: 1,
            window_id: window,
            generation: 1,
        },
        app_name: "Editor".into(),
        width: 1280,
        height: 800,
        image: "fixture-png".into(),
    }
}
struct Fake {
    screens: usize,
    actions: Vec<Step>,
    switch_app: bool,
}
impl Computer for Fake {
    fn capture(&mut self) -> Result<Screen> {
        self.screens += 1;
        let mut result = screen(self.screens as u64);
        if self.switch_app && self.screens > 1 {
            result.target.process_start += 1;
        }
        Ok(result)
    }
    fn act(&mut self, action: &Step, _: &Screen) -> Result<()> {
        self.actions.push(action.clone());
        Ok(())
    }
}
#[test]
fn agent_recaptures_and_replans_after_each_cursor_action() {
    let mut fake = Fake {
        screens: 0,
        actions: vec![],
        switch_app: false,
    };
    let session = Session::default();
    let token = session.begin(Phase::Interpreting).unwrap();
    let mut seen = vec![];
    let result = computer::run(
        &mut fake,
        &session,
        token,
        &screen(1).target,
        |frame, history| {
            seen.push(frame.target.window_id);
            Ok(match history.len() {
                0 => Step::Move { x: 100, y: 200 },
                1 => Step::Click { x: 100, y: 200 },
                _ => Step::Done {
                    message: "New document is visible".into(),
                },
            })
        },
    )
    .unwrap();
    assert!(matches!(result, Step::Done { .. }));
    assert_eq!(seen, vec![1, 2, 3]);
    assert_eq!(fake.actions.len(), 2);
    assert_eq!(session.snapshot().action_count, 2);
}
#[test]
fn app_switch_cancellation_and_budget_stop_further_input() {
    for mode in 0..3 {
        let session = Session::default();
        let token = session.begin(Phase::Interpreting).unwrap();
        let mut fake = Fake {
            screens: 0,
            actions: vec![],
            switch_app: mode == 0,
        };
        let result = computer::run(&mut fake, &session, token, &screen(1).target, |_, _| {
            if mode == 1 {
                session.cancel(false);
            }
            Ok(Step::Click { x: 10, y: 10 })
        });
        assert!(result.is_err());
        assert_eq!(fake.actions.len(), [1, 0, 20][mode]);
    }
}
#[test]
fn malformed_offscreen_or_unbounded_steps_are_rejected() {
    for input in [
        r#"{"kind":"click","x":-1,"y":10}"#,
        r#"{"kind":"click","x":10,"y":10,"app":"other"}"#,
        r#"{"kind":"key","key":"shell"}"#,
        r#"{"kind":"wait","milliseconds":60000}"#,
        r#"{"kind":"type","text":""}"#,
        r#"{"kind":"done","message":""}"#,
    ] {
        assert!(Step::decode(input, &screen(1)).is_err(), "{input}");
    }
    assert!(Step::decode(r#"{"kind":"click","x":1280,"y":10}"#, &screen(1)).is_err());
    assert!(Step::decode(r#"{"kind":"click","x":1279,"y":799}"#, &screen(1)).is_ok());
}
