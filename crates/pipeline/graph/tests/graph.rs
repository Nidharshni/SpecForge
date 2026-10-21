use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Edge, FileChange, Graph, GraphBuild, GraphConfig, Node, SpecFile};
use specforge_parser::parse;
use specforge_test_macros::test as specforge_test;

fn make_node(id: &str, kind: &str) -> Node {
    let span = SourceSpan {
        file: Sym::new("test.spec"),
        start_line: 1,
        start_col: 1,
        end_line: 1,
        end_col: 1,
    };
    Node {
        title: Some(id.to_string()),
        ..Node::new(id, kind, span)
    }
}

fn make_edge(source: &str, target: &str, label: &str) -> Edge {
    Edge {
        source: Sym::new(source),
        target: Sym::new(target),
        label: Sym::new(label),
    }
}

// --- build_in_memory_graph ---
// (the entity-to-node and reference-to-edge proofs run through `build_graph`
// below: build_graph_one_node_per_entity / build_graph_one_edge_per_resolved_reference)

#[test]
fn graph_edges_connect_existing_nodes() {
    let mut graph = Graph::new();
    graph.add_node(make_node("feat", "feature"));
    graph.add_node(make_node("beh", "behavior"));
    graph.add_edge(make_edge("feat", "beh", "behaviors"));

    let edges = graph.edges();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].source, "feat");
    assert_eq!(edges[0].target, "beh");
    assert_eq!(edges[0].label, "behaviors");
}

// --- maintain_mutable_graph ---
// The graph changes one whole file at a time: `GraphBuild::apply` strips the
// changed files' entities and links every edge again, which is what the
// incremental update runs. Each step is checked against a cold build of the
// same files.

/// One behavior declaration in a file of its own.
fn behavior_in(file: &str, id: &str) -> SpecFile {
    parse(
        &format!("behavior {id} \"{id}\" {{\n  contract \"c\"\n}}\n"),
        file,
    )
}

/// A feature in `file` listing `behaviors`.
fn feature_in(file: &str, id: &str, behaviors: &[&str]) -> SpecFile {
    parse(
        &format!(
            "feature {id} \"{id}\" {{\n  behaviors [{}]\n}}\n",
            behaviors.join(", ")
        ),
        file,
    )
}

fn verified(files: Vec<SpecFile>) -> GraphBuild {
    let mut build = GraphBuild::of(files, GraphConfig::default());
    build.set_verify(true);
    build
}

fn edge_triples(graph: &Graph) -> Vec<(String, String, String)> {
    let mut edges: Vec<_> = graph
        .edges()
        .iter()
        .map(|e| {
            (
                e.source.to_string(),
                e.target.to_string(),
                e.label.to_string(),
            )
        })
        .collect();
    edges.sort();
    edges
}

#[specforge_test(
    behavior = "maintain_mutable_graph",
    verify = "add and remove nodes from graph"
)]
fn removing_a_file_removes_its_entities() {
    let mut build = verified(vec![
        behavior_in("a.spec", "alpha"),
        behavior_in("b.spec", "beta"),
    ]);

    let applied = build.apply([FileChange::Removed("a.spec".into())]);

    assert_eq!(applied.verification, Some(Ok(())));
    assert_eq!(build.graph().nodes().len(), 1);
    assert!(build.graph().node("alpha").is_none());
    assert!(build.graph().node("beta").is_some());
}

#[specforge_test(
    behavior = "maintain_mutable_graph",
    verify = "removing a node removes its edges"
)]
fn removing_a_file_removes_the_edges_of_its_entities() {
    let mut build = verified(vec![
        feature_in("feat.spec", "feat", &["beh"]),
        behavior_in("beh.spec", "beh"),
    ]);
    assert_eq!(build.graph().edges().len(), 1);

    let applied = build.apply([FileChange::Removed("beh.spec".into())]);

    assert_eq!(applied.verification, Some(Ok(())));
    assert!(
        build.graph().edges().is_empty(),
        "edges to removed node should be gone"
    );
}

#[specforge_test(
    behavior = "maintain_mutable_graph",
    verify = "removing a node removes its edges"
)]
fn edges_to_updated_after_node_removal() {
    let mut build = verified(vec![
        behavior_in("a.spec", "a"),
        behavior_in("b.spec", "b"),
        feature_in("c.spec", "c", &["a", "b"]),
    ]);
    assert_eq!(build.graph().edges_to("a").len(), 1);
    assert_eq!(build.graph().edges_to("b").len(), 1);

    let applied = build.apply([FileChange::Removed("a.spec".into())]);

    assert_eq!(applied.verification, Some(Ok(())));
    let graph = build.graph();
    assert!(
        graph.edges_to("a").is_empty(),
        "removed node should have no incoming edges"
    );
    assert_eq!(graph.edges_to("b").len(), 1, "b still has incoming edge");
    assert_eq!(
        graph.edges_from("c").len(),
        1,
        "c should have one edge left"
    );
}

#[specforge_test(
    behavior = "maintain_mutable_graph",
    verify = "graph consistency after batch mutations"
)]
fn graph_consistency_after_batch_mutations() {
    let mut build = verified(vec![
        behavior_in("a.spec", "a"),
        behavior_in("b.spec", "b"),
        feature_in("c.spec", "c", &["a", "b", "d"]),
    ]);

    // Remove a (its edge goes, c's dangling d stays unresolved), add d (its
    // edge appears), remove b.
    let first = build.apply([FileChange::Removed("a.spec".into())]);
    let second = build.apply([FileChange::Parsed(behavior_in("d.spec", "d"))]);
    let third = build.apply([FileChange::Removed("b.spec".into())]);

    for applied in [&first, &second, &third] {
        assert_eq!(applied.verification, Some(Ok(())));
    }
    let graph = build.graph();
    let ids: Vec<&str> = graph.nodes().iter().map(|n| n.id.raw.as_str()).collect();
    assert_eq!(ids, ["c", "d"]);
    assert_eq!(
        edge_triples(graph),
        [("c".to_string(), "d".to_string(), "behaviors".to_string())]
    );
    for edge in graph.edges() {
        assert!(graph.node(edge.source.as_str()).is_some(), "{edge:?}");
        assert!(graph.node(edge.target.as_str()).is_some(), "{edge:?}");
    }
    assert_eq!(graph.edges_from("c").len(), 1);
    assert_eq!(graph.edges_to("d").len(), 1);
}

// --- subgraph ---

#[test]
fn subgraph_for_file() {
    let mut graph = Graph::new();

    let mut node_a = make_node("alpha", "behavior");
    node_a.source_span.file = Sym::new("a.spec");
    let mut node_b = make_node("beta", "behavior");
    node_b.source_span.file = Sym::new("b.spec");
    let mut node_c = make_node("gamma", "feature");
    node_c.source_span.file = Sym::new("a.spec");

    graph.add_node(node_a);
    graph.add_node(node_b);
    graph.add_node(node_c);
    graph.add_edge(make_edge("gamma", "alpha", "behaviors"));

    let sub = graph.nodes_in_file("a.spec");
    assert_eq!(sub.len(), 2);
}

// === build_graph integration ===

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "graph contains one node per entity"
)]
fn build_graph_one_node_per_entity() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let source = r#"
feature alpha "A" { problem "first" }
feature beta "B" { problem "second" }
behavior gamma "G" { features [alpha, beta] }
"#;
    let spec_file = parse(source, "main.spec");
    let (graph, diagnostics) = build_graph(&[spec_file]);

    assert_eq!(graph.node_count(), 3, "one node per entity");
    assert!(graph.node("alpha").is_some());
    assert!(graph.node("beta").is_some());
    assert!(graph.node("gamma").is_some());
    assert!(
        diagnostics
            .iter()
            .all(|d| d.severity != specforge_graph::Severity::Error),
        "no errors: {:?}",
        diagnostics
    );
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "graph contains one edge per resolved reference"
)]
fn build_graph_one_edge_per_resolved_reference() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let source = r#"
feature alpha "A" { problem "first" }
feature beta "B" { problem "second" }
behavior gamma "G" { features [alpha, beta] }
"#;
    let spec_file = parse(source, "main.spec");
    let (graph, _) = build_graph(&[spec_file]);

    assert_eq!(graph.edge_count(), 2, "one edge per reference");
    let edges_from_gamma = graph.edges_from("gamma");
    assert_eq!(edges_from_gamma.len(), 2);
    assert!(edges_from_gamma.iter().any(|e| e.target == "alpha"));
    assert!(edges_from_gamma.iter().any(|e| e.target == "beta"));
    assert!(edges_from_gamma.iter().all(|e| e.label == "features"));
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "graph contains one edge per resolved reference"
)]
fn build_graph_unresolved_ref_produces_e003() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let source = r#"
feature alpha "A" { problem "first" }
behavior gamma "G" { features [alpha, nonexistent] }
"#;
    let spec_file = parse(source, "main.spec");
    let (graph, diagnostics) = build_graph(&[spec_file]);

    assert_eq!(graph.edge_count(), 1, "only valid ref becomes edge");
    // Unresolved references use E003 ("Unresolved reference"); E001 is parse-only.
    let errors: Vec<_> = diagnostics.iter().filter(|d| d.code == "E003").collect();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("nonexistent"));
    assert!(
        diagnostics.iter().all(|d| d.code != "E001"),
        "an unresolved reference must not be reported as a parse error (E001)"
    );
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "graph contains one node per entity"
)]
fn build_graph_duplicate_entity_id_produces_e002() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let file_a = parse(r#"behavior alpha "A" { contract "first" }"#, "a.spec");
    let file_b = parse(r#"behavior alpha "Duplicate" { contract "dup" }"#, "b.spec");
    let (graph, diagnostics) = build_graph(&[file_a, file_b]);

    let errors: Vec<_> = diagnostics.iter().filter(|d| d.code == "E002").collect();
    assert_eq!(errors.len(), 1, "duplicate ID should produce E002");
    assert!(errors[0].message.contains("alpha"));
    // First declaration wins — node still exists
    assert_eq!(graph.node_count(), 1);
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "same ID with different kinds does not produce E002"
)]
fn same_id_different_kind_no_e002() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let file_a = parse(r#"feature code_formatting "Code Formatting" { }"#, "a.spec");
    let file_b = parse(
        r#"milestone code_formatting "Phase 8: Code Formatting" { }"#,
        "b.spec",
    );
    let (graph, diagnostics) = build_graph(&[file_a, file_b]);

    let errors: Vec<_> = diagnostics.iter().filter(|d| d.code == "E002").collect();
    assert!(
        errors.is_empty(),
        "same ID with different kinds should not produce E002, got: {:?}",
        errors
    );
    // W060 should be emitted for cross-kind ID collision
    let w060: Vec<_> = diagnostics.iter().filter(|d| d.code == "W060").collect();
    assert_eq!(w060.len(), 1, "should emit W060 for same-ID-different-kind");

    // First-writer-wins: the first entity (feature) is retained, second (milestone) is skipped.
    // The graph stores one node per raw ID; the first declaration wins deterministically.
    assert_eq!(graph.node_count(), 1);
    let node = graph.node("code_formatting").expect("node should exist");
    assert_eq!(
        node.kind.raw.to_string(),
        "feature",
        "first-writer-wins: feature was declared first, should be retained"
    );
}

#[specforge_test(
    behavior = "two_phase_parse_structural",
    verify = "parse errors collected without aborting"
)]
fn build_graph_surfaces_parse_errors() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    // Malformed spec: `???` is not a valid entity ID, triggers parse errors
    let source = r#"
behavior good "Good" { status planned }
behavior ??? { broken content }
behavior also_good "Also Good" { status done }
"#;
    let spec = parse(source, "bad.spec");
    assert!(
        !spec.errors.is_empty(),
        "parser should report errors for malformed input"
    );

    let (graph, diagnostics) = build_graph(&[spec]);
    let parse_diags: Vec<_> = diagnostics.iter().filter(|d| d.code == "E001").collect();
    assert!(
        !parse_diags.is_empty(),
        "build_graph should surface parse errors as E001 diagnostics, got: {:?}",
        diagnostics
    );
    // Without aborting: the valid entities on either side of the error are still built.
    assert!(graph.node("good").is_some(), "entity before the error lost");
    assert!(
        graph.node("also_good").is_some(),
        "entity after the error lost: parsing aborted at the error"
    );
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "graph contains one edge per resolved reference"
)]
fn build_graph_cross_file_references() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let types = parse(r#"behavior alpha "A" { contract "first" }"#, "types.spec");
    let main = parse(r#"feature gamma "G" { behaviors [alpha] }"#, "main.spec");
    let (graph, diagnostics) = build_graph(&[types, main]);

    assert_eq!(graph.node_count(), 2);
    assert_eq!(graph.edge_count(), 1);
    assert_eq!(graph.edges_from("gamma")[0].target, "alpha");
    assert!(
        diagnostics
            .iter()
            .all(|d| d.severity != specforge_graph::Severity::Error),
        "cross-file ref should resolve: {:?}",
        diagnostics
    );
}

#[specforge_test(
    behavior = "provide_did_you_mean_suggestions",
    verify = "close match produces suggestion"
)]
fn build_graph_did_you_mean_for_close_match() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let source = r#"
feature alpha_parser "A" { problem "first" }
behavior gamma "G" { features [alpha_parsr] }
"#;
    let spec_file = parse(source, "main.spec");
    let (_, diagnostics) = build_graph(&[spec_file]);

    let errors: Vec<_> = diagnostics.iter().filter(|d| d.code == "E003").collect();
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0]
            .suggestion
            .as_ref()
            .is_some_and(|s| s.contains("alpha_parser")),
        "should suggest close match, got: {:?}",
        errors[0].suggestion
    );
}

#[specforge_test(
    behavior = "provide_did_you_mean_suggestions",
    verify = "distant match produces no suggestion"
)]
fn build_graph_no_suggestion_for_distant_match() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let source = r#"
feature alpha "A" { problem "first" }
behavior gamma "G" { features [zzzzz_totally_unrelated] }
"#;
    let spec_file = parse(source, "main.spec");
    let (_, diagnostics) = build_graph(&[spec_file]);

    let errors: Vec<_> = diagnostics.iter().filter(|d| d.code == "E003").collect();
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].suggestion.is_none(),
        "distant match should not produce suggestion, got: {:?}",
        errors[0].suggestion
    );
}

#[specforge_test(
    behavior = "provide_did_you_mean_suggestions",
    verify = "suggestion appears in help text"
)]
fn build_graph_suggestion_appears_in_help_text() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let source = r#"
feature alpha_parser "A" { problem "first" }
behavior gamma "G" { features [alpha_parsr] }
"#;
    let spec_file = parse(source, "main.spec");
    let (_, diagnostics) = build_graph(&[spec_file]);

    let errors: Vec<_> = diagnostics.iter().filter(|d| d.code == "E003").collect();
    assert_eq!(errors.len(), 1);
    let suggestion = errors[0]
        .suggestion
        .as_ref()
        .expect("should have suggestion");
    assert!(
        suggestion.contains("did you mean") && suggestion.contains("alpha_parser"),
        "suggestion should be human-readable help text, got: {:?}",
        suggestion
    );
}

// === edge types match relationship semantics ===

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "edge types match relationship semantics"
)]
fn edge_labels_match_field_names() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let source = r#"
feature alpha "A" { problem "first" }
invariant inv1 "I1" { guarantee "always" }
behavior gamma "G" {
  features [alpha]
  invariants [inv1]
}
"#;
    let spec_file = parse(source, "main.spec");
    let (graph, diagnostics) = build_graph(&[spec_file]);

    assert!(
        diagnostics
            .iter()
            .all(|d| d.severity != specforge_graph::Severity::Error),
        "no errors: {:?}",
        diagnostics
    );
    let edges = graph.edges_from("gamma");
    assert_eq!(edges.len(), 2, "two edges from gamma");

    let feature_edge = edges.iter().find(|e| e.target == "alpha").unwrap();
    assert_eq!(
        feature_edge.label, "features",
        "edge label matches field name"
    );

    let invariant_edge = edges.iter().find(|e| e.target == "inv1").unwrap();
    assert_eq!(
        invariant_edge.label, "invariants",
        "edge label matches field name"
    );
}

// === ref blocks as graph nodes ===

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "graph contains one node per entity"
)]
fn ref_blocks_become_graph_nodes() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let source = r#"
behavior alpha "A" { contract "first" }
ref gh.issue:42 "Support Wasm extensions"
"#;
    let spec_file = parse(source, "main.spec");
    let (graph, _) = build_graph(&[spec_file]);

    assert_eq!(graph.node_count(), 2);
    let ref_node = graph.node("gh.issue:42").expect("ref should be a node");
    assert_eq!(ref_node.kind.raw, "ref");
}

#[specforge_test(
    behavior = "resolve_external_ref_declarations",
    verify = "ref node is added to graph with scheme metadata"
)]
fn ref_blocks_carry_scheme_metadata() {
    use specforge_graph::{FieldValue, build_graph};
    use specforge_parser::parse;

    let source = r#"ref gh.issue:42 "Support Wasm extensions""#;
    let spec_file = parse(source, "main.spec");
    let (graph, _) = build_graph(&[spec_file]);

    let ref_node = graph.node("gh.issue:42").unwrap();
    let scheme = ref_node.fields.get("scheme");
    assert!(
        matches!(scheme, Some(FieldValue::String(s)) if s == "gh"),
        "ref node should carry scheme field"
    );
}

// === resolve_external_ref_declarations ===

#[specforge_test(
    behavior = "resolve_external_ref_declarations",
    verify = "ref with unknown scheme emits I005"
)]
fn ref_with_unknown_scheme_emits_i005() {
    use specforge_graph::{GraphConfig, build_graph_with_config};
    use specforge_parser::parse;

    let source = r#"ref gh.issue:42 "Support Wasm extensions""#;
    let spec_file = parse(source, "main.spec");
    let config = GraphConfig {
        known_provider_schemes: vec!["jira".to_string()].into_iter().collect(),
        ..Default::default()
    };
    let (_, diagnostics) = build_graph_with_config(&[spec_file], &config);

    let infos: Vec<_> = diagnostics.iter().filter(|d| d.code == "I005").collect();
    assert_eq!(infos.len(), 1, "unknown scheme 'gh' should produce I005");
    assert!(
        infos[0].message.contains("gh"),
        "I005 should mention the scheme"
    );
}

#[test]
fn ref_with_no_providers_configured_skips_i005() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let source = r#"ref gh.issue:42 "Support Wasm extensions""#;
    let spec_file = parse(source, "main.spec");
    // Default build_graph — no providers configured
    let (_, diagnostics) = build_graph(&[spec_file]);

    let infos: Vec<_> = diagnostics.iter().filter(|d| d.code == "I005").collect();
    assert!(infos.is_empty(), "no I005 when no providers configured");
}

#[specforge_test(
    behavior = "resolve_external_ref_declarations",
    verify = "ref with known scheme is registered and marked for provider validation"
)]
fn ref_with_known_scheme_registered_and_no_i005() {
    use specforge_graph::{FieldValue, GraphConfig, build_graph_with_config};
    use specforge_parser::parse;

    let source = r#"ref gh.issue:42 "Support Wasm extensions""#;
    let spec_file = parse(source, "main.spec");
    let config = GraphConfig {
        known_provider_schemes: vec!["gh".to_string()].into_iter().collect(),
        ..Default::default()
    };
    let (graph, diagnostics) = build_graph_with_config(&[spec_file], &config);

    let infos: Vec<_> = diagnostics.iter().filter(|d| d.code == "I005").collect();
    assert!(infos.is_empty(), "known scheme should not produce I005");

    // Ref node is registered in the graph with scheme metadata
    let ref_node = graph
        .node("gh.issue:42")
        .expect("ref should be a graph node");
    assert_eq!(ref_node.kind.raw, "ref");
    assert!(
        matches!(ref_node.fields.get("scheme"), Some(FieldValue::String(s)) if s == "gh"),
        "ref node should carry scheme field 'gh'"
    );
}

// === resolve_soft_cross_extension_references: installed keyword + missing entity ===

#[specforge_test(
    behavior = "resolve_soft_cross_extension_references",
    verify = "installed extension with missing entity emits E003"
)]
fn installed_keyword_with_missing_entity_emits_e003() {
    use specforge_graph::{GraphConfig, build_graph_with_config};
    use specforge_parser::parse;

    // "behavior" is installed, but "nonexistent" entity doesn't exist
    let source = r#"
feature alpha "A" { problem "first" }
behavior gamma "G" { features [alpha, nonexistent] }
"#;
    let spec_file = parse(source, "main.spec");
    let config = GraphConfig::default();
    let (_, diagnostics) = build_graph_with_config(&[spec_file], &config);

    // Should get E003 (unresolved ref), and nothing else
    let e003s: Vec<_> = diagnostics.iter().filter(|d| d.code == "E003").collect();
    assert_eq!(e003s.len(), 1, "missing entity should produce E003");
    assert!(e003s[0].message.contains("nonexistent"));
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
}

#[specforge_test(
    behavior = "resolve_soft_cross_extension_references",
    verify = "installed extension with imported file but missing entity emits E003"
)]
fn installed_keyword_cross_file_missing_entity_emits_e003() {
    use specforge_graph::{GraphConfig, build_graph_with_config};
    use specforge_parser::parse;

    // "behavior" is installed, file is imported, but referenced entity doesn't exist there
    let types = parse(r#"feature alpha "A" { problem "first" }"#, "types.spec");
    let main = parse(
        r#"behavior gamma "G" { features [alpha, missing_beta] }"#,
        "main.spec",
    );
    let config = GraphConfig::default();
    let (graph, diagnostics) = build_graph_with_config(&[types, main], &config);

    let e003s: Vec<_> = diagnostics.iter().filter(|d| d.code == "E003").collect();
    assert_eq!(
        e003s.len(),
        1,
        "missing cross-file entity should produce E003"
    );
    assert!(e003s[0].message.contains("missing_beta"));
    assert_eq!(graph.edge_count(), 1, "only valid ref alpha becomes edge");
}

// === end-to-end pipeline: filesystem → resolve → build_graph ===

fn project_dir(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    for (path, content) in files {
        let full = dir.path().join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&full, content).unwrap();
    }
    dir
}

/// The `.spec` files under `dir`, read and parsed as a compile reads them.
fn parse_dir(dir: &std::path::Path) -> Vec<specforge_graph::SpecFile> {
    specforge_common::walk_spec_files(dir, |_| false)
        .files
        .into_iter()
        .map(|path| {
            let key = path
                .strip_prefix(dir)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            specforge_parser::parse(&std::fs::read_to_string(&path).unwrap(), &key)
        })
        .collect()
}

/// The project at `dir`, read and built into a graph.
fn resolve_and_build(dir: &std::path::Path) -> (Graph, Vec<specforge_common::Diagnostic>) {
    specforge_graph::build_graph(&parse_dir(dir))
}

// The E003 span covers the unresolved identifier token itself, not the
// whole entity block, even when the bad entry is not first and the entity
// spans several lines.
#[specforge_test(
    behavior = "link_entity_references",
    verify = "E003 span covers exactly the unresolved identifier token"
)]
fn e003_span_points_at_reference_token() {
    let dir = project_dir(&[(
        "main.spec",
        r#"
behavior fulfillment "F" { contract "ships orders" }
entity order {
  title "Order"
  depends_on [fulfillment, ghost]
}"#,
    )]);

    let (_, diagnostics) = resolve_and_build(dir.path());

    let errors: Vec<_> = diagnostics.iter().filter(|d| d.code == "E003").collect();
    assert_eq!(errors.len(), 1, "{diagnostics:?}");
    assert!(errors[0].message.contains("ghost"));
    // `ghost` sits on line 5 of main.spec in
    // `  depends_on [fulfillment, ghost]`, columns 28-32.
    let span = errors[0].span.as_ref().expect("E003 must carry a span");
    assert!(span.file.as_str().ends_with("main.spec"), "{}", span.file);
    assert_eq!(span.start_line, 5, "bad entry is on the 5th source line");
    assert_eq!(span.start_col, 28, "span starts at 'g' of ghost");
    assert_eq!(span.end_line, 5, "span must not extend past the token");
    assert_eq!(span.end_col, 33, "exclusive end column, one past 't'");
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "Link Entity References: entity reference linking holds — registries_populated, all_files_parsed, all_references_resolved, no_silent_ignoring"
)]
fn link_entity_references_contract() {
    // all_files_parsed: the references cross files.
    let dir = project_dir(&[
        (
            "a.spec",
            "behavior alpha \"A\" { contract \"first\" }\nbehavior beta \"B\" { contract \"second\" }\n",
        ),
        (
            "main.spec",
            "feature gamma \"G\" {\n  behaviors [alpha, beta, nonexistent]\n}\n",
        ),
    ]);

    let (graph, diagnostics) = resolve_and_build(dir.path());

    // all_references_resolved: each resolvable reference is an edge from
    // gamma.
    let targets: Vec<&str> = graph
        .edges_from("gamma")
        .iter()
        .map(|e| e.target.as_str())
        .collect();
    assert_eq!(targets.len(), 2, "{targets:?}");
    assert!(targets.contains(&"alpha") && targets.contains(&"beta"));
    // no_silent_ignoring: the unresolvable one is an E003 naming it.
    let e003: Vec<_> = diagnostics.iter().filter(|d| d.code == "E003").collect();
    assert_eq!(e003.len(), 1, "{diagnostics:?}");
    assert!(e003[0].message.contains("nonexistent"));
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "cross-file duplicate entity ID produces E002 naming the first declaration"
)]
fn cross_file_duplicate_entity_id_produces_e002() {
    let dir = project_dir(&[
        (
            "a.spec",
            r#"behavior alpha "Alpha in file A" { contract "first" }"#,
        ),
        (
            "b.spec",
            r#"behavior alpha "Alpha in file B" { contract "second" }"#,
        ),
    ]);

    let (_, diagnostics) = resolve_and_build(dir.path());

    let e002: Vec<_> = diagnostics.iter().filter(|d| d.code == "E002").collect();
    assert_eq!(e002.len(), 1, "{diagnostics:?}");
    assert!(e002[0].message.contains("'alpha'"), "{}", e002[0].message);
    assert!(e002[0].message.contains("a.spec"), "{}", e002[0].message);
    assert!(
        e002[0]
            .span
            .as_ref()
            .unwrap()
            .file
            .as_str()
            .ends_with("b.spec")
    );
}

// One ID declared as two kinds across files is an ambiguous identity, and
// warns.
#[specforge_test(
    behavior = "link_entity_references",
    verify = "same ID different kind across files warns W060 with both kinds named"
)]
fn same_id_different_kind_across_files_warns_w060() {
    let dir = project_dir(&[
        (
            "a.spec",
            r#"behavior alpha "Alpha behavior" { contract "first" }"#,
        ),
        (
            "b.spec",
            r#"feature alpha "Alpha feature" { problem "different kind" }"#,
        ),
    ]);

    let (_, diagnostics) = resolve_and_build(dir.path());

    let w060: Vec<_> = diagnostics.iter().filter(|d| d.code == "W060").collect();
    assert_eq!(w060.len(), 1, "{diagnostics:?}");
    let msg = &w060[0].message;
    assert!(msg.contains("behavior") && msg.contains("feature"), "{msg}");
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "graph contains one node per entity"
)]
fn end_to_end_resolve_and_build() {
    use specforge_graph::build_graph;

    let dir = project_dir(&[
        ("types.spec", r#"feature alpha "A" { problem "first" }"#),
        (
            "main.spec",
            "use \"types\"\nbehavior gamma \"G\" {\n  features [alpha]\n}",
        ),
    ]);

    let (graph, diagnostics) = build_graph(&parse_dir(dir.path()));

    assert!(
        diagnostics
            .iter()
            .all(|d| d.severity != specforge_graph::Severity::Error),
        "build errors: {:?}",
        diagnostics
    );
    assert_eq!(graph.node_count(), 2, "alpha + gamma");
    assert_eq!(graph.edge_count(), 1, "gamma -> alpha");
    assert_eq!(graph.edges_from("gamma")[0].target, "alpha");
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "graph contains one edge per resolved reference"
)]
fn end_to_end_with_errors() {
    use specforge_graph::build_graph;

    let dir = project_dir(&[(
        "main.spec",
        r#"
feature alpha "A" { problem "first" }
behavior gamma "G" { features [alpha, nonexistent] }
"#,
    )]);

    let (graph, diagnostics) = build_graph(&parse_dir(dir.path()));

    assert_eq!(graph.node_count(), 2);
    assert_eq!(graph.edge_count(), 1, "only valid ref becomes edge");
    let errors: Vec<_> = diagnostics.iter().filter(|d| d.code == "E003").collect();
    assert_eq!(errors.len(), 1);
}

// === edge index correctness ===

#[specforge_test(
    behavior = "maintain_mutable_graph",
    verify = "added edges are reflected in outgoing edge queries"
)]
fn edges_from_returns_correct_edges_after_multiple_adds() {
    let mut graph = Graph::new();
    graph.add_node(make_node("a", "behavior"));
    graph.add_node(make_node("b", "behavior"));
    graph.add_node(make_node("c", "feature"));
    graph.add_node(make_node("d", "feature"));
    graph.add_edge(make_edge("c", "a", "behaviors"));
    graph.add_edge(make_edge("c", "b", "behaviors"));
    graph.add_edge(make_edge("d", "a", "behaviors"));

    let from_c = graph.edges_from("c");
    assert_eq!(from_c.len(), 2);
    assert!(from_c.iter().any(|e| e.target == "a"));
    assert!(from_c.iter().any(|e| e.target == "b"));

    let from_d = graph.edges_from("d");
    assert_eq!(from_d.len(), 1);
    assert_eq!(from_d[0].target, "a");

    let from_a = graph.edges_from("a");
    assert!(from_a.is_empty(), "a has no outgoing edges");
}

// === cycle detection in build_graph ===

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "build_graph emits W061 for reference cycles"
)]
fn build_graph_emits_w061_for_reference_cycles() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let source = r#"
behavior a "A" { contract "c" depends_on [b] }
behavior b "B" { contract "c" depends_on [c] }
behavior c "C" { contract "c" depends_on [a] }
"#;
    let spec_file = parse(source, "main.spec");
    let (_, diagnostics) = build_graph(&[spec_file]);

    let w061s: Vec<_> = diagnostics.iter().filter(|d| d.code == "W061").collect();
    assert!(!w061s.is_empty(), "reference cycles should produce W061");
    assert!(
        w061s[0].message.contains("cycle"),
        "W061 should mention cycle"
    );
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "W061 names the cycle's entities in its data"
)]
fn w061_names_the_cycles_entities_in_its_data() {
    use specforge_common::DiagnosticData;
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let source = r#"
behavior a "A" { contract "c" depends_on [b] }
behavior b "B" { contract "c" depends_on [a] }
"#;
    let (_, diagnostics) = build_graph(&[parse(source, "main.spec")]);
    let w061 = diagnostics
        .iter()
        .find(|d| d.code == "W061")
        .expect("a two-cycle is a W061");
    assert!(w061.span.is_none(), "a cycle has no one place");
    let data = w061.data.as_deref().expect("W061 carries data");
    assert_eq!(
        data,
        &DiagnosticData::ReferenceCycle {
            path: vec!["a".into(), "b".into(), "a".into()]
        }
    );
    assert_eq!(
        data.entities(),
        ["a", "b"],
        "each entity once, in path order"
    );
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "build_graph no W061 for acyclic refs"
)]
fn build_graph_no_w061_for_acyclic_refs() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let source = r#"
feature f "F" { problem "c" }
behavior a "A" { contract "c" features [f] }
behavior b "B" { contract "c" features [f] depends_on [a] }
"#;
    let spec_file = parse(source, "main.spec");
    let (_, diagnostics) = build_graph(&[spec_file]);

    let w061s: Vec<_> = diagnostics.iter().filter(|d| d.code == "W061").collect();
    assert!(w061s.is_empty(), "acyclic graph should not produce W061");
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "graph contains one edge per resolved reference"
)]
fn union_type_variants_do_not_produce_unresolved_ref() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let source = r#"
type Status = active | inactive | archived
behavior my_behavior "B" {
    contract "must track status"
}
"#;
    let spec_file = parse(source, "main.spec");
    let (graph, diagnostics) = build_graph(&[spec_file]);

    // Variant identifiers are not reference lists, so they must not be reported
    // as unresolved references (E003) or parse errors (E001).
    let unresolved: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == "E001" || d.code == "E003")
        .collect();
    assert!(
        unresolved.is_empty(),
        "union variants should not produce E001/E003: {:?}",
        unresolved
    );
    assert_eq!(graph.node_count(), 2, "type + behavior");
    assert_eq!(graph.edge_count(), 0, "no edges from variant lists");
}

// === Phase 9: Predicate Query API ===

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "kind filter restricts results to specified entity kinds"
)]
fn filter_nodes_by_kind() {
    let mut graph = Graph::new();
    graph.add_node(make_node("login", "behavior"));
    graph.add_node(make_node("data_ok", "invariant"));
    graph.add_node(make_node("logout", "behavior"));
    graph.add_node(make_node("user_id", "type"));

    let behaviors = graph.filter_nodes(|n| n.kind.raw == "behavior");
    assert_eq!(behaviors.len(), 2, "should find 2 behaviors");

    let invariants = graph.nodes_by_kind("invariant");
    assert_eq!(invariants.len(), 1, "should find 1 invariant");
    assert_eq!(invariants[0].id.raw.as_str(), "data_ok");
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "filter_nodes with field predicate"
)]
fn filter_nodes_by_field_value() {
    use specforge_parser::FieldValue;

    let mut graph = Graph::new();
    let mut node1 = make_node("login", "behavior");
    node1.fields.push(
        Sym::new("status"),
        FieldValue::Identifier("planned".to_string()),
    );
    graph.add_node(node1);

    let mut node2 = make_node("logout", "behavior");
    node2.fields.push(
        Sym::new("status"),
        FieldValue::Identifier("implemented".to_string()),
    );
    graph.add_node(node2);

    let planned = graph.filter_nodes(
        |n| matches!(n.fields.get("status"), Some(FieldValue::Identifier(s)) if s == "planned"),
    );
    assert_eq!(planned.len(), 1);
    assert_eq!(planned[0].id.raw.as_str(), "login");
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "kind filter restricts results to specified entity kinds"
)]
fn nodes_by_kind_returns_empty_for_unknown() {
    let mut graph = Graph::new();
    graph.add_node(make_node("login", "behavior"));
    graph.add_node(make_node("user_id", "type"));

    // A known kind returns exactly its members (so the filter is not "always empty")...
    let behaviors: Vec<&str> = graph
        .nodes_by_kind("behavior")
        .iter()
        .map(|n| n.id.raw.as_str())
        .collect();
    assert_eq!(behaviors, vec!["login"]);
    // ...and a kind no node has returns nothing (not "everything").
    assert!(graph.nodes_by_kind("nonexistent").is_empty());
}

// --- one-way authoring, bidirectional query ---

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "depth 1 returns direct neighbors"
)]
fn subgraph_depth_finds_behavior_via_incoming_implements_edge() {
    let mut graph = Graph::new();
    // Feature has NO behaviors field -- it's a plain node
    graph.add_node(make_node("my_feature", "feature"));
    // Behavior declares features [my_feature] -- creates an Implements edge
    graph.add_node(make_node("my_behavior", "behavior"));
    graph.add_edge(make_edge(
        "my_behavior",
        "my_feature",
        "BehaviorImplementsFeature",
    ));

    // Querying the feature at depth 1 should find the behavior
    // via the INCOMING Implements edge -- no reverse edge needed
    let sub = graph.subgraph_depth("my_feature", 1).unwrap();
    assert_eq!(sub.nodes().len(), 2, "feature + behavior");
    assert!(sub.node("my_feature").is_some());
    assert!(sub.node("my_behavior").is_some());
    assert_eq!(sub.edges().len(), 1);
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "depth 1 returns direct neighbors"
)]
fn subgraph_depth_no_reverse_edge_needed_for_feature_behavior_link() {
    let mut graph = Graph::new();
    graph.add_node(make_node("feat_a", "feature"));
    graph.add_node(make_node("beh_1", "behavior"));
    graph.add_node(make_node("beh_2", "behavior"));
    // Both behaviors point to feat_a -- one-way authoring
    graph.add_edge(make_edge("beh_1", "feat_a", "BehaviorImplementsFeature"));
    graph.add_edge(make_edge("beh_2", "feat_a", "BehaviorImplementsFeature"));

    let sub = graph.subgraph_depth("feat_a", 1).unwrap();
    assert_eq!(sub.nodes().len(), 3, "feature + 2 behaviors");
    assert!(sub.node("beh_1").is_some());
    assert!(sub.node("beh_2").is_some());
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "depth 1 returns direct neighbors"
)]
fn subgraph_depth_invariant_found_via_incoming_enforced_by_edge() {
    let mut graph = Graph::new();
    graph.add_node(make_node("my_invariant", "invariant"));
    graph.add_node(make_node("my_behavior", "behavior"));
    // Behavior says invariants [my_invariant] -- one-way authoring
    graph.add_edge(make_edge("my_behavior", "my_invariant", "invariants"));

    // Querying the invariant should find the behavior via incoming edge
    let sub = graph.subgraph_depth("my_invariant", 1).unwrap();
    assert_eq!(sub.nodes().len(), 2);
    assert!(sub.node("my_behavior").is_some());
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "depth 1 returns direct neighbors"
)]
fn subgraph_depth_constraint_found_via_incoming_constrains_edge() {
    let mut graph = Graph::new();
    graph.add_node(make_node("my_constraint", "constraint"));
    graph.add_node(make_node("my_behavior", "behavior"));
    // Constraint says constrains [my_behavior] -- one-way authoring
    graph.add_edge(make_edge("my_constraint", "my_behavior", "constrains"));

    // Querying the behavior should find the constraint via incoming edge
    let sub = graph.subgraph_depth("my_behavior", 1).unwrap();
    assert_eq!(sub.nodes().len(), 2);
    assert!(sub.node("my_constraint").is_some());
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "depth 1 returns direct neighbors"
)]
fn subgraph_depth_event_found_via_incoming_produces_edge() {
    let mut graph = Graph::new();
    graph.add_node(make_node("my_event", "event"));
    graph.add_node(make_node("my_behavior", "behavior"));
    // Behavior says produces [my_event] -- one-way authoring
    graph.add_edge(make_edge(
        "my_behavior",
        "my_event",
        "BehaviorProducesEvent",
    ));

    // Querying the event should find the producing behavior via incoming edge
    let sub = graph.subgraph_depth("my_event", 1).unwrap();
    assert_eq!(sub.nodes().len(), 2);
    assert!(sub.node("my_behavior").is_some());
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "depth 1 returns direct neighbors"
)]
fn subgraph_depth_event_found_via_incoming_consumes_edge() {
    let mut graph = Graph::new();
    graph.add_node(make_node("my_event", "event"));
    graph.add_node(make_node("consumer_behavior", "behavior"));
    // Behavior says consumes [my_event] -- one-way authoring
    graph.add_edge(make_edge(
        "consumer_behavior",
        "my_event",
        "BehaviorConsumesEvent",
    ));

    // Querying the event should find the consuming behavior via incoming edge
    let sub = graph.subgraph_depth("my_event", 1).unwrap();
    assert_eq!(sub.nodes().len(), 2);
    assert!(sub.node("consumer_behavior").is_some());
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "depth 1 returns direct neighbors"
)]
fn subgraph_depth_invariant_found_via_incoming_invariants_edge() {
    let mut graph = Graph::new();
    graph.add_node(make_node("inv_1", "invariant"));
    graph.add_node(make_node("beh_a", "behavior"));
    graph.add_node(make_node("beh_b", "behavior"));
    // Both behaviors declare invariants [inv_1] -- one-way authoring
    graph.add_edge(make_edge("beh_a", "inv_1", "BehaviorEnforcesInvariant"));
    graph.add_edge(make_edge("beh_b", "inv_1", "BehaviorEnforcesInvariant"));

    // Querying the invariant should find both behaviors
    let sub = graph.subgraph_depth("inv_1", 1).unwrap();
    assert_eq!(sub.nodes().len(), 3, "invariant + 2 behaviors");
    assert!(sub.node("beh_a").is_some());
    assert!(sub.node("beh_b").is_some());
}

// === M3/M4: Diagnostics carry actionable suggestions ===

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "W061 carries actionable suggestion"
)]
fn w061_cycle_warning_has_suggestion() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let source = r#"
behavior a "A" { contract "c" depends_on [b] }
behavior b "B" { contract "c" depends_on [c] }
behavior c "C" { contract "c" depends_on [a] }
"#;
    let spec_file = parse(source, "main.spec");
    let (_, diagnostics) = build_graph(&[spec_file]);

    let w061s: Vec<_> = diagnostics.iter().filter(|d| d.code == "W061").collect();
    assert!(!w061s.is_empty(), "reference cycles should produce W061");
    assert!(
        w061s[0].suggestion.is_some(),
        "W061 should carry an actionable suggestion, got None"
    );
    assert!(
        w061s[0]
            .suggestion
            .as_ref()
            .unwrap()
            .contains("break the cycle"),
        "W061 suggestion should advise breaking the cycle, got: {:?}",
        w061s[0].suggestion
    );
}

#[specforge_test(
    behavior = "detect_duplicate_entity_ids",
    verify = "duplicate ID across files produces E002"
)]
fn e002_duplicate_entity_has_suggestion() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let file_a = parse(r#"behavior alpha "A" { contract "first" }"#, "a.spec");
    let file_b = parse(r#"behavior alpha "Duplicate" { contract "dup" }"#, "b.spec");
    let (_, diagnostics) = build_graph(&[file_a, file_b]);

    let e002s: Vec<_> = diagnostics.iter().filter(|d| d.code == "E002").collect();
    assert_eq!(e002s.len(), 1, "duplicate ID should produce E002");
    assert!(
        e002s[0].message.contains("alpha"),
        "E002 message should name the duplicate ID"
    );
    assert!(
        e002s[0].suggestion.is_some(),
        "E002 should carry an actionable suggestion, got None"
    );
    assert!(
        e002s[0].suggestion.as_ref().unwrap().contains("rename"),
        "E002 suggestion should advise renaming, got: {:?}",
        e002s[0].suggestion
    );
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "W060 carries actionable suggestion"
)]
fn w060_cross_kind_collision_has_suggestion() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let file_a = parse(r#"feature code_fmt "Code Formatting" { }"#, "a.spec");
    let file_b = parse(r#"milestone code_fmt "Phase 8" { }"#, "b.spec");
    let (_, diagnostics) = build_graph(&[file_a, file_b]);

    let w060s: Vec<_> = diagnostics.iter().filter(|d| d.code == "W060").collect();
    assert_eq!(w060s.len(), 1, "same-ID-different-kind should produce W060");
    assert!(
        w060s[0].suggestion.is_some(),
        "W060 should carry an actionable suggestion, got None"
    );
    assert!(
        w060s[0].suggestion.as_ref().unwrap().contains("distinct"),
        "W060 suggestion should advise using distinct IDs, got: {:?}",
        w060s[0].suggestion
    );
}

// --- every edge the build adds joins two entities ---

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "every edge connects two existing nodes"
)]
fn every_edge_the_build_adds_joins_two_entities() {
    let (graph, diagnostics) = specforge_graph::build_graph(&[
        parse(
            "feature a \"A\" {\n  problem \"c\"\n}\nbehavior f \"F\" {\n  features [a, ghost]\n}\n",
            "main.spec",
        ),
        parse(
            "behavior g \"G\" {\n  features [a, missing]\n}\n",
            "other.spec",
        ),
    ]);

    // A reference to nothing is E003 and no edge; the others are edges.
    assert_eq!(graph.edges().len(), 2, "{:?}", graph.edges());
    for edge in graph.edges() {
        assert!(graph.node(edge.source.as_str()).is_some(), "{edge:?}");
        assert!(graph.node(edge.target.as_str()).is_some(), "{edge:?}");
    }
    let unresolved: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == "E003")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(unresolved.len(), 2, "{diagnostics:?}");
    assert!(
        unresolved.iter().any(|m| m.contains("ghost")),
        "{unresolved:?}"
    );
    assert!(
        unresolved.iter().any(|m| m.contains("missing")),
        "{unresolved:?}"
    );
}

// --- reach: the one breadth-first traversal ---

/// A diamond `top -> left -> bottom`, `top -> right -> bottom`, plus a tail
/// `bottom -> tail` and an edge to an id no node declares.
fn diamond() -> Graph {
    let mut graph = Graph::new();
    for id in ["top", "left", "right", "bottom", "tail"] {
        graph.add_node(make_node(id, "behavior"));
    }
    graph.add_edge(make_edge("top", "right", "r"));
    graph.add_edge(make_edge("top", "left", "l"));
    graph.add_edge(make_edge("left", "bottom", "lb"));
    graph.add_edge(make_edge("right", "bottom", "rb"));
    graph.add_edge(make_edge("bottom", "tail", "t"));
    graph.add_edge(make_edge("tail", "ghost", "g"));
    graph
}

#[test]
fn reach_is_breadth_first_with_deterministic_paths() {
    let graph = diamond();
    let reached = graph.reach("top", None).unwrap();
    let order: Vec<(&str, usize)> = reached.iter().map(|r| (r.id.as_str(), r.depth)).collect();
    // Neighbours in (id, label) order: left before right; bottom is first
    // reached from left, whatever order the edges were added in.
    assert_eq!(
        order,
        [
            ("top", 0),
            ("left", 1),
            ("right", 1),
            ("bottom", 2),
            ("tail", 3)
        ]
    );
    let labels = |id: &str| -> Vec<String> {
        Graph::reach_path(&reached, Sym::new(id))
            .iter()
            .map(|l| l.as_str().to_string())
            .collect()
    };
    assert_eq!(labels("top"), Vec::<String>::new());
    assert_eq!(labels("bottom"), ["l", "lb"]);
    assert_eq!(labels("tail"), ["l", "lb", "t"]);
    // The edge to an id no node declares is not walked.
    assert!(reached.iter().all(|r| r.id.as_str() != "ghost"));
    assert!(Graph::reach_path(&reached, Sym::new("ghost")).is_empty());
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "depth N returns all entities within N hops"
)]
fn reach_stops_at_its_depth_bound() {
    let graph = diamond();
    let ids = |depth: usize| -> Vec<String> {
        graph
            .reach("tail", Some(depth))
            .unwrap()
            .iter()
            .map(|r| r.id.as_str().to_string())
            .collect()
    };
    assert_eq!(ids(0), ["tail"]);
    assert_eq!(ids(1), ["tail", "bottom"]);
    assert_eq!(ids(2), ["tail", "bottom", "left", "right"]);
    assert_eq!(ids(3), ["tail", "bottom", "left", "right", "top"]);
    // The subgraphs are what reach reaches, and every edge between them.
    let sub = graph.subgraph_depth("tail", 2).unwrap();
    let mut nodes: Vec<&str> = sub.nodes().iter().map(|n| n.id.raw.as_str()).collect();
    nodes.sort_unstable();
    assert_eq!(nodes, ["bottom", "left", "right", "tail"]);
    assert_eq!(sub.edges().len(), 3, "lb, rb and t; not the edges to top");
    assert_eq!(graph.subgraph("tail").unwrap().nodes().len(), 5);
}

#[test]
fn reach_from_an_unknown_root_is_none() {
    let graph = diamond();
    assert!(graph.reach("ghost", None).is_none());
    assert!(graph.reach("nope", Some(1)).is_none());
    assert!(graph.subgraph("nope").is_none());
    assert!(graph.subgraph_depth("nope", 2).is_none());
}

// === first declaration wins, in path order ===

#[specforge_test(
    behavior = "detect_duplicate_entity_ids",
    verify = "duplicate ID across files produces E002"
)]
fn a_duplicate_goes_to_the_first_file_in_path_order() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let source = "behavior dup \"Dup\" { contract \"x\" }\n";
    let (graph, diagnostics) = build_graph(&[parse(source, "b.spec"), parse(source, "a.spec")]);

    assert_eq!(
        graph.node("dup").unwrap().source_span.file.as_str(),
        "a.spec"
    );
    let e002: Vec<_> = diagnostics.iter().filter(|d| d.code == "E002").collect();
    assert_eq!(e002.len(), 1);
    assert_eq!(e002[0].span.as_ref().unwrap().file.as_str(), "b.spec");
}

#[specforge_test(
    behavior = "detect_duplicate_entity_ids",
    verify = "duplicate ID in same file produces E002"
)]
fn duplicate_id_in_the_same_file_produces_e002() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let source = r#"
behavior alpha "First Alpha" { contract "first" }
behavior alpha "Second Alpha" { contract "second" }
"#;
    let (_, diagnostics) = build_graph(&[parse(source, "main.spec")]);

    let e002: Vec<_> = diagnostics.iter().filter(|d| d.code == "E002").collect();
    assert_eq!(
        e002.len(),
        1,
        "duplicate ID in same file should produce E002"
    );
    assert!(e002[0].message.contains("alpha"));
}

#[specforge_test(
    behavior = "detect_duplicate_entity_ids",
    verify = "E002 includes both source locations"
)]
fn e002_includes_both_source_locations() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    let file_a = parse(
        "\nbehavior alpha \"Alpha in file A\" { contract \"first\" }\n",
        "a.spec",
    );
    let file_b = parse(
        "\nbehavior alpha \"Alpha in file B\" { contract \"second\" }\n",
        "b.spec",
    );
    let (_, diagnostics) = build_graph(&[file_a, file_b]);

    let e002: Vec<_> = diagnostics.iter().filter(|d| d.code == "E002").collect();
    assert_eq!(e002.len(), 1, "should have exactly one E002");

    // The span points at the duplicate (second) declaration; the message
    // names the first, whose node the graph retains.
    let diag = &e002[0];
    let span = diag
        .span
        .as_ref()
        .expect("E002 carries the duplicate's span");
    assert_eq!(span.file.as_str(), "b.spec");
    assert_eq!((span.start_line, span.start_col), (2, 1));
    assert_eq!(
        diag.message,
        "duplicate entity ID 'alpha' (first declared at a.spec:2:1)"
    );
}

#[specforge_test(
    behavior = "detect_duplicate_entity_ids",
    verify = "Detect Duplicate Entity IDs: duplicate entity ID detection holds — all_files_parsed, duplicate_ids_diagnosed"
)]
fn duplicate_id_contract_consistency() {
    use specforge_graph::build_graph;
    use specforge_parser::parse;

    // Case 1: unique IDs → no E002.
    let source_unique = r#"
behavior alpha "A" { contract "first" }
behavior beta "B" { contract "second" }
"#;
    let (_, diagnostics) = build_graph(&[parse(source_unique, "main.spec")]);
    assert!(
        diagnostics.iter().all(|d| d.code != "E002"),
        "unique IDs must not produce E002"
    );

    // Case 2: duplicate IDs → one E002 with a declaration site.
    let file_a = parse(
        "\nbehavior gamma \"Gamma A\" { contract \"first\" }\n",
        "first.spec",
    );
    let file_b = parse(
        "\nbehavior gamma \"Gamma B\" { contract \"second\" }\n",
        "second.spec",
    );
    let (_, diagnostics) = build_graph(&[file_a, file_b]);
    let e002: Vec<_> = diagnostics.iter().filter(|d| d.code == "E002").collect();
    assert_eq!(e002.len(), 1, "duplicate IDs must produce exactly one E002");
    assert!(e002[0].message.contains("gamma"));
    assert!(
        e002[0].span.is_some(),
        "E002 must include source span identifying a declaration site"
    );
}
