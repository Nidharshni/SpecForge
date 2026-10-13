use assert_cmd::Command;
use specforge_test::prelude::*;
use std::fs;
use tempfile::TempDir;

fn setup_project(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (path, content) in files {
        let full = dir.path().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
    }
    dir
}

fn specforge_cmd() -> Command {
    assert_cmd::cargo_bin_cmd!("specforge")
}

#[test]
fn stats_reports_entity_and_edge_counts() {
    let dir = setup_project(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" }
behavior beta "B" { contract "second" }
feature gamma "G" { behaviors [alpha, beta] }
"#,
    )]);

    let output = specforge_cmd()
        .arg("stats")
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("3"), "should report 3 entities: {}", stdout);
    assert!(stdout.contains("2"), "should report 2 edges: {}", stdout);
}

#[test]
fn stats_on_empty_project() {
    let dir = setup_project(&[("main.spec", "")]);

    let output = specforge_cmd()
        .arg("stats")
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("0"), "should report 0 entities: {}", stdout);
}

#[test]
fn stats_json_format() {
    let dir = setup_project(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" }
feature gamma "G" { behaviors [alpha] }
"#,
    )]);

    let output = specforge_cmd()
        .args(["stats", "--format=json"])
        .arg("--path")
        .arg(dir.path())
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
    let dir = setup_project(&[
        (
            "specforge.json",
            r#"{"name":"test","version":"0.1.0","extensions":["@specforge/software","@specforge/testing"]}"#,
        ),
        (
            "main.spec",
            r#"
behavior alpha "A" { contract "first" verify unit "alpha works" }
behavior beta "B" { contract "second" }
feature gamma "G" { behaviors [alpha, beta] }
"#,
        ),
    ]);

    let output = specforge_cmd()
        .args(["stats", "--format=json"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(parsed["testable_count"], 2, "{parsed}");
    assert_eq!(parsed["declared_pct"], 50.0, "{parsed}");

    let output = specforge_cmd()
        .arg("stats")
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Declared: 50% of 2 testable"), "{stdout}");
}
