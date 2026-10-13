// `specforge analyze` — end-to-end tests against the real binary.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;
use specforge_test_macros::test as specforge_test;
use tempfile::TempDir;

#[allow(deprecated)]
fn specforge_cmd() -> Command {
    Command::cargo_bin("specforge").unwrap()
}

/// Scaffold a minimal project with the formal and testing extensions
/// installed and the given main.spec content.
fn project(spec: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"extensions": ["@specforge/formal", "@specforge/testing"]}"#,
    )
    .unwrap();
    fs::write(dir.path().join("main.spec"), spec).unwrap();
    dir
}

/// The coverage pass report (owned by @specforge/testing, ADR 0002).
fn coverage(doc: &serde_json::Value) -> &serde_json::Value {
    doc["passes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pass"] == "@specforge/testing:coverage")
        .unwrap_or_else(|| panic!("no coverage pass in {doc}"))
}

fn json_body(dir: &TempDir, extra_args: &[&str]) -> (serde_json::Value, i32) {
    let output = specforge_cmd()
        .args([
            "analyze",
            "--path",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .args(extra_args)
        .output()
        .unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    (doc, output.status.code().unwrap_or(-1))
}

#[test]
fn analyze_clean_project_exits_zero() {
    let dir = project(
        "invariant i1 \"Has obligations\" {\n  guarantee \"always holds\"\n  risk low\n  verify property \"i1 holds under all inputs\"\n}\n",
    );
    let (doc, code) = json_body(&dir, &[]);
    assert_eq!(code, 0, "clean project must exit 0: {doc}");
    assert_eq!(doc["ok"], true);

    let coverage = coverage(&doc);
    assert_eq!(coverage["summary"]["invariants"][0]["risk"], "low");
    assert_eq!(coverage["summary"]["invariants"][0]["total"], 1);
    assert_eq!(coverage["summary"]["invariants"][0]["unverified"], 0);
    assert_eq!(coverage["summary"]["obligations"], 1);
}

#[specforge_test(
    behavior = "te_coverage_pass",
    verify = "a high-risk invariant without obligations is an A002 error"
)]
fn analyze_high_risk_unverified_invariant_fails() {
    let dir =
        project("invariant bad \"Never verified\" {\n  guarantee \"nothing\"\n  risk high\n}\n");
    let (doc, code) = json_body(&dir, &[]);
    assert_eq!(code, 1, "high-risk unverified invariant must exit 1: {doc}");
    assert_eq!(doc["ok"], false);
    let findings: Vec<&serde_json::Value> = coverage(&doc)["findings"]
        .as_array()
        .unwrap()
        .iter()
        .collect();
    assert!(
        findings.iter().any(|f| f["code"] == "A002"),
        "expected A002 finding: {findings:?}"
    );
}

// Findings follow check's colour rule, on stdout: plain into a pipe,
// colour-coded when forced, and NO_COLOR wins over forcing.
#[test]
fn analyze_colours_findings_only_when_asked() {
    let dir =
        project("invariant bad \"Never verified\" {\n  guarantee \"nothing\"\n  risk high\n}\n");
    let stdout = |no_color: Option<&str>, force: Option<&str>| {
        let mut cmd = specforge_cmd();
        cmd.env_remove("NO_COLOR").env_remove("CLICOLOR_FORCE");
        if let Some(v) = no_color {
            cmd.env("NO_COLOR", v);
        }
        if let Some(v) = force {
            cmd.env("CLICOLOR_FORCE", v);
        }
        let output = cmd
            .args(["analyze", "--path", dir.path().to_str().unwrap()])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        String::from_utf8(output.stdout).unwrap()
    };

    let plain = stdout(None, None);
    assert!(!plain.contains('\x1b'), "{plain:?}");
    assert!(plain.contains("\n[A002] Error: "), "{plain:?}");

    let colored = stdout(None, Some("1"));
    assert!(
        colored.contains("\n\x1b[31m[A002] Error:\x1b[0m "),
        "{colored:?}"
    );

    assert_eq!(stdout(Some("1"), Some("1")), plain);
}

#[test]
fn analyze_low_risk_warning_promoted_by_strict() {
    let dir = project("invariant soft \"Softly unverified\" {\n  guarantee \"x\"\n  risk low\n}\n");
    // Without --strict: A002 is a warning, exit 0.
    let (doc, code) = json_body(&dir, &[]);
    assert_eq!(code, 0, "warning alone must not fail: {doc}");
    assert_eq!(doc["ok"], true);

    // With --strict: the warning promotes to an error.
    let (_, code) = json_body(&dir, &["--strict"]);
    assert_eq!(code, 1, "--strict must promote the warning: exit {code}");
}

#[test]
fn analyze_unknown_pass_exits_two() {
    let dir = project("");
    let output = specforge_cmd()
        .args([
            "analyze",
            "nonsense",
            "--path",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn analyze_single_pass_selection() {
    let dir = project(
        "invariant only \"Only coverage\" {\n  guarantee \"g\"\n  risk low\n  verify unit \"u1\"\n}\n",
    );
    let output = specforge_cmd()
        .args([
            "analyze",
            "coverage",
            "--path",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let doc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let passes = doc["passes"].as_array().unwrap();
    assert_eq!(passes.len(), 1, "only the requested pass runs");
    assert_eq!(passes[0]["pass"], "@specforge/testing:coverage");
}

#[specforge_test(
    behavior = "te_coverage_pass",
    verify = "invariant references count as enforcement"
)]
fn analyze_enforcement_maps_invariant_references() {
    // invariant referenced by a behavior via `invariants [...]` -> enforced;
    // an unreferenced invariant is counted as an orphan. Reporting it is
    // W003's job in `check`, so analyze emits no finding for it.
    let spec = r#"
invariant held "Held" {
  guarantee "g"
  risk low
  verify property "holds"
}

invariant orphan "Orphan" {
  guarantee "nothing points here"
  risk low
  verify property "holds too"
}

behavior keeper "Keeper" {
  title "Keeper"
  invariants [held]
  verify unit "keeps held"
}
"#;
    let dir = project(spec.trim_start());
    let (doc, code) = json_body(&dir, &[]);
    assert_eq!(code, 0, "warnings must not fail: {doc}");

    let coverage = coverage(&doc);
    assert_eq!(coverage["summary"]["invariant_enforced"], 1);
    assert_eq!(coverage["summary"]["invariant_unreferenced"], 1);

    assert!(
        coverage["findings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| !f["message"].as_str().unwrap().contains("'orphan'")),
        "the orphan is counted, not reported: {coverage}"
    );
}

#[specforge_test(
    behavior = "se_validate_unused_invariants",
    verify = "invariant nothing references produces W003"
)]
#[specforge_test(
    behavior = "se_validate_unused_invariants",
    verify = "invariant with incoming reference edge passes"
)]
fn check_reports_an_unreferenced_invariant_as_w003() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"extensions": ["@specforge/software"]}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("main.spec"),
        concat!(
            "invariant held \"Held\" {\n  guarantee \"g\"\n}\n",
            "invariant orphan \"Orphan\" {\n  guarantee \"g\"\n}\n",
            "behavior keeper \"Keeper\" {\n  contract \"c\"\n  invariants [held]\n}\n",
        ),
    )
    .unwrap();
    let output = specforge_cmd()
        .args(["check", "--format=json"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();
    let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let w003: Vec<&str> = diagnostics
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == "W003")
        .map(|d| d["message"].as_str().unwrap())
        .collect();
    assert_eq!(w003.len(), 1, "{diagnostics}");
    assert!(w003[0].contains("'orphan'"), "{w003:?}");
}

#[specforge_test(
    behavior = "te_coverage_pass",
    verify = "recorded test results prove entities and failing tests are A014"
)]
fn analyze_discharge_intent_and_proof() {
    let dir = project(concat!(
        "invariant held \"Held\" {\n",
        "  guarantee \"g\"\n",
        "  risk low\n",
        "  verify property \"holds\"\n",
        "}\n",
        "\n",
        "invariant untested \"Untested\" {\n",
        "  guarantee \"g\"\n",
        "  risk low\n",
        "  verify property \"holds\"\n",
        "}\n",
    ));

    // Intent: both declare obligations; nothing is proven without results.
    let (doc, code) = json_body(&dir, &[]);
    assert_eq!(code, 0);
    let funnel = &coverage(&doc)["summary"]["discharge_funnel"];
    assert_eq!(funnel["entities_with_obligations"], 2);
    assert_eq!(funnel["entities_proven"], 0);

    // Proof: a failing test in the recorded results is an error.
    let analyze_with = |report: serde_json::Value| {
        fs::write(dir.path().join("report.json"), report.to_string()).unwrap();
        let output = specforge_cmd()
            .args([
                "analyze",
                "coverage",
                "--path",
                dir.path().to_str().unwrap(),
                "--format",
                "json",
                "--test-results",
                dir.path().join("report.json").to_str().unwrap(),
            ])
            .output()
            .unwrap();
        let doc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        (doc, output.status.code())
    };
    let (doc, code) = analyze_with(serde_json::json!({
        "runner": "manual",
        "results": {"held": {"tests": [{"name": "holds", "status": "failed"}]}}
    }));
    assert_eq!(code, Some(1), "failing proof must exit 1: {doc}");
    assert_eq!(doc["ok"], false);
    let a014: Vec<&serde_json::Value> = coverage(&doc)["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "A014")
        .collect();
    assert_eq!(a014.len(), 1, "A014 reported: {a014:?}");

    // Proven: all recorded tests pass.
    let (doc, code) = analyze_with(serde_json::json!({
        "runner": "manual",
        "results": {"held": {"tests": [{"name": "holds", "status": "passed", "verify": "holds"}]}}
    }));
    assert_eq!(code, Some(0));
    let summary = &coverage(&doc)["summary"];
    assert_eq!(summary["discharge_funnel"]["entities_proven"], 1);
    assert_eq!(summary["test_results"]["runner"], "manual");
}

#[specforge_test(
    behavior = "te_coverage_pass",
    verify = "an obligation no passing test names is A015 and a test naming an undeclared obligation is A016"
)]
fn analyze_reports_unproven_and_undeclared_obligations() {
    let dir = project(concat!(
        "invariant unique_emails \"Unique emails\" {\n",
        "  guarantee \"g\"\n",
        "  risk low\n",
        "  verify unit \"rejects a duplicate email\"\n",
        "  verify unit \"stores a hashed password\"\n",
        "}\n",
    ));
    let report = serde_json::json!({
        "runner": "manual",
        "results": {"unique_emails": {"tests": [
            {"name": "dup", "status": "passed", "verify": "rejects a duplicate email"},
            {"name": "hash", "status": "passed", "verify": "stores a hashed pasword"}
        ]}}
    });
    fs::write(dir.path().join("specforge-report.json"), report.to_string()).unwrap();
    let (doc, code) = json_body(&dir, &[]);
    assert_eq!(code, 0, "warnings only: {doc}");
    let findings = coverage(&doc)["findings"].as_array().unwrap().clone();
    let message = |code: &str| {
        findings
            .iter()
            .find(|f| f["code"] == code)
            .unwrap_or_else(|| panic!("{code} missing: {findings:?}"))["message"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert!(message("A015").contains("\"stores a hashed password\""));
    assert!(message("A016").contains("\"stores a hashed pasword\""));
    let summary = &coverage(&doc)["summary"];
    assert_eq!(summary["discharge_funnel"]["entities_proven"], 0);
    assert_eq!(summary["test_results"]["obligations_proven"], 1);
}

#[test]
fn analyze_dispatches_extension_compiler_pass() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"extensions": ["@specforge/formal"]}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("main.spec"),
        concat!(
            "behavior obligated \"Obligated\" {\n",
            "  title \"Obligated\"\n",
            "  requires {\n",
            "    auth_ready \"auth is configured\"\n",
            "  }\n",
            "  verify unit \"runs\"\n",
            "}\n",
        ),
    )
    .unwrap();

    let output = specforge_cmd()
        .args([
            "analyze",
            "--path",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    let pass = doc["passes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pass"] == "@specforge/formal:condition_check")
        .expect("formal condition_check pass must be dispatched");
    let a = pass["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "W096")
        .count();
    assert_eq!(a, 1, "requires-without-ensures must surface W096");
    assert_eq!(pass["summary"]["entities_analyzed"], 1);
    assert_eq!(pass["summary"]["extension"], "@specforge/formal");
}

#[test]
fn analyze_event_graph_flags_unconsumed_events() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"extensions": ["@specforge/formal", "@specforge/software"]}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("main.spec"),
        concat!(
            "event tick \"Tick\" {\n",
            "  title \"Tick\"\n",
            "  channel \"sys.tick\"\n",
            "}\n",
            "\n",
            "behavior ticker \"Ticker\" {\n",
            "  title \"Ticker\"\n",
            "  produces [tick]\n",
            "  verify unit \"emits tick\"\n",
            "}\n",
        ),
    )
    .unwrap();

    let output = specforge_cmd()
        .args([
            "analyze",
            "--path",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let report = doc["passes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pass"] == "@specforge/formal:event_graph_analyze")
        .expect("event_graph_analyze pass must be dispatched");
    let w029: Vec<&serde_json::Value> = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "W029")
        .collect();
    assert_eq!(w029.len(), 1, "unconsumed producer must surface W029");
}

fn layering_findings(doc: &serde_json::Value) -> Vec<(String, String)> {
    doc["passes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pass"] == "@specforge/formal:layering_verify")
        .expect("layering_verify pass must be dispatched")["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["code"].as_str().unwrap().to_string(),
                f["message"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

/// Through the real binary and embedded formal blob: the pass must read the
/// edges `refinement` entities actually produce (C10-02 was unreachable when
/// it matched labels no field emits).
#[test]
fn analyze_refinement_that_drops_an_ensures_condition_is_e031() {
    let dir = project(concat!(
        "behavior provision \"Provision\" {\n",
        "  title \"Provision\"\n",
        "  ensures {\n",
        "    config_created \"config exists\"\n",
        "    file_created \"file exists\"\n",
        "  }\n",
        "  verify contract \"abstract contract\"\n",
        "}\n",
        "behavior provision_full \"Provision (full)\" {\n",
        "  title \"Provision (full)\"\n",
        "  ensures {\n",
        "    config_created \"config exists\"\n",
        "    file_created \"file exists\"\n",
        "    audit_logged \"audit entry written\"\n",
        "  }\n",
        "  verify unit \"full\"\n",
        "}\n",
        "behavior provision_lazy \"Provision (lazy)\" {\n",
        "  title \"Provision (lazy)\"\n",
        "  ensures {\n",
        "    config_created \"config exists\"\n",
        "  }\n",
        "  verify unit \"lazy\"\n",
        "}\n",
        "refinement full_refines \"Full\" {\n",
        "  abstract_entity provision\n",
        "  concrete_entity provision_full\n",
        "}\n",
        "refinement lazy_refines \"Lazy\" {\n",
        "  abstract_entity provision\n",
        "  concrete_entity provision_lazy\n",
        "}\n",
    ));

    let (doc, code) = json_body(&dir, &[]);
    let findings = layering_findings(&doc);
    let e031: Vec<&String> = findings
        .iter()
        .filter(|(c, _)| c == "E031")
        .map(|(_, m)| m)
        .collect();
    assert_eq!(
        e031.len(),
        1,
        "only the weakening refinement fires: {findings:?}"
    );
    assert!(
        e031[0].contains("lazy_refines") && e031[0].contains("file_created"),
        "names the refinement and the dropped condition: {}",
        e031[0]
    );
    assert_ne!(code, 0, "an E031 error must fail analyze");
}

/// The field form of specification layering (`abstract true` + `refines`),
/// declared by @specforge/formal on @specforge/software behaviors.
#[test]
fn refines_and_abstract_fields_drive_layering_end_to_end() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"extensions": ["@specforge/formal", "@specforge/software", "@specforge/testing"]}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("main.spec"),
        concat!(
            "behavior provision \"Provision\" {\n",
            "  title \"Provision\"\n",
            "  abstract true\n",
            "  ensures {\n",
            "    config_created \"config exists\"\n",
            "    file_created \"file exists\"\n",
            "  }\n",
            "}\n",
            "behavior provision_lazy \"Provision (lazy)\" {\n",
            "  title \"Provision (lazy)\"\n",
            "  refines provision\n",
            "  ensures {\n",
            "    config_created \"config exists\"\n",
            "  }\n",
            "  verify unit \"lazy\"\n",
            "}\n",
            "behavior plain \"Plain\" {\n  title \"Plain\"\n  verify unit \"plain\"\n}\n",
            "behavior derived \"Derived\" {\n",
            "  title \"Derived\"\n",
            "  refines plain\n",
            "  verify unit \"derived\"\n",
            "}\n",
            "behavior orphan_spec \"Orphan spec\" {\n",
            "  title \"Orphan spec\"\n",
            "  abstract true\n",
            "}\n",
        ),
    )
    .unwrap();

    let check = specforge_cmd()
        .args([
            "check",
            "--path",
            dir.path().to_str().unwrap(),
            "--format=json",
        ])
        .output()
        .unwrap();
    let diagnostics: Vec<serde_json::Value> = serde_json::from_slice(&check.stdout).unwrap();
    let flagged: Vec<String> = diagnostics
        .iter()
        .filter(|d| d["code"] == "W020" || d["code"] == "W004")
        .map(|d| d["message"].as_str().unwrap().to_string())
        .collect();
    assert!(
        flagged.is_empty(),
        "abstract/refines are declared fields and abstract behaviors need no verify: {flagged:?}"
    );

    let (doc, code) = json_body(&dir, &[]);
    let findings = layering_findings(&doc);
    let codes: Vec<&str> = findings.iter().map(|(c, _)| c.as_str()).collect();
    assert_eq!(codes, vec!["E031", "W030", "W110"], "{findings:?}");
    assert!(
        findings[0]
            .1
            .contains("'provision_lazy' refines 'provision'")
    );
    assert!(
        findings[1].1.contains("'orphan_spec'"),
        "only the unrefined abstract"
    );
    assert!(findings[2].1.contains("'derived' refines 'plain'"));
    assert_ne!(code, 0, "E031 fails analyze");
}

#[test]
fn analyze_refinement_cycle_is_e041() {
    let dir = project(concat!(
        "behavior alpha \"Alpha\" {\n  title \"Alpha\"\n  verify unit \"a\"\n}\n",
        "behavior beta \"Beta\" {\n  title \"Beta\"\n  verify unit \"b\"\n}\n",
        "refinement alpha_refines_beta \"A\" {\n",
        "  abstract_entity beta\n",
        "  concrete_entity alpha\n",
        "}\n",
        "refinement beta_refines_alpha \"B\" {\n",
        "  abstract_entity alpha\n",
        "  concrete_entity beta\n",
        "}\n",
    ));

    let (doc, _) = json_body(&dir, &[]);
    let codes: Vec<String> = layering_findings(&doc)
        .into_iter()
        .map(|(c, _)| c)
        .collect();
    assert_eq!(codes, vec!["E041"], "a two-refinement loop is one cycle");
}

#[test]
fn analyze_orders_extension_passes_by_constraints() {
    // The formal extension declares its passes shuffled (event_graph_analyze
    // first); the host must order them by the after-constraints:
    // condition_check -> layering_verify -> event_graph_analyze.
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"extensions": ["@specforge/formal"]}"#,
    )
    .unwrap();
    fs::write(dir.path().join("main.spec"), "").unwrap();

    let output = specforge_cmd()
        .args([
            "analyze",
            "--path",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let formal: Vec<&str> = doc["passes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p["pass"].as_str())
        .filter(|n| n.starts_with("@specforge/formal:"))
        .map(|n| n.trim_start_matches("@specforge/formal:"))
        .collect();
    assert_eq!(
        formal,
        vec![
            "condition_check",
            "layering_verify",
            "event_graph_analyze",
            "coverage_tracking"
        ],
        "constraint order must beat declaration order: {formal:?}"
    );
}

#[test]
fn analyze_prove_flags_unsatisfiable_constraint() {
    if std::process::Command::new("z3")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("z3 not installed — skipping prove e2e");
        return;
    }

    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"extensions": ["@specforge/governance"]}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("main.spec"),
        concat!(
            "constraint impossible \"Impossible Bounds\" {\n",
            "  description \"Bounds that can never both hold\"\n",
            "  category performance\n",
            "  priority critical\n",
            "  metric \"\"\"\n",
            "    latency < 100ms\n",
            "    latency > 500ms\n",
            "  \"\"\"\n",
            "}\n",
        ),
    )
    .unwrap();

    let output = specforge_cmd()
        .args([
            "analyze",
            "--prove",
            "--path",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "unsatisfiable must exit 1");
    let doc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(doc["ok"], false);
    let prove = doc["passes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pass"] == "prove")
        .expect("prove pass must be dispatched");
    assert!(
        prove["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["code"] == "E046"),
        "E046 must surface: {prove}"
    );
    assert_eq!(prove["summary"]["unsatisfiable"], true);
}

#[test]
fn analyze_prove_satisfiable_constraint_exits_zero() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"extensions": ["@specforge/governance"]}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("main.spec"),
        concat!(
            "constraint tight \"Tight But Possible\" {\n",
            "  description \"Bounds that can both hold\"\n",
            "  category performance\n",
            "  priority critical\n",
            "  metric \"\"\"\n",
            "    latency < 100ms\n",
            "    latency > 10ms\n",
            "  \"\"\"\n",
            "}\n",
        ),
    )
    .unwrap();

    let output = specforge_cmd()
        .args([
            "analyze",
            "--prove",
            "--path",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let doc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(doc["ok"], true);
    let prove = doc["passes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pass"] == "prove")
        .expect("prove pass must be dispatched");
    assert_eq!(prove["summary"]["satisfiable"], true);
    assert_eq!(prove["summary"]["unsatisfiable"], false);
}

#[specforge_test(
    behavior = "te_coverage_pass",
    verify = "a proved formal claim discharges verify property obligations"
)]
fn analyze_proved_claims_discharge_verify_property_obligations() {
    // This test is a linked proof: skipping it would still record a pass,
    // so without z3 it fails instead (CI installs z3).
    assert!(
        std::process::Command::new("z3")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success()),
        "z3 is required for this test: install it (brew install z3 / apt-get install z3)"
    );

    // A constraint's metric is a bound (governance), an invariant's
    // expression a claim (formal's enhancement of software's invariant).
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"extensions": ["@specforge/software", "@specforge/governance", "@specforge/formal", "@specforge/testing"]}"#,
    )
    .unwrap();
    fs::create_dir(dir.path().join("spec")).unwrap();
    fs::write(
        dir.path().join("spec/main.spec"),
        r#"
constraint budget "Latency Budget" {
    description "Budget"
    metric expr {
        latency < 100ms
    }
}

invariant responsive "System Stays Responsive" {
    description "Follows from the declared budget"
    expression expr {
        latency < 250ms
    }
    verify property "latency bound entails responsiveness"
}
"#,
    )
    .unwrap();

    let output = specforge_cmd()
        .args([
            "analyze",
            "--prove",
            "--path",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let doc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    let prove = doc["passes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pass"] == "prove")
        .expect("prove pass must be dispatched");
    assert_eq!(prove["summary"]["claims"], 1);
    assert_eq!(prove["summary"]["claims_proved"], 1);

    assert_eq!(
        coverage(&doc)["summary"]["discharge_funnel"]["formally_discharged"],
        1,
        "the proved claim must discharge the verify property obligation"
    );
}

#[specforge_test(
    behavior = "detect_unknown_entity_fields",
    verify = "expression is checked like any other field (W020 where undeclared)"
)]
fn analyze_reads_no_claim_from_an_expression_no_extension_declares() {
    // Without @specforge/formal nothing declares an invariant's
    // `expression`: check reports W020 and the prove pass reads no claim.
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"extensions": ["@specforge/software", "@specforge/governance"]}"#,
    )
    .unwrap();
    fs::create_dir(dir.path().join("spec")).unwrap();
    fs::write(
        dir.path().join("spec/main.spec"),
        r#"
constraint budget "Latency Budget" {
    description "Budget"
    metric expr {
        latency < 100ms
    }
}

invariant responsive "System Stays Responsive" {
    guarantee "The system MUST stay responsive"
    expression expr {
        latency < 250ms
    }
}
"#,
    )
    .unwrap();

    let output = specforge_cmd()
        .args([
            "check",
            "--path",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let w020: Vec<&serde_json::Value> = diagnostics
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == "W020")
        .collect();
    assert_eq!(w020.len(), 1, "{diagnostics}");
    assert!(
        w020[0]["message"].as_str().unwrap().contains("expression"),
        "{diagnostics}"
    );
    // As E024 does for a kind, the W020 names the extension declaring it.
    assert!(
        w020[0]["suggestion"]
            .as_str()
            .is_some_and(|s| s.contains("specforge add @specforge/formal")),
        "{diagnostics}"
    );

    let output = specforge_cmd()
        .args([
            "analyze",
            "--prove",
            "--path",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let prove = doc["passes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pass"] == "prove")
        .expect("prove pass must be dispatched");
    assert_eq!(prove["summary"]["claims"], 0, "{prove}");
    assert_eq!(prove["summary"]["entities_with_bounds"], 1, "{prove}");
}

// A formal `property` accepts verify statements (`supports_verify`) but its
// kind is not declared testable, so it stays out of the coverage
// denominator and is never A001 (plan 02, S4).
#[specforge_test(
    invariant = "testable_entity_classification",
    verify = "testable=false entity excluded from coverage"
)]
fn analyze_coverage_leaves_out_kinds_not_declared_testable() {
    let dir = project(concat!(
        "behavior b1 \"Saves\" {\n  contract \"The system MUST save\"\n",
        "  verify unit \"b1 saves\"\n}\n",
        "property p1 \"Nothing is lost\" {\n  property_type safety\n",
        "  expression \"nothing is ever lost\"\n}\n",
    ));
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"extensions": ["@specforge/software", "@specforge/formal", "@specforge/testing"]}"#,
    )
    .unwrap();
    let (doc, _) = json_body(&dir, &["coverage"]);
    let coverage = coverage(&doc);
    assert_eq!(coverage["summary"]["testable_total"], 1, "{coverage}");
    assert_eq!(coverage["summary"]["testable_verified"], 1, "{coverage}");
    let a001: Vec<&serde_json::Value> = coverage["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "A001")
        .collect();
    assert!(a001.is_empty(), "the property is not testable: {a001:?}");
}

/// The `available: …` list of an unknown-pass refusal.
fn available_passes(message: &str) -> &str {
    let start = message
        .find("(available: ")
        .unwrap_or_else(|| panic!("no available list: {message}"));
    message[start..].trim_end()
}

// Analysis passes are an open set: the CLI takes any name and relays the
// operation's refusal, so it runs and refuses what MCP does (ADR 0027 D6).
#[specforge_test(
    behavior = "name_enumerated_options_once",
    verify = "the analysis passes the CLI accepts are the project's"
)]
fn the_cli_runs_a_pass_an_extension_declares() {
    let tmp = TempDir::new().unwrap();
    crate::coverage_corpus::copy_tree(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/read_views/rv1"),
        tmp.path(),
    );
    let root = tmp.path().to_str().unwrap();

    let ran = specforge_cmd()
        .args([
            "analyze",
            "@specforge/testing:coverage",
            "--path",
            root,
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        ran.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
    let doc: serde_json::Value = serde_json::from_slice(&ran.stdout).unwrap();
    let passes: Vec<&str> = doc["passes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["pass"].as_str().unwrap())
        .collect();
    assert_eq!(passes, ["@specforge/testing:coverage"], "{doc}");

    let refused = specforge_cmd()
        .args(["analyze", "nosuch", "--path", root])
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert_eq!(
        stderr.trim_end(),
        "error[unknown_pass]: Unknown analysis pass 'nosuch' (available: all, coverage, \
         contracts, @specforge/testing:coverage)"
    );

    // MCP lists the same passes for the same project.
    let mcp = crate::coverage_corpus::mcp_calls(
        tmp.path(),
        &[serde_json::json!({"name": "specforge.analyze", "arguments": {"pass": "nosuch"}})],
    );
    let message = mcp[0]["isError"]["message"].as_str().unwrap();
    assert_eq!(available_passes(message), available_passes(&stderr));
}

#[specforge_test_macros::test(
    behavior = "fa_detect_unmatched_producers",
    verify = "an event participating in a process counts as used and produces no W029"
)]
fn an_event_in_a_process_is_used_and_not_w029() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"extensions": ["@specforge/formal", "@specforge/software"]}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("main.spec"),
        concat!(
            "event tick \"Tick\" {\n",
            "  title \"Tick\"\n",
            "  channel \"sys.tick\"\n",
            "  participates_in [clock]\n",
            "}\n",
            "\n",
            "process clock \"Clock\" {\n",
            "  alphabet [\"tick\"]\n",
            "  states [\"idle\"]\n",
            "  initial_state \"idle\"\n",
            "}\n",
            "\n",
            "behavior ticker \"Ticker\" {\n",
            "  title \"Ticker\"\n",
            "  produces [tick]\n",
            "  verify unit \"emits tick\"\n",
            "}\n",
        ),
    )
    .unwrap();
    let output = specforge_cmd()
        .args([
            "analyze",
            "--path",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let report = doc["passes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pass"] == "@specforge/formal:event_graph_analyze")
        .expect("event_graph_analyze pass must be dispatched");
    let w029 = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "W029")
        .count();
    assert_eq!(w029, 0, "participation is usage: {report}");
}

/// A project whose two bounds contradict each other (governance's metric is a bound).
fn impossible_bounds() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"extensions": ["@specforge/governance"]}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("main.spec"),
        concat!(
            "constraint impossible \"Impossible Bounds\" {\n",
            "  description \"Bounds that can never both hold\"\n",
            "  category performance\n",
            "  priority critical\n",
            "  metric \"\"\"\n",
            "    latency < 100ms\n",
            "    latency > 500ms\n",
            "  \"\"\"\n",
            "}\n",
        ),
    )
    .unwrap();
    dir
}

/// The prove pass's report from `analyze --prove --format json` with `path` as PATH.
fn prove_report(dir: &Path, path: &Path, extra: &[&str]) -> (Value, Option<i32>) {
    let output = specforge_cmd()
        .args(["analyze", "--prove", "--format", "json", "--path"])
        .arg(dir)
        .args(extra)
        .env("PATH", path)
        .output()
        .unwrap();
    let doc: Value = serde_json::from_slice(&output.stdout).unwrap();
    let prove = doc["passes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pass"] == "prove")
        .unwrap_or_else(|| panic!("no prove pass in {doc}"))
        .clone();
    (prove, output.status.code())
}

// Pins today's wording: T4 says why the z3 does not run.
#[cfg(unix)]
#[test]
fn a_z3_that_does_not_run_is_called_not_on_path_today() {
    let dir = impossible_bounds();
    let (prove, code) = prove_report(dir.path(), &crate::toolchain::z3_broken(), &[]);
    assert_eq!(code, Some(0));
    assert_eq!(
        prove["findings"][0]["message"],
        "SMT solver 'z3' not found on PATH; formal entailment and consistency checks were skipped"
    );
}

#[cfg(unix)]
#[ignore = "fixed by T5"]
#[test]
fn doctor_answers_within_the_probe_timeout_when_z3_hangs() {
    let dir = impossible_bounds();
    let started = std::time::Instant::now();
    let out = specforge_cmd()
        .args(["doctor", "--format", "json", "--path"])
        .arg(dir.path())
        .env("PATH", crate::toolchain::z3_hung())
        .output()
        .unwrap();
    assert!(
        started.elapsed() < std::time::Duration::from_secs(15),
        "{:?}",
        started.elapsed()
    );
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    let toolchain = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["about"] == "toolchain")
        .expect("a toolchain finding");
    assert_eq!(toolchain["code"], "W098");
    assert!(
        toolchain["check"]
            .as_str()
            .unwrap()
            .contains("did not answer within 5s"),
        "{toolchain}"
    );
}
