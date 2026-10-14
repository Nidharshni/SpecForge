//! The built-in `contracts` pass (`specforge analyze contracts`), run as the
//! analyze operation runs it.

use super::compiled_support::{TestProject, analyzed};
use specforge_ops::analysis::analyze::AnalyzeOptions;
use specforge_test_macros::test as specforge_test;

const SPEC: &str = "invariant inv \"I\" {\n  guarantee \"g\"\n  risk low\n}\n\n\
behavior alpha \"A\" {\n  contract \"a\"\n}\n\n\
behavior beta \"B\" {\n  contract \"b\"\n  invariants [inv]\n}\n";

#[specforge_test(
    behavior = "report_entities_without_contract_obligations",
    verify = "an entity of a contract-bearing kind that references no contract target is A010, an info"
)]
#[specforge_test(
    behavior = "report_entities_without_contract_obligations",
    verify = "the contracts pass's suggestion names the kind's contract fields, sorted"
)]
fn an_entity_without_obligations_is_a010() {
    let project = TestProject::new()
        .enabling(&["@specforge/software"])
        .file("main.spec", SPEC);
    let doc = analyzed(
        &project.compile(),
        &AnalyzeOptions {
            pass: "contracts".into(),
            ..Default::default()
        },
    );
    let pass = &doc["passes"][0];
    assert_eq!(pass["pass"], "contracts", "{doc}");
    let findings = pass["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1, "{doc}");
    assert_eq!(findings[0]["code"], "A010");
    assert_eq!(findings[0]["severity"], "Info");
    assert_eq!(
        findings[0]["message"],
        "behavior 'alpha' declares no contract obligations"
    );
    // With @specforge/software alone, `invariants` is a behavior's one contract field
    // (the CLI snapshot `tests__parity__analyze.snap` prints the same suggestion).
    assert_eq!(
        findings[0]["suggestion"],
        "add requires/ensures/maintains references (registered for 'behavior': invariants)"
    );
    assert_eq!(
        pass["summary"],
        serde_json::json!({
            "contract_entities": 1, "unconstrained_entities": 1, "obligation_references": 1 })
    );
}
