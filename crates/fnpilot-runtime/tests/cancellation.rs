use fnpilot_core::{
    context::CommandContext,
    session::{Phase, Session},
};
use fnpilot_runtime::{Configuration, Interpreter, Runtime};
use std::{
    io::Read,
    net::TcpListener,
    sync::mpsc,
    time::{Duration, Instant},
};

#[test]
fn cancellation_drops_a_pending_http_request_without_waiting_for_server_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (accepted, received) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut request = [0; 1024];
        let _ = stream.read(&mut request).unwrap();
        accepted.send(()).unwrap();
        let _ = wait.recv_timeout(Duration::from_secs(3));
    });
    let session = Session::default();
    let token = session.begin(Phase::Interpreting).unwrap();
    let worker_session = session.clone();
    let worker = std::thread::spawn(move || {
        let interpreter = Interpreter::new(Configuration {
            runtime: Runtime::OpenAICompatible,
            endpoint: format!("http://{address}/v1"),
            ..Default::default()
        })
        .unwrap();
        interpreter.interpret_session("mute", &CommandContext::default(), &worker_session, token)
    });
    received.recv_timeout(Duration::from_secs(3)).unwrap();
    let before = Instant::now();
    session.cancel(false);
    assert!(worker.join().unwrap().is_err());
    assert!(before.elapsed() < Duration::from_secs(1));
    release.send(()).unwrap();
    server.join().unwrap();
}
