use super::*;
use specforge_extension_sdk::PassSeverity;

fn input(json: serde_json::Value) -> PassInput {
    serde_json::from_value(json).unwrap()
}

#[test]
fn coverage_tracking_lists_what_the_coverage_rule_does_not_prove() {
    let input = input(serde_json::json!({
        "entities": [
            // An invariant (always an item) nothing proves.
            {"id": "inv1", "kind": "invariant", "verify_kinds": ["unit"], "verify_texts": ["holds"]},
            // Proven by a passing test that names its obligation.
            {"id": "b1", "kind": "behavior", "testable": true,
             "verify_kinds": ["unit"], "verify_texts": ["works"]},
            // A union type: exempt, not an item.
            {"id": "Status", "kind": "type", "testable": true, "owes_none": true},
            // A feature, linked by a test: not testable, not an item.
            {"id": "feat1", "kind": "feature",
             "fields": {"tests": "tests/x.rs"}}
        ],
        "test_results": {"results": {
            "b1": {"tests": [{"name": "t", "status": "pass", "verify": "works"}]},
            "feat1": {"tests": [{"name": "u", "status": "pass"}]}
        }}
    }));
    let all = pass_coverage_tracking(&input);
    let i008: Vec<&str> = all
        .iter()
        .filter(|f| f.code == "I008")
        .map(|f| f.message.as_str())
        .collect();
    assert_eq!(
        i008,
        ["behavior 'b1': all 1 obligation(s) proven by recorded test(s): t"],
        "the proven behavior names its test"
    );
    let findings: Vec<&PassDiagnostic> = all.iter().filter(|f| f.code == "W035").collect();
    assert_eq!(findings.len(), 1, "one aggregated W035");
    assert!(matches!(findings[0].severity, PassSeverity::Warning));
    assert_eq!(
        findings[0].message,
        "1 coverage item(s) are not proven by a recorded test or an entailed claim: inv1"
    );
    assert!(
        !findings[0]
            .suggestion
            .as_deref()
            .unwrap()
            .contains("tests ["),
        "never suggests the retired `tests` field"
    );
}

#[test]
fn coverage_tracking_silent_when_everything_is_proven() {
    let input = input(serde_json::json!({
        "entities": [{"id": "inv1", "kind": "invariant",
                      "verify_kinds": ["property"], "verify_texts": ["holds"]}],
        "test_results": {"results": {}},
        "proved_claims": ["inv1"]
    }));
    assert!(pass_coverage_tracking(&input).is_empty());
}
