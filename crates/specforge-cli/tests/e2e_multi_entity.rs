use crate::e2e_fixtures::*;
use specforge_test_macros::test as specforge_test;

// --- Phase 1b: All entity kinds through check/export/query/trace/stats ---

#[test]
fn check_accepts_all_software_entity_kinds() {
    let dir = setup_project(&[("main.spec", SOFTWARE_SPEC)]);

    specforge_cmd()
        .args(["check"])
        .arg("--path")
        .arg(dir.path())
        .assert()
        .success();
}

#[test]
fn check_accepts_all_product_entity_kinds() {
    // Product spec references parse_input from software, include it
    let combined = format!("{}\n{}", SOFTWARE_SPEC, PRODUCT_SPEC);
    let dir = setup_project(&[("main.spec", &combined)]);

    specforge_cmd()
        .args(["check"])
        .arg("--path")
        .arg(dir.path())
        .assert()
        .success();
}

#[test]
fn check_accepts_all_governance_entity_kinds() {
    // Governance references parse_input via mitigations
    let combined = format!("{}\n{}", SOFTWARE_SPEC, GOVERNANCE_SPEC);
    let dir = setup_project(&[("main.spec", &combined)]);

    specforge_cmd()
        .args(["check"])
        .arg("--path")
        .arg(dir.path())
        .assert()
        .success();
}

#[specforge_test(
    behavior = "check_mode_for_ci",
    verify = "Check Mode for CI: CI check mode holds — validation_complete_fired, no_output_files_produced, diagnostics_to_stderr, appropriate_exit_code"
)]
fn check_full_multi_extension_project_exits_zero() {
    let dir = setup_project(&[("main.spec", MULTI_EXTENSION_SPEC)]);
    let tree = |root: &std::path::Path| {
        let mut entries: Vec<_> = walk(root)
            .into_iter()
            .map(|p| (p.clone(), std::fs::read(&p).unwrap_or_default()))
            .collect();
        entries.sort();
        entries
    };
    let before = tree(dir.path());

    // validation_complete_fired, appropriate_exit_code: a clean project
    // validates and exits 0.
    let output = specforge_cmd()
        .args(["check"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stderr}");

    // diagnostics_to_stderr: the report goes to stderr, nothing to stdout.
    assert!(output.stdout.is_empty());
    // No specforge.json: checked structurally, which I002 says.
    assert!(stderr.contains("0 errors, 0 warnings, 1 info"), "{stderr}");

    // no_output_files_produced
    assert_eq!(tree(dir.path()), before, "check must not write files");

    // An error in the same project flips the exit code, still on stderr.
    let broken =
        format!("{MULTI_EXTENSION_SPEC}\nfeature broken_ref \"B\" {{ behaviors [missing_one] }}\n");
    let dir = setup_project(&[("main.spec", &broken)]);
    let output = specforge_cmd()
        .args(["check"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(output.stdout.is_empty());
    assert!(stderr.contains("missing_one"), "{stderr}");
}

/// Every file under `dir`, recursively.
fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files
}

#[specforge_test(
    behavior = "serialize_json_graph",
    verify = "JSON output contains all nodes"
)]
fn export_graph_includes_all_entity_kinds() {
    let dir = setup_project(&[("main.spec", MULTI_EXTENSION_SPEC)]);

    let output = specforge_cmd()
        .args(["export", "--format=graph"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
    let nodes = parsed["nodes"].as_array().unwrap();

    let kinds: Vec<&str> = nodes.iter().map(|n| n["kind"].as_str().unwrap()).collect();
    for expected_kind in &[
        "behavior",
        "invariant",
        "event",
        "type",
        "port",
        "feature",
        "journey",
        "deliverable",
        "milestone",
        "module",
        "term",
        "decision",
        "constraint",
        "failure_mode",
    ] {
        assert!(
            kinds.contains(expected_kind),
            "missing kind '{}' in graph export. found: {:?}",
            expected_kind,
            kinds,
        );
    }
}

#[specforge_test(
    behavior = "export_agent_brief_format",
    verify = "brief format includes only IDs, kinds, titles, and edges"
)]
fn export_brief_includes_all_entity_kinds() {
    let dir = setup_project(&[("main.spec", MULTI_EXTENSION_SPEC)]);

    let output = specforge_cmd()
        .args(["export", "--format=brief"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
    let nodes = parsed["nodes"].as_array().unwrap();

    let kinds: Vec<&str> = nodes.iter().map(|n| n["kind"].as_str().unwrap()).collect();
    for expected_kind in &["behavior", "feature", "decision", "constraint"] {
        assert!(
            kinds.contains(expected_kind),
            "missing kind '{}' in brief",
            expected_kind
        );
    }

    // Only IDs, kinds and titles: no contract, fields or source location.
    for node in nodes {
        let keys: std::collections::BTreeSet<&str> = node
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert!(
            keys.is_subset(&["id", "kind", "title"].into_iter().collect()),
            "brief node carries more than id/kind/title: {node}"
        );
        assert!(keys.contains("id") && keys.contains("kind"), "{node}");
    }
    let parse_input = nodes.iter().find(|n| n["id"] == "parse_input").unwrap();
    assert_eq!(parse_input["kind"], "behavior");
    assert!(parse_input["title"].is_string());

    // And the edges, as the full graph export has them.
    let graph = specforge_cmd()
        .args(["export", "--format=graph"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();
    let graph = parse_json_stdout(&graph);
    assert!(!parsed["edges"].as_array().unwrap().is_empty());
    let edge_set = |v: &serde_json::Value| {
        v["edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["source"].as_str().unwrap().to_string(),
                    e["target"].as_str().unwrap().to_string(),
                    e["label"].as_str().unwrap().to_string(),
                )
            })
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(edge_set(&parsed), edge_set(&graph));
}

#[specforge_test(
    behavior = "export_agent_context_format",
    verify = "context format includes entity IDs and contracts"
)]
fn export_context_shows_contracts_for_all_kinds() {
    let dir = setup_project(&[
        ("specforge.json", BUILTINS_CONFIG),
        ("main.spec", MULTI_EXTENSION_SPEC),
    ]);

    let output = specforge_cmd()
        .args(["export", "--format=context"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
    let nodes = parsed["nodes"].as_array().unwrap();

    // Behaviors have contracts
    let parse_input = nodes.iter().find(|n| n["id"] == "parse_input").unwrap();
    assert!(
        parse_input.get("contract").is_some(),
        "behavior should have contract in context format"
    );
}

#[test]
fn query_by_kind_filter_product_entities() {
    let dir = setup_project(&[("main.spec", MULTI_EXTENSION_SPEC)]);

    let output = specforge_cmd()
        .args(["query", "fast_parsing", "--depth=0", "--kind=feature"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
    let nodes = parsed["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["kind"], "feature");
}

#[test]
fn query_by_kind_filter_governance_entities() {
    let dir = setup_project(&[("main.spec", MULTI_EXTENSION_SPEC)]);

    let output = specforge_cmd()
        .args(["query", "use_treesitter", "--depth=0", "--kind=decision"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
    let nodes = parsed["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["kind"], "decision");
}

#[specforge_test(
    behavior = "compute_project_statistics",
    verify = "stats reports correct entity counts"
)]
fn stats_entities_by_kind_counts_all_kinds() {
    let dir = setup_project(&[("main.spec", MULTI_EXTENSION_SPEC)]);

    let output = specforge_cmd()
        .args(["stats", "--format=json"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);

    // 15 entities total in MULTI_EXTENSION_SPEC (2 behaviors + 13 other kinds)
    let total = parsed["entity_count"].as_u64().unwrap();
    assert_eq!(total, 15, "expected 15 entities, got {}", total);

    let by_kind: std::collections::BTreeMap<&str, u64> = parsed["entity_counts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["kind"].as_str().unwrap(),
                row["count"].as_u64().unwrap(),
            )
        })
        .collect();
    for kind in ["behavior", "feature", "decision"] {
        assert!(
            by_kind.contains_key(kind),
            "missing {kind} in entity_counts"
        );
    }
    assert_eq!(by_kind["behavior"], 2, "expected 2 behaviors");
}

#[test]
fn stats_human_format_lists_all_kinds() {
    let dir = setup_project(&[("main.spec", MULTI_EXTENSION_SPEC)]);

    let output = specforge_cmd()
        .args(["stats"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

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

#[test]
fn multi_file_project_with_use_imports() {
    let dir = setup_project(&[
        (
            "behaviors.spec",
            r#"
behavior parse_input "Parse Input" {
    contract "The system MUST parse all valid input"
}
behavior emit_output "Emit Output" {
    contract "The system MUST emit structured output"
}
"#,
        ),
        (
            "features.spec",
            r#"
use "behaviors"

feature fast_parsing "Fast Parsing" {
    problem "Users need quick feedback"
    solution "Incremental parsing"
    behaviors [parse_input]
}
"#,
        ),
    ]);

    // Check succeeds with cross-file references
    specforge_cmd()
        .args(["check"])
        .arg("--path")
        .arg(dir.path())
        .assert()
        .success();

    // Export includes entities from both files
    let output = specforge_cmd()
        .args(["export", "--format=graph"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
    let nodes = parsed["nodes"].as_array().unwrap();
    assert!(nodes.len() >= 3, "should have entities from both files");
}
