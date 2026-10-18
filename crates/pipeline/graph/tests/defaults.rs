//! File defaults (ADR 0149): a `defaults <kind> { ... }` block gives its
//! fields to every block of that kind in its file (behavior
//! `apply_file_defaults`).

use specforge_graph::{
    FieldForm, FieldForms, Graph, GraphConfig, GraphInputs, build_graph_with_config,
};
use specforge_parser::parse;
use specforge_protocol_types::FieldType;

/// Kinds `behavior` (`ports`, `features`: reference lists; `owner` is not
/// declared), `port` and `feature`, standing in for an extension.
fn forms() -> FieldForms {
    let list =
        |target: &str| FieldForm::new(FieldType::ReferenceList, Vec::new()).targeting(target);
    FieldForms::new(
        [
            (("behavior".to_string(), "ports".to_string()), list("port")),
            (
                ("behavior".to_string(), "features".to_string()),
                list("feature"),
            ),
        ]
        .into_iter()
        .collect(),
        Default::default(),
        ["behavior", "port", "feature"]
            .into_iter()
            .map(String::from)
            .collect(),
    )
}

/// The graph of `files` (name, text) with the forms above.
pub(crate) fn build(files: &[(&str, &str)]) -> (Graph, Vec<specforge_graph::Diagnostic>) {
    let config = GraphConfig {
        inputs: GraphInputs {
            field_forms: forms(),
            ..Default::default()
        },
        ..Default::default()
    };
    let parsed: Vec<_> = files.iter().map(|(n, t)| parse(t, n)).collect();
    build_graph_with_config(&parsed, &config)
}

const A: &str = "defaults behavior {\n  ports [Repo]\n}\n\n\
                 behavior a \"A\" {\n}\n\nbehavior b \"B\" {\n}\n\nbehavior c \"C\" {\n}\n\n\
                 port Repo \"Repo\" {\n}\n";

#[test]
#[ignore = "flipped by T3"]
fn a_default_reaches_every_block_of_its_kind_in_its_file() {
    let (graph, _) = build(&[("a.spec", A)]);
    for id in ["a", "b", "c"] {
        assert!(
            graph.node(id).unwrap().fields.get("ports").is_some(),
            "{id}"
        );
        assert_eq!(graph.edges_from(id).len(), 1, "{id}");
    }
}

#[test]
#[ignore = "flipped by T3"]
fn a_defaults_block_is_no_node() {
    let (graph, diagnostics) = build(&[
        ("a.spec", "defaults behavior {\n  ports [Repo]\n}\n"),
        ("b.spec", "defaults behavior {\n  ports [Repo]\n}\n"),
    ]);
    assert!(graph.node("behavior").is_none());
    assert!(
        diagnostics.iter().all(|d| d.code != "E002"),
        "{diagnostics:?}"
    );
}
