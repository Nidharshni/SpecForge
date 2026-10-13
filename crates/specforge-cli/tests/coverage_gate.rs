//! C11-01 + C11-02 acceptance: `analyze coverage --min` gates the exit code,
//! and orphaned test records surface a W097 instead of dropping silently.

use assert_cmd::Command;
use specforge_test_macros::test as specforge_test;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn specforge() -> Command {
    Command::new(env!("CARGO_BIN_EXE_specforge"))
}

fn seed(path: &Path) {
    std::fs::create_dir_all(path.join("src")).unwrap();
    std::fs::write(
        path.join("specforge.json"),
        r#"{"name":"cov","spec_root":"src","extensions":["@specforge/software","@specforge/testing"]}"#,
    )
    .unwrap();
    std::fs::write(
        path.join("src/a.spec"),
        "type widget \"Widget\" {\n  id string @unique\n  verify unit \"widget valid\"\n}\n",
    )
    .unwrap();
}

fn report(path: &Path, proven: bool, extra: Option<&str>) -> String {
    let status = if proven { "pass" } else { "fail" };
    let mut json = format!(
        r#"{{"runner":"specforge-test","results":{{"widget":{{"tests":[{{"name":"w test","status":"{status}","verify":"widget valid"}}]}}}}}}"#
    );
    if let Some(orphan) = extra {
        json = json.trim_end_matches('}').to_string();
        json.push_str(
            format!(r#", "{orphan}": {{"tests":[{{"name":"orphan test","status":"pass"}}]}}}}}}"#)
                .as_str(),
        );
    }
    std::fs::write(path.join("specforge-report.json"), &json).unwrap();
    json
}

#[specforge_test(
    behavior = "te_coverage_gate",
    verify = "coverage at or above the threshold passes"
)]
fn min_gate_passes_when_coverage_meets_threshold() {
    let tmp = TempDir::new().unwrap();
    seed(tmp.path());
    report(tmp.path(), true, None);
    let out = specforge()
        .args([
            "analyze",
            "--path",
            tmp.path().to_str().unwrap(),
            "coverage",
            "--test-results",
            tmp.path().join("specforge-report.json").to_str().unwrap(),
            "--min",
            "50",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[specforge_test(
    behavior = "te_coverage_gate",
    verify = "coverage below the threshold fails with E048"
)]
fn min_gate_fails_when_coverage_below_threshold() {
    let tmp = TempDir::new().unwrap();
    seed(tmp.path());
    // 1 of 1 entities proven = 100%; make it fail by NOT proving: failing test
    let report = r#"{"runner":"r","results":{"widget":{"tests":[{"name":"w","status":"fail"}]}}}"#;
    std::fs::write(tmp.path().join("specforge-report.json"), report).unwrap();
    let out = specforge()
        .args([
            "analyze",
            "--path",
            tmp.path().to_str().unwrap(),
            "coverage",
            "--test-results",
            tmp.path().join("specforge-report.json").to_str().unwrap(),
            "--min",
            "50",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "gate must fail below threshold");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("E048"), "gate names E048: {stderr}");
    assert!(stderr.contains("below the required minimum"), "{stderr}");
}

#[specforge_test(
    behavior = "te_coverage_gate",
    verify = "a proven entity whose kind is not testable does not raise the gate"
)]
fn a_feature_with_a_passing_test_does_not_raise_the_gate() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(
        tmp.path().join("specforge.json"),
        r#"{"name":"cov","spec_root":"src","extensions":["@specforge/software","@specforge/testing","@specforge/product"]}"#,
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("src/a.spec"),
        "type widget \"Widget\" {\n  id string @unique\n  verify unit \"widget valid\"\n}\n\n\
         feature signin \"Sign in\" {\n  problem \"Users cannot reach their data\"\n  solution \"Let them log in\"\n}\n",
    )
    .unwrap();
    // The only testable entity's test fails; the feature's passes.
    std::fs::write(
        tmp.path().join("specforge-report.json"),
        r#"{"runner":"r","results":{
            "widget":{"tests":[{"name":"w","status":"fail","verify":"widget valid"}]},
            "signin":{"tests":[{"name":"s","status":"pass"}]}}}"#,
    )
    .unwrap();
    let out = specforge()
        .args([
            "analyze",
            "--path",
            tmp.path().to_str().unwrap(),
            "coverage",
            "--min",
            "50",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("proof coverage 0.0%") && stderr.contains("(0/1 testable entities proven)"),
        "{stderr}"
    );
}

#[specforge_test(behavior = "te_coverage_gate", verify = "the gate needs test results")]
fn min_requires_test_results() {
    let tmp = TempDir::new().unwrap();
    seed(tmp.path());
    let out = specforge()
        .args([
            "analyze",
            "--path",
            tmp.path().to_str().unwrap(),
            "coverage",
            "--min",
            "50",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--min needs test results"), "{stderr}");
}

#[specforge_test(
    behavior = "te_coverage_gate",
    verify = "a gate without the coverage pass exits 2 with E068"
)]
fn min_without_the_coverage_pass_exits_2_with_e068() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(
        tmp.path().join("specforge.json"),
        r#"{"name":"cov","spec_root":"src","extensions":["@specforge/software"]}"#,
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("src/a.spec"),
        "type widget \"Widget\" {\n  id string @unique\n}\n",
    )
    .unwrap();
    report(tmp.path(), true, None);
    let out = specforge()
        .args([
            "analyze",
            "--path",
            tmp.path().to_str().unwrap(),
            "all",
            "--test-results",
            tmp.path().join("specforge-report.json").to_str().unwrap(),
            "--min",
            "50",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains("error[E068]: a proof-coverage minimum needs the coverage pass"),
        "{stderr}"
    );
}

#[specforge_test(
    behavior = "te_stray_test_records",
    verify = "an unknown entity in a test record warns W097 with a close-match hint and does not fail the run"
)]
fn stray_test_records_warn_with_suggestion() {
    let tmp = TempDir::new().unwrap();
    seed(tmp.path());
    // Report proves "widget" and orphans "wodget" (typo of widget).
    let report = r#"{"runner":"r","results":{"widget":{"tests":[{"name":"w","status":"pass"}]},"wodget":{"tests":[{"name":"x","status":"pass"}]}}}"#;
    std::fs::write(tmp.path().join("specforge-report.json"), report).unwrap();
    let out = specforge()
        .args([
            "analyze",
            "--path",
            tmp.path().to_str().unwrap(),
            "coverage",
            "--test-results",
            tmp.path().join("specforge-report.json").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("W097"), "orphan warning emitted: {stderr}");
    assert!(stderr.contains("wodget"), "names the orphaned id: {stderr}");
    assert!(
        stderr.contains("widget") && stderr.contains("did you mean"),
        "suggests the close match: {stderr}"
    );
    // Warnings do not fail the run.
    assert!(out.status.success(), "orphan warnings don't gate: {stderr}");
}

fn write_report(path: &Path, ids: &[&str]) {
    let results: serde_json::Map<String, serde_json::Value> = ids
        .iter()
        .map(|id| {
            (
                id.to_string(),
                serde_json::json!({"tests": [{"name": "t", "status": "pass", "verify": "widget valid"}]}),
            )
        })
        .collect();
    let doc = serde_json::json!({"runner": "r", "results": results});
    std::fs::write(path.join("specforge-report.json"), doc.to_string()).unwrap();
}

fn analyze_json(path: &Path, strict: bool) -> (Option<i32>, serde_json::Value) {
    let mut args = vec![
        "analyze",
        "--path",
        path.to_str().unwrap(),
        "coverage",
        "--format",
        "json",
    ];
    if strict {
        args.push("--strict");
    }
    let out = specforge().args(args).output().unwrap();
    let doc = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stderr)));
    (out.status.code(), doc)
}

#[specforge_test(
    behavior = "te_stray_test_records",
    verify = "stray_records appear in the json output only when records exist"
)]
fn json_carries_stray_records_only_when_records_exist() {
    let tmp = TempDir::new().unwrap();
    seed(tmp.path());
    write_report(tmp.path(), &["widget"]);
    let (_, clean) = analyze_json(tmp.path(), false);
    assert!(clean.get("stray_records").is_none(), "{clean}");

    write_report(tmp.path(), &["widget", "wodget"]);
    let (_, doc) = analyze_json(tmp.path(), false);
    assert_eq!(
        doc["stray_records"],
        serde_json::json!([{"entity_id": "wodget", "near": "widget"}]),
        "{doc}"
    );
}

#[specforge_test(
    behavior = "te_stray_test_records",
    verify = "strict neither promotes a stray record nor changes ok or the exit code"
)]
fn strict_leaves_stray_records_alone() {
    let tmp = TempDir::new().unwrap();
    seed(tmp.path());
    write_report(tmp.path(), &["widget", "wodget"]);
    let (lax_code, lax) = analyze_json(tmp.path(), false);
    let (strict_code, strict) = analyze_json(tmp.path(), true);
    assert_eq!(lax_code, strict_code);
    assert_eq!(lax["ok"], strict["ok"]);
    assert_eq!(lax["stray_records"], strict["stray_records"]);
}

// C1-06 rot guard: the flagship example's traceability loop must keep
// working — collect from the committed runner report, analyze, and trace.
#[test]
fn todo_app_traceability_loop_stays_wired() {
    // Flat-file copy of the example (spec/, specforge.json, fixture report),
    // skipping build dirs.
    fn walk(from: &Path, to: &Path) -> Vec<(PathBuf, PathBuf)> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(from).unwrap().flatten() {
            let path = e.path();
            let dest = to.join(e.file_name());
            if path.is_dir() {
                if path
                    .file_name()
                    .is_some_and(|n| n == "target" || n == "tests")
                {
                    continue;
                }
                out.extend(walk(&path, &dest));
            } else if path.is_file() {
                out.push((path, dest));
            }
        }
        out
    }

    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples/todo-app");
    let tmp = TempDir::new().unwrap();
    // Copy the project (spec + config + committed fixture) so collect writes
    // its report into the temp copy, not the repo.
    let copy = |from: &Path, to: &Path| {
        for e in walk(from, to) {
            std::fs::create_dir_all(e.1.parent().unwrap()).unwrap();
            std::fs::copy(&e.0, &e.1).unwrap();
        }
    };
    copy(&example, tmp.path());

    let specforge = || Command::new(env!("CARGO_BIN_EXE_specforge"));

    // collect the committed fixture report
    let out = specforge()
        .args([
            "collect",
            "--path",
            tmp.path().to_str().unwrap(),
            "--report",
            example.join("runner-report.fixture.json").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "collect: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // analyze proves the recorded entities
    let out = specforge()
        .args([
            "analyze",
            "--path",
            tmp.path().to_str().unwrap(),
            "coverage",
            "--test-results",
            tmp.path().join("specforge-report.json").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "analyze: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Two entities have every obligation named by a passing test; the
    // others' untested obligations are listed as A015.
    assert!(
        stdout.contains("discharge_funnel.entities_proven: 2"),
        "the fixture proves 2 entities: {stdout}"
    );
    assert!(
        stdout.contains("[A015]"),
        "unproven obligations listed: {stdout}"
    );

    // trace surfaces provenance for a proven behavior
    let out = specforge()
        .args([
            "trace",
            "--path",
            tmp.path().to_str().unwrap(),
            "create_task",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "trace failed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("create_task"), "trace names the entity");
}

/// `analyze <pass> --min 50 --json` over `path`, with the report it holds.
fn analyze_min_json(path: &Path, pass: &str) -> std::process::Output {
    specforge()
        .args([
            "analyze",
            "--path",
            path.to_str().unwrap(),
            pass,
            "--test-results",
            path.join("specforge-report.json").to_str().unwrap(),
            "--min",
            "50",
            "--format",
            "json",
        ])
        .output()
        .unwrap()
}

#[specforge_test(
    behavior = "te_coverage_gate",
    verify = "the analysis's ok is the run verdict, the gate included, and its JSON says where the gate landed"
)]
fn the_json_verdict_is_the_exit_verdict() {
    let tmp = TempDir::new().unwrap();
    seed(tmp.path());
    std::fs::write(
        tmp.path().join("specforge-report.json"),
        r#"{"runner":"r","results":{}}"#,
    )
    .unwrap();

    let out = analyze_min_json(tmp.path(), "coverage");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(doc["ok"], false, "{doc}");
    assert_eq!(
        doc["gate"],
        serde_json::json!({"status": "below", "min": 50.0, "pct": 0.0, "proven": 0, "total": 1}),
        "{doc}"
    );
    assert!(stderr.is_empty(), "{stderr}");
}

#[specforge_test(
    behavior = "te_coverage_gate",
    verify = "a gate without the coverage pass exits 2 with E068"
)]
fn a_gate_without_the_coverage_pass_is_refused_before_the_analysis() {
    let tmp = TempDir::new().unwrap();
    seed(tmp.path());
    std::fs::write(
        tmp.path().join("specforge-report.json"),
        r#"{"runner":"r","results":{}}"#,
    )
    .unwrap();

    let out = analyze_min_json(tmp.path(), "contracts");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(doc["code"], "E068", "{doc}");
    assert!(doc.get("passes").is_none(), "{doc}");
    assert!(stderr.is_empty(), "{stderr}");
}

// ADR-NEW-08 (plan 08, T0): the committed example report must be what
// `collect` writes from the committed runner fixture. It does not today (no
// names, no `verify`, a `total` key), so it is ignored until T5 fixes it.
#[test]
#[ignore = "fixed by T5"]
fn the_example_report_is_what_collect_writes() {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap().flatten() {
            let path = e.path();
            if path.is_dir() {
                if path
                    .file_name()
                    .is_some_and(|n| n == "target" || n == "tests")
                {
                    continue;
                }
                copy(&path, &to.join(e.file_name()));
            } else {
                std::fs::copy(&path, to.join(e.file_name())).unwrap();
            }
        }
    }
    let example = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples/todo-app");
    let tmp = TempDir::new().unwrap();
    copy(&example, tmp.path());
    let committed = std::fs::read_to_string(tmp.path().join("specforge-report.json")).unwrap();
    std::fs::remove_file(tmp.path().join("specforge-report.json")).unwrap();

    let out = specforge()
        .args([
            "collect",
            "--path",
            tmp.path().to_str().unwrap(),
            "--report",
            tmp.path()
                .join("runner-report.fixture.json")
                .to_str()
                .unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "collect: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let written = std::fs::read_to_string(tmp.path().join("specforge-report.json")).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&written).unwrap(),
        serde_json::from_str::<serde_json::Value>(&committed).unwrap(),
        "examples/todo-app/specforge-report.json is not what collect writes"
    );
}
