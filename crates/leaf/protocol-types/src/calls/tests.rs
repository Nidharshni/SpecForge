use super::*;

#[test]
fn a_pass_is_called_at_its_prefixed_export() {
    assert_eq!(pass_export("audit"), "__pass_audit");
    assert_eq!(pass_export("a-b_c"), "__pass_a-b_c");
}

#[specforge_test_macros::test(
    invariant = "collector_output_conformance",
    verify = "a test status is passed, failed or skipped, and no other value decodes"
)]
fn a_test_status_is_passed_failed_or_skipped() {
    for (text, status) in [
        ("passed", TestStatus::Passed),
        ("failed", TestStatus::Failed),
        ("skipped", TestStatus::Skipped),
    ] {
        let json = serde_json::json!(text);
        assert_eq!(serde_json::from_value::<TestStatus>(json).unwrap(), status);
        assert_eq!(serde_json::to_value(status).unwrap(), text);
    }
    for other in ["pass", "ok", "Passed", ""] {
        assert!(
            serde_json::from_value::<TestStatus>(serde_json::json!(other)).is_err(),
            "{other:?} must not decode"
        );
    }
    let nameless = serde_json::from_value::<TestResult>(serde_json::json!({"status": "passed"}));
    assert!(
        nameless
            .unwrap_err()
            .to_string()
            .contains("missing field `name`")
    );
    assert!(TestResults::default().of("x").is_empty());
}
