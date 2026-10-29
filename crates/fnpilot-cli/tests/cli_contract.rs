use std::process::Command;
fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fnpilot-cli"))
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn machine_readable_nonexecuting_contract() {
    for (args, code) in [
        (vec!["parse", "volume up"], 0),
        (vec!["parse", "volume up", "--model-only"], 0),
        (vec!["parse", "open the app"], 2),
        (vec!["parse", "delete all files"], 3),
        (vec!["parse", "volume up", "--execute"], 64),
        (
            vec!["parse", "volume up", "--model", "a", "--model", "b"],
            64,
        ),
    ] {
        let out = run(&args);
        assert_eq!(out.status.code(), Some(code));
        if code < 4 {
            let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
            assert!(result.get("status").is_some());
            assert!(out.stderr.is_empty());
        } else {
            assert!(out.stdout.is_empty());
            assert!(!out.stderr.is_empty());
        }
    }
}
