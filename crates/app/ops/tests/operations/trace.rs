use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Edge, Graph, Node};
use specforge_ops::read::trace::{Target, TraceExpectations, TraceLinkStatus};
use specforge_registry::RegistryBuild;

use crate::view_support::{Assembled, Viewed, chain, chain_in, every_chain, plain};
use specforge_parser::{EntityId, EntityKind, FieldMap};
use specforge_test::prelude::*;

fn span() -> SourceSpan {
    SourceSpan {
        file: Sym::new("test.spec"),
        start_line: 1,
        start_col: 0,
        end_line: 1,
        end_col: 0,
    }
}

fn node(id: &str, kind: &str) -> Node {
    Node {
        id: EntityId { raw: Sym::new(id) },
        kind: EntityKind {
            raw: Sym::new(kind),
        },
        title: Some(format!("Title {}", id)),
        fields: FieldMap::new(),
        source_span: span(),
        methods: Vec::new(),
        obligations: Vec::new(),
    }
}

/// a -> b -> c (linear chain)
fn build_chain() -> Graph {
    let mut graph = Graph::new();
    graph.add_node(node("a", "behavior"));
    graph.add_node(node("b", "behavior"));
    graph.add_node(node("c", "invariant"));
    graph.add_edge(Edge {
        source: "a".into(),
        target: "b".into(),
        label: "depends_on".into(),
    });
    graph.add_edge(Edge {
        source: "b".into(),
        target: "c".into(),
        label: "invariants".into(),
    });
    graph
}

// B:compute_traceability_chain — verify unit "trace from entity shows upstream and downstream connections"
#[specforge_test(
    behavior = "compute_traceability_chain",
    verify = "trace from entity shows upstream and downstream connections"
)]
fn trace_shows_upstream_and_downstream() {
    let graph = build_chain();
    let trace = chain(&graph, "b");

    assert!(
        trace.upstream.iter().any(|l| l.entity_id == "a"),
        "upstream should include a"
    );
    assert!(
        trace.downstream.iter().any(|l| l.entity_id == "c"),
        "downstream should include c"
    );
}

// B:compute_traceability_chain — verify unit "trace shows full chain depth"
#[specforge_test(
    behavior = "compute_traceability_chain",
    verify = "trace shows full chain depth"
)]
fn trace_shows_full_chain_depth() {
    let mut graph = build_chain();
    graph.add_node(node("d", "event"));
    graph.add_edge(Edge {
        source: "c".into(),
        target: "d".into(),
        label: "produces".into(),
    });

    let trace = chain(&graph, "a");
    // a is root, so no upstream, downstream = b, c, d
    assert!(trace.upstream.is_empty());
    let ids: Vec<&str> = trace
        .downstream
        .iter()
        .map(|l| l.entity_id.as_str())
        .collect();
    assert!(ids.contains(&"b"));
    assert!(ids.contains(&"c"));
    assert!(ids.contains(&"d"));
}

// B:compute_traceability_chain — verify unit "trace from entity shows upstream and downstream connections"
// (error case: nonexistent entity returns error)
#[specforge_test(behavior = "compute_traceability_chain")]
fn trace_nonexistent_entity_returns_error() {
    let graph = build_chain();
    let project = Assembled::over(RegistryBuild::default()).graph(graph);
    let result = specforge_ops::read::trace::trace(&project.view(), Target::Entity("nonexistent"));
    assert!(result.is_err());
}

// B:serialize_traceability_data — verify unit "output conforms to Graph Protocol schema"
#[specforge_test(
    behavior = "serialize_traceability_data",
    verify = "output conforms to Graph Protocol schema"
)]
fn trace_serializes_to_json() {
    let graph = build_chain();
    let project = Assembled::over(RegistryBuild::default()).graph(graph);
    let outcome = specforge_ops::read::trace::trace(&project.view(), Target::Entity("b")).unwrap();
    let json = serde_json::to_string_pretty(&plain(outcome)).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["entity_id"], "b");
    assert!(parsed["upstream"].is_array());
    assert!(parsed["downstream"].is_array());
    assert!(parsed["schema_version"].is_string());
}

// B:compute_traceability_chain — verify unit "trace from entity shows upstream and downstream connections"
// (edge case: leaf has upstream only)
#[specforge_test(behavior = "compute_traceability_chain")]
fn trace_on_leaf_has_upstream_only() {
    let graph = build_chain();
    let trace = chain(&graph, "c");

    assert!(!trace.upstream.is_empty(), "leaf should have upstream");
    assert!(
        trace.downstream.is_empty(),
        "leaf should have no downstream"
    );
}

// B:compute_traceability_chain — verify unit "trace from entity shows upstream and downstream connections"
// (edge case: root has downstream only)
#[specforge_test(behavior = "compute_traceability_chain")]
fn trace_on_root_has_downstream_only() {
    let graph = build_chain();
    let trace = chain(&graph, "a");

    assert!(trace.upstream.is_empty(), "root should have no upstream");
    assert!(!trace.downstream.is_empty(), "root should have downstream");
}

// B:serialize_traceability_data — verify unit "full trace covers all root entities across registered edge types"
#[specforge_test(
    behavior = "serialize_traceability_data",
    verify = "full trace covers all root entities across registered edge types"
)]
fn tracing_every_entity_covers_all_root_entities() {
    let graph = build_chain();
    let traces = every_chain(&graph, RegistryBuild::default()).chains;
    // All 3 entities should have a trace
    assert_eq!(traces.len(), 3);
    let ids: Vec<&str> = traces.iter().map(|t| t.entity_id.as_str()).collect();
    assert!(ids.contains(&"a"));
    assert!(ids.contains(&"b"));
    assert!(ids.contains(&"c"));
}

// B:serialize_traceability_data — verify unit "output conforms to Graph Protocol schema"
#[specforge_test(behavior = "serialize_traceability_data")]
fn every_trace_serializes_as_json_array() {
    let graph = build_chain();
    let traces = every_chain(&graph, RegistryBuild::default());
    let json = serde_json::to_string_pretty(&traces).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed["schema_version"].is_string());
    assert!(parsed["traces"].is_array());
    assert_eq!(parsed["traces"].as_array().unwrap().len(), 3);
}

fn expectations() -> TraceExpectations {
    TraceExpectations::of(&crate::trace_support::build())
}

/// The missing links of a chain as (from, edge_label, expected_kind).
fn missing(chain: &specforge_ops::read::trace::TraceChain) -> Vec<(&str, &str, &str)> {
    chain
        .missing
        .iter()
        .map(|m| {
            assert_eq!(m.status, TraceLinkStatus::Missing);
            (
                m.from.as_str(),
                m.edge_label.as_str(),
                m.expected_kind.as_str(),
            )
        })
        .collect()
}

// B:compute_traceability_chain — verify unit "missing link in chain is flagged"
#[specforge_test(
    behavior = "compute_traceability_chain",
    verify = "missing link in chain is flagged"
)]
fn trace_missing_link_flagged() {
    // b -features-> a: b declares no invariant, which its kind expects.
    let mut graph = Graph::new();
    graph.add_node(node("a", "feature"));
    graph.add_node(node("b", "behavior"));
    graph.add_edge(Edge {
        source: "b".into(),
        target: "a".into(),
        label: "features".into(),
    });

    let trace = chain_in(&graph, crate::trace_support::build(), "b").unwrap();
    assert_eq!(missing(&trace), vec![("b", "invariants", "invariant")]);
    let gap = &trace.missing[0];
    assert_eq!(gap.from_kind, "behavior");
    assert_eq!(gap.edge_type.as_deref(), Some("behavior_invariants"));
    assert_eq!(gap.depth, 1);
    assert!(!gap.required);
    // The resolved links are unchanged and say so.
    assert_eq!(trace.downstream.len(), 1);
    assert_eq!(trace.downstream[0].status, TraceLinkStatus::Resolved);

    // Declaring the invariant closes the gap.
    graph.add_node(node("i", "invariant"));
    graph.add_edge(Edge {
        source: "b".into(),
        target: "i".into(),
        label: "invariants".into(),
    });
    let trace = chain_in(&graph, crate::trace_support::build(), "b").unwrap();
    assert!(trace.missing.is_empty(), "{:?}", trace.missing);

    // A feature expects nothing (its behaviors write the link); a behavior
    // that links nothing misses both.
    graph.add_node(node("lonely", "feature"));
    graph.add_node(node("orphan", "behavior"));
    let lonely = chain_in(&graph, crate::trace_support::build(), "lonely").unwrap();
    assert_eq!(missing(&lonely), vec![]);
    let orphan = chain_in(&graph, crate::trace_support::build(), "orphan").unwrap();
    assert_eq!(
        missing(&orphan),
        vec![
            ("orphan", "features", "feature"),
            ("orphan", "invariants", "invariant"),
        ]
    );
}

// B:compute_traceability_chain — a missing link is an expected edge that is
// absent, not a broken reference: a dangling edge is E003 in check.
#[specforge_test(behavior = "compute_traceability_chain")]
fn trace_dangling_edge_is_not_a_missing_link() {
    let mut graph = Graph::new();
    graph.add_node(node("a", "feature"));
    graph.add_node(node("b", "behavior"));
    graph.add_edge(Edge {
        source: "b".into(),
        target: "a".into(),
        label: "features".into(),
    });
    // b declares an invariant that does not exist.
    graph.add_edge(Edge {
        source: "b".into(),
        target: "phantom".into(),
        label: "invariants".into(),
    });

    let trace = chain_in(&graph, crate::trace_support::build(), "b").unwrap();
    assert!(trace.missing.is_empty(), "{:?}", trace.missing);
    assert!(
        !trace.downstream.iter().any(|l| l.entity_id == "phantom"),
        "an undeclared entity is not part of the chain"
    );
}

// B:compute_traceability_chain — the expected edges come from the
// registries alone.
#[specforge_test(behavior = "compute_traceability_chain")]
fn trace_expectations_come_from_the_registries() {
    let expected = expectations();
    let labels = |kind: &str| -> Vec<String> {
        expected
            .for_kind(kind)
            .iter()
            .map(|e| e.label.clone())
            .collect()
    };
    // ports: its kind isn't loaded. depends_on: a self-relation.
    // satisfies: another extension's. contract: not a reference.
    assert_eq!(labels("behavior"), vec!["features", "invariants"]);
    assert!(labels("feature").is_empty());
    assert!(labels("invariant").is_empty());
    let features = &expected.for_kind("behavior")[0];
    assert_eq!(features.target_kind, "feature");

    // A required reference is expected wherever it comes from.
    let expected = TraceExpectations::of(&crate::trace_support::build_with_required_parent());
    let parent = expected
        .for_kind("behavior")
        .iter()
        .find(|e| e.label == "parent")
        .expect("required reference is expected");
    assert!(parent.required);

    // No extensions, no expectations.
    assert!(TraceExpectations::of(&specforge_registry::build_registries(vec![])).is_empty());
}

// B:compute_traceability_chain — the other side of a relation is derived.
#[specforge_test(
    behavior = "compute_traceability_chain",
    verify = "a relation written on its owning field shows on the other side as upstream, under the owning field's label"
)]
fn the_other_side_of_a_relation_is_upstream() {
    let mut graph = Graph::new();
    graph.add_node(node("a", "feature"));
    graph.add_node(node("b", "behavior"));
    graph.add_edge(Edge {
        source: "b".into(),
        target: "a".into(),
        label: "features".into(),
    });
    let trace = chain_in(&graph, crate::trace_support::build(), "a").unwrap();
    assert_eq!(trace.upstream.len(), 1);
    assert_eq!(trace.upstream[0].entity_id, "b");
    assert_eq!(trace.upstream[0].edge_label, "features");
    assert!(trace.missing.is_empty(), "{:?}", trace.missing);
}

// B:compute_traceability_chain — human output marks each missing link.
#[specforge_test(behavior = "compute_traceability_chain")]
fn trace_human_output_marks_missing_links() {
    let mut graph = Graph::new();
    graph.add_node(node("a", "feature"));
    graph.add_node(node("b", "behavior"));
    graph.add_edge(Edge {
        source: "b".into(),
        target: "a".into(),
        label: "features".into(),
    });
    let project = Assembled::over(crate::trace_support::build()).graph(graph);
    let outcome = specforge_ops::read::trace::trace(&project.view(), Target::Entity("b")).unwrap();
    assert_eq!(
        plain(outcome).to_human(),
        concat!(
            "b [behavior]\n",
            "  upstream:\n",
            "    (none)\n",
            "  downstream:\n",
            "    -features-> a [feature] (depth 1)\n",
            "  missing:\n",
            "    MISSING b -invariants-> [invariant] (depth 1)\n",
        )
    );
}

// B:read_views_over_the_project_view — verify unit "the reach of some entities is every entity their trace chains reach, the seeds included, and its unverified entities are those that count toward coverage and are not proven"
#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "the reach of some entities is every entity their trace chains reach, the seeds included, and its unverified entities are those that count toward coverage and are not proven"
)]
fn reach_lists_what_the_chains_reach_and_what_is_unverified() {
    let mut project = Assembled::over(crate::view_support::registries(&["behavior"], &["feature"]))
        .source(
            "behavior alpha \"A\" {\n  verify unit \"test alpha\"\n}\n\nfeature beta \"B\" {\n}\n",
        );
    project.graph_mut().add_edge(Edge {
        source: "alpha".into(),
        target: "beta".into(),
        label: "features".into(),
    });

    let reach = specforge_ops::read::trace::reach(&project.view(), ["beta"]).unwrap();
    assert_eq!(reach.affected, ["alpha", "beta"]);
    // alpha is testable and unproven; beta, a feature, is not obligated.
    assert_eq!(reach.unverified, ["alpha"]);

    let project = {
        project.record_passing(&[("alpha", "test alpha")]);
        project
    };
    let reach = specforge_ops::read::trace::reach(&project.view(), ["beta"]).unwrap();
    assert_eq!(reach.unverified, Vec::<String>::new());

    let refused = specforge_ops::read::trace::reach(&project.view(), ["nope"]).unwrap_err();
    assert_eq!(refused.code, "E003", "{refused:?}");
}

/// The end-to-end trace tests, which `specforge-cli` ran through the binary: the same requests, over a compiled
/// project.
mod compiled {
    use crate::compiled_support::{TestProject, built, specs, trace_document, traced};
    use specforge_ops::read::trace::Target;
    use specforge_project::CompiledProject;
    use specforge_test_macros::test as specforge_test;
    use std::collections::BTreeSet;

    /// One direction of a trace: its (entity_id, depth) links, in order.
    type Links = Vec<(String, u64)>;

    /// `specforge trace <id>`: (upstream, downstream).
    fn trace(project: &CompiledProject, id: &str) -> (Links, Links) {
        let parsed = trace_document(project, id);
        assert_eq!(parsed["entity_id"], id);
        let links = |direction: &str| {
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
        (links("upstream"), links("downstream"))
    }

    fn links(list: &[(&str, u64)]) -> Links {
        list.iter().map(|(id, d)| (id.to_string(), *d)).collect()
    }

    /// The links as a set, after checking none repeats.
    fn once_each(list: &Links) -> BTreeSet<(String, u64)> {
        let set: BTreeSet<_> = list.iter().cloned().collect();
        assert_eq!(set.len(), list.len(), "a node is visited twice: {list:?}");
        set
    }

    // --- Trace depth tests ---

    #[specforge_test(
        behavior = "compute_traceability_chain",
        verify = "trace from entity shows upstream and downstream connections"
    )]
    fn trace_isolated_entity_has_empty_upstream_and_downstream() {
        let spec = format!(
            "{}\nfeature alpha \"A\" {{ problem \"p\" solution \"s\" }}\nbehavior beta \"B\" {{ contract \"first\" features [alpha] }}\n",
            specs::ISOLATED
        );
        let (_project, compiled) = built(&[("main.spec", &spec)]);

        // Unconnected: nothing either way.
        assert_eq!(trace(&compiled, "isolated_node"), (links(&[]), links(&[])));
        // beta -features-> alpha: each sees the other on its own side.
        assert_eq!(
            trace(&compiled, "alpha"),
            (links(&[("beta", 1)]), links(&[]))
        );
        assert_eq!(
            trace(&compiled, "beta"),
            (links(&[]), links(&[("alpha", 1)]))
        );
    }

    #[specforge_test(
        behavior = "compute_traceability_chain",
        verify = "trace shows full chain depth"
    )]
    fn trace_linear_chain_shows_correct_depths() {
        let (_project, compiled) = built(&[("main.spec", specs::DEEP_CHAIN)]);

        // beh_middle -invariants-> inv_deep and beh_middle -features->
        // feat_root: each far end sees the behavior upstream, one hop away.
        let (upstream, downstream) = trace(&compiled, "inv_deep");
        assert_eq!(upstream, links(&[("beh_middle", 1)]));
        assert_eq!(downstream, links(&[]));
    }

    #[test]
    fn trace_includes_edge_labels() {
        let (_project, compiled) = built(&[("main.spec", specs::DEEP_CHAIN)]);

        let parsed = trace_document(&compiled, "inv_deep");
        let upstream = parsed["upstream"].as_array().unwrap();

        for link in upstream {
            let label = link["edge_label"].as_str().unwrap();
            assert!(
                !label.is_empty(),
                "every TraceLink must have a non-empty edge_label"
            );
        }
    }

    #[specforge_test(
        invariant = "graph_traversal_integrity",
        verify = "traversal from any node visits every reachable node exactly once"
    )]
    fn trace_handles_cycles_without_hanging() {
        let (_project, compiled) = built(&[("main.spec", specs::CYCLE)]);

        // Edges: cycle_a -> cycle_b, cycle_b -> cycle_c, cycle_c -> cycle_a.
        // From cycle_c the cycle is walked both ways, each other node once,
        // and the walk stops rather than coming back to cycle_c.
        let (upstream, downstream) = trace(&compiled, "cycle_c");
        assert_eq!(
            once_each(&upstream),
            BTreeSet::from([("cycle_b".to_string(), 1), ("cycle_a".to_string(), 2)])
        );
        assert_eq!(
            once_each(&downstream),
            BTreeSet::from([("cycle_a".to_string(), 1), ("cycle_b".to_string(), 2)])
        );
    }

    #[specforge_test(
        invariant = "graph_traversal_integrity",
        verify = "traversal from any node visits every reachable node exactly once"
    )]
    fn trace_cycle_visits_each_node_once() {
        let (_project, compiled) = built(&[("main.spec", specs::CYCLE)]);

        // Upstream of cycle_a: cycle_c points at it, cycle_b points at that.
        // Downstream: cycle_a -> cycle_b -> cycle_c, whose edge leads back.
        let (upstream, downstream) = trace(&compiled, "cycle_a");
        assert_eq!(
            once_each(&upstream),
            BTreeSet::from([("cycle_c".to_string(), 1), ("cycle_b".to_string(), 2)])
        );
        assert_eq!(
            once_each(&downstream),
            BTreeSet::from([("cycle_b".to_string(), 1), ("cycle_c".to_string(), 2)])
        );
    }

    #[specforge_test(
        invariant = "graph_traversal_integrity",
        verify = "identical graph inputs produce identical traversal results"
    )]
    fn trace_deterministic_output() {
        let (_project, compiled) = built(&[("main.spec", specs::DEEP_CHAIN)]);

        let stdout1 = trace_document(&compiled, "beh_middle").to_string();
        let stdout2 = trace_document(&compiled, "beh_middle").to_string();
        assert_eq!(stdout1, stdout2, "trace output should be deterministic");
    }

    #[specforge_test(
        behavior = "compute_traceability_chain",
        verify = "trace from entity shows upstream and downstream connections"
    )]
    fn trace_root_entity_has_no_upstream() {
        let (_project, compiled) = built(&[("main.spec", specs::DEEP_CHAIN)]);

        // beh_middle is a root: nothing points at it, but it reaches the
        // feature and the invariant.
        let (upstream, downstream) = trace(&compiled, "beh_middle");
        assert_eq!(upstream, links(&[]), "beh_middle should have no upstream");
        assert_eq!(downstream, links(&[("feat_root", 1), ("inv_deep", 1)]));

        // feat_root is a leaf: only beh_middle points at it.
        let (upstream, downstream) = trace(&compiled, "feat_root");
        assert_eq!(upstream, links(&[("beh_middle", 1)]));
        assert_eq!(downstream, links(&[]));

        // typ_leaf has no references either way.
        assert_eq!(trace(&compiled, "typ_leaf"), (links(&[]), links(&[])));
    }

    #[specforge_test(
        behavior = "compute_traceability_chain",
        verify = "trace from entity shows upstream and downstream connections"
    )]
    fn trace_leaf_entity_has_no_downstream() {
        let (_project, compiled) = built(&[(
            "main.spec",
            r#"
feature alpha "A" { problem "p" solution "s" }
behavior beta "B" { contract "first" features [alpha] }
"#,
        )]);

        // alpha has no outgoing edges (it's a leaf), but beta points to it
        let parsed = trace_document(&compiled, "alpha");
        assert_eq!(parsed["entity_id"], "alpha");
        assert_eq!(
            parsed["downstream"].as_array().unwrap().len(),
            0,
            "alpha should have no downstream"
        );
        assert!(
            !parsed["upstream"].as_array().unwrap().is_empty(),
            "alpha should have upstream (beta)"
        );
    }

    #[test]
    fn trace_multi_kind_chain_preserves_entity_kind() {
        let (_project, compiled) = built(&[("main.spec", specs::DEEP_CHAIN)]);

        let parsed = trace_document(&compiled, "inv_deep");

        assert_eq!(parsed["entity_kind"], "invariant");

        let upstream = parsed["upstream"].as_array().unwrap();
        for link in upstream {
            let kind = link["entity_kind"].as_str().unwrap();
            assert!(
                !kind.is_empty(),
                "every TraceLink should have a non-empty entity_kind"
            );
        }

        // beh_middle should have kind "behavior"
        if let Some(beh) = upstream.iter().find(|l| l["entity_id"] == "beh_middle") {
            assert_eq!(beh["entity_kind"], "behavior");
        }
    }

    // --- Missing links ---

    /// software + product loaded: `pay` implements a feature and enforces an
    /// invariant; `refund` is linked to nothing.
    fn gap_project() -> (TestProject, CompiledProject) {
        let project = TestProject::new()
            .enabling(&["@specforge/software", "@specforge/product"])
            .file("main.spec", specs::TRACE_GAPS);
        let compiled = project.compile();
        (project, compiled)
    }

    /// `specforge trace [entity]`'s document: one entity's chain, or every entity's.
    fn run_trace(project: &CompiledProject, args: &[&str]) -> serde_json::Value {
        match args {
            [] => serde_json::to_value(traced(project, Target::Every)).unwrap(),
            [id] => trace_document(project, id),
            _ => unreachable!("one entity or none"),
        }
    }

    /// A chain's missing links as (from, edge_label, edge_type, expected_kind),
    /// after checking each is marked missing at depth 1.
    fn missing_links(chain: &serde_json::Value) -> Vec<(String, String, String, String)> {
        chain["missing"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| {
                assert_eq!(m["status"], "missing");
                assert_eq!(m["depth"], 1);
                (
                    m["from"].as_str().unwrap().to_string(),
                    m["edge_label"].as_str().unwrap().to_string(),
                    m["edge_type"].as_str().unwrap().to_string(),
                    m["expected_kind"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }

    fn gap(
        from: &str,
        label: &str,
        edge_type: &str,
        kind: &str,
    ) -> (String, String, String, String) {
        (
            from.to_string(),
            label.to_string(),
            edge_type.to_string(),
            kind.to_string(),
        )
    }

    /// What the software extension declares every behavior may reach, as
    /// missing links of `from`.
    fn software_gaps(from: &str) -> Vec<(String, String, String, String)> {
        vec![
            gap(from, "consumes", "BehaviorConsumesEvent", "event"),
            gap(from, "ports", "BehaviorUsesPort", "port"),
            gap(from, "produces", "BehaviorProducesEvent", "event"),
            gap(from, "types", "BehaviorReferencesType", "type"),
        ]
    }

    #[specforge_test(
        behavior = "compute_traceability_chain",
        verify = "missing link in chain is flagged"
    )]
    fn trace_json_flags_missing_links() {
        let (_project, compiled) = gap_project();

        // pay has its feature and its invariant: only the edges the software
        // extension declares for behaviors that pay leaves out are missing.
        let pay = run_trace(&compiled, &["pay"]);
        assert_eq!(missing_links(&pay), software_gaps("pay"));
        for link in pay["downstream"].as_array().unwrap() {
            assert_eq!(link["status"], "resolved");
        }

        // refund also lacks a feature and an invariant.
        let refund = run_trace(&compiled, &["refund"]);
        let mut expected = software_gaps("refund");
        expected.push(gap(
            "refund",
            "features",
            "BehaviorImplementsFeature",
            "feature",
        ));
        expected.push(gap(
            "refund",
            "invariants",
            "BehaviorEnforcesInvariant",
            "invariant",
        ));
        expected.sort_by(|a, b| a.1.cmp(&b.1));
        assert_eq!(missing_links(&refund), expected);

        // A feature's only reference fields point at features: a hierarchy,
        // not an expected edge.
        let checkout = run_trace(&compiled, &["checkout"]);
        assert_eq!(missing_links(&checkout), vec![]);

        // With no extensions loaded nothing is expected, so nothing is missing.
        let (_bare_project, bare) = built(&[("main.spec", specs::ISOLATED)]);
        let isolated = run_trace(&bare, &["isolated_node"]);
        assert_eq!(isolated["missing"], serde_json::json!([]));
    }

    #[specforge_test(
        behavior = "serialize_traceability_data",
        verify = "gaps in chain are highlighted"
    )]
    fn trace_without_entity_serializes_every_chain_with_its_gaps() {
        let (_project, compiled) = gap_project();

        let all = run_trace(&compiled, &[]);
        assert!(all["schema_version"].is_string());
        let traces = all["traces"].as_array().unwrap();
        let ids: Vec<&str> = traces
            .iter()
            .map(|t| t["entity_id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, vec!["checkout", "paid_once", "pay", "refund"]);

        // Each chain carries the same gaps as its own trace.
        for trace in traces {
            let id = trace["entity_id"].as_str().unwrap();
            let single = run_trace(&compiled, &[id]);
            assert_eq!(trace["missing"], single["missing"], "{id}");
        }
        assert_eq!(missing_links(&traces[2]), software_gaps("pay"));
        assert_eq!(missing_links(&traces[3]).len(), 6);
    }

    #[specforge_test(
        behavior = "compute_traceability_chain",
        verify = "trace output includes schema version"
    )]
    fn trace_output_includes_schema_version() {
        let (_project, compiled) = built(&[("main.spec", specs::ISOLATED)]);

        let parsed = trace_document(&compiled, "isolated_node");
        assert!(
            parsed["schema_version"].is_string(),
            "trace output must include schema_version field"
        );
        assert!(!parsed["schema_version"].as_str().unwrap().is_empty());
    }
}
