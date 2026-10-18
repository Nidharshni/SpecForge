//! Query, list and search (`specforge_ops::read::query`, ADR 0015 "Query"): the
//! read views the CLI and MCP render, tested through the operation's
//! interface. The query tests are the emitter's former `query` tests,
//! re-homed where the behavior now lives.

use serde_json::{Value, json};
use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Edge, Graph, Node};
use specforge_ops::read::query::{
    DEFAULT_SEARCH_LIMIT, FieldHolds, ListRequest, QueryRequest, SearchRequest, list, query, search,
};
use specforge_ops::read::{export::Format, schema::json_schema};
use specforge_test::prelude::*;

use crate::view_support::{Assembled, Viewed, plain, registries};

fn node(id: &str, kind: &str) -> Node {
    Node {
        title: Some(format!("Title {id}")),
        ..Node::new(
            id,
            kind,
            SourceSpan {
                file: Sym::new("test.spec"),
                start_line: 1,
                start_col: 0,
                end_line: 1,
                end_col: 0,
            },
        )
    }
}

/// a(feature) -> b(behavior) -> c(invariant) -> d(event)
fn linear() -> Assembled {
    let mut graph = Graph::new();
    graph.add_node(node("a", "feature"));
    graph.add_node(node("b", "behavior"));
    graph.add_node(node("c", "invariant"));
    graph.add_node(node("d", "event"));
    for (source, target, label) in [
        ("a", "b", "behaviors"),
        ("b", "c", "invariants"),
        ("c", "d", "produces"),
    ] {
        graph.add_edge(Edge {
            source: source.into(),
            target: target.into(),
            label: label.into(),
        });
    }
    Assembled::over(Default::default()).graph(graph)
}

/// The document `query` answers for `entity` at `depth`, filtered by `kinds`.
fn document(project: &Assembled, entity: &str, depth: usize, kinds: &[&str]) -> Value {
    let outcome = query(
        &project.view(),
        &QueryRequest {
            entity_id: entity,
            depth: Some(depth),
            kinds: kinds.to_vec(),
            ..QueryRequest::default()
        },
    )
    .unwrap();
    serde_json::from_str(&outcome.into_value_and_notices().0).unwrap()
}

fn ids(document: &Value) -> Vec<&str> {
    document["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect()
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "depth 0 returns only the target entity"
)]
fn depth_0_returns_only_target_entity() {
    assert_eq!(ids(&document(&linear(), "b", 0, &[])), ["b"]);
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "depth 1 returns direct neighbors"
)]
fn depth_1_returns_direct_neighbors() {
    let project = linear();
    assert_eq!(ids(&document(&project, "b", 1, &[])), ["a", "b", "c"]);
    // A request that names no depth is a query at depth 1.
    let outcome = query(
        &project.view(),
        &QueryRequest {
            entity_id: "b",
            ..QueryRequest::default()
        },
    )
    .unwrap();
    let default: Value = serde_json::from_str(&outcome.into_value_and_notices().0).unwrap();
    assert_eq!(default, document(&project, "b", 1, &[]));
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "depth N returns all entities within N hops"
)]
fn depth_n_returns_all_within_n_hops() {
    let project = linear();
    assert_eq!(ids(&document(&project, "a", 3, &[])), ["a", "b", "c", "d"]);
    assert_eq!(ids(&document(&project, "a", 2, &[])), ["a", "b", "c"]);
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "kind filter restricts results to specified entity kinds"
)]
fn kind_filter_restricts_results() {
    let document = document(&linear(), "b", 2, &["invariant"]);
    // The root is always kept; an edge is kept when both ends are.
    assert_eq!(ids(&document), ["b", "c"]);
    assert_eq!(
        document["edges"],
        json!([{ "source": "b", "target": "c", "label": "invariants" }])
    );
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "multiple kind filters combine as union"
)]
fn multiple_kind_filters_combine_as_union() {
    let project = linear();
    assert_eq!(
        ids(&document(&project, "b", 2, &["feature", "event"])),
        ["a", "b", "d"]
    );
    assert_eq!(ids(&document(&project, "b", 2, &["feature"])), ["a", "b"]);
    assert_eq!(ids(&document(&project, "b", 2, &["event"])), ["b", "d"]);
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "output conforms to Graph Protocol schema"
)]
fn query_conforms_to_graph_protocol_schema() {
    let document = document(&linear(), "b", 1, &[]);
    assert!(document["schema_version"].is_string());
    assert!(document["edges"].is_array());
    for node in document["nodes"].as_array().unwrap() {
        assert!(node["id"].is_string() && node["kind"].is_string());
    }
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "output includes schema_version field"
)]
fn query_includes_schema_version() {
    let project = linear();
    let document = document(&project, "b", 1, &[]);
    // The export schema policy: a graph-format query references the
    // published schema, Graph Protocol 2.0, as specforge://graph/{id} does.
    assert_eq!(document["format_version"], "2.0");
    assert_eq!(
        document["schema_version"].as_str().unwrap(),
        project.view().versioned_schema().version().to_string()
    );
    assert!(document["schema_ref"]["content_hash"].is_string());
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "querying same entity at same depth produces identical subgraph"
)]
fn query_same_entity_same_depth_is_deterministic() {
    let project = linear();
    assert_eq!(
        document(&project, "b", 1, &[]),
        document(&project, "b", 1, &[])
    );
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "a non-existent entity is E003 naming the closest entity"
)]
fn query_of_a_missing_entity_is_e003() {
    let project = linear();
    let mut graph = Graph::new();
    graph.add_node(node("login", "behavior"));
    let near = Assembled::over(Default::default()).graph(graph);
    let error = query(
        &near.view(),
        &QueryRequest {
            entity_id: "logn",
            ..QueryRequest::default()
        },
    )
    .unwrap_err();
    assert!(error.error.is(specforge_common::codes::E003));
    assert_eq!(
        error.error.message,
        "unresolved entity 'logn' — not found in graph"
    );
    assert_eq!(
        error.error.suggestion.as_deref(),
        Some("did you mean 'login'?")
    );
    // Nothing close: no suggestion.
    let error = query(
        &project.view(),
        &QueryRequest {
            entity_id: "zzzzzzzz",
            ..QueryRequest::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.error.suggestion, None);
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "an unknown --kind is reported with I020 and the closest kind"
)]
fn query_reports_an_unknown_kind() {
    let project = Assembled::over(registries(&["behavior"], &[])).graph({
        let mut graph = Graph::new();
        graph.add_node(node("login", "behavior"));
        graph
    });
    let outcome = query(
        &project.view(),
        &QueryRequest {
            entity_id: "login",
            kinds: vec!["behaviour"],
            ..QueryRequest::default()
        },
    )
    .unwrap();
    let notice = &outcome.notices()[0];
    assert_eq!(notice.code, "I020");
    assert_eq!(notice.message, "unknown entity kind 'behaviour'");
    assert_eq!(
        notice.suggestion.as_deref(),
        Some("did you mean 'behavior'?")
    );
    // The filter still drops the unknown kind: the root alone remains.
    let document: Value = serde_json::from_str(&outcome.into_value_and_notices().0).unwrap();
    assert_eq!(ids(&document), ["login"]);
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "--include-coverage gives each entity its coverage status"
)]
fn query_with_coverage_marks_each_node() {
    let project = Assembled::over(registries(&["behavior"], &[])).source("behavior login \"Login\" {\n  verify unit \"logs in\"\n}\n\nbehavior logout \"Logout\" {\n  verify unit \"logs out\"\n}\n");
    project.record_passing(&[("login", "logs in")]);
    let status = |entity: &str, include_coverage: bool| -> Value {
        let outcome = query(
            &project.view(),
            &QueryRequest {
                entity_id: entity,
                depth: Some(0),
                include_coverage,
                ..QueryRequest::default()
            },
        )
        .unwrap();
        let document: Value = serde_json::from_str(&outcome.into_value_and_notices().0).unwrap();
        document["nodes"][0]["coverage_status"].clone()
    };
    assert_eq!(status("login", true), "covered");
    assert_eq!(status("logout", true), "uncovered");
    assert_eq!(status("login", false), Value::Null);
}

const COVERED_SOURCE: &str = "behavior login \"Login\" {\n  verify unit \"logs in\"\n}\n\nbehavior logout \"Logout\" {\n  verify unit \"logs out\"\n}\n";

/// The `login`/`logout` project with `login`'s obligation proven.
fn covered() -> Assembled {
    let project = Assembled::over(registries(&["behavior"], &[])).source(COVERED_SOURCE);
    project.record_passing(&[("login", "logs in")]);
    project
}

/// The document `query` answers for `entity` at depth 0 in `format`.
fn queried(project: &Assembled, entity: &str, format: Format, include_coverage: bool) -> String {
    query(
        &project.view(),
        &QueryRequest {
            entity_id: entity,
            depth: Some(0),
            format: Some(format),
            include_coverage,
            ..QueryRequest::default()
        },
    )
    .map(|read| read.into_value_and_notices().0)
    .unwrap()
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "a query with --include-coverage validates against the published schema of its format"
)]
fn a_query_with_coverage_conforms_to_the_published_schema() {
    use specforge_common::shape::{unchecked_keywords, violations};
    let project = covered();
    for format in [Format::Graph, Format::Context, Format::Brief] {
        let published = plain(json_schema(&project.view(), format).unwrap());
        assert!(unchecked_keywords(&published).is_empty(), "{format:?}");
        let document: Value =
            serde_json::from_str(&queried(&project, "login", format, true)).unwrap();
        assert_eq!(
            violations(&published, &document),
            Vec::<String>::new(),
            "{format:?}"
        );
        assert_eq!(
            document["nodes"][0]["coverage_status"], "covered",
            "{format:?}"
        );
    }
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "--include-coverage gives each entity its coverage status"
)]
fn a_query_with_coverage_is_the_export_with_one_key_per_node() {
    let project = covered();
    for format in [Format::Graph, Format::Context, Format::Brief] {
        for (entity, status) in [("login", "covered"), ("logout", "uncovered")] {
            let without = queried(&project, entity, format, false);
            let with = queried(&project, entity, format, true)
                .replace(&format!(",\"coverage_status\":\"{status}\""), "");
            assert_eq!(with, without, "{format:?} {entity}");
        }
    }
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "a non-existent entity is E003 naming the closest entity"
)]
fn an_unknown_entity_is_refused_before_the_report_is_read() {
    let project = Assembled::over(registries(&["behavior"], &[])).source(COVERED_SOURCE);
    std::fs::write(project.root().join("specforge-report.json"), "{").unwrap();
    let ask = |entity_id| {
        query(
            &project.view(),
            &QueryRequest {
                entity_id,
                include_coverage: true,
                ..QueryRequest::default()
            },
        )
        .unwrap_err()
    };
    let unknown = ask("logn");
    assert_eq!(unknown.error.code, "E003");
    assert_eq!(
        unknown.error.suggestion.as_deref(),
        Some("did you mean 'login'?")
    );
    assert_eq!(ask("login").error.code, "E045");
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "an unknown format is refused naming the expected formats"
)]
fn query_refuses_dot() {
    let error = query(
        &linear().view(),
        &QueryRequest {
            entity_id: "b",
            format: Some(Format::Dot),
            ..QueryRequest::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.error.code, "invalid_input");
    assert_eq!(
        error.error.message,
        "Unknown format: dot. Expected: graph, context, brief"
    );
}

/// Three behaviors with a `contract` each, `b2` and `b3` sharing one.
fn behaviors() -> Assembled {
    Assembled::over(registries(&["behavior"], &[])).source("behavior b3 \"Three\" {\n  contract \"x\"\n}\n\nbehavior b1 \"One\" {\n  contract \"x\"\n}\n\nbehavior b2 \"Two\" {\n  contract \"y\"\n}\n")
}

fn listed(project: &Assembled, request: &ListRequest) -> Vec<String> {
    plain(list(&project.view(), request))
        .iter()
        .map(|n| n.id.raw.to_string())
        .collect()
}

#[specforge_test(
    behavior = "select_entities_over_the_view",
    verify = "list pages the entities of a kind sorted by id, from offset, at most limit"
)]
fn list_pages_sorted_by_id() {
    let project = behaviors();
    let page = |offset, limit| {
        listed(
            &project,
            &ListRequest {
                kind: Some("behavior"),
                offset,
                limit,
                ..ListRequest::default()
            },
        )
    };
    assert_eq!(page(0, None), ["b1", "b2", "b3"]);
    assert_eq!(page(1, Some(1)), ["b2"]);
    assert_eq!(page(2, Some(5)), ["b3"]);
    // No kind, or an empty one, lists every kind.
    let all = |kind| {
        listed(
            &project,
            &ListRequest {
                kind,
                ..ListRequest::default()
            },
        )
    };
    assert_eq!(all(None), ["b1", "b2", "b3"]);
    assert_eq!(all(Some("")), ["b1", "b2", "b3"]);
}

#[specforge_test(
    behavior = "select_entities_over_the_view",
    verify = "list keeps the entities whose fields hold every where value"
)]
fn list_matches_where_values() {
    let project = behaviors();
    let wanted = |value: Value| value.as_object().unwrap().clone();
    let with = |fields: &serde_json::Map<String, Value>| {
        listed(
            &project,
            &ListRequest {
                fields: Some(fields),
                ..ListRequest::default()
            },
        )
    };
    assert_eq!(with(&wanted(json!({"contract": "x"}))), ["b1", "b3"]);
    assert_eq!(with(&wanted(json!({"contract": "y"}))), ["b2"]);
    assert!(with(&wanted(json!({"contract": "z"}))).is_empty());
    assert!(with(&wanted(json!({"missing": "x"}))).is_empty());
}

#[test]
fn list_reports_an_unknown_kind() {
    let project = behaviors();
    let listing = list(
        &project.view(),
        &ListRequest {
            kind: Some("behaviour"),
            ..ListRequest::default()
        },
    );
    let (entities, notices) = listing.into_value_and_notices();
    assert!(entities.is_empty());
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].code, "I020");
    assert_eq!(
        notices[0].suggestion.as_deref(),
        Some("did you mean 'behavior'?")
    );
}

#[specforge_test(
    behavior = "select_entities_over_the_view",
    verify = "a search returns at most limit hits, twenty when no limit is given"
)]
fn search_defaults_to_twenty() {
    let mut graph = Graph::new();
    for i in 0..30 {
        graph.add_node(node(&format!("item_{i:02}"), "behavior"));
    }
    let project = Assembled::over(Default::default()).graph(graph);
    let found = |limit| {
        plain(search(
            &project.view(),
            &SearchRequest {
                text: "item",
                limit,
                ..SearchRequest::default()
            },
        ))
        .len()
    };
    assert_eq!(DEFAULT_SEARCH_LIMIT, 20);
    assert_eq!(found(None), 20);
    assert_eq!(found(Some(3)), 3);
    assert_eq!(found(Some(100)), 30);
}

#[test]
fn search_filters_by_a_field_pair_and_snippets_the_match() {
    let project = behaviors();
    let view = project.view();
    let hits = plain(search(
        &view,
        &SearchRequest {
            text: "",
            field: Some(FieldHolds {
                field: "contract",
                value: "X",
            }),
            ..SearchRequest::default()
        },
    ));
    let found: Vec<&str> = hits
        .iter()
        .map(|hit| hit.found.node.id.raw.as_str())
        .collect();
    assert_eq!(found, ["b1", "b3"]);

    // A match in a string field carries the text around it.
    let outcome = search(
        &view,
        &SearchRequest {
            text: "y",
            kinds: vec!["behaviour"],
            ..SearchRequest::default()
        },
    );
    assert_eq!(outcome.notices()[0].code, "I020");
    assert!(plain(outcome).is_empty());
}

// The contract of query_graph_multi_resolution, over a view: a(feature) ->
// b(behavior) -> c(behavior) -> x(invariant).
#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "Query Graph at Multiple Resolutions: multi-resolution graph query holds — validation_complete_fired, depth_respected, kind_filter_applied, graph_protocol_conformance, graph_queried_emitted"
)]
fn query_contract_valid_entity_returns_subgraph() {
    let mut graph = Graph::new();
    graph.add_node(node("a", "feature"));
    graph.add_node(node("b", "behavior"));
    graph.add_node(node("c", "behavior"));
    graph.add_node(node("x", "invariant"));
    for (source, target, label) in [
        ("a", "b", "behaviors"),
        ("b", "c", "depends_on"),
        ("c", "x", "invariants"),
    ] {
        graph.add_edge(Edge {
            source: source.into(),
            target: target.into(),
            label: label.into(),
        });
    }
    let project = Assembled::over(Default::default()).graph(graph);
    let query = |depth: usize, kinds: &[&str]| document(&project, "a", depth, kinds);

    // depth_respected: exactly the entities within N hops.
    assert_eq!(ids(&query(0, &[])), ["a"]);
    assert_eq!(ids(&query(1, &[])), ["a", "b"]);
    assert_eq!(ids(&query(2, &[])), ["a", "b", "c"]);
    assert_eq!(ids(&query(3, &[])), ["a", "b", "c", "x"]);

    // kind_filter_applied: only the listed kinds, plus the queried root.
    assert_eq!(ids(&query(3, &["behavior"])), ["a", "b", "c"]);
    assert_eq!(ids(&query(3, &["invariant"])), ["a", "x"]);

    // graph_protocol_conformance: schema_version, and edges only between
    // returned nodes.
    let result = query(2, &[]);
    assert!(result["schema_version"].is_string());
    assert_eq!(
        result["edges"],
        json!([
            { "source": "a", "target": "b", "label": "behaviors" },
            { "source": "b", "target": "c", "label": "depends_on" },
        ])
    );
}

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
)]
fn query_states_which_field_a_refusal_is_about() {
    use specforge_ops::read::query::QueryField;
    let project = linear();
    let missing = query(
        &project.view(),
        &QueryRequest {
            entity_id: "zzzzzzzz",
            ..QueryRequest::default()
        },
    )
    .unwrap_err();
    assert_eq!(missing.subject, Some(QueryField::EntityId));
    let dot = query(
        &project.view(),
        &QueryRequest {
            entity_id: "b",
            format: Some(Format::Dot),
            ..QueryRequest::default()
        },
    )
    .unwrap_err();
    assert_eq!(dot.subject, Some(QueryField::Format));
}

/// The end-to-end query tests, which `specforge-cli` ran through the binary: the same requests, over a compiled
/// project.
mod compiled {
    use crate::compiled_support::{built, published, queried, query_document, specs};
    use specforge_ops::read::export::Format;
    use specforge_ops::read::query::QueryRequest;
    use specforge_test_macros::test as specforge_test;
    // --- Query depth tests, over a compiled project ---

    #[specforge_test(
        behavior = "query_graph_multi_resolution",
        verify = "depth 0 returns only the target entity"
    )]
    fn query_depth_0_returns_only_root() {
        let (_project, compiled) = built(&[("main.spec", specs::DEEP_CHAIN)]);

        let parsed = query_document(&compiled, "beh_middle", Some(0), &[]);
        let nodes = parsed["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 1, "depth 0 should return only the root entity");
        assert_eq!(nodes[0]["id"], "beh_middle");
    }

    #[specforge_test(
        behavior = "query_graph_multi_resolution",
        verify = "depth 0 returns only the target entity"
    )]
    fn query_depth_0_edges_behavior() {
        let (_project, compiled) = built(&[("main.spec", specs::DEEP_CHAIN)]);

        let parsed = query_document(&compiled, "beh_middle", Some(0), &[]);
        let edges = parsed["edges"].as_array().unwrap();
        // With only one node, no edges can survive (both endpoints must be present)
        assert_eq!(edges.len(), 0, "depth 0 single node should have no edges");
    }

    #[specforge_test(
        behavior = "query_graph_multi_resolution",
        verify = "multiple kind filters combine as union"
    )]
    fn query_multiple_kind_filters() {
        let (_project, compiled) = built(&[("main.spec", specs::DEEP_CHAIN)]);

        let parsed = query_document(&compiled, "beh_middle", Some(2), &["behavior", "invariant"]);
        let nodes = parsed["nodes"].as_array().unwrap();

        let kinds: Vec<&str> = nodes.iter().map(|n| n["kind"].as_str().unwrap()).collect();
        for kind in &kinds {
            assert!(
                *kind == "behavior" || *kind == "invariant",
                "all nodes should be behavior or invariant, got '{}'",
                kind
            );
        }
        assert!(nodes.len() >= 2, "should include at least root + inv_deep");
    }

    #[specforge_test(
        behavior = "query_graph_multi_resolution",
        verify = "kind filter restricts results to specified entity kinds"
    )]
    fn query_kind_filter_no_match_returns_root_only() {
        let (_project, compiled) = built(&[("main.spec", specs::DEEP_CHAIN)]);

        let parsed = query_document(&compiled, "beh_middle", Some(2), &["nonexistent_kind"]);
        let nodes = parsed["nodes"].as_array().unwrap();
        assert_eq!(
            nodes.len(),
            1,
            "only root should survive when kind filter matches nothing"
        );
        assert_eq!(nodes[0]["id"], "beh_middle");
    }

    #[specforge_test(
        behavior = "query_graph_multi_resolution",
        verify = "kind filter restricts results to specified entity kinds"
    )]
    fn query_kind_filter_prunes_edges() {
        let (_project, compiled) = built(&[("main.spec", specs::DEEP_CHAIN)]);

        // Query with kind=behavior should prune edges to non-behavior nodes
        let parsed = query_document(&compiled, "beh_middle", Some(3), &["behavior"]);
        let nodes = parsed["nodes"].as_array().unwrap();
        let edges = parsed["edges"].as_array().unwrap();

        let node_ids: Vec<&str> = nodes.iter().map(|n| n["id"].as_str().unwrap()).collect();

        // Unfiltered, depth 3 reaches feat_root and inv_deep; --kind=behavior
        // keeps only behaviors.
        assert_eq!(node_ids, ["beh_middle"], "{parsed}");
        for node in nodes {
            assert_eq!(node["kind"], "behavior", "{node}");
        }
        assert!(edges.is_empty(), "edges to filtered-out nodes are pruned");
        let unfiltered = query_document(&compiled, "beh_middle", Some(3), &[]);
        assert_eq!(unfiltered["nodes"].as_array().unwrap().len(), 3);
        assert_eq!(unfiltered["edges"].as_array().unwrap().len(), 2);

        // Every edge endpoint must reference an existing node
        for edge in edges {
            let source = edge["source"].as_str().unwrap();
            let target = edge["target"].as_str().unwrap();
            assert!(
                node_ids.contains(&source),
                "edge source '{}' not in filtered nodes {:?}",
                source,
                node_ids
            );
            assert!(
                node_ids.contains(&target),
                "edge target '{}' not in filtered nodes {:?}",
                target,
                node_ids
            );
        }
    }

    #[specforge_test(
        behavior = "query_graph_multi_resolution",
        verify = "an unknown --kind is reported with I020 and the closest kind"
    )]
    fn query_reports_an_unknown_kind() {
        let (_project, compiled) = built(&[("main.spec", specs::DEEP_CHAIN)]);

        // The notice is the command's stderr; the document is the behavior-only
        // query, unchanged by it.
        let outcome = queried(
            &compiled,
            &QueryRequest {
                entity_id: "beh_middle",
                depth: None,
                kinds: vec!["behaviour", "behavior"],
                ..Default::default()
            },
        )
        .unwrap();
        let (document, notices) = outcome.into_value_and_notices();
        let stderr: String = notices
            .iter()
            .map(|notice| format!("{}\n", specforge_common::render_plain(notice)))
            .collect();
        assert_eq!(
            stderr,
            "info[I020]: unknown entity kind 'behaviour'\n  = help: did you mean 'behavior'?\n"
        );
        let parsed: serde_json::Value = serde_json::from_str(&document).unwrap();
        let ids: Vec<&str> = parsed["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["beh_middle"], "{parsed}");
    }

    #[specforge_test(
        behavior = "query_graph_multi_resolution",
        verify = "depth 1 returns direct neighbors"
    )]
    fn query_isolated_entity_depth_1() {
        let spec = format!("{}\n{}", specs::DEEP_CHAIN, specs::ISOLATED);
        let (_project, compiled) = built(&[("main.spec", &spec)]);
        let query = |id: &str| {
            let parsed = query_document(&compiled, id, Some(1), &[]);
            let mut ids: Vec<String> = parsed["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n["id"].as_str().unwrap().to_string())
                .collect();
            ids.sort();
            ids
        };

        // inv_deep's one neighbor is beh_middle; feat_root is two hops away.
        assert_eq!(query("inv_deep"), ["beh_middle", "inv_deep"]);
        // beh_middle neighbors both feat_root and inv_deep.
        assert_eq!(query("beh_middle"), ["beh_middle", "feat_root", "inv_deep"]);
        // An isolated node has no neighbors.
        assert_eq!(query("isolated_node"), ["isolated_node"]);
    }

    #[specforge_test(
        invariant = "graph_traversal_integrity",
        verify = "traversal from any node visits every reachable node exactly once"
    )]
    fn query_handles_cycles() {
        let (_project, compiled) = built(&[("main.spec", specs::CYCLE)]);

        let parsed = query_document(&compiled, "cycle_a", Some(10), &[]);
        let nodes = parsed["nodes"].as_array().unwrap();

        // Should contain all cycle nodes without duplicates
        let ids: Vec<&str> = nodes.iter().map(|n| n["id"].as_str().unwrap()).collect();
        let unique: std::collections::BTreeSet<&str> = ids.iter().copied().collect();
        assert_eq!(
            ids.len(),
            unique.len(),
            "no duplicate nodes in query results"
        );
        assert_eq!(
            unique,
            std::collections::BTreeSet::from(["cycle_a", "cycle_b", "cycle_c"]),
            "every node reachable around the cycle is visited"
        );
    }

    #[specforge_test(
        behavior = "query_graph_multi_resolution",
        verify = "depth N returns all entities within N hops"
    )]
    fn query_large_depth_on_small_graph() {
        let (_project, compiled) = built(&[("main.spec", specs::DEEP_CHAIN)]);

        let parsed = query_document(&compiled, "beh_middle", Some(100), &[]);
        let nodes = parsed["nodes"].as_array().unwrap();
        // Should get all connected nodes (beh_middle + feat_root + inv_deep, maybe typ_leaf if connected)
        assert!(
            nodes.len() >= 3,
            "depth 100 on 4-node graph should return all connected nodes, got {}",
            nodes.len()
        );
    }

    #[specforge_test(
        behavior = "query_graph_multi_resolution",
        verify = "output includes schema_version field"
    )]
    fn query_output_has_schema_version() {
        let (_project, compiled) = built(&[("main.spec", specs::ISOLATED)]);

        let parsed = query_document(&compiled, "isolated_node", Some(0), &[]);
        assert!(
            parsed["schema_version"].is_string(),
            "query output must include schema_version field"
        );
    }

    #[specforge_test(
        behavior = "query_graph_multi_resolution",
        verify = "output conforms to Graph Protocol schema"
    )]
    fn query_nodes_have_required_fields() {
        let (_project, compiled) = built(&[("main.spec", specs::DEEP_CHAIN)]);

        let parsed = query_document(&compiled, "beh_middle", Some(2), &[]);

        // The Graph Protocol shape, as the published schema declares it for a
        // node and an edge (`specforge schema --publish`).
        let published = published(&compiled, Format::Graph);
        let props = &published["properties"];
        for key in ["format_version", "schema_version", "nodes", "edges"] {
            assert!(parsed.get(key).is_some(), "missing top-level '{key}'");
        }
        assert!(parsed["schema_version"].is_string());
        for key in parsed.as_object().unwrap().keys() {
            assert!(
                props.get(key).is_some(),
                "top-level '{key}' is not in the Graph Protocol"
            );
        }
        // The reply is a document the published schema admits.
        assert_eq!(
            specforge_common::shape::violations(&published, &parsed),
            Vec::<String>::new()
        );
        let nodes = parsed["nodes"].as_array().unwrap();
        let edges = parsed["edges"].as_array().unwrap();
        assert_eq!(nodes.len(), 3, "{parsed}");
        assert_eq!(edges.len(), 2, "{parsed}");
        let beh = nodes.iter().find(|n| n["id"] == "beh_middle").unwrap();
        assert_eq!(beh["kind"], "behavior");
        assert_eq!(beh["title"], "Behavior Middle");
        assert_eq!(beh["fields"]["contract"], "The system MUST validate");
    }
}
