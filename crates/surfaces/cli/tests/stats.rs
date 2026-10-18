use crate::support::{Cli, TestProject};
use specforge_project::testing::specs;
use specforge_test::prelude::*;

#[test]
fn stats_reports_entity_and_edge_counts() {
    let dir = TestProject::new().without_config().files(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" features [gamma] }
behavior beta "B" { contract "second" features [gamma] }
feature gamma "G" { problem "third" }
"#,
    )]);

    let output = Cli::on(&dir).command(&["stats"]).output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("3"), "should report 3 entities: {}", stdout);
    assert!(stdout.contains("2"), "should report 2 edges: {}", stdout);
}

#[test]
fn stats_on_empty_project() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", "")]);

    let output = Cli::on(&dir).command(&["stats"]).output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("0"), "should report 0 entities: {}", stdout);
}

#[test]
fn stats_json_format() {
    let dir = TestProject::new().without_config().files(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" features [gamma] }
feature gamma "G" { problem "third" }
"#,
    )]);

    let output = Cli::on(&dir)
        .command(&["stats", "--format=json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {}\noutput: {}", e, stdout));

    assert_eq!(parsed["entity_count"], 2);
    assert_eq!(parsed["edge_count"], 1);
}

// Coverage is over the kinds the enabled extensions declare testable:
// alpha and beta are behaviors, gamma is a feature (not testable).
#[specforge_test(
    behavior = "compute_project_statistics",
    verify = "stats reports coverage percentage"
)]
fn stats_reports_coverage_over_testable_kinds() {
    let dir = TestProject::new().without_config().files(&[
        (
            "specforge.json",
            r#"{"name":"test","version":"0.1.0","extensions":["@specforge/software","@specforge/testing"]}"#,
        ),
        (
            "main.spec",
            r#"
behavior alpha "A" { contract "first" verify unit "alpha works" features [gamma] }
behavior beta "B" { contract "second" features [gamma] }
feature gamma "G" { problem "third" }
"#,
        ),
    ]);

    let output = Cli::on(&dir)
        .command(&["stats", "--format=json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(parsed["testable_count"], 2, "{parsed}");
    assert_eq!(parsed["declared_pct"], 50.0, "{parsed}");

    let output = Cli::on(&dir).command(&["stats"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Declared: 50% of 2 testable"), "{stdout}");
}

#[test]
fn stats_human_format_lists_all_kinds() {
    let dir = TestProject::new().file("main.spec", specs::MULTI_EXTENSION);

    let output = Cli::on(&dir).run(&["stats"]);

    assert_eq!(output.code, 0, "{}", output.stderr);
    let stdout = &output.stdout;

    for kind in &[
        "behavior",
        "feature",
        "decision",
        "constraint",
        "failure_mode",
    ] {
        assert!(
            stdout.contains(kind),
            "human stats missing kind '{}': {}",
            kind,
            stdout
        );
    }
}
