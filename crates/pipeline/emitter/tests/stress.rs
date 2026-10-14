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

fn build_graph(n: usize) -> Graph {
    let mut graph = Graph::new();
    for i in 0..n {
        let id = format!("entity_{i}");
        let mut fields = FieldMap::new();
        fields.push(Sym::new("status"), FieldValue::String("draft".to_string()));
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
#[ignore] // Run with: cargo test -p specforge-emitter -- stress --ignored
fn stress_emit_json_1000() {
    let graph = build_graph(1000);
    let output = crate::support::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["nodes"].as_array().unwrap().len(), 1000);
}

#[test]
#[ignore]
fn stress_emit_context_1000() {
    let graph = build_graph(1000);
    let output = crate::export(&graph, specforge_emitter::EmitFormat::Context, None);
    assert!(!output.is_empty());
}

#[test]
#[ignore]
fn stress_emit_brief_1000() {
    let graph = build_graph(1000);
    let output = crate::export(&graph, specforge_emitter::EmitFormat::Brief, None);
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["nodes"].as_array().unwrap().len(), 1000);
}
