use fnpilot_core::{
    computer::{Screen, Step},
    runner::Target,
};
use fnpilot_runtime::{Configuration, OutputMode, Runtime, computer};
use serde_json::json;

fn screen() -> Screen {
    Screen {
        id: 1,
        target: Target {
            platform: "test".into(),
            app_identity: "editor".into(),
            process_id: 1,
            process_start: 1,
            window_id: 1,
            generation: 1,
        },
        app_name: "Untrusted editor text".into(),
        width: 1280,
        height: 800,
        image: "fixture-base64-png".into(),
    }
}
#[test]
fn screenshot_payloads_use_each_providers_image_protocol_and_one_step_schema() {
    for runtime in [Runtime::Ollama, Runtime::OpenAICompatible] {
        for output_mode in [
            OutputMode::JsonSchema,
            OutputMode::JsonObject,
            OutputMode::PromptJSON,
            OutputMode::ToolCalls,
        ] {
            let config = Configuration {
                runtime,
                output_mode,
                ..Default::default()
            };
            let request = computer::request(
                &config,
                "new document",
                &screen(),
                &[Step::Move { x: 30, y: 40 }],
            );
            let user = &request["messages"][1];
            if runtime == Runtime::Ollama {
                assert_eq!(user["images"][0], "fixture-base64-png");
                assert!(user["content"].as_str().unwrap().contains("new document"));
            } else {
                assert_eq!(
                    user["content"][1]["image_url"]["url"],
                    "data:image/png;base64,fixture-base64-png"
                );
                assert!(
                    user["content"][0]["text"]
                        .as_str()
                        .unwrap()
                        .contains("new document")
                );
            }
            assert!(request.get("tools").is_none());
            assert!(request.get("tool_choice").is_none());
            assert_eq!(request["stream"], false);
            let system = request["messages"][0]["content"].as_str().unwrap();
            assert!(system.contains("untrusted"));
            assert!(system.contains("ONE step"));
            if matches!(output_mode, OutputMode::JsonSchema | OutputMode::ToolCalls) {
                let schema = if runtime == Runtime::Ollama {
                    &request["format"]
                } else {
                    &request["response_format"]["json_schema"]["schema"]
                };
                assert!(
                    schema["oneOf"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|item| item["properties"]["kind"]["const"] == "click")
                );
            }
        }
    }
}
#[test]
fn incomplete_extra_or_unavailable_model_actions_never_decode_to_input() {
    for runtime in [Runtime::Ollama, Runtime::OpenAICompatible] {
        let config = Configuration {
            runtime,
            ..Default::default()
        };
        let message = json!({"content":r#"{"kind":"click","x":100,"y":200}"#});
        let mut response = if runtime == Runtime::Ollama {
            json!({"done":true,"done_reason":"stop","message":message})
        } else {
            json!({"choices":[{"finish_reason":"stop","message":message}]})
        };
        assert_eq!(
            computer::decode(&config, &response, &screen()).unwrap(),
            Step::Click { x: 128, y: 160 }
        );
        if runtime == Runtime::Ollama {
            response["done_reason"] = json!("length");
        } else {
            response["choices"][0]["finish_reason"] = json!("length");
        }
        assert!(computer::decode(&config, &response, &screen()).is_err());
    }
}

#[test]
fn model_coordinates_are_normalized_and_native_steps_use_image_pixels() {
    let config = Configuration::default();
    for kind in ["move", "click", "double_click", "right_click", "scroll"] {
        let mut step = json!({"kind":kind,"x":1000,"y":1000});
        if kind == "scroll" {
            step["direction"] = json!("down");
        }
        let response =
            json!({"done":true,"done_reason":"stop","message":{"content":step.to_string()}});
        assert_eq!(
            computer::decode(&config, &response, &screen())
                .unwrap()
                .point(),
            Some((1279, 799))
        );
    }
    let center = json!({"done":true,"done_reason":"stop","message":{"content":r#"{"kind":"click","x":500,"y":500}"#}});
    assert_eq!(
        computer::decode(&config, &center, &screen())
            .unwrap()
            .point(),
        Some((640, 400))
    );
    let bad = json!({"done":true,"done_reason":"stop","message":{"content":r#"{"kind":"click","x":1001,"y":500}"#}});
    assert!(computer::decode(&config, &bad, &screen()).is_err());
    let request = computer::request(
        &config,
        "new document",
        &screen(),
        &[Step::Click { x: 640, y: 400 }],
    );
    let content = request["messages"][1]["content"].as_str().unwrap();
    assert!(content.contains(r#""x":500"#));
    assert!(content.contains(r#""y":500"#));
    for item in request["format"]["oneOf"].as_array().unwrap() {
        if item["properties"].get("x").is_some() {
            assert_eq!(item["properties"]["x"]["maximum"], 1000);
        }
    }
}
