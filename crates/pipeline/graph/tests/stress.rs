use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Edge, Graph, Node};
use specforge_parser::{EntityId, EntityKind, FieldMap, FieldValue, SpannedRef};

fn span() -> SourceSpan {
    SourceSpan {
        file: Sym::new("stress.spec"),
        start_line: 1,
        start_col: 0,
        end_line: 1,
        end_col: 0,
    }
}

fn build_chain_graph(n: usize) -> Graph {
    let mut graph = Graph::new();
    for i in 0..n {
        let id = format!("entity_{i}");
        let mut fields = FieldMap::new();
        if i > 0 {
            fields.push(
                Sym::new("depends_on"),
                FieldValue::ReferenceList(vec![SpannedRef {
                    id: format!("entity_{}", i - 1),
                    span: span(),
                }]),
            );
        }
        graph.add_node(Node {
            id: EntityId { raw: Sym::new(&id) },
            kind: EntityKind {
                raw: Sym::new("behavior"),
            },
            title: Some(format!("Entity {i}")),
            fields,
            source_span: span(),
            methods: Vec::new(),
        });
        if i > 0 {
            graph.add_edge(Edge {
                source: Sym::new(&format!("entity_{i}")),
                target: Sym::new(&format!("entity_{}", i - 1)),
                label: Sym::new("depends_on"),
            });
        }
    }
    graph
}

#[test]
#[ignore] // Run with: cargo test -p specforge-graph -- stress --ignored
fn stress_build_1000_node_chain() {
    let graph = build_chain_graph(1000);
    assert_eq!(graph.nodes().len(), 1000);
    assert_eq!(graph.edges().len(), 999);
}

#[test]
#[ignore]
fn stress_subgraph_depth_on_1000_chain() {
    let graph = build_chain_graph(1000);
    let sub = graph.subgraph_depth("entity_999", 10).unwrap();
    assert!(sub.nodes().len() <= 11); // root + 10 depth
}

#[test]
#[ignore]
fn stress_subgraph_full_1000_chain() {
    let graph = build_chain_graph(1000);
    let sub = graph.subgraph("entity_500").unwrap();
    // Should include entity_500 and everything it transitively depends on
    assert!(!sub.nodes().is_empty());
}
