// Every reader of the project's schema reports what versioning it against
// `.specforge/schema-cache.json` found (I016 for a record that cannot be read,
// one W053 per breaking change), as `specforge export` does (ADR 0111).

use crate::support::{Cli, TestProject};
use specforge_project::testing::specs;
use specforge_test::prelude::*;
use std::fs;

const SOFTWARE_ONLY: &str =
    r#"{"name":"t","version":"0.1.0","spec_root":"spec","extensions":["@specforge/software"]}"#;
const SOFTWARE_AND_GOVERNANCE: &str = r#"{"name":"t","version":"0.1.0","spec_root":"spec","extensions":["@specforge/software","@specforge/governance"]}"#;

fn software_project(config: &str) -> TestProject {
    TestProject::new()
        .raw_config(config)
        .file("spec/main.spec", specs::SOFTWARE)
}

/// The stderr of a command that must exit 0.
fn stderr_of(dir: &TestProject, args: &[&str]) -> String {
    let output = Cli::on(dir).command(args).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(output.status.success(), "{args:?} failed: {stderr}");
    stderr
}

/// A project that exported once, whose record was then corrupted.
fn project_with_unreadable_record() -> TestProject {
    let dir = software_project(SOFTWARE_ONLY);
    stderr_of(&dir, &["export"]);
    fs::write(
        dir.root().join(".specforge").join("schema-cache.json"),
        r#"{"schema": {"#,
    )
    .unwrap();
    dir
}

fn assert_reports_i016(dir: &TestProject, args: &[&str]) {
    let stderr = stderr_of(dir, args);
    let lines = stderr
        .lines()
        .filter(|l| l.starts_with("info[I016]: the schema cache "))
        .count();
    assert_eq!(lines, 1, "{args:?}: {stderr}");
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "every reader of the schema reports the notices export reports for the same record"
)]
fn schema_and_its_publication_report_an_unreadable_record() {
    let dir = project_with_unreadable_record();
    assert_reports_i016(&dir, &["schema"]);
    assert_reports_i016(&dir, &["schema", "--publish"]);
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "every reader of the schema reports the notices export reports for the same record"
)]
#[ignore = "flipped by T6"]
fn trace_and_model_report_an_unreadable_record() {
    let dir = project_with_unreadable_record();
    assert_reports_i016(&dir, &["trace", "parse_input"]);
    assert_reports_i016(&dir, &["model", "--format", "json"]);
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "every reader of the schema reports the notices export reports for the same record"
)]
fn query_reports_an_unreadable_record() {
    let dir = project_with_unreadable_record();
    assert_reports_i016(&dir, &["query", "parse_input", "--format", "graph"]);
}

fn w053_lines(dir: &TestProject, args: &[&str]) -> Vec<String> {
    stderr_of(dir, args)
        .lines()
        .filter(|l| l.starts_with("warning[W053]"))
        .map(str::to_owned)
        .collect()
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "every reader of the schema reports the notices export reports for the same record"
)]
#[ignore = "flipped by T6"]
fn every_reader_reports_a_breaking_change_as_export_does() {
    let dir = software_project(SOFTWARE_AND_GOVERNANCE);
    stderr_of(&dir, &["export"]);
    fs::write(dir.root().join("specforge.json"), SOFTWARE_ONLY).unwrap();

    let readers: [&[&str]; 5] = [
        &["schema"],
        &["schema", "--publish"],
        &["trace", "parse_input"],
        &["model"],
        &["query", "parse_input"],
    ];
    let reads: Vec<Vec<String>> = readers.iter().map(|args| w053_lines(&dir, args)).collect();
    // Export records, so it goes last.
    let exported = w053_lines(&dir, &["export"]);

    assert!(!exported.is_empty(), "export reported no W053");
    for (args, lines) in readers.iter().zip(&reads) {
        assert_eq!(lines, &exported, "{args:?}");
    }
}
