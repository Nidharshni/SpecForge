use crate::e2e_fixtures::*;
use specforge_test_macros::test as specforge_test;
use std::collections::BTreeSet;

type EdgeSet = BTreeSet<(String, String, String)>;

/// Check `spec` (it must pass), then export its graph's edges.
fn checked_edges(spec: &str) -> EdgeSet {
    let dir = setup_project(&[("main.spec", spec)]);
    specforge_cmd()
        .args(["check"])
        .arg("--path")
        .arg(dir.path())
        .assert()
        .success();
    let output = specforge_cmd()
        .args(["export", "--format=graph"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
    let edges: Vec<(String, String, String)> = parsed["edges"]
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
        .collect();
    let set: EdgeSet = edges.iter().cloned().collect();
    assert_eq!(set.len(), edges.len(), "no duplicate edges: {edges:?}");
    set
}

fn edges(list: &[(&str, &str, &str)]) -> EdgeSet {
    list.iter()
        .map(|(s, t, l)| (s.to_string(), t.to_string(), l.to_string()))
        .collect()
}

/// One edge per reference-list entry of CROSS_REF_SPEC (trigger is a plain
/// field, not a reference list, so it makes no edge).
fn cross_ref_spec_edges() -> EdgeSet {
    edges(&[
        ("validate_graph", "graph_validation", "features"),
        ("resolve_refs", "graph_validation", "features"),
        ("graph_validation", "validate_graph", "behaviors"),
        ("graph_validation", "resolve_refs", "behaviors"),
        ("refs_resolved", "validate_graph", "enforced_by"),
        ("unresolved_ref", "resolve_refs", "mitigations"),
    ])
}

// --- Phase 2a: Cross-extension references, I004 soft resolution, did-you-mean ---

#[specforge_test(
    behavior = "link_entity_references",
    verify = "reference list IDs create graph edges"
)]
fn cross_kind_references_resolve() {
    assert_eq!(checked_edges(CROSS_REF_SPEC), cross_ref_spec_edges());
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "unresolvable reference produces E003"
)]
fn unresolved_cross_ref_produces_e003() {
    let dir = setup_project(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" }
feature gamma "G" { behaviors [alpha, nonexistent_behavior] }
"#,
    )]);

    let output = specforge_cmd()
        .args(["check", "--format=json"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let parsed = parse_json_stdout(&output);
    let diagnostics = parsed.as_array().unwrap();
    assert!(
        diagnostics.iter().any(|d| d["code"] == "E003"),
        "should have E003 for unresolved reference: {:?}",
        diagnostics,
    );
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "reference list IDs create graph edges"
)]
fn governance_references_software_entities() {
    let found = checked_edges(
        r#"
behavior parse_input "P" { contract "must parse" }
failure_mode parser_crash "PC" {
    severity 8
    occurrence 2
    detection 3
    cause "Bad input"
    effect "Crash"
    mitigations [parse_input]
}
"#,
    );
    assert_eq!(
        found,
        edges(&[("parser_crash", "parse_input", "mitigations")])
    );
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "reference list IDs create graph edges"
)]
fn product_references_software_entities() {
    let found = checked_edges(
        r#"
behavior parse_input "P" { contract "must parse" }
feature fast_parsing "F" {
    problem "need speed"
    solution "be fast"
    behaviors [parse_input]
}
"#,
    );
    assert_eq!(
        found,
        edges(&[("fast_parsing", "parse_input", "behaviors")])
    );
}

#[specforge_test(
    behavior = "export_diagnostics_as_json",
    verify = "diagnostics serialized as JSON array to stdout"
)]
fn check_json_shows_cross_extension_errors() {
    let dir = setup_project(&[(
        "main.spec",
        r#"
feature gamma "G" { behaviors [nonexistent] }
decision use_rust "D" {
    status accepted
    context "need performance"
    decision_text "use Rust"
    consequences "fast"
}
"#,
    )]);

    let output = specforge_cmd()
        .args(["check", "--format=json"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    let parsed = parse_json_stdout(&output);
    let diagnostics = parsed.as_array().unwrap();
    assert!(
        diagnostics.iter().any(|d| d["code"] == "E003"),
        "should produce E003 for nonexistent ref in cross-kind context"
    );
}

#[specforge_test(
    behavior = "serialize_json_graph",
    verify = "JSON output contains all edges"
)]
fn export_includes_cross_kind_edges() {
    // Every reference-list entry of the spec, and nothing else.
    assert_eq!(checked_edges(CROSS_REF_SPEC), cross_ref_spec_edges());
}

#[specforge_test(
    behavior = "compute_traceability_chain",
    verify = "trace from entity shows upstream and downstream connections"
)]
fn trace_crosses_extension_boundaries() {
    let dir = setup_project(&[("main.spec", CROSS_REF_SPEC)]);

    let trace = |id: &str| {
        let output = specforge_cmd()
            .args(["trace", id])
            .arg("--path")
            .arg(dir.path())
            .output()
            .unwrap();
        assert!(output.status.success());
        let parsed = parse_json_stdout(&output);
        assert_eq!(parsed["entity_id"], id);
        let ids = |direction: &str| -> BTreeSet<(String, u64)> {
            parsed[direction]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| {
                    (
                        l["entity_id"].as_str().unwrap().to_string(),
                        l["depth"].as_u64().unwrap(),
                    )
                })
                .collect()
        };
        (ids("upstream"), ids("downstream"))
    };
    let set = |list: &[(&str, u64)]| -> BTreeSet<(String, u64)> {
        list.iter().map(|(id, d)| (id.to_string(), *d)).collect()
    };

    // unresolved_ref (governance failure_mode) -mitigations-> resolve_refs
    // (software behavior) -features-> graph_validation (product feature)
    // -behaviors-> validate_graph. Nothing points at unresolved_ref.
    let (upstream, downstream) = trace("unresolved_ref");
    assert_eq!(upstream, set(&[]));
    assert_eq!(
        downstream,
        set(&[
            ("resolve_refs", 1),
            ("graph_validation", 2),
            ("validate_graph", 3)
        ])
    );

    // resolve_refs sits in the middle: connected both ways across kinds.
    let (upstream, downstream) = trace("resolve_refs");
    assert_eq!(
        upstream,
        set(&[
            ("graph_validation", 1),
            ("unresolved_ref", 1),
            ("validate_graph", 2),
            ("refs_resolved", 3),
        ])
    );
    assert_eq!(
        downstream,
        set(&[("graph_validation", 1), ("validate_graph", 2)])
    );
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "close match triggers did-you-mean suggestion"
)]
fn did_you_mean_works_across_entity_kinds() {
    let dir = setup_project(&[(
        "main.spec",
        r#"
behavior validate_graph "V" { contract "must validate" }
feature graph_validation "G" {
    problem "must validate"
    solution "validation"
    behaviors [validat_graph]
}
"#,
    )]);

    let output = specforge_cmd()
        .args(["check"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("did you mean") && stderr.contains("validate_graph"),
        "should suggest correct entity across kinds: {}",
        stderr,
    );
}

#[specforge_test(
    behavior = "cp_missing_product_from_software",
    verify = "I004 names the kind no enabled extension provides"
)]
#[specforge_test(
    behavior = "cp_missing_product_from_software",
    verify = "E003 not emitted for soft cross-extension reference"
)]
#[specforge_test(
    behavior = "cp_missing_product_from_software",
    verify = "references resolve after product is installed"
)]
fn software_without_product_treats_feature_references_as_soft() {
    let spec = r#"
behavior user_login "Log in" {
  contract "c"
  features [user_authentication]
}
"#;
    let dir = setup_project_with_config(
        r#"{"name":"t","version":"0.1.0","spec_root":"spec","extensions":["@specforge/software"]}"#,
        &[("main.spec", spec)],
    );
    let output = specforge_cmd()
        .args(["check", "--format=json"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "a soft reference is not an error"
    );
    let diagnostics = parse_json_stdout(&output);
    let codes: Vec<&str> = diagnostics
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .collect();
    assert!(!codes.contains(&"E003"), "{diagnostics}");
    let i004 = diagnostics
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "I004")
        .unwrap_or_else(|| panic!("I004 expected: {diagnostics}"));
    assert!(
        i004["message"]
            .as_str()
            .unwrap()
            .contains("targets kind 'feature'"),
        "{i004}"
    );

    // With product enabled and the feature declared, the same file resolves.
    let dir = setup_project_with_config(
        r#"{"name":"t","version":"0.1.0","spec_root":"spec","extensions":["@specforge/software","@specforge/product"]}"#,
        &[
            ("main.spec", spec),
            (
                "product.spec",
                "feature user_authentication \"Auth\" {\n  problem \"p\"\n}\n",
            ),
        ],
    );
    let output = specforge_cmd()
        .args(["export", "--format", "graph"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();
    let graph = parse_json_stdout(&output);
    assert!(
        graph["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| { e["source"] == "user_login" && e["target"] == "user_authentication" }),
        "the reference resolves once product is enabled: {graph}"
    );
}
