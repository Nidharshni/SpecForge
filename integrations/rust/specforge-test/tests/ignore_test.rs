//! `#[ignore]` on a `#[specforge_test]` test is a real ignore: libtest
//! reports it as ignored and nothing is recorded, and `--include-ignored`
//! runs the body and records the result. Runs the `ignore_fixture` test
//! binary, which `cargo test` builds next to this one.

mod support;
use specforge_test_macros::test as specforge_test;

/// Run the fixture with `args`, reports going to a fresh directory.
/// Returns its stdout and the recorded entries.
fn run(args: &[&str]) -> (String, Vec<serde_json::Value>) {
    let reports = tempfile::tempdir().unwrap();
    let out = support::run_fixture(args, reports.path());
    let mut entries = Vec::new();
    for file in std::fs::read_dir(reports.path()).unwrap() {
        let path = file.unwrap().path();
        if path.extension().is_some_and(|e| e == "json") {
            let report: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            entries.extend(report["entries"].as_array().unwrap().iter().cloned());
        }
    }
    (String::from_utf8(out.stdout).unwrap(), entries)
}

#[specforge_test(
    behavior = "record_test_via_drop_guard",
    verify = "an ignored test runs and is recorded only when libtest is asked to run it"
)]
fn an_ignored_test_is_recorded_only_when_it_runs() {
    let (listed, _) = run(&["--list", "--ignored"]);
    assert!(listed.contains("ignored_by_default: test"), "{listed}");
    assert!(listed.contains("ignored_should_panic: test"), "{listed}");

    let (stdout, entries) = run(&[]);
    assert!(
        stdout.contains("test ignored_by_default ... ignored"),
        "{stdout}"
    );
    assert!(
        stdout.contains("test ignored_should_panic ... ignored"),
        "{stdout}"
    );
    assert!(
        entries.is_empty(),
        "ignored tests record nothing: {entries:?}"
    );

    let (stdout, entries) = run(&["--include-ignored"]);
    assert!(
        stdout.contains("test ignored_by_default ... ok"),
        "{stdout}"
    );
    assert!(
        stdout.contains("test ignored_should_panic - should panic ... ok"),
        "{stdout}"
    );
    let mut recorded: Vec<(String, String)> = entries
        .iter()
        .map(|e| {
            (
                e["test_name"].as_str().unwrap().to_string(),
                e["status"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    recorded.sort();
    assert_eq!(
        recorded,
        vec![
            ("ignored_by_default".to_string(), "passed".to_string()),
            ("ignored_should_panic".to_string(), "passed".to_string()),
        ]
    );
}
