use fnpilot_core::{Interpretation, capabilities, context::CommandContext, direct_ui, parser};
use serde_json::{Value, json};

#[test]
fn frozen_swift_contracts() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("../../../fixtures/contracts/parser.json")).unwrap();
    for case in cases {
        let input = case["input"].as_str().unwrap();
        let context: CommandContext = serde_json::from_value(case["context"].clone()).unwrap();
        let direct = parser::parse(input);
        let expected: Option<Interpretation> =
            serde_json::from_value(case["direct"].clone()).unwrap();
        assert_eq!(direct, expected, "direct: {input}");
        let expected: Option<Interpretation> =
            serde_json::from_value(case["routed"].clone()).unwrap();
        assert_eq!(
            direct.or_else(|| direct_ui::parse(input, &context)),
            expected,
            "routed: {input}"
        );
        assert_eq!(json!(parser::parts(input)), case["parts"], "parts: {input}");
        assert_eq!(
            json!(capabilities::kinds(input)),
            case["capabilities"],
            "capabilities: {input}"
        );
    }
}
