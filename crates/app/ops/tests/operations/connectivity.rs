//! The connectivity rule of the project view (plan 04, ADR 0015 "Prompt read
//! views"): one `Degree` per entity, stats' unconnected count and the count
//! of entities per kind.

use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Edge, Graph, Node};
use specforge_ops::view::Degree;
use specforge_parser::{EntityId, EntityKind, FieldMap};
use specforge_registry::RegistryBuild;
use specforge_test::prelude::*;

use crate::view_support::{Project, R1_SOURCE};

fn node(id: &str, kind: &str) -> Node {
    Node {
        id: EntityId { raw: Sym::new(id) },
        kind: EntityKind {
            raw: Sym::new(kind),
        },
        title: None,
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

#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "an entity is unconnected when no edge links it to another entity: a reference that does not resolve or names the entity itself links nothing"
)]
fn an_unresolved_or_self_reference_links_nothing() {
    let project = Project::new(R1_SOURCE, RegistryBuild::default());
    let connectivity = project.view().connectivity();

    assert_eq!(
        connectivity.unconnected().collect::<Vec<_>>(),
        ["alone", "dangling", "lonely", "selfish"]
    );
    assert_eq!(connectivity.degree("selfish"), Degree::default());
    assert_eq!(
        connectivity.degree("hub"),
        Degree {
            incoming: 1,
            outgoing: 0
        }
    );
    assert_eq!(connectivity.degree("ghost"), Degree::default());
    assert_eq!(
        Degree {
            incoming: 1,
            outgoing: 3
        }
        .lead(),
        2
    );
}

#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "the entities of each kind are counted once, for every kind an entity is written with"
)]
fn entities_are_counted_by_kind() {
    let mut graph = Graph::new();
    graph.add_node(node("a", "behavior"));
    graph.add_node(node("b", "behavior"));
    graph.add_node(node("w", "widget")); // a kind no extension declares
    graph.add_edge(Edge {
        source: Sym::new("a"),
        target: Sym::new("b"),
        label: Sym::new("needs"),
    });

    let project = Project::of_graph(graph, RegistryBuild::default());
    let counts = project.view().entities_by_kind();
    assert_eq!(
        counts.into_iter().collect::<Vec<_>>(),
        [("behavior", 2), ("widget", 1)]
    );
}
