use super::*;

/// `cargo test --workspace --no-fail-fast` on a crate with a lib test
/// module, a nested module, an integration test and a doc test.
const LIBTEST: &str = include_str!("../tests/fixtures/libtest-stdout.txt");

fn file(path: &str, content: &str) -> CollectReportFile {
    CollectReportFile {
        path: path.to_string(),
        content: content.to_string(),
    }
}

#[test]
fn groups_entries_by_entity_across_binaries() {
    let input = CollectInput {
        reports: vec![
            file(
                "target/specforge/a.json",
                r#"{"schema_version":"1.0","binary_name":"a","entries":[
                        {"entity_kind":"behavior","entity_id":"login","test_name":"ok","file":"a.rs","verify":"accepts","duration_ms":3,"status":"pass"},
                        {"entity_kind":"behavior","entity_id":"login","test_name":"bad","file":"a.rs","duration_ms":1,"status":"fail"}]}"#,
            ),
            file(
                "target/specforge/b.json",
                r#"{"entries":[{"entity_kind":"type","entity_id":"user","test_name":"t","file":"b.rs","duration_ms":0,"status":"skipped"}]}"#,
            ),
            file("target/specforge/notes.json", r#"{"nodes":[]}"#),
            file("target/specforge/junk.json", "not json"),
        ],
        ..CollectInput::default()
    };
    let out = collect(&input).unwrap();
    let ids: Vec<&str> = out
        .entity_results
        .iter()
        .map(|e| e.entity_id.as_str())
        .collect();
    assert_eq!(ids, vec!["login", "user"]);
    let login = &out.entity_results[0].test_results;
    assert_eq!(login.len(), 2);
    assert_eq!(login[0].status, TestStatus::Passed);
    assert_eq!(login[0].verify.as_deref(), Some("accepts"));
    assert_eq!(login[1].status, TestStatus::Failed);
    assert_eq!(
        out.entity_results[1].test_results[0].status,
        TestStatus::Skipped
    );
    assert!(out.unlinked.is_empty());
}

#[test]
fn parses_libtest_output() {
    let parsed = parse_libtest(LIBTEST);
    assert_eq!(
        parsed,
        vec![
            ("tests::plain_ignored".to_string(), TestStatus::Skipped),
            ("tests::slow_one".to_string(), TestStatus::Skipped),
            (
                "tests::add_item::rejects_a_duplicate_item".to_string(),
                TestStatus::Passed
            ),
            (
                "tests::add_item__rejects_an_empty_name".to_string(),
                TestStatus::Passed
            ),
            ("tests::panics".to_string(), TestStatus::Passed),
            ("tests::fails".to_string(), TestStatus::Failed),
            ("cart__is_empty_at_first".to_string(), TestStatus::Passed),
        ],
        "doc tests and a failing test's replayed output are skipped"
    );
    let coloured = "test \u{1b}[32ma\u{1b}[0m ... \u{1b}[32mok\u{1b}[0m\r\n";
    assert_eq!(
        parse_libtest(coloured),
        vec![("a".to_string(), TestStatus::Passed)]
    );
    assert!(parse_libtest("test b ... bench:  10 ns/iter (+/- 1)").is_empty());
}

#[test]
fn reports_tests_the_attribute_did_not_record_as_unlinked() {
    let report = r#"{"entries":[
            {"entity_id":"cart","test_name":"rejects_a_duplicate_item","module_path":"shop_lib::tests::add_item","file":"src/lib.rs","status":"pass"},
            {"entity_id":"cart","test_name":"panics","file":"src/lib.rs","status":"pass"}]}"#;
    let out = collect(&CollectInput {
        reports: vec![file("target/specforge/shop_lib.json", report)],
        stdout: Some(LIBTEST.to_string()),
    })
    .unwrap();
    let names: Vec<&str> = out.unlinked.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "tests::plain_ignored",
            "tests::slow_one",
            "tests::add_item__rejects_an_empty_name",
            "tests::fails",
            "cart__is_empty_at_first",
        ],
        "recorded by module path, or by name for an old report"
    );
    assert_eq!(
        out.unlinked[2].path,
        vec!["tests", "add_item__rejects_an_empty_name"]
    );
    assert_eq!(out.unlinked[3].status, TestStatus::Failed);
}

#[test]
fn reads_libtest_output_passed_as_a_report_file() {
    let out = collect(&CollectInput {
        reports: vec![file("out.txt", LIBTEST)],
        ..CollectInput::default()
    })
    .unwrap();
    assert_eq!(out.unlinked.len(), 7);
}

#[test]
fn an_entry_whose_status_is_not_written_by_specforge_test_is_an_error() {
    let report = r#"{"entries":[{"entity_id":"login","test_name":"flaky","status":"flaky"}]}"#;
    let err = collect(&CollectInput {
        reports: vec![file("target/specforge/a.json", report)],
        ..CollectInput::default()
    })
    .unwrap_err();
    assert_eq!(
        err,
        "target/specforge/a.json: entry for 'flaky' has status 'flaky', which specforge-test does not write"
    );
    // An entries array that does not decode is the collector's error too.
    let undecodable = r#"{"entries":[{"test_name":"nameless_entity"}]}"#;
    let err = collect(&CollectInput {
        reports: vec![file("target/specforge/b.json", undecodable)],
        ..CollectInput::default()
    })
    .unwrap_err();
    assert!(err.starts_with("target/specforge/b.json: "), "{err}");
}
