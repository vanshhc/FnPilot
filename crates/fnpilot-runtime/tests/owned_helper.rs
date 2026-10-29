use fnpilot_core::session::{Phase, Session};
use fnpilot_runtime::managed::ManagedRuntime;
use std::{net::TcpListener, path::PathBuf};

#[test]
fn occupied_port_is_never_adopted() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let executable = std::env::current_exe().unwrap();
    let session = Session::default();
    let token = session.begin(Phase::Interpreting).unwrap();
    assert!(
        ManagedRuntime::launch(
            &executable,
            &executable,
            listener.local_addr().unwrap().port(),
            &session,
            token
        )
        .is_err()
    );
    assert!(listener.local_addr().is_ok());
}

#[test]
fn owned_packaged_helper_lifecycle_when_explicitly_provisioned() {
    let (Some(helper), Some(model)) = (
        std::env::var_os("FNPILOT_TEST_HELPER"),
        std::env::var_os("FNPILOT_TEST_GGUF"),
    ) else {
        return;
    };
    let port = 18090;
    let session = Session::default();
    let token = session.begin(Phase::Interpreting).unwrap();
    let mut helper = ManagedRuntime::launch(
        &PathBuf::from(helper),
        &PathBuf::from(model),
        port,
        &session,
        token,
    )
    .unwrap();
    assert!(helper.running().unwrap());
    assert_eq!(helper.port(), port);
    let interpreter = fnpilot_runtime::Interpreter::new(fnpilot_runtime::Configuration {
        runtime: fnpilot_runtime::Runtime::OpenAICompatible,
        endpoint: format!("http://127.0.0.1:{port}/v1"),
        model: "local-gguf".into(),
        ..Default::default()
    })
    .unwrap();
    let interpretation = interpreter
        .interpret(
            "Mute the sound",
            &fnpilot_core::context::CommandContext::default(),
        )
        .unwrap();
    interpretation.validate().unwrap();
    drop(helper);
    let listener = TcpListener::bind(("127.0.0.1", port))
        .expect("Owned helper should release its listening port");
    drop(listener);
}
