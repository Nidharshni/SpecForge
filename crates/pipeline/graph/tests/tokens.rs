//! The one finder of written tokens (ADR 0163): which tokens of a file name an entity, a default no block takes and
//! a type name a derived field links included.

use crate::defaults::forms;
use crate::derived::{derived, inputs};
use specforge_graph::{
    Form, GraphConfig, GraphInputs, Holder, WrittenToken, build_graph_with_config, written_tokens,
};
use specforge_parser::parse;
use specforge_test_macros::test as specforge_test;

fn defaults_inputs() -> GraphInputs {
    GraphInputs {
        field_forms: forms(),
        ..Default::default()
    }
}

/// The graph of `source` and the finder's tokens for it, both with `make`'s inputs.
fn find(source: &str, make: fn() -> GraphInputs) -> (specforge_graph::Graph, Vec<WrittenToken>) {
    let file = parse(source, "main.spec");
    let config = GraphConfig {
        inputs: make(),
        ..Default::default()
    };
    let (graph, _) = build_graph_with_config(std::slice::from_ref(&file), &config);
    let tokens = written_tokens(&file, &make(), |n| graph.node(n).map(|n| n.kind.raw));
    (graph, tokens)
}

fn derived_inputs() -> GraphInputs {
    inputs(derived())
}

/// `"target holder field form L:C-L:C"`, to compare a token list at a glance.
fn show(tokens: &[WrittenToken]) -> Vec<String> {
    tokens
        .iter()
        .map(|t| {
            let holder = match t.holder {
                Holder::Block(id) => id.to_string(),
                Holder::Defaults(kind) => format!("defaults {kind}"),
            };
            format!(
                "{} {holder} {} {:?} {}:{}-{}:{}",
                t.target,
                t.field,
                t.form,
                t.span.start_line,
                t.span.start_col,
                t.span.end_line,
                t.span.end_col
            )
        })
        .collect()
}

#[specforge_test(
    behavior = "apply_file_defaults",
    verify = "the written tokens of a file include a default no block takes"
)]
fn a_default_no_block_takes_is_a_token() {
    let (_, tokens) = find("defaults behavior {\n  ports [Repo]\n}\n", defaults_inputs);
    assert_eq!(
        show(&tokens),
        ["Repo defaults behavior ports Item 2:10-2:14"]
    );
}

#[test]
fn a_taken_default_is_one_token_however_many_blocks_take_it() {
    let source = "defaults behavior {\n  ports [Repo]\n}\n\nbehavior a \"A\" {\n}\n\nbehavior b \"B\" {\n}\n\nbehavior c \"C\" {\n}\n";
    let (_, tokens) = find(source, defaults_inputs);
    assert_eq!(
        show(&tokens),
        ["Repo defaults behavior ports Item 2:10-2:14"]
    );
}

#[test]
fn an_unapplied_default_names_nothing() {
    // `owner` is no field of behavior (E077); the second block for the kind is not applied (E078).
    let source = "defaults behavior {\n  ports [Repo]\n  owner [Gone]\n}\n\ndefaults behavior {\n  features [Other]\n}\n";
    let (_, tokens) = find(source, defaults_inputs);
    assert_eq!(
        show(&tokens),
        ["Repo defaults behavior ports Item 2:10-2:14"]
    );
}

#[test]
fn a_list_written_as_one_value_is_an_item() {
    let (_, tokens) = find("behavior a \"A\" {\n  ports Repo\n}\n", defaults_inputs);
    assert_eq!(show(&tokens), ["Repo a ports Item 2:9-2:13"]);
}

#[test]
fn an_undeclared_field_names_by_its_bare_words() {
    let (_, tokens) = find("behavior a \"A\" {\n  owner [Repo]\n}\n", defaults_inputs);
    assert_eq!(show(&tokens), ["Repo a owner Undeclared 2:10-2:14"]);
}

#[test]
fn a_single_reference_is_a_single_token() {
    let (_, tokens) = find("shape a \"A\" {\n  base Wheel\n}\n", derived_inputs);
    assert_eq!(show(&tokens), ["Wheel a base Single 2:8-2:13"]);
}

const SHAPES: &str = "shape Wheel \"Wheel\" {\n  size number\n}\n\nshape Axle \"Axle\" {\n  ends Wheel\n}\n\n\
shape Car \"Car\" {\n  wheels Wheel[]\n  front  Axle | Wheel\n  nested Option<Axle>[]\n  quoted \"Wheel\" | Axle\n}\n\n\
iface Detector \"Detector\" {\n  method detect(Wheel: Axle) -> Result<Wheel, Error>\n}\n";

#[specforge_test(
    behavior = "link_derived_references",
    verify = "the written tokens of a file include every name a derived field links"
)]
fn every_written_span_of_an_edge_is_a_token() {
    for (source, make) in [
        (SHAPES, derived_inputs as fn() -> GraphInputs),
        (
            "defaults behavior {\n  ports [Repo]\n}\n\nbehavior a \"A\" {\n}\n\nbehavior b \"B\" {\n}\n\nport Repo \"Repo\" {\n}\n",
            defaults_inputs,
        ),
    ] {
        let (graph, tokens) = find(source, make);
        assert!(!graph.edges().is_empty(), "{source}");
        for edge in graph.edges() {
            for span in graph.written(edge) {
                assert!(
                    tokens
                        .iter()
                        .any(|t| t.target == edge.target && &t.span == span),
                    "{edge:?} written at {span:?} is no token: {tokens:?}"
                );
            }
        }
        for token in tokens.iter().filter(|t| t.form == Form::Derived) {
            let Holder::Block(holder) = token.holder else {
                panic!("a derived token of a block: {token:?}");
            };
            assert!(
                graph.edges_from(holder.as_str()).iter().any(|e| {
                    e.target == token.target
                        && e.label == token.field
                        && graph.written(e).contains(&token.span)
                }),
                "{token:?} has no edge"
            );
        }
    }
}

#[test]
fn a_derived_name_of_another_kind_or_the_block_itself_is_no_token() {
    let source = "iface Other \"Other\" {\n  method ping() -> string\n}\n\nshape Holder \"Holder\" {\n  peer Other\n  me   Holder[]\n}\n";
    let (_, tokens) = find(source, derived_inputs);
    assert_eq!(show(&tokens), Vec::<String>::new());
}

#[test]
fn a_parameters_name_and_a_quoted_member_are_no_token() {
    let (_, tokens) = find(SHAPES, derived_inputs);
    // Line 17's `Wheel: Axle` names Axle only, and line 13's `"Wheel" | Axle` names Axle only.
    assert_eq!(
        show(&tokens),
        [
            "Wheel Axle parts Derived 6:8-6:13",
            "Wheel Car parts Derived 10:10-10:15",
            "Axle Car parts Derived 11:10-11:14",
            "Wheel Car parts Derived 11:17-11:22",
            "Axle Car parts Derived 12:17-12:21",
            "Axle Car parts Derived 13:20-13:24",
            "Axle Detector uses Derived 17:24-17:28",
            "Wheel Detector uses Derived 17:40-17:45",
        ]
    );
}
