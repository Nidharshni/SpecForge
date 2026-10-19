use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Edge, Graph, Node};
use specforge_parser::{EntityId, EntityKind, FieldMap, FieldValue};
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

fn node_with_contract(id: &str, kind: &str, contract: &str) -> Node {
    let mut fields = FieldMap::new();
    fields.push(
        Sym::new("contract"),
        FieldValue::String(contract.to_string()),
    );
    Node {
        id: EntityId { raw: Sym::new(id) },
        kind: EntityKind {
            raw: Sym::new(kind),
        },
        title: Some(format!("Title {}", id)),
        fields,
        source_span: span(),
        methods: Vec::new(),
        obligations: Vec::new(),
    }
}

fn build_large_graph() -> Graph {
    let mut graph = Graph::new();
    // Create 10 nodes with decent-sized contracts
    for i in 0..10 {
        let id = format!("entity_{}", i);
        let contract = format!(
            "The system MUST handle case {} with full traceability and validation across all registered edge types and entity kinds in the graph. {}",
            i,
            "x".repeat(200)
        );
        graph.add_node(node_with_contract(&id, "behavior", &contract));
    }
    // Chain edges: 0->1->2->...->9
    for i in 0..9 {
        graph.add_edge(Edge {
            source: Sym::from(format!("entity_{}", i)),
            target: Sym::from(format!("entity_{}", i + 1)),
            label: Sym::new("depends_on"),
        });
    }
    graph
}

/// Degree (in + out edges) of each entity in `build_hub_graph`, stated by
/// hand from its edge list.
const HUB_DEGREES: [(&str, usize); 9] = [
    ("hub", 4),
    ("spoke_1", 2),
    ("spoke_2", 2),
    ("spoke_3", 1),
    ("spoke_4", 1),
    ("pair_x", 1),
    ("pair_y", 1),
    ("lone_a", 0),
    ("lone_b", 0),
];

/// Entities of equal size and different centrality: a hub with four
/// spokes (two of them also linked), a separate pair, two loners.
fn build_hub_graph() -> Graph {
    let mut graph = Graph::new();
    for (id, _) in HUB_DEGREES {
        graph.add_node(node_with_contract(
            id,
            "behavior",
            "The system MUST keep every entity the same size here",
        ));
    }
    for (source, target) in [
        ("hub", "spoke_1"),
        ("hub", "spoke_2"),
        ("hub", "spoke_3"),
        ("hub", "spoke_4"),
        ("spoke_1", "spoke_2"),
        ("pair_x", "pair_y"),
    ] {
        graph.add_edge(Edge {
            source: Sym::new(source),
            target: Sym::new(target),
            label: Sym::new("depends_on"),
        });
    }
    graph
}

/// The entity IDs of a JSON export's `nodes`, sorted.
fn sorted_node_ids(parsed: &serde_json::Value) -> Vec<String> {
    let mut ids: Vec<String> = parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap().to_string())
        .collect();
    ids.sort();
    ids
}

// B:enforce_token_budget — verify unit "output within budget includes all entities"
#[specforge_test(
    behavior = "enforce_token_budget",
    verify = "output within budget includes all entities"
)]
fn output_within_budget_includes_all_entities() {
    let graph = build_large_graph();
    // Large budget — everything fits
    let result = crate::support::budgeted_json(&graph, 100_000);
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

    let nodes = parsed["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 10);
    assert!(
        parsed.get("token_budget").is_none() || parsed["token_budget"].is_null(),
        "no budget metadata when everything fits"
    );
}

// B:enforce_token_budget — verify unit "output exceeding budget truncates low-priority entities"
#[specforge_test(
    behavior = "enforce_token_budget",
    verify = "output exceeding budget truncates low-priority entities"
)]
fn output_exceeding_budget_truncates_low_priority_entities() {
    let graph = build_hub_graph();
    let degree = |id: &str| HUB_DEGREES.iter().find(|(n, _)| *n == id).unwrap().1;

    // Room for about half the entities.
    let result = crate::support::budgeted_json(&graph, 300);
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    let kept = sorted_node_ids(&parsed);
    let truncated: Vec<&str> = parsed["token_budget"]["truncated_entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();

    // Least connected first: the loners, the pair, then one of the
    // degree-1 spokes (ties broken by ID).
    assert_eq!(
        truncated,
        vec!["lone_a", "lone_b", "pair_x", "pair_y", "spoke_3"]
    );
    assert_eq!(kept, vec!["hub", "spoke_1", "spoke_2", "spoke_4"]);
    // Every kept entity is at least as central as every truncated one.
    let weakest_kept = kept.iter().map(|id| degree(id)).min().unwrap();
    let strongest_cut = truncated.iter().map(|id| degree(id)).max().unwrap();
    assert!(
        strongest_cut <= weakest_kept,
        "cut degree {strongest_cut} above kept degree {weakest_kept}"
    );
}

// B:enforce_token_budget — verify unit "TokenBudgetResult included in metadata when budget applied"
#[specforge_test(
    behavior = "enforce_token_budget",
    verify = "TokenBudgetResult included in metadata when budget applied"
)]
fn token_budget_result_included_in_metadata() {
    let graph = build_large_graph();
    let result = crate::support::budgeted_json(&graph, 500);
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

    let budget = &parsed["token_budget"];
    assert!(
        budget.is_object(),
        "token_budget metadata must be present when truncated"
    );
    assert!(budget["strategy"].is_string());
    assert!(budget["truncated_entities"].is_array());
}

// B:enforce_token_budget — verify unit "truncated_entities lists omitted entity IDs"
#[specforge_test(
    behavior = "enforce_token_budget",
    verify = "truncated_entities lists omitted entity IDs"
)]
fn truncated_entities_list_contains_omitted_ids() {
    let graph = build_large_graph();
    let result = crate::support::budgeted_json(&graph, 500);
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

    let truncated = parsed["token_budget"]["truncated_entities"]
        .as_array()
        .unwrap();
    let remaining_ids: Vec<&str> = parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();

    // Truncated IDs should not appear in remaining nodes
    for id in truncated {
        let id_str = id.as_str().unwrap();
        assert!(
            !remaining_ids.contains(&id_str),
            "truncated entity {} should not appear in nodes",
            id_str
        );
    }

    // Together they should account for all 10
    assert_eq!(truncated.len() + remaining_ids.len(), 10);
}

// B:enforce_token_budget — verify unit "no --max-tokens skips budget enforcement"
#[specforge_test(
    behavior = "enforce_token_budget",
    verify = "no --max-tokens skips budget enforcement"
)]
fn no_max_tokens_skips_budget_enforcement() {
    let graph = build_large_graph();
    // The options `specforge export` and the MCP export tool build: no
    // --max-tokens is `token_budget: None`.
    let export = |format, token_budget| {
        specforge_emitter::emit(
            &graph,
            &specforge_emitter::EmitOptions {
                format,
                token_budget,
                ..Default::default()
            },
        )
        .unwrap()
    };
    use specforge_emitter::EmitFormat::{Brief, Context, Json};

    for format in [Json, Context, Brief] {
        let unbudgeted = export(format, None);
        let parsed: serde_json::Value = serde_json::from_str(&unbudgeted).unwrap();
        assert_eq!(parsed["nodes"].as_array().unwrap().len(), 10, "{format:?}");
        assert_eq!(parsed["edges"].as_array().unwrap().len(), 9, "{format:?}");
        assert!(parsed.get("token_budget").is_none(), "{format:?}");

        // The same export with a budget does truncate, so the full output
        // above is the budget being skipped, not a budget that fits.
        let budgeted: serde_json::Value = serde_json::from_str(&export(format, Some(100))).unwrap();
        assert!(
            budgeted["nodes"].as_array().unwrap().len() < 10,
            "{format:?}: a 100-token budget truncates this graph"
        );
    }
    // Unbudgeted graph export is the plain full export.
    assert_eq!(export(Json, None), crate::support::emit_json(&graph));
}

// B:enforce_token_budget — verify unit "truncated_entities lists omitted entity IDs"
// (validates no dangling edges remain after truncation)
#[specforge_test(behavior = "enforce_token_budget")]
fn no_dangling_edges_after_truncation() {
    let graph = build_large_graph();
    let result = crate::support::budgeted_json(&graph, 500);
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

    let node_ids: std::collections::HashSet<&str> = parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();

    for edge in parsed["edges"].as_array().unwrap() {
        let source = edge["source"].as_str().unwrap();
        let target = edge["target"].as_str().unwrap();
        assert!(
            node_ids.contains(source),
            "dangling edge source: {}",
            source
        );
        assert!(
            node_ids.contains(target),
            "dangling edge target: {}",
            target
        );
    }
}

// B:enforce_token_budget — verify integration "export with max_tokens produces output within budget and includes metadata"
#[specforge_test(
    behavior = "enforce_token_budget",
    verify = "export with max_tokens produces output within budget and includes metadata"
)]
fn export_with_max_tokens_within_budget_includes_metadata() {
    let graph = build_large_graph();
    // The graph export as the MCP export tool (and `specforge export
    // --max-tokens` without --with-schema) runs it, with a budget that
    // forces truncation.
    let export = |budget: usize| -> serde_json::Value {
        let out = specforge_emitter::emit(
            &graph,
            &specforge_emitter::EmitOptions {
                token_budget: Some(budget),
                ..Default::default()
            },
        )
        .unwrap();
        serde_json::from_str(&out).unwrap()
    };
    let parsed = export(500);

    // Metadata: the strategy, the budget, the estimate, what was cut.
    let meta = &parsed["token_budget"];
    assert_eq!(meta["strategy"], "prioritize");
    assert_eq!(meta["budget_tokens"], 500);
    let truncated: Vec<String> = meta["truncated_entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(!truncated.is_empty(), "{meta}");

    // Within budget.
    let estimated = meta["estimated_tokens"].as_u64().unwrap() as usize;
    assert!(estimated <= 500, "estimated {estimated} tokens over 500");
    // The estimate is the output's real cost: exactly that budget keeps the
    // same entities, one token less must cut another.
    let kept = sorted_node_ids(&parsed);
    assert_eq!(sorted_node_ids(&export(estimated)), kept);
    assert!(sorted_node_ids(&export(estimated - 1)).len() < kept.len());

    // Kept and truncated entities together are the whole graph.
    let mut all: Vec<String> = kept.iter().cloned().chain(truncated).collect();
    all.sort();
    let mut expected: Vec<String> = (0..10).map(|i| format!("entity_{i}")).collect();
    expected.sort();
    assert_eq!(all, expected);
}

#[test]
fn a_budget_counts_each_nodes_coverage_status() {
    use specforge_emitter::{EmitOptions, estimate_tokens};
    use std::collections::BTreeMap;

    let graph = build_hub_graph();
    let verdicts: BTreeMap<String, specforge_coverage::Verdict> = HUB_DEGREES
        .iter()
        .map(|(id, _)| {
            (
                id.to_string(),
                specforge_coverage::Verdict {
                    obligations: 1,
                    proven: 1,
                    ..Default::default()
                },
            )
        })
        .collect();
    let plain = specforge_emitter::emit(&graph, &EmitOptions::default()).unwrap();
    let budget = estimate_tokens(&plain);
    let fitted = specforge_emitter::emit(
        &graph,
        &EmitOptions {
            coverage: Some(&verdicts),
            token_budget: Some(budget),
            ..EmitOptions::default()
        },
    )
    .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&fitted).unwrap();
    let truncated = parsed["token_budget"]["truncated_entities"]
        .as_array()
        .expect("the keys no longer fit the plain export's budget");
    assert!(!truncated.is_empty());
    for node in parsed["nodes"].as_array().unwrap() {
        assert_eq!(node["coverage_status"], "covered", "{node}");
    }
}

#[specforge_test(
    behavior = "enforce_token_budget",
    verify = "an edge from an entity to itself adds nothing to its centrality"
)]
fn a_self_reference_adds_nothing_to_centrality() {
    use specforge_emitter::{EmitOptions, estimate_tokens};

    // selfish -depends_on-> selfish, a -depends_on-> b, lone: all the same size
    let mut graph = Graph::new();
    for id in ["selfish", "a", "b", "lone"] {
        graph.add_node(node_with_contract(id, "feature", "same size"));
    }
    for (source, target) in [("selfish", "selfish"), ("a", "b")] {
        graph.add_edge(Edge {
            source: Sym::new(source),
            target: Sym::new(target),
            label: Sym::new("depends_on"),
        });
    }
    let full = estimate_tokens(&crate::support::budgeted_json(&graph, 100_000));
    // At every budget that cuts something, the cut is a prefix of the one priority order.
    let order = ["lone", "selfish", "a", "b"];
    for budget in (1..full).rev() {
        // E062 below the cheapest export: stop there.
        let Ok(text) = specforge_emitter::emit(
            &graph,
            &EmitOptions {
                token_budget: Some(budget),
                ..Default::default()
            },
        ) else {
            break;
        };
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        let cut: Vec<&str> = parsed["token_budget"]["truncated_entities"]
            .as_array()
            .map(|ids| ids.iter().map(|v| v.as_str().unwrap()).collect())
            .unwrap_or_default();
        assert_eq!(cut, order[..cut.len()], "budget {budget}");
    }
}
