use fnpilot_core::context::CommandContext;
use fnpilot_runtime::{Configuration, Interpreter};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    time::{Duration, Instant},
};

fn stub(
    replies: Vec<(u16, serde_json::Value)>,
) -> (String, std::thread::JoinHandle<Vec<serde_json::Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let join = std::thread::spawn(move || {
        let mut requests = vec![];
        for (status, response) in replies {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut socket = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("stub accept failed: {e}"),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let mut length = 0;
            let mut line = String::new();
            loop {
                line.clear();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            requests.push(serde_json::from_slice(&body).unwrap());
            let body = response.to_string();
            write!(socket,"HTTP/1.1 {status} Result\r\nContent-Length: {}\r\nContent-Type: application/json\r\nLocation: http://example.invalid/inference\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
        requests
    });
    (endpoint, join)
}

#[test]
fn actual_transport_rejects_redirects_remote_weights_and_truncated_responses() {
    for responses in [
        vec![(302, serde_json::json!({}))],
        vec![(200, serde_json::json!({"remote_host":"example.invalid"}))],
        vec![
            (200, serde_json::json!({"details":{"format":"gguf"}})),
            (
                200,
                serde_json::json!({"done":true,"done_reason":"length","message":{"content":"{\"status\":\"actions\",\"actions\":[{\"kind\":\"mute\"}]}"}}),
            ),
        ],
    ] {
        let expected = responses.len();
        let (endpoint, server) = stub(responses);
        let config = Configuration {
            endpoint,
            ..Default::default()
        };
        assert!(
            Interpreter::new(config)
                .unwrap()
                .interpret("mute", &CommandContext::default())
                .is_err()
        );
        assert_eq!(server.join().unwrap().len(), expected);
    }
}

#[test]
fn computer_transport_requires_local_vision_and_transmits_the_observed_image() {
    use fnpilot_core::{
        computer::{Screen, Step},
        runner::Target,
        session::{Phase, Session},
    };
    for metadata in [
        serde_json::json!({"capabilities":["completion"]}),
        serde_json::json!({"capabilities":["vision"],"remote_host":"remote.invalid"}),
        serde_json::json!({"capabilities":["vision"]}),
    ] {
        let supported =
            metadata.get("remote_host").is_none() && metadata["capabilities"][0] == "vision";
        let mut replies = vec![(200, metadata)];
        if supported {
            replies.push((200,serde_json::json!({"done":true,"done_reason":"stop","message":{"content":r#"{"kind":"click","x":100,"y":100}"#}})));
        }
        let (endpoint, server) = stub(replies);
        let session = Session::default();
        let token = session.begin(Phase::Interpreting).unwrap();
        let screen = Screen {
            id: 1,
            target: Target {
                platform: "fixture".into(),
                app_identity: "editor".into(),
                process_id: 1,
                process_start: 1,
                window_id: 1,
                generation: 1,
            },
            app_name: "Editor".into(),
            width: 1280,
            height: 800,
            image: "fixture-png".into(),
        };
        let interpreter = Interpreter::new(Configuration {
            endpoint,
            ..Default::default()
        })
        .unwrap();
        let result =
            interpreter.computer_step_session("new document", &screen, &[], &session, token);
        assert_eq!(result.is_ok(), supported);
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), if supported { 2 } else { 1 });
        if supported {
            assert_eq!(result.unwrap(), Step::Click { x: 100, y: 100 });
            assert_eq!(requests[1]["messages"][1]["images"][0], "fixture-png");
        }
    }
}
