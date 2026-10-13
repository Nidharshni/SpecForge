//! Query, list and search (`specforge_ops::query`, ADR 0015 "Query"): the
//! read views the CLI and MCP render, tested through the operation's
//! interface. The query tests are the emitter's former `query` tests,
//! re-homed where the behavior now lives.

use serde_json::{Value, json};
use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Edge, Graph, Node};
use specforge_ops::export::Format;
use specforge_ops::query::{
    DEFAULT_SEARCH_LIMIT, FieldHolds, ListRequest, QueryRequest, SearchRequest, list, query, search,
};
use specforge_parser::{EntityId, EntityKind, FieldMap};
use specforge_test::prelude::*;

use crate::view_support::{Project, registries};

fn node(id: &str, kind: &str) -> Node {
    Node {
        id: EntityId { raw: Sym::new(id) },
        kind: EntityKind {
            raw: Sym::new(kind),
        },
        title: Some(format!("Title {id}")),
        fields: FieldMap::new(),
        source_span: SourceSpan {
            file: Sym::new("test.spec"),
            start_line: 1,
            start_col: 0,
            end_line: 1,
            end_col: 0,
        },
        methods: Vec::new(),
    }
}

/// a(feature) -> b(behavior) -> c(invariant) -> d(event)
fn linear() -> Project {
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
    Project::of_graph(graph, Default::default())
}

/// The document `query` answers for `entity` at `depth`, filtered by `kinds`.
fn document(project: &Project, entity: &str, depth: usize, kinds: &[&str]) -> Value {
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
    serde_json::from_str(&outcome.document).unwrap()
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
    let default: Value = serde_json::from_str(&outcome.document).unwrap();
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
    let near = Project::of_graph(graph, Default::default());
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
    let project = Project::of_graph(
        {
            let mut graph = Graph::new();
            graph.add_node(node("login", "behavior"));
            graph
        },
        registries(&["behavior"], &[]),
    );
    let outcome = query(
        &project.view(),
        &QueryRequest {
            entity_id: "login",
            kinds: vec!["behaviour"],
            ..QueryRequest::default()
        },
    )
    .unwrap();
    let notice = &outcome.notices[0];
    assert_eq!(notice.code, "I020");
    assert_eq!(notice.message, "unknown entity kind 'behaviour'");
    assert_eq!(
        notice.suggestion.as_deref(),
        Some("did you mean 'behavior'?")
    );
    // The filter still drops the unknown kind: the root alone remains.
    let document: Value = serde_json::from_str(&outcome.document).unwrap();
    assert_eq!(ids(&document), ["login"]);
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "--include-coverage gives each entity its coverage status"
)]
fn query_with_coverage_marks_each_node() {
    let project = Project::new(
        "behavior login \"Login\" {\n  verify unit \"logs in\"\n}\n\nbehavior logout \"Logout\" {\n  verify unit \"logs out\"\n}\n",
        registries(&["behavior"], &[]),
    );
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
        let document: Value = serde_json::from_str(&outcome.document).unwrap();
        document["nodes"][0]["coverage_status"].clone()
    };
    assert_eq!(status("login", true), "covered");
    assert_eq!(status("logout", true), "uncovered");
    assert_eq!(status("login", false), Value::Null);
}

const COVERED_SOURCE: &str = "behavior login \"Login\" {\n  verify unit \"logs in\"\n}\n\nbehavior logout \"Logout\" {\n  verify unit \"logs out\"\n}\n";

/// The `login`/`logout` project with `login`'s obligation proven.
fn covered() -> Project {
    let project = Project::new(COVERED_SOURCE, registries(&["behavior"], &[]));
    project.record_passing(&[("login", "logs in")]);
    project
}

/// The document `query` answers for `entity` at depth 0 in `format`.
fn queried(project: &Project, entity: &str, format: Format, include_coverage: bool) -> String {
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
    .unwrap()
    .document
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "a query with --include-coverage validates against the published schema of its format"
)]
#[ignore = "fixed by T2"]
fn a_query_with_coverage_conforms_to_the_published_schema() {
    use specforge_common::shape::{unchecked_keywords, violations};
    let project = covered();
    for format in [Format::Graph, Format::Context, Format::Brief] {
        let published = specforge_ops::schema::json_schema(&project.view(), format).unwrap();
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
#[ignore = "fixed by T2"]
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
    let project = Project::new(COVERED_SOURCE, registries(&["behavior"], &[]));
    std::fs::write(project.dir.path().join("specforge-report.json"), "{").unwrap();
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
    behavior = "provide_mcp_query_tool",
    verify = "an unknown format is an invalid-input error naming the expected formats"
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
fn behaviors() -> Project {
    Project::new(
        "behavior b3 \"Three\" {\n  contract \"x\"\n}\n\nbehavior b1 \"One\" {\n  contract \"x\"\n}\n\nbehavior b2 \"Two\" {\n  contract \"y\"\n}\n",
        registries(&["behavior"], &[]),
    )
}

fn listed(project: &Project, request: &ListRequest) -> Vec<String> {
    list(&project.view(), request)
        .entities
        .iter()
        .map(|n| n.id.raw.to_string())
        .collect()
}

#[specforge_test(
    behavior = "provide_mcp_entities_by_kind",
    verify = "specforge.list pages the entities sorted by id with offset and limit"
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
    behavior = "provide_mcp_entities_by_kind",
    verify = "specforge.list keeps the entities whose fields hold the where values"
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
    assert!(listing.entities.is_empty());
    assert_eq!(listing.notices.len(), 1);
    assert_eq!(listing.notices[0].code, "I020");
    assert_eq!(
        listing.notices[0].suggestion.as_deref(),
        Some("did you mean 'behavior'?")
    );
}

#[specforge_test(
    behavior = "provide_mcp_search_tool",
    verify = "limit caps the number of returned results"
)]
fn search_defaults_to_twenty() {
    let mut graph = Graph::new();
    for i in 0..30 {
        graph.add_node(node(&format!("item_{i:02}"), "behavior"));
    }
    let project = Project::of_graph(graph, Default::default());
    let found = |limit| {
        search(
            &project.view(),
            &SearchRequest {
                text: "item",
                limit,
                ..SearchRequest::default()
            },
        )
        .hits
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
    let outcome = search(
        &view,
        &SearchRequest {
            text: "",
            field: Some(FieldHolds {
                field: "contract",
                value: "X",
            }),
            ..SearchRequest::default()
        },
    );
    let found: Vec<&str> = outcome
        .hits
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
    assert_eq!(outcome.notices[0].code, "I020");
    assert!(outcome.hits.is_empty());
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
    let project = Project::of_graph(graph, Default::default());
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
    use specforge_ops::query::QueryField;
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
