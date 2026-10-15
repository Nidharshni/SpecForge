//! What counts as a reference position in the editor is the field form's reading of the field (ADR 0093).

use specforge_common::{SourceSpan, Sym};
use specforge_extension_sdk::prelude::FieldType;
use specforge_graph::{Graph, Node};
use specforge_lsp::{Document, MOD_REFERENCE};
use specforge_ops::view::ProjectView;
use specforge_project::coverage::RecordedCoverage;

use crate::registries::registries;

#[test]
#[ignore = "flipped by T6"]
fn a_bare_word_on_a_string_list_is_no_reference_position() {
    let registries = registries("@test/ext", |c| {
        c.kind("term", |k| {
            k.field("aliases", |f| {
                f.field_type(FieldType::StringList);
            });
        });
    });
    let mut graph = Graph::new();
    graph.add_node(Node::new(
        "alpha",
        "term",
        SourceSpan {
            file: Sym::new("other.spec"),
            start_line: 1,
            start_col: 1,
            end_line: 1,
            end_col: 1,
        },
    ));
    let env = specforge_project::Environment::with_registries(registries, None);
    let recorded = RecordedCoverage::over(&graph, &env);
    let view = ProjectView::new(&graph, &env, None, &recorded);
    let tokens = Document::new("term t2 \"T2\" {\n  aliases [alpha]\n}\n".into()).tokens(&view);
    let alpha = tokens.iter().find(|t| t.text == "alpha").unwrap();
    assert_eq!(
        alpha.modifiers & MOD_REFERENCE,
        0,
        "a string list's bare item is a string: {alpha:?}"
    );
}
