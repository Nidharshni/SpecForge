use super::*;

/// Trimmed from a real vitest 5 JSON report (`--reporter=json`).
const REPORT: &str = include_str!("../fixtures/vitest-report.json");

fn try_run(content: &str) -> Result<CollectOutput, String> {
    collect(&CollectInput {
        reports: vec![CollectReportFile {
            path: ".specforge/reports/vitest.json".to_string(),
            content: content.to_string(),
        }],
        ..CollectInput::default()
    })
}

fn run(content: &str) -> CollectOutput {
    try_run(content).unwrap()
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
    assert_eq!(create[0].status, TestStatus::Passed);
    assert_eq!(
        create[0].verify.as_deref(),
        Some("rejects a duplicate email")
    );
    assert_eq!(create[1].status, TestStatus::Failed);

    // Suite metadata is inherited; the skipped test is kept as skipped.
    let delete = &out.entity_results[1].test_results;
    let statuses: Vec<TestStatus> = delete.iter().map(|t| t.status).collect();
    assert_eq!(statuses, vec![TestStatus::Passed, TestStatus::Skipped]);
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

#[test]
fn an_undocumented_status_on_a_linked_test_is_an_error() {
    let linked = |status: &str| {
        format!(
            r#"{{"testResults":[{{"assertionResults":[{{"fullName":"t","status":"{status}",
            "meta":{{"specforge":{{"behavior":"a"}}}}}}]}}]}}"#
        )
    };
    let err = try_run(&linked("flaky")).unwrap_err();
    assert_eq!(
        err,
        ".specforge/reports/vitest.json: test 't' has status 'flaky', which vitest's JSON reporter does not write"
    );
    // An absent status is no status vitest writes either.
    let nameless = r#"{"testResults":[{"assertionResults":[{"fullName":"t",
        "meta":{"specforge":{"behavior":"a"}}}]}]}"#;
    assert!(try_run(nameless).is_err());
    // pending, todo and disabled are vitest's spellings of not run.
    for skipped in ["pending", "todo", "disabled", "skipped"] {
        let out = run(&linked(skipped));
        assert_eq!(
            out.entity_results[0].test_results[0].status,
            TestStatus::Skipped,
            "{skipped}"
        );
    }
    // A test that links to nothing is ignored whatever its status.
    let unlinked =
        r#"{"testResults":[{"assertionResults":[{"fullName":"t","status":"flaky","meta":{}}]}]}"#;
    assert!(run(unlinked).entity_results.is_empty());
}
