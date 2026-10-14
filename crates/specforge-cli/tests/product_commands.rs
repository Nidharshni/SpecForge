//! The `specforge product <command>` commands: `@specforge/product`'s
//! `cmd__product_*` exports, which the CLI routes to from the commands the
//! extension declares (ADR 0008). Output, flags and names are the ones the
//! built-in `product` subcommands had.

use crate::support::{Cli, TestProject};
use specforge_project::testing::specs;
use specforge_test_macros::test as specforge_test;

pub fn setup_product_project() -> TestProject {
    TestProject::new()
        .config(|c| c["name"] = "test-project".into())
        .enabling(&["@specforge/product"])
        .file("spec.spec", specs::PRODUCT_COMMANDS)
}

#[test]
fn test_product_features_json() {
    let dir = setup_product_project();
    let mut cmd = Cli::on(&dir).command(&["product", "features", "--format", "json"]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["total"], 2);
}

#[test]
fn test_product_features_filter_status() {
    let dir = setup_product_project();
    let mut cmd = Cli::on(&dir).command(&[
        "product", "features", "--status", "proposed", "--format", "json",
    ]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["total"], 1);
    assert_eq!(result["features"][0]["id"], "f1");
}

#[test]
fn test_product_milestones_json() {
    let dir = setup_product_project();
    let mut cmd = Cli::on(&dir).command(&["product", "milestones", "--format", "json"]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["total"], 1);
}

#[test]
fn test_product_milestone_completion() {
    let dir = setup_product_project();
    let mut cmd =
        Cli::on(&dir).command(&["product", "milestone-completion", "m1", "--format", "json"]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["milestone_id"], "m1");
    assert_eq!(result["total_features"], 2);
    assert_eq!(result["done_count"], 1); // f2 is done
    assert_eq!(result["done_features"], serde_json::json!(["f2"]));
    assert_eq!(result["completion_ratio"], 0.5);
}

#[test]
fn deliverable_completion_takes_details_as_a_flag() {
    let dir = setup_product_project();
    let mut cmd = Cli::on(&dir).command(&[
        "product",
        "deliverable-completion",
        "d1",
        "--details",
        "--format",
        "json",
    ]);
    let output = cmd.output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // m1 is planned, so none of d1's one milestone is completed.
    assert_eq!(
        (
            result["milestone_count"].clone(),
            result["completed_count"].clone()
        ),
        (serde_json::json!(1), serde_json::json!(0))
    );
    assert_eq!(result["milestone_details"][0]["milestone_id"], "m1");
    assert_eq!(result["milestone_details"][0]["done_count"], 1);
}

#[test]
fn test_product_feature_impact() {
    let dir = setup_product_project();
    let mut cmd = Cli::on(&dir).command(&["product", "feature-impact", "f1", "--format", "json"]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["feature_id"], "f1");
    assert!(!result["affected_journeys"].as_array().unwrap().is_empty());
    assert!(!result["affected_milestones"].as_array().unwrap().is_empty());
    assert!(!result["affected_modules"].as_array().unwrap().is_empty());
}

#[specforge_test(
    behavior = "pe_query_feature_impact",
    verify = "a feature that only relates to the feature is not a dependent"
)]
fn a_related_feature_is_not_a_dependent_in_the_impact() {
    let dir = setup_product_project();
    // f3 relates to f1 (`features`); f2 depends on it (`depends_on`).
    dir.write(
        "related.spec",
        "feature f3 \"Related Feature\" {\n    status proposed\n    features [f1]\n}\n",
    );
    let output = Cli::on(&dir)
        .command(&["product", "feature-impact", "f1", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        result["dependent_features"],
        serde_json::json!(["f2"]),
        "{result}"
    );
}

#[test]
fn test_product_feature_dependents() {
    let dir = setup_product_project();
    let mut cmd =
        Cli::on(&dir).command(&["product", "feature-dependents", "f1", "--format", "json"]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    // f2 depends on f1.
    assert_eq!(
        result,
        serde_json::json!({"feature_id": "f1", "dependents": ["f2"], "count": 1})
    );
}

#[test]
fn test_product_health() {
    let dir = setup_product_project();
    let mut cmd = Cli::on(&dir).command(&["product", "health", "--format", "json"]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(result["score"]["overall"].as_f64().unwrap() > 0.0);
    assert!(!result["entity_counts"].as_array().unwrap().is_empty());
}

#[test]
fn test_product_modules_json() {
    let dir = setup_product_project();
    let mut cmd = Cli::on(&dir).command(&["product", "modules", "--format", "json"]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["total"], 1);
}

#[test]
fn test_product_terms_json() {
    let dir = setup_product_project();
    let mut cmd = Cli::on(&dir).command(&["product", "terms", "--format", "json"]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["total"], 1);
}

#[test]
fn test_product_releases_json() {
    let dir = setup_product_project();
    let mut cmd = Cli::on(&dir).command(&["product", "releases", "--format", "json"]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["total"], 1);
}

#[test]
fn test_product_personas_json() {
    let dir = setup_product_project();
    let mut cmd = Cli::on(&dir).command(&["product", "personas", "--format", "json"]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["total"], 1);
}

#[test]
fn test_product_channels_json() {
    let dir = setup_product_project();
    let mut cmd = Cli::on(&dir).command(&["product", "channels", "--format", "json"]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["total"], 1);
}

#[test]
fn test_product_journey_coverage() {
    let dir = setup_product_project();
    let mut cmd = Cli::on(&dir).command(&["product", "journey-coverage", "j1", "--format", "json"]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["journey_id"], "j1");
    assert_eq!(result["total_features"], 2);
    assert_eq!(result["covered_count"], 1); // f2 is done
    assert_eq!(result["uncovered_features"], serde_json::json!(["f1"]));
}

#[test]
fn test_product_persona_features() {
    let dir = setup_product_project();
    let mut cmd =
        Cli::on(&dir).command(&["product", "persona-features", "dev", "--format", "json"]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    // f1 and f2 via journey j1
    assert_eq!(result["features"], serde_json::json!(["f1", "f2"]));
    assert_eq!(result["via_journey_ids"], serde_json::json!(["j1"]));
    assert_eq!(result["count"], 2);
}

#[test]
fn test_product_channel_features() {
    let dir = setup_product_project();
    let mut cmd =
        Cli::on(&dir).command(&["product", "channel-features", "cli", "--format", "json"]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    // f1 and f2 via journey j1
    assert_eq!(result["channel_id"], "cli");
    assert_eq!(result["features"], serde_json::json!(["f1", "f2"]));
    assert_eq!(result["count"], 2);
}

#[test]
fn test_product_bulk_status() {
    let dir = setup_product_project();
    let mut cmd = Cli::on(&dir).command(&["product", "bulk-status", "--format", "json"]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let kinds: Vec<&str> = result["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "feature",
            "milestone",
            "deliverable",
            "persona",
            "channel",
            "release"
        ]
    );
}

#[test]
fn test_product_nonexistent_milestone_exits_one() {
    let dir = setup_product_project();
    let mut cmd = Cli::on(&dir).command(&["product", "milestone-completion", "nonexistent"]);
    cmd.assert().failure();
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "exit code, stdout, stderr returned to CLI"
)]
fn an_extension_command_prints_what_its_export_returns() {
    let dir = setup_product_project();
    let output = Cli::on(&dir)
        .command(&["product", "milestone-completion", "m1"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Milestone: m1 (planned)\nCompletion: 50% (1/2 features done)\n\
         Evidence:   none recorded (run `specforge collect`)\n  f1 [proposed]\n  f2 [done]\n"
    );
    assert!(output.stderr.is_empty(), "{output:?}");

    let output = Cli::on(&dir)
        .command(&["product", "milestone-completion", "nope"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "error: milestone 'nope' not found\n"
    );
}

#[test]
fn an_extension_command_has_the_declared_command_line() {
    let dir = setup_product_project();
    // `ext:command` names the same command as `ext command`.
    let output = Cli::on(&dir)
        .command(&["product:features", "--limit", "1"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "id  title         status    priority\nf1  Core Feature  proposed  high\n1 of 2 features; --offset 1 for more\n"
    );
    // The declared enum refuses other values; an undeclared flag is refused.
    for args in [
        ["product", "features", "--format", "xml"],
        ["product", "journeys", "--status", "done"],
    ] {
        let output = Cli::on(&dir).command(&args).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
    }
    // A name no built-in command or extension has is refused.
    let output = Cli::on(&dir)
        .command(&["nonesuch", "features"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unrecognized subcommand 'nonesuch'"),
        "{output:?}"
    );
}

#[specforge_test(
    behavior = "surface_list_features",
    verify = "pagination offset and limit are respected"
)]
fn the_features_command_pages_after_counting() {
    let dir = setup_product_project();
    let output = Cli::on(&dir)
        .command(&[
            "product", "features", "--offset", "1", "--limit", "1", "--format", "json",
        ])
        .output()
        .unwrap();
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["total"], 2);
    let ids: Vec<&str> = result["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["f2"]);
}

#[test]
fn completions_include_the_commands_of_the_project_here() {
    let dir = setup_product_project();
    let output = Cli::on(&dir)
        .bare_command(&["completions", "bash"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let script = String::from_utf8_lossy(&output.stdout);
    assert!(
        script.contains("specforge__subcmd__product__subcmd__milestone__subcmd__completion"),
        "the product commands are completed"
    );

    // Outside a project, the built-ins only.
    let empty = TestProject::new().without_config();
    let output = Cli::on(&empty)
        .bare_command(&["completions", "bash"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let script = String::from_utf8_lossy(&output.stdout);
    assert!(script.contains("specforge__subcmd__check"));
    assert!(!script.contains("specforge__subcmd__product"));

    // `--path` names the project from anywhere.
    let output = Cli::on(&dir)
        .command(&["completions", "bash"])
        .current_dir(empty.root())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let script = String::from_utf8_lossy(&output.stdout);
    assert!(
        script.contains("specforge__subcmd__product__subcmd__milestone__subcmd__completion"),
        "the product commands of the --path project are completed"
    );
}

#[specforge_test_macros::test(
    behavior = "locate_project_root",
    verify = "a command given a path that does not exist refuses it as file_not_found"
)]
fn completions_refuse_a_path_that_does_not_exist() {
    let empty = TestProject::new().without_config();
    let output = Cli::on(&empty)
        .bare_command(&["completions", "bash", "--path", "nope"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("error[file_not_found]: path not found: nope"),
        "{output:?}"
    );
}

#[specforge_test(
    behavior = "surface_error_handling",
    verify = "CLI errors go to stderr"
)]
fn a_command_that_cannot_answer_writes_its_error_to_stderr() {
    let dir = setup_product_project();
    let run = |args: &[&str]| {
        let mut argv = vec!["product"];
        argv.extend(args);
        Cli::on(&dir).command(&argv).output().unwrap()
    };
    // A typo of m1: the nearest milestone is suggested.
    let human = run(&["milestone-completion", "m2"]);
    assert_eq!(human.status.code(), Some(1));
    assert!(human.stdout.is_empty(), "{human:?}");
    assert_eq!(
        String::from_utf8_lossy(&human.stderr),
        "error: milestone 'm2' not found\ndid you mean 'm1'?\n"
    );
    let json = run(&["milestone-completion", "m2", "--format", "json"]);
    assert_eq!(json.status.code(), Some(1));
    assert!(json.stdout.is_empty(), "{json:?}");
    let error: serde_json::Value = serde_json::from_slice(&json.stderr).unwrap();
    assert_eq!(
        error,
        serde_json::json!({"code": "ENTITY_NOT_FOUND", "message": "milestone 'm2' not found",
            "entity_id": "m2", "suggestion": "m1"})
    );
    // An input the command refuses exits 2, as clap's usage errors do.
    let invalid = run(&["features", "--limit=-1", "--format", "json"]);
    assert_eq!(invalid.status.code(), Some(2), "{invalid:?}");
    assert!(invalid.stdout.is_empty(), "{invalid:?}");
    let error: serde_json::Value = serde_json::from_slice(&invalid.stderr).unwrap();
    assert_eq!(error["code"], "INVALID_INPUT", "{error}");
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "under --format json a usage error the command line catches is one INVALID_INPUT error object on stderr, exit 2"
)]
fn under_json_a_usage_error_is_an_invalid_input_object() {
    let dir = setup_product_project();
    let run = |args: &[&str]| {
        let mut argv = vec!["product"];
        argv.extend(args);
        Cli::on(&dir).command(&argv).output().unwrap()
    };
    let cases: [(&[&str], serde_json::Value); 5] = [
        (
            &["features", "--status", "bogus"],
            serde_json::json!({"code": "INVALID_INPUT",
                "message": "status must be one of proposed, accepted, in_progress, done, deferred, deprecated, got 'bogus'"}),
        ),
        (
            &["milestone-completion"],
            serde_json::json!({"code": "INVALID_INPUT",
                "message": "missing required arg 'milestone'"}),
        ),
        (
            &["features", "--statsu", "done"],
            serde_json::json!({"code": "INVALID_INPUT",
                "message": "unknown argument '--statsu'", "suggestion": "--status"}),
        ),
        (
            &["features", "--limit", "abc"],
            serde_json::json!({"code": "INVALID_INPUT",
                "message": "limit must be a non-negative integer, got 'abc'"}),
        ),
        // A count below its minimum is the command line's to refuse, as it
        // is MCP's: the export never runs.
        (
            &["features", "--limit", "-1"],
            serde_json::json!({"code": "INVALID_INPUT",
                "message": "limit must be a non-negative integer, got -1"}),
        ),
    ];
    for (args, expected) in &cases {
        // `--format json` before or after the bad arg, in either spelling.
        for (before, after) in [
            (vec!["--format", "json"], vec![]),
            (vec![], vec!["--format", "json"]),
            (vec![], vec!["--format=json"]),
        ] {
            let (command, rest) = args.split_first().unwrap();
            let mut argv = vec![*command];
            argv.extend(&before);
            argv.extend(rest);
            argv.extend(&after);
            let output = run(&argv);
            assert_eq!(output.status.code(), Some(2), "{argv:?}: {output:?}");
            assert!(output.stdout.is_empty(), "{argv:?}: {output:?}");
            let error: serde_json::Value = serde_json::from_slice(&output.stderr)
                .unwrap_or_else(|e| panic!("{argv:?}: {e}: {output:?}"));
            assert_eq!(&error, expected, "{argv:?}");
        }
        // Under human the error is clap's usage text.
        let output = run(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
        assert!(output.stdout.is_empty(), "{args:?}: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.starts_with("error: ") && stderr.contains("For more information, try '--help'."),
            "{args:?}: {stderr}"
        );
    }
    // Help is clap's whatever the format, exit 0.
    let help = run(&["features", "--help", "--format", "json"]);
    assert_eq!(help.status.code(), Some(0), "{help:?}");
    assert!(
        String::from_utf8_lossy(&help.stdout).contains("Usage: specforge product features"),
        "{help:?}"
    );
}

/// [`setup_product_project`] with `m1` due in 2000.
fn setup_overdue_project() -> TestProject {
    let dir = setup_product_project();
    let spec = dir.read("spec.spec");
    let spec = spec.replace(
        "    status planned\n    features [f1, f2]",
        "    status planned\n    target_date \"2000-01-01\"\n    features [f1, f2]",
    );
    assert!(spec.contains("2000-01-01"));
    dir.write("spec.spec", spec);
    dir
}

fn timeline(dir: &TestProject, extra: &[&str]) -> std::process::Output {
    let mut args = vec!["product", "milestone-timeline", "--format", "json"];
    args.extend(extra);
    Cli::on(dir).command(&args).output().unwrap()
}

#[specforge_test(
    behavior = "surface_milestone_timeline",
    verify = "as-of flag overrides current date for overdue calculation"
)]
fn the_timeline_compares_against_today_unless_as_of_says_otherwise() {
    let dir = setup_overdue_project();
    // The host passes today, long after 2000.
    let output = timeline(&dir, &[]);
    assert!(output.status.success(), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["milestones"][0]["milestone_id"], "m1");
    assert_eq!(result["milestones"][0]["is_overdue"], true);
    let output = timeline(&dir, &["--as-of", "1999-12-31"]);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["overdue_count"], 0);
    let output = timeline(&dir, &["--as-of", "31/12/1999"]);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "INVALID_INPUT");
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "the CommandInput carries what the recorded tests prove, per entity that counts toward coverage"
)]
fn milestone_completion_reads_the_projects_recorded_tests() {
    let dir = TestProject::new().raw_config(
        serde_json::json!({"name": "p", "version": "0.1.0",
            "extensions": ["@specforge/product", "@specforge/software", "@specforge/testing"]})
        .to_string(),
    );
    dir.write(
        "main.spec",
        r#"feature f1 "One" {
  status done
}

feature f2 "Two" {
  status done
}

milestone m1 "M" {
  status completed
  features [f1, f2]
  exit_criteria ["done"]
}

behavior b1 "B1" {
  features [f1]
  verify unit "b1 works"
}

behavior b2 "B2" {
  features [f2]
  verify unit "b2 works"
  verify unit "b2 still works"
}
"#,
    );
    dir.write(
        "specforge-report.json",
        serde_json::json!({"runner": "fixture", "results": {
            "b1": {"tests": [{"name": "t1", "status": "passed", "verify": "b1 works"}]},
            "b2": {"tests": [{"name": "t2", "status": "passed", "verify": "b2 works"}]},
        }})
        .to_string(),
    );
    let output = Cli::on(&dir)
        .command(&["product", "milestone-completion", "m1", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let mc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(mc["done_count"], 2);
    assert_eq!(mc["proven_features"], serde_json::json!(["f1"]), "{mc}");
    assert_eq!(mc["feature_evidence"][1]["proven_obligations"], 1);
}

/// A project enabling product, software and testing, whose `f_proven` is
/// implemented by a proven behavior, `f_half` by one with an unproven
/// obligation and `f_alone` by none; all three are done.
fn evidence_project(report: bool) -> TestProject {
    let dir = TestProject::new().raw_config(
        serde_json::json!({"name": "p", "version": "0.1.0",
            "extensions": ["@specforge/product", "@specforge/software", "@specforge/testing"]})
        .to_string(),
    );
    dir.write(
        "main.spec",
        r#"feature f_proven "Proven" {
  status done
}

feature f_half "Half" {
  status done
}

feature f_alone "Alone" {
  status done
}

behavior b_proven "Proven" {
  features [f_proven]
  verify unit "works"
}

behavior b_half "Half" {
  features [f_half]
  verify unit "works"
  verify unit "still works"
}
"#,
    );
    if report {
        dir.write(
            "specforge-report.json",
            serde_json::json!({"runner": "fixture", "results": {
                "b_proven": {"tests": [{"name": "t1", "status": "passed", "verify": "works"}]},
                "b_half": {"tests": [{"name": "t2", "status": "passed", "verify": "works"}]},
            }})
            .to_string(),
        );
    }
    dir
}

fn delivery_evidence(dir: &TestProject) -> serde_json::Value {
    let output = Cli::on(dir)
        .command(&[
            "analyze",
            "@specforge/product:delivery_evidence",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| panic!("{e}: {output:?}"))
}

#[specforge_test(
    behavior = "detect_done_feature_without_evidence",
    verify = "done feature with an unproven implementing behavior produces I071"
)]
#[specforge_test(
    behavior = "detect_done_feature_without_evidence",
    verify = "done feature whose implementing behaviors are all proven suppresses I071"
)]
fn delivery_evidence_reports_done_features_the_recorded_tests_do_not_prove() {
    let report = delivery_evidence(&evidence_project(true));
    let pass = &report["passes"][0];
    let mut findings: Vec<(String, String)> = pass["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["code"].as_str().unwrap().to_string(),
                f["message"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    findings.sort();
    assert_eq!(
        findings,
        [
            (
                "I071".to_string(),
                "feature 'f_alone' is done but no behavior implements it, so no recorded test can prove it".to_string()
            ),
            (
                "I071".to_string(),
                "feature 'f_half' is done but the recorded tests prove 0 of the 1 behaviors implementing it (1/2 obligations)".to_string()
            ),
        ],
        "{report}"
    );
}

#[specforge_test(
    behavior = "detect_done_feature_without_evidence",
    verify = "without recorded test results delivery_evidence reports nothing"
)]
fn delivery_evidence_without_a_report_reports_nothing() {
    let report = delivery_evidence(&evidence_project(false));
    let pass = &report["passes"][0];
    assert_eq!(pass["findings"], serde_json::json!([]), "{report}");
}
