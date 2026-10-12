use super::*;

/// Trimmed from a real vitest 5 JSON report (`--reporter=json`).
const REPORT: &str = include_str!("../fixtures/vitest-report.json");

fn run(content: &str) -> CollectOutput {
    collect(&CollectInput {
        reports: vec![CollectReportFile {
            path: ".specforge/reports/vitest.json".to_string(),
            content: content.to_string(),
        }],
        ..CollectInput::default()
    })
}

#[test]
fn maps_linked_tests_to_entities() {
    let out = run(REPORT);
    let ids: Vec<&str> = out
        .entity_results
        .iter()
        .map(|e| e.entity_id.as_str())
        .collect();
    assert_eq!(ids, vec!["create_user", "delete_user", "unique_ids"]);

    let create = &out.entity_results[0].test_results;
    assert_eq!(create.len(), 2);
    assert_eq!(create[0].name, "rejects a duplicate email");
    assert_eq!(create[0].status, "passed");
    assert_eq!(
        create[0].verify.as_deref(),
        Some("rejects a duplicate email")
    );
    assert_eq!(create[1].status, "failed");

    // Suite metadata is inherited; the skipped test is kept as skipped.
    let delete = &out.entity_results[1].test_results;
    let statuses: Vec<&str> = delete.iter().map(|t| t.status.as_str()).collect();
    assert_eq!(statuses, vec!["passed", "skipped"]);
    assert_eq!(delete[0].name, "suite meta inherits");
}

#[test]
fn a_list_links_one_test_to_several_entities() {
    let report = r#"{"testResults":[{"assertionResults":[{"fullName":"t","status":"passed",
            "meta":{"specforge":[{"behavior":"a","verify":"x"},{"invariant":"b"}]}}]}]}"#;
    let out = run(report);
    let ids: Vec<&str> = out
        .entity_results
        .iter()
        .map(|e| e.entity_id.as_str())
        .collect();
    assert_eq!(ids, vec!["a", "b"]);
    assert_eq!(
        out.entity_results[0].test_results[0].verify.as_deref(),
        Some("x")
    );
    assert_eq!(out.entity_results[1].test_results[0].verify, None);
}

#[test]
fn unlinked_tests_and_unreadable_reports_prove_nothing() {
    let report =
        r#"{"testResults":[{"assertionResults":[{"fullName":"t","status":"passed","meta":{}}]}]}"#;
    assert!(run(report).entity_results.is_empty());
    assert!(run("not json").entity_results.is_empty());
}
