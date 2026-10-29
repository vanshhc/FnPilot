use fnpilot_core::{
    Action, Interpretation, Result, action, browser, capabilities,
    context::CommandContext,
    runner::{self, Observation, Outcome, Platform, Target},
    session::{Phase, Session},
};

#[test]
fn browser_group_uses_native_identity_not_app_or_document_titles() {
    for bundle in [
        "com.apple.Safari",
        "com.google.Chrome",
        "org.mozilla.firefox",
        "com.microsoft.edgemac",
        "com.brave.Browser",
        "org.chromium.Chromium",
    ] {
        assert!(browser::supported("macos", bundle));
    }
    for exe in [
        "chrome.exe",
        "FIREFOX.EXE",
        "msedge.exe",
        "brave.exe",
        "chromium.exe",
    ] {
        assert!(browser::supported(
            "windows",
            &format!(r"win32:C:\Program Files\Browser\{exe}")
        ));
    }
    for (platform, identity) in [
        ("macos", "com.apple.TextEdit"),
        ("macos", "com.google.Chrome.evil"),
        ("macos", "Chrome"),
        ("windows", r"win32:C:\Notes\chrome.exe.txt"),
        ("windows", "chrome.exe"),
        ("test", "com.google.Chrome"),
    ] {
        assert!(!browser::supported(platform, identity));
    }
}

#[test]
fn navigation_accepts_web_addresses_and_rejects_executable_schemes_and_searches() {
    for (input, expected) in [
        ("example.com", "https://example.com/"),
        (
            " example.com/a?q=one#two ",
            "https://example.com/a?q=one#two",
        ),
        ("http://localhost:8080", "http://localhost:8080/"),
        ("https://example.org/a%20b", "https://example.org/a%20b"),
        (
            "example.org/?next=https://a.test",
            "https://example.org/?next=https://a.test",
        ),
    ] {
        assert_eq!(browser::navigation_url(input).unwrap(), expected);
        let mut action = Action::new("browser_navigate");
        action.text = Some(input.into());
        action.validate().unwrap();
    }
    for input in [
        "",
        "search for cats",
        "javascript:alert(1)",
        "javascript://example.org/",
        "data:text/html,hello",
        "file:///tmp/a",
        "ftp://example.com",
        "https://user:pass@example.com",
        "//example.org",
        "https://example.org/\nx",
        "https://example.org/\u{0000}x",
        "https:\\example.org",
        "somewhere",
    ] {
        let mut action = Action::new("browser_navigate");
        action.text = Some(input.into());
        assert!(action.validate().is_err(), "{input:?}");
    }
    assert!(browser::navigation_url(&format!("https://example.org/{}", "x".repeat(2048))).is_err());
    for input in [
        r#"{"kind":"browser_navigate"}"#,
        r#"{"kind":"browser_new_tab","text":"hello"}"#,
        r#"{"kind":"browser_reload","controlID":"x"}"#,
    ] {
        assert!(
            Interpretation::decode(&format!(r#"{{"status":"actions","actions":[{input}]}}"#))
                .is_err()
        );
    }
    for kind in browser::KINDS {
        assert!(action::KINDS.contains(kind));
    }
}

#[test]
fn browser_tools_follow_target_capabilities_without_phrase_filters() {
    let plain = CommandContext::default();
    assert!(
        capabilities::model_kinds(&plain)
            .iter()
            .all(|k| !browser::KINDS.contains(k))
    );
    let browser_context = CommandContext {
        browser_actions: browser::KINDS.iter().map(|k| (*k).into()).collect(),
        ..Default::default()
    };
    for kind in browser::KINDS {
        assert!(capabilities::model_kinds(&browser_context).contains(kind));
    }
}

struct Fake {
    observation: Observation,
    calls: Vec<String>,
    switch_after_first: bool,
}
impl Platform for Fake {
    fn observe(&mut self) -> Result<Observation> {
        Ok(self.observation.clone())
    }
    fn execute(&mut self, action: &Action, _: &Observation) -> Result<Outcome> {
        self.calls.push(action.kind.clone());
        if self.switch_after_first {
            self.observation.target.window_id += 1;
        }
        Ok(Outcome {
            message: "requested".into(),
            verified: false,
        })
    }
}
fn observed() -> Observation {
    Observation {
        target: Target {
            platform: "macos".into(),
            app_identity: "com.google.Chrome".into(),
            process_id: 1,
            process_start: 1,
            window_id: 1,
            generation: 1,
        },
        context: CommandContext::default(),
        capabilities: browser::KINDS.iter().map(|k| (*k).into()).collect(),
    }
}
#[test]
fn new_tab_then_url_stays_in_the_browser_and_stops_on_window_changes() {
    let mut navigate = Action::new("browser_navigate");
    navigate.text = Some("example.org".into());
    let result = Interpretation::actions(vec![Action::new("browser_new_tab"), navigate]);
    for mode in 0..4 {
        let mut expected = observed();
        if mode == 2 {
            expected.target.app_identity = "com.apple.TextEdit".into();
        }
        if mode == 3 {
            expected.capabilities.clear();
        }
        let mut fake = Fake {
            observation: expected.clone(),
            calls: vec![],
            switch_after_first: mode == 1,
        };
        let session = Session::default();
        let token = session.begin(Phase::Acting).unwrap();
        let outcome = runner::execute(&mut fake, &session, token, &result, &mut expected);
        assert_eq!(outcome.is_ok(), mode == 0);
        assert_eq!(
            fake.calls.len(),
            match mode {
                0 => 2,
                1 => 1,
                _ => 0,
            }
        );
    }
}

#[test]
fn invalid_url_rejects_the_whole_sequence_before_opening_a_tab() {
    let mut navigate = Action::new("browser_navigate");
    navigate.text = Some("javascript:alert(1)".into());
    let result = Interpretation::actions(vec![Action::new("browser_new_tab"), navigate]);
    let mut expected = observed();
    let mut fake = Fake {
        observation: expected.clone(),
        calls: vec![],
        switch_after_first: false,
    };
    let session = Session::default();
    let token = session.begin(Phase::Acting).unwrap();
    assert!(runner::execute(&mut fake, &session, token, &result, &mut expected).is_err());
    assert!(fake.calls.is_empty());
}

#[test]
fn webpage_toolbars_and_dialogs_cannot_supply_the_browser_address_field() {
    assert!(browser::address_in_toolbar([
        "AXTextField",
        "AXToolbar",
        "AXGroup",
        "AXWindow"
    ]));
    for roles in [
        vec!["AXTextField", "AXToolbar", "AXWebArea", "AXWindow"],
        vec!["AXTextField", "AXToolbar", "AXSheet", "AXWindow"],
        vec!["AXTextField", "AXToolbar", "AXDialog", "AXWindow"],
        vec!["AXTextField", "AXWindow"],
        vec!["AXTextField", "AXToolbar"],
    ] {
        assert!(!browser::address_in_toolbar(roles));
    }
}
