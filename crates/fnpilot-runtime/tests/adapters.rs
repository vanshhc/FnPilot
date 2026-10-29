use fnpilot_core::context::CommandContext;
use fnpilot_runtime::{Configuration, OutputMode, Runtime, decode, endpoint};
use serde_json::json;

#[test]
fn endpoints_are_loopback_without_proxy_redirect_or_dns_authority() {
    for endpoint in [
        "https://api.example.com",
        "http://192.168.1.2",
        "http://127.0.0.1.evil.test",
        "http://user:pass@localhost",
        "http://localhost?remote=true",
        "http://localhost/#x",
    ] {
        assert!(
            endpoint::local_url(endpoint, "local", "api/chat").is_err(),
            "{endpoint}"
        );
    }
    assert_eq!(
        endpoint::local_url("http://localhost:11434", "local", "api/chat")
            .unwrap()
            .host_str(),
        Some("127.0.0.1")
    );
    assert!(endpoint::local_url("http://127.0.0.1:11434", "model:cloud", "api/chat").is_err());
}
#[test]
fn providers_and_formats_produce_identical_validated_actions() {
    for runtime in [Runtime::Ollama, Runtime::OpenAICompatible] {
        for mode in [
            OutputMode::JsonSchema,
            OutputMode::JsonObject,
            OutputMode::PromptJSON,
            OutputMode::ToolCalls,
        ] {
            let config = Configuration {
                runtime,
                output_mode: mode,
                ..Default::default()
            };
            let message = if mode == OutputMode::ToolCalls {
                json!({"tool_calls":[{"function":{"name":"set_volume","arguments":{"value":37}}}]})
            } else {
                json!({"content":"{\"status\":\"actions\",\"actions\":[{\"kind\":\"set_volume\",\"value\":37}]}"})
            };
            let object = if runtime == Runtime::Ollama {
                json!({"done":true,"done_reason":"stop","message":message})
            } else {
                json!({"choices":[{"finish_reason":"stop","message":message}]})
            };
            let result = decode::decode(&config, &object).unwrap();
            assert_eq!(result.actions[0].value, Some(37.0));
            let mut truncated = object;
            if runtime == Runtime::Ollama {
                truncated["done_reason"] = json!("length");
            } else {
                truncated["choices"][0]["finish_reason"] = json!("length");
            }
            assert!(decode::decode(&config, &truncated).is_err());
            let request = config.request("set volume to 37", &CommandContext::default());
            assert_eq!(request["stream"], false);
            assert_eq!(request.get("options").is_some(), runtime == Runtime::Ollama);
        }
    }
}

#[test]
fn malformed_clarification_arguments_fail_without_panicking() {
    let config = Configuration {
        output_mode: OutputMode::ToolCalls,
        ..Default::default()
    };
    let response = json!({"done":true,"done_reason":"stop","message":{"tool_calls":[{"function":{"name":"clarify","arguments":{"unexpected":"Which?"}}}]}});
    assert!(decode::decode(&config, &response).is_err());
}

#[test]
fn model_tools_and_decoding_do_not_require_command_keywords() {
    use fnpilot_core::{action, context::UIControl};
    let context = CommandContext {
        controls_complete: Some(true),
        controls: vec![UIControl {
            id: "draft".into(),
            role: "AXTextField".into(),
            label: "Address".into(),
            focused: true,
            value: Some("https://example.org".into()),
        }],
        ..Default::default()
    };
    for runtime in [Runtime::Ollama, Runtime::OpenAICompatible] {
        let config = Configuration {
            runtime,
            output_mode: OutputMode::ToolCalls,
            ..Default::default()
        };
        for phrase in ["send it", "go there", "that's ready, let them know"] {
            let request = config.request(phrase, &context);
            let tools = request["tools"].as_array().unwrap();
            for kind in action::KINDS
                .iter()
                .filter(|kind| !fnpilot_core::browser::KINDS.contains(kind))
            {
                assert!(tools.iter().any(|tool| tool["function"]["name"] == *kind));
            }
            let content = request["messages"][1]["content"].as_str().unwrap();
            assert!(content.contains("https://example.org"));
            assert!(content.contains("focused"));
            for name in ["press_control", "submit_control", "execute_shell"] {
                let message = json!({"tool_calls":[{"function":{"name":name,"arguments":{"controlID":"draft"}}}]});
                let response = if runtime == Runtime::Ollama {
                    json!({"done":true,"done_reason":"stop","message":message})
                } else {
                    json!({"choices":[{"finish_reason":"tool_calls","message":message}]})
                };
                let result = decode::decode(&config, &response);
                assert_eq!(result.is_ok(), name != "execute_shell");
            }
        }
    }
}

#[test]
fn browser_group_is_scoped_by_context_in_tools_and_schema_for_both_providers() {
    use fnpilot_core::browser;
    for runtime in [Runtime::Ollama, Runtime::OpenAICompatible] {
        for mode in [
            OutputMode::ToolCalls,
            OutputMode::JsonSchema,
            OutputMode::JsonObject,
            OutputMode::PromptJSON,
        ] {
            let config = Configuration {
                runtime,
                output_mode: mode,
                ..Default::default()
            };
            for enabled in [false, true] {
                let context = CommandContext {
                    browser_actions: if enabled {
                        browser::KINDS.iter().map(|k| (*k).into()).collect()
                    } else {
                        vec![]
                    },
                    ..Default::default()
                };
                let request = config.request("take me to example.org in a fresh tab", &context);
                let offered = match mode {
                    OutputMode::ToolCalls => Some(request["tools"].to_string()),
                    OutputMode::JsonSchema if runtime == Runtime::Ollama => {
                        Some(request["format"].to_string())
                    }
                    OutputMode::JsonSchema => {
                        Some(request["response_format"]["json_schema"]["schema"].to_string())
                    }
                    _ => None,
                };
                if let Some(offered) = offered {
                    for kind in browser::KINDS {
                        assert_eq!(offered.contains(kind), enabled, "{kind}");
                    }
                }
                // This is a canned response: it verifies protocol plumbing, not model accuracy.
                if enabled {
                    let message = if mode == OutputMode::ToolCalls {
                        json!({"tool_calls":[{"function":{"name":"browser_new_tab","arguments":{}}},{"function":{"name":"browser_navigate","arguments":{"text":"example.org"}}}]})
                    } else {
                        json!({"content":r#"{"status":"actions","actions":[{"kind":"browser_new_tab"},{"kind":"browser_navigate","text":"example.org"}]}"#})
                    };
                    let response = if runtime == Runtime::Ollama {
                        json!({"done":true,"done_reason":"stop","message":message})
                    } else {
                        json!({"choices":[{"finish_reason":"stop","message":message}]})
                    };
                    let result = decode::decode(&config, &response).unwrap();
                    assert_eq!(result.actions.len(), 2);
                    assert_eq!(result.actions[1].text.as_deref(), Some("example.org"));
                }
            }
        }
    }
}
