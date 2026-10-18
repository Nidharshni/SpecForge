use super::compiled_support::{built, check_document, checked, exported, specs, trace_document};
use specforge_ops::analysis::check::CheckOptions;
use specforge_project::testing::TestProject;
use specforge_test_macros::test as specforge_test;
use std::collections::BTreeSet;

type EdgeSet = BTreeSet<(String, String, String)>;

/// Check `spec` (it must pass), then export its graph's edges.
fn checked_edges(spec: &str) -> EdgeSet {
    let (_project, compiled) = built(&[("main.spec", spec)]);
    assert!(checked(&compiled, CheckOptions::default()).ok());
    let parsed = exported(&compiled);
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

/// One edge per reference-list entry of specs::CROSS_REF, each written on the
/// field that owns its relation.
fn cross_ref_spec_edges() -> EdgeSet {
    edges(&[
        ("validate_graph", "graph_validation", "features"),
        ("resolve_refs", "graph_validation", "features"),
        ("validate_graph", "refs_resolved", "invariants"),
        ("validate_graph", "validation_complete", "produces"),
        ("unresolved_ref", "resolve_refs", "mitigations"),
    ])
}

// --- Phase 2a: Cross-extension references, I004 soft resolution, did-you-mean ---

#[specforge_test(
    behavior = "link_entity_references",
    verify = "reference list IDs create graph edges"
)]
fn cross_kind_references_resolve() {
    assert_eq!(checked_edges(specs::CROSS_REF), cross_ref_spec_edges());
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "unresolvable reference produces E003"
)]
fn unresolved_cross_ref_produces_e003() {
    let (_project, compiled) = built(&[(
        "main.spec",
        r#"
feature alpha "A" { problem "first" solution "s" }
behavior gamma "G" { contract "second" features [alpha, nonexistent_feature] }
"#,
    )]);

    let outcome = checked(&compiled, CheckOptions::default());
    assert!(!outcome.ok());
    let diagnostics = check_document(&compiled, CheckOptions::default());
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
feature fast_parsing "F" {
    problem "need speed"
    solution "be fast"
}
behavior parse_input "P" { contract "must parse" features [fast_parsing] }
"#,
    );
    assert_eq!(found, edges(&[("parse_input", "fast_parsing", "features")]));
}

#[specforge_test(
    behavior = "serialize_json_graph",
    verify = "JSON output contains all edges"
)]
fn export_includes_cross_kind_edges() {
    // Every reference-list entry of the spec, and nothing else.
    assert_eq!(checked_edges(specs::CROSS_REF), cross_ref_spec_edges());
}

#[specforge_test(
    behavior = "compute_traceability_chain",
    verify = "trace from entity shows upstream and downstream connections"
)]
fn trace_crosses_extension_boundaries() {
    let (_project, compiled) = built(&[("main.spec", specs::CROSS_REF)]);

    let trace = |id: &str| {
        let parsed = trace_document(&compiled, id);
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
    // (software behavior) -features-> graph_validation (product feature).
    // Nothing points at unresolved_ref.
    let (upstream, downstream) = trace("unresolved_ref");
    assert_eq!(upstream, set(&[]));
    assert_eq!(
        downstream,
        set(&[("resolve_refs", 1), ("graph_validation", 2)])
    );

    // graph_validation is the far end of two behaviors' `features`, and the
    // failure mode reaches it through resolve_refs: connected across kinds.
    let (upstream, downstream) = trace("graph_validation");
    assert_eq!(
        upstream,
        set(&[
            ("validate_graph", 1),
            ("resolve_refs", 1),
            ("unresolved_ref", 2),
        ])
    );
    assert_eq!(downstream, set(&[]));
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "close match triggers did-you-mean suggestion"
)]
fn did_you_mean_works_across_entity_kinds() {
    let (_project, compiled) = built(&[(
        "main.spec",
        r#"
feature graph_validation "G" {
    problem "must validate"
    solution "validation"
}
behavior validate_graph "V" { contract "must validate" features [graph_validatio] }
"#,
    )]);

    let rendered: String = check_document(&compiled, CheckOptions::default())
        .iter()
        .map(|d| format!("{} {}", d["message"], d["suggestion"]))
        .collect();
    assert!(
        rendered.contains("did you mean") && rendered.contains("graph_validation"),
        "should suggest correct entity across kinds: {}",
        rendered,
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
    let project = TestProject::new()
        .enabling(&["@specforge/software"])
        .config(|c| c["spec_root"] = "spec".into())
        .file("spec/main.spec", spec);
    let compiled = project.compile();
    assert!(
        checked(&compiled, CheckOptions::default()).ok(),
        "a soft reference is not an error"
    );
    let diagnostics = check_document(&compiled, CheckOptions::default());
    let codes: Vec<&str> = diagnostics
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .collect();
    assert!(!codes.contains(&"E003"), "{diagnostics:?}");
    let i004 = diagnostics
        .iter()
        .find(|d| d["code"] == "I004")
        .unwrap_or_else(|| panic!("I004 expected: {diagnostics:?}"));
    assert!(
        i004["message"]
            .as_str()
            .unwrap()
            .contains("targets kind 'feature'"),
        "{i004}"
    );

    // With product enabled and the feature declared, the same file resolves.
    let project = TestProject::new()
        .enabling(&["@specforge/software", "@specforge/product"])
        .config(|c| c["spec_root"] = "spec".into())
        .files(&[
            ("spec/main.spec", spec),
            (
                "spec/product.spec",
                "feature user_authentication \"Auth\" {\n  problem \"p\"\n}\n",
            ),
        ]);
    let graph = exported(&project.compile());
    assert!(
        graph["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| { e["source"] == "user_login" && e["target"] == "user_authentication" }),
        "the reference resolves once product is enabled: {graph}"
    );
}

// --- ADR 0117: the linker names the extension that declares an absent target kind ---

#[specforge_test(
    behavior = "register_extension_entity_types",
    verify = "I004 message includes unresolved kind name and suggested extension"
)]
fn an_absent_target_kinds_i004_names_the_extension_that_declares_it() {
    let project = TestProject::new()
        .enabling(&["@specforge/software"])
        .config(|c| c["spec_root"] = "spec".into())
        .file(
            "spec/main.spec",
            "behavior user_login \"Log in\" {\n  contract \"c\"\n  features [user_authentication]\n}\n",
        );
    let diagnostics = check_document(&project.compile(), CheckOptions::default());
    let i004: Vec<_> = diagnostics.iter().filter(|d| d["code"] == "I004").collect();
    assert_eq!(i004.len(), 1, "{diagnostics:?}");
    assert!(i004[0]["message"].as_str().unwrap().contains("'feature'"));
    assert_eq!(
        i004[0]["suggestion"],
        "enable the extension that declares 'feature' with `specforge add @specforge/product`, or drop the field"
    );
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "a reference to an existing entity of another kind than its field's target kind is E022 at the reference"
)]
fn a_wrong_kind_reference_to_an_absent_kind_is_e022_at_the_reference() {
    let project = TestProject::new()
        .enabling(&["@specforge/software"])
        .config(|c| c["spec_root"] = "spec".into())
        .file(
            "spec/main.spec",
            "behavior b1 \"B1\" {\n  contract \"x\"\n  features [b2, f_missing]\n}\nbehavior b2 \"B2\" {\n  contract \"y\"\n}\n",
        );
    let diagnostics = check_document(&project.compile(), CheckOptions::default());
    let codes: Vec<&str> = diagnostics
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .collect();
    assert_eq!(&codes[..2], ["E022", "I004"], "{diagnostics:?}");
    let span = &diagnostics[0]["span"];
    let at = (
        span["start_line"].as_u64(),
        span["start_col"].as_u64(),
        span["end_col"].as_u64(),
    );
    assert_eq!(at, (Some(3), Some(13), Some(15)), "{diagnostics:?}");
    assert!(
        diagnostics[0]["suggestion"]
            .as_str()
            .is_some_and(|s| s.contains("specforge add @specforge/product")),
        "{diagnostics:?}"
    );
}
