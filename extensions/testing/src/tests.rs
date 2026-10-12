use super::*;
use serde_json::{json, Value};

/// The rule's golden vectors (`crates/specforge-coverage`), run through
/// this pass: the Wasm side reports what the crate's own tests expect.
#[test]
fn the_pass_matches_the_shared_golden_vectors() {
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../../../crates/specforge-coverage/tests/fixtures/coverage-cases.json"
    ))
    .unwrap();
    assert!(!cases.is_empty());
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let expect = &case["expect"];
        // As the pass reads the snapshot: the SDK's input, each entity
        // carrying whether the host found it exempt.
        let input: PassInput = serde_json::from_value(case["input"].clone()).unwrap();
        let out = pass_coverage(&input);
        assert_eq!(
            Value::Object(out.summary),
            expect["summary"],
            "summary of {name:?}"
        );
        let findings: Vec<Value> = out
            .diagnostics
            .iter()
            .map(|d| {
                let severity = match d.severity {
                    PassSeverity::Error => "error",
                    PassSeverity::Warning => "warning",
                    PassSeverity::Info => "info",
                };
                json!({
                    "code": d.code,
                    "severity": severity,
                    "message": d.message,
                    "suggestion": d.suggestion,
                })
            })
            .collect();
        let expected: Vec<Value> = expect["findings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                let mut f = f.clone();
                f.as_object_mut().unwrap().remove("entity");
                f
            })
            .collect();
        assert_eq!(findings, expected, "findings of {name:?}");
    }
}

#[test]
fn findings_carry_their_entity_span() {
    let out = pass_coverage(
            &serde_json::from_value(json!({
                "entities": [
                    {"id": "b", "kind": "type", "testable": true,
                     "span": {"file": "b.spec", "start_line": 3, "start_col": 1, "end_line": 3, "end_col": 9}},
                    {"id": "a", "kind": "type", "testable": true,
                     "span": {"file": "a.spec", "start_line": 1, "start_col": 1, "end_line": 1, "end_col": 9}}
                ]
            }))
            .unwrap(),
        );
    let files: Vec<&str> = out
        .diagnostics
        .iter()
        .map(|d| d.span.as_ref().unwrap().file.as_str())
        .collect();
    assert_eq!(files, ["a.spec", "b.spec"]);
}

/// The pass reads each entity's exemption from the snapshot: an exempt
/// testable entity that declares nothing is not reported (A001).
#[test]
fn the_pass_reads_the_exemption_from_each_entity() {
    let input = |exempt: bool| -> PassInput {
        serde_json::from_value(json!({"entities": [
            {"id": "u", "kind": "type", "testable": true, "owes_none": exempt}
        ]}))
        .unwrap()
    };
    let codes =
        |out: PassOutput| -> Vec<String> { out.diagnostics.into_iter().map(|d| d.code).collect() };
    assert_eq!(codes(pass_coverage(&input(false))), ["A001"]);
    assert!(codes(pass_coverage(&input(true))).is_empty());
}
