use specforge_common::DiagnosticData;
use specforge_graph::{FileChange, GraphBuild, GraphConfig, build_graph_with_config};
use specforge_parser::parse;
use specforge_test_macros::test as specforge_test;

// B:build_in_memory_graph — verify contract "requires/ensures consistency for in-memory graph construction"
#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "Build In-Memory Graph: in-memory graph construction holds — resolution_complete, one_node_per_entity, one_edge_per_reference, no_dangling_edges"
)]
fn build_in_memory_graph_contract() {
    // Two files: `b.spec` references entities declared in `a.spec` (cross-file
    // resolution) plus one reference to an entity that does not exist.
    let a = parse(
        r#"
behavior alpha "A" { contract "first" }
behavior beta "B" { contract "second" }
"#,
        "a.spec",
    );
    let b = parse(
        r#"
feature gamma "G" { behaviors [alpha, beta, ghost] }
feature delta "D" { behaviors [alpha] }
"#,
        "b.spec",
    );
    let (graph, diagnostics) = build_graph_with_config(&[a, b], &GraphConfig::default());

    // resolution_complete: resolution ran before materialisation — the one
    // reference that cannot be resolved is reported (E003), not turned into an edge.
    let e003: Vec<_> = diagnostics.iter().filter(|d| d.code == "E003").collect();
    assert_eq!(
        e003.len(),
        1,
        "exactly one unresolved reference: {diagnostics:?}"
    );
    assert!(e003[0].message.contains("ghost"), "{}", e003[0].message);
    // What it names is data as well as text: target, entity and field.
    assert_eq!(
        e003[0].data.as_deref().cloned(),
        Some(DiagnosticData::UnresolvedReference {
            target: "ghost".into(),
            entity: "gamma".into(),
            field: "behaviors".into(),
            did_you_mean: None,
        })
    );

    // one_node_per_entity: exactly the four declared entities, nothing else.
    let mut ids: Vec<String> = graph.nodes().iter().map(|n| n.id.raw.to_string()).collect();
    ids.sort();
    assert_eq!(ids, vec!["alpha", "beta", "delta", "gamma"]);

    // one_edge_per_reference: the three resolved references, each exactly once.
    let mut edges: Vec<(String, String, String)> = graph
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
    let expected: Vec<(String, String, String)> = [
        ("delta", "alpha", "behaviors"),
        ("gamma", "alpha", "behaviors"),
        ("gamma", "beta", "behaviors"),
    ]
    .iter()
    .map(|(s, t, l)| (s.to_string(), t.to_string(), l.to_string()))
    .collect();
    assert_eq!(edges, expected);

    // no_dangling_edges: every edge connects two existing nodes.
    for e in graph.edges() {
        assert!(
            graph.node(e.source.as_str()).is_some(),
            "orphan source {e:?}"
        );
        assert!(
            graph.node(e.target.as_str()).is_some(),
            "orphan target {e:?}"
        );
    }
    assert!(
        graph.edges_to("ghost").is_empty(),
        "unresolved reference must not become a dangling edge"
    );
}

// B:maintain_mutable_graph — verify contract "requires/ensures consistency for mutable graph maintenance"
#[specforge_test(
    behavior = "maintain_mutable_graph",
    verify = "Maintain Mutable Graph: mutable graph maintenance holds — graph_initialized, mutations_applied, no_dangling_edges_enforced, graph_consistency"
)]
fn maintain_mutable_graph_contract() {
    // Requires: existing graph with nodes and edges
    // Ensures: add/remove mutations reflect correctly, edges cleaned on node removal
    let mut build = GraphBuild::of(
        [
            parse("behavior a \"a\" {\n  contract \"c\"\n}\n", "a.spec"),
            parse("feature b \"b\" {\n  behaviors [a]\n}\n", "b.spec"),
        ],
        GraphConfig::default(),
    );
    build.set_verify(true);

    assert_eq!(build.graph().node_count(), 2);
    assert_eq!(build.graph().edge_count(), 1);

    // Add a new node
    let added = build.apply([FileChange::Parsed(parse(
        "behavior c \"c\" {\n  contract \"c\"\n}\n",
        "c.spec",
    ))]);
    assert_eq!(added.verification, Some(Ok(())));
    assert_eq!(
        build.graph().node_count(),
        3,
        "adding a file must add its node"
    );

    // Remove node with edge — edge must be cleaned up
    let removed = build.apply([FileChange::Removed("a.spec".into())]);
    assert_eq!(removed.verification, Some(Ok(())));
    let graph = build.graph();
    assert_eq!(
        graph.node_count(),
        2,
        "removing a file must remove its node"
    );
    assert!(
        graph.node("a").is_none(),
        "removed node must not be queryable"
    );
    assert!(
        graph.edges().is_empty(),
        "edges to removed node must be cleaned"
    );

    // Remaining nodes intact
    assert!(graph.node("b").is_some());
    assert!(graph.node("c").is_some());
}

// B:resolve_external_ref_declarations — verify contract "requires/ensures consistency for external ref resolution"
#[specforge_test(
    behavior = "resolve_external_ref_declarations",
    verify = "Resolve External Ref Declarations: external ref resolution holds — ref_blocks_parsed, filesystem_available, refs_registered, known_schemes_marked, unknown_schemes_deferred"
)]
fn resolve_external_ref_declarations_contract() {
    // Requires: graph with ref nodes having scheme metadata
    // Ensures: ref nodes are in graph with scheme field; known scheme = no I005, unknown = I005
    let source = r#"
ref gh.issue:42 "Wasm support"
ref jira.story:ABC-1 "Backend work"
"#;
    let spec_file = parse(source, "main.spec");

    // Known scheme = "gh", unknown = "jira"
    let config = GraphConfig {
        known_provider_schemes: vec!["gh".to_string()].into_iter().collect(),
        ..Default::default()
    };
    let (graph, diagnostics) = build_graph_with_config(&[spec_file], &config);

    // Both refs are nodes in the graph
    assert!(
        graph.node("gh.issue:42").is_some(),
        "ref with known scheme must be in graph"
    );
    assert!(
        graph.node("jira.story:ABC-1").is_some(),
        "ref with unknown scheme must be in graph"
    );

    // Known scheme: no I005
    let gh_infos: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == "I005" && d.message.contains("gh"))
        .collect();
    assert!(gh_infos.is_empty(), "known scheme should not produce I005");

    // Unknown scheme: I005
    let jira_infos: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == "I005" && d.message.contains("jira"))
        .collect();
    assert_eq!(jira_infos.len(), 1, "unknown scheme should produce I005");
}

// B:provide_did_you_mean_suggestions — verify contract "requires/ensures consistency for did-you-mean suggestions"
#[specforge_test(
    behavior = "provide_did_you_mean_suggestions",
    verify = "Provide Did-You-Mean Suggestions: did-you-mean suggestions holds — unresolved_reference_available, kind_registry_populated, distance_threshold, sorted_by_distance"
)]
fn provide_did_you_mean_suggestions_contract() {
    // Requires: unresolved reference with entity IDs available
    // Ensures: close match → suggestion, distant match → no suggestion
    let source_close = r#"
behavior alpha_parser "A" { contract "first" }
feature gamma "G" { behaviors [alpha_parsr] }
"#;
    let spec_file = parse(source_close, "test.spec");
    let config = GraphConfig::default();
    let (_, diagnostics) = build_graph_with_config(&[spec_file], &config);
    let e003: Vec<_> = diagnostics.iter().filter(|d| d.code == "E003").collect();
    assert_eq!(e003.len(), 1);
    assert!(
        e003[0].suggestion.is_some(),
        "close match must produce suggestion"
    );
    // The close match rides in the data, so a quick fix needn't parse it.
    assert!(
        matches!(
            e003[0].data.as_deref(),
            Some(DiagnosticData::UnresolvedReference { did_you_mean: Some(m), .. })
                if m == "alpha_parser"
        ),
        "{:?}",
        e003[0].data
    );

    let source_far = r#"
behavior alpha "A" { contract "first" }
feature gamma "G" { behaviors [zzzzz_completely_different] }
"#;
    let spec_file = parse(source_far, "test.spec");
    let (_, diagnostics) = build_graph_with_config(&[spec_file], &config);
    let e003: Vec<_> = diagnostics.iter().filter(|d| d.code == "E003").collect();
    assert_eq!(e003.len(), 1);
    assert!(
        e003[0].suggestion.is_none(),
        "distant match must not produce suggestion"
    );
}
