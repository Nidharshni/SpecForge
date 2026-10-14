use crate::support::{Cli, TestProject};
use specforge_project::testing::specs;
use specforge_test_macros::test as specforge_test;

const SPEC_CONTENT: &str = r#"
behavior alpha "Alpha" { contract "first" }
behavior beta "Beta" { contract "second" }
feature gamma "Gamma" { behaviors [alpha, beta] }
"#;

#[test]
fn trace_produces_upstream_and_downstream_json() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", SPEC_CONTENT)]);

    let output = Cli::on(&dir).command(&["trace", "alpha"]).output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {}\noutput: {}", e, stdout));

    assert_eq!(parsed["entity_id"], "alpha");
    assert!(parsed["upstream"].is_array());
    assert!(parsed["downstream"].is_array());
    assert!(parsed["schema_version"].is_string());

    // alpha has upstream = gamma (gamma -> alpha via behaviors)
    let upstream = parsed["upstream"].as_array().unwrap();
    assert!(upstream.iter().any(|l| l["entity_id"] == "gamma"));
}

#[test]
fn trace_nonexistent_entity_exits_one() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", SPEC_CONTENT)]);

    Cli::on(&dir)
        .command(&["trace", "nonexistent"])
        .assert()
        .code(1);
}

/// `specforge trace --plan <plan>` over the project: the exit code, stdout, stderr.
fn trace_plan(dir: &TestProject, plan: &str, extra: &[&str]) -> (Option<i32>, String, String) {
    dir.write("plan.json", plan);
    let plan_path = dir.root().join("plan.json");
    let mut args = vec!["trace", "--plan", plan_path.to_str().unwrap()];
    args.extend(extra);
    let output = Cli::on(dir).run(&args);
    (Some(output.code), output.stdout, output.stderr)
}

#[specforge_test(
    behavior = "validate_agent_plan",
    verify = "specforge trace --plan exits 1 when an entry names no entity or comes before what it depends on"
)]
fn trace_plan_exits_by_the_plan_verdict() {
    let dir = TestProject::new().file(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" }
feature beta "B" { problem "p" solution "s" behaviors [alpha] }
"#,
    );
    let entry = |id: &str| format!(r#"{{"entity_id":"{id}","action":"implement"}}"#);
    let plan = |ids: &[&str]| {
        let entries: Vec<String> = ids.iter().map(|id| entry(id)).collect();
        format!(r#"{{"plan_id":"p","entries":[{}]}}"#, entries.join(","))
    };

    assert_eq!(trace_plan(&dir, &plan(&["alpha"]), &[]).0, Some(0));
    assert_eq!(trace_plan(&dir, &plan(&["ghost"]), &[]).0, Some(1));
    // beta depends on alpha: alpha first.
    assert_eq!(trace_plan(&dir, &plan(&["alpha", "beta"]), &[]).0, Some(0));
    assert_eq!(trace_plan(&dir, &plan(&["beta", "alpha"]), &[]).0, Some(1));

    // The human text ends with the verdict.
    let (_, stdout, _) = trace_plan(&dir, &plan(&["alpha"]), &["--format", "human"]);
    assert_eq!(
        stdout.lines().last(),
        Some("plan check: passed"),
        "{stdout}"
    );

    // A file that is not an AgentPlan, or is not there, is the command line's refusal.
    let (code, _, stderr) = trace_plan(&dir, r#"{"entries": []}"#, &[]);
    assert_eq!(code, Some(2));
    assert!(
        stderr.contains("plan must be an AgentPlan: plan: missing required plan_id"),
        "{stderr}"
    );
    let missing = dir.root().join("missing.json");
    let output = Cli::on(&dir).run(&["trace", "--plan", missing.to_str().unwrap()]);
    assert_eq!(output.code, 2);
    assert!(output.stderr.contains("cannot read"));
    // It conflicts with an entity.
    let plan_path = dir.root().join("plan.json");
    let output = Cli::on(&dir).run(&["trace", "alpha", "--plan", plan_path.to_str().unwrap()]);
    assert_eq!(output.code, 2);
}

#[specforge_test(
    behavior = "compute_traceability_chain",
    verify = "missing link in chain is flagged"
)]
fn trace_human_marks_missing_links() {
    let dir = TestProject::new()
        .enabling(&["@specforge/software", "@specforge/product"])
        .file("main.spec", specs::TRACE_GAPS);

    let output = Cli::on(&dir)
        .run(&["trace", "pay", "--format", "human"])
        .exits(0);
    assert_eq!(
        output.stdout,
        concat!(
            "pay [behavior]\n",
            "  upstream:\n",
            "    (none)\n",
            "  downstream:\n",
            "    -features-> checkout [feature] (depth 1)\n",
            "    -invariants-> paid_once [invariant] (depth 1)\n",
            "  missing:\n",
            "    MISSING pay -consumes-> [event] (depth 1)\n",
            "    MISSING pay -ports-> [port] (depth 1)\n",
            "    MISSING pay -produces-> [event] (depth 1)\n",
            "    MISSING pay -types-> [type] (depth 1)\n",
        )
    );

    // A chain with nothing missing has no missing section.
    let output = Cli::on(&dir)
        .run(&["trace", "checkout", "--format", "human"])
        .exits(0);
    let text = &output.stdout;
    assert!(text.starts_with("checkout [feature]\n"), "{text}");
    assert!(!text.contains("MISSING"), "{text}");
}
