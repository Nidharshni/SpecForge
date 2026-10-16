//! What counts as a reference position in the editor is the field form's reading of the field (ADR 0093).

use specforge_common::{SourceSpan, Sym};
use specforge_extension_sdk::prelude::FieldType;
use specforge_graph::{Graph, Node};
use specforge_lsp::{Document, MOD_REFERENCE};
use specforge_ops::view::ProjectView;
use specforge_project::testing::Assembled;

use crate::registries::registries;
use specforge_test_macros::test as specforge_test;

#[specforge_test(
    behavior = "provide_semantic_tokens",
    verify = "a string list's bare item is no reference"
)]
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
    let project = Assembled::over(registries).graph(graph);
    let view = ProjectView::of(project.detached());
    let tokens = Document::new("term t2 \"T2\" {\n  aliases [alpha]\n}\n".into()).tokens(&view);
    let modifiers = tokens
        .iter()
        .find(|t| t.text == "alpha")
        .map_or(0, |t| t.modifiers);
    assert_eq!(
        modifiers & MOD_REFERENCE,
        0,
        "a string list's bare item is a string: {tokens:?}"
    );
}
