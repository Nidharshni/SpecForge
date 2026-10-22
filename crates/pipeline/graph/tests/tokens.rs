//! The one finder of written tokens (ADR 0163): which tokens of a file name an entity, a default no block takes and
//! a type name a derived field links included.

use crate::defaults::forms;
use crate::derived::{derived, inputs};
use specforge_graph::{
    GraphConfig, GraphInputs, Holder, WrittenToken, build_graph_with_config, written_tokens,
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

#[specforge_test(
    behavior = "link_derived_references",
    verify = "the written tokens of a file read a list written as one value as a list"
)]
fn a_list_written_as_one_value_is_no_type_name() {
    let source =
        "shape Car \"Car\" {\n  parts Wheel\n}\n\nshape Wheel \"Wheel\" {\n  size number\n}\n";
    let (_, tokens) = find(source, derived_inputs);
    assert_eq!(show(&tokens), ["Wheel Car parts Item 2:9-2:14"]);
}

#[test]
fn a_word_in_a_string_list_is_no_type_name() {
    for source in [
        "shape Car \"Car\" {\n  tags Wheel\n}\n\nshape Wheel \"Wheel\" {\n  size number\n}\n",
        "defaults shape {\n  tags Wheel\n}\n\nshape Car \"Car\" {\n}\n\nshape Wheel \"Wheel\" {\n  size number\n}\n",
    ] {
        let (graph, tokens) = find(source, derived_inputs);
        assert_eq!(show(&tokens), Vec::<String>::new(), "{source}");
        assert!(graph.edges_from("Car").is_empty(), "{source}");
    }
}

const SHAPES: &str = "shape Wheel \"Wheel\" {\n  size number\n}\n\nshape Axle \"Axle\" {\n  ends Wheel\n}\n\n\
shape Car \"Car\" {\n  wheels Wheel[]\n  front  Axle | Wheel\n  nested Option<Axle>[]\n  quoted \"Wheel\" | Axle\n}\n\n\
iface Detector \"Detector\" {\n  method detect(Wheel: Axle) -> Result<Wheel, Error>\n}\n";

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

/// `"target L:C-L:C held_by,…"` (an empty list prints nothing after the span).
fn held(tokens: &[WrittenToken]) -> Vec<String> {
    tokens
        .iter()
        .map(|t| {
            let holders: Vec<&str> = t.held_by.iter().map(|id| id.as_str()).collect();
            format!(
                "{} {}:{}-{}:{} {}",
                t.target,
                t.span.start_line,
                t.span.start_col,
                t.span.end_line,
                t.span.end_col,
                holders.join(",")
            )
        })
        .collect()
}

const TAKEN: &str = "defaults behavior {\n  ports [Repo]\n}\n\nbehavior a \"A\" {\n}\n\nbehavior b \"B\" {\n}\n\n\
behavior c \"C\" {\n  ports [Repo]\n}\n\nport Repo \"Repo\" {\n}\n";
const SELF_DEFAULT: &str =
    "defaults shape {\n  slot Wheel\n}\n\nshape Wheel \"Wheel\" {\n}\n\nshape Car \"Car\" {\n}\n";
const DUPLICATE: &str = "behavior a \"A\" {\n  ports [Repo]\n}\n\nbehavior a \"Again\" {\n  ports [Repo]\n}\n\n\
port Repo \"Repo\" {\n}\n";

#[test]
fn a_default_is_held_by_every_block_that_takes_it() {
    let (_, tokens) = find(TAKEN, defaults_inputs);
    assert_eq!(held(&tokens), ["Repo 2:10-2:14 a,b", "Repo 12:10-12:14 c"]);
}

#[test]
fn a_default_no_block_takes_is_held_by_none() {
    let (_, tokens) = find("defaults behavior {\n  ports [Repo]\n}\n", defaults_inputs);
    assert_eq!(held(&tokens), ["Repo 2:10-2:14 "]);
}

#[test]
fn a_block_does_not_hold_a_default_type_that_names_it() {
    let (_, tokens) = find(SELF_DEFAULT, derived_inputs);
    assert_eq!(held(&tokens), ["Wheel 2:8-2:13 Car"]);
}

#[test]
fn a_duplicate_declarations_tokens_are_held_by_its_id() {
    let (_, tokens) = find(DUPLICATE, defaults_inputs);
    assert_eq!(held(&tokens), ["Repo 2:10-2:14 a", "Repo 6:10-6:14 a"]);
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "every edge is a token the graph's finder reads"
)]
fn every_edge_is_held_by_a_token() {
    for (source, make, duplicate) in [
        (SHAPES, derived_inputs as fn() -> GraphInputs, false),
        (TAKEN, defaults_inputs, false),
        (SELF_DEFAULT, derived_inputs, false),
        (DUPLICATE, defaults_inputs, true),
    ] {
        let (graph, tokens) = find(source, make);
        for edge in graph.edges() {
            assert!(
                tokens.iter().any(|t| t.target == edge.target
                    && t.field == edge.label
                    && t.held_by.contains(&edge.source)),
                "{edge:?} is held by no token: {tokens:?}"
            );
        }
        if duplicate {
            continue;
        }
        for token in tokens
            .iter()
            .filter(|t| graph.node(t.target.as_str()).is_some())
        {
            for holder in &token.held_by {
                assert!(
                    graph
                        .edges_from(holder.as_str())
                        .iter()
                        .any(|e| e.target == token.target && e.label == token.field),
                    "{token:?} held by {holder} has no edge"
                );
            }
        }
    }
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "every edge is a token the graph's finder reads"
)]
fn a_list_item_is_a_token_at_the_item() {
    let source =
        "shape Wheel \"Wheel\" {\n  size number\n}\nshape Car \"Car\" {\n  parts [Wheel]\n}\n";
    let (_, tokens) = find(source, derived_inputs);
    assert_eq!(show(&tokens), ["Wheel Car parts Item 5:10-5:15"]);
}

#[test]
fn a_name_written_twice_is_two_tokens() {
    let source = "shape Wheel \"Wheel\" {\n  size number\n}\nshape Car \"Car\" {\n  parts [Wheel, Wheel]\n}\n";
    let (graph, tokens) = find(source, derived_inputs);
    assert_eq!(graph.edges_from("Car").len(), 1);
    assert_eq!(
        show(&tokens),
        [
            "Wheel Car parts Item 5:10-5:15",
            "Wheel Car parts Item 5:17-5:22"
        ]
    );
}

/// The delta compares methods through their serialized form, which has no positions.
#[test]
fn a_methods_positions_are_not_serialized() {
    let file = parse("port P {\n  method run(c: Config) -> Out\n}\n", "p.spec");
    let method = &file.entities[0].methods[0];
    assert!(method.params[0].ty_span.is_some());

    let json = serde_json::to_value(method).unwrap().to_string();

    assert!(
        !json.contains("ty_span") && !json.contains("returns_span"),
        "{json}"
    );
}

#[specforge_test(
    behavior = "link_derived_references",
    verify = "the written tokens of a file include every name a derived field links"
)]
fn every_name_of_a_signature_is_a_token() {
    let source = r#"
shape Config "Config" {
  root string
}
shape Project "Project" {
  name string
}
shape Report "Report" {
  ok boolean
}
iface Detector "Detector" {
  method detect(config: Config) -> Result<Project, Error>
  method report(items: Project[]) -> Report
}
"#;
    let (_, tokens) = find(source, derived_inputs);
    assert_eq!(
        show(&tokens),
        [
            "Config Detector uses Derived 12:25-12:31",
            "Project Detector uses Derived 12:43-12:50",
            "Project Detector uses Derived 13:24-13:31",
            "Report Detector uses Derived 13:38-13:44",
        ]
    );
}

#[test]
fn a_parameters_name_is_no_token_and_its_type_is() {
    let source = "shape Config \"Config\" {\n  root string\n}\niface X \"X\" {\n  method keep(Config: Config) -> Config\n}\n";
    let (_, tokens) = find(source, derived_inputs);
    assert_eq!(
        show(&tokens),
        [
            "Config X uses Derived 5:23-5:29",
            "Config X uses Derived 5:34-5:40"
        ]
    );
}

#[test]
fn a_union_member_is_a_token_at_its_name() {
    let source = "shape Wheel \"Wheel\" {\n  size number\n}\nshape Config \"Config\" {\n  root string\n}\n\
shape Kind = Wheel | Config\nshape Holder \"Holder\" {\n  slot Wheel | Config\n}\n";
    let (_, tokens) = find(source, derived_inputs);
    assert_eq!(
        show(&tokens),
        [
            "Wheel Kind parts Derived 7:14-7:19",
            "Config Kind parts Derived 7:22-7:28",
            "Wheel Holder parts Derived 9:8-9:13",
            "Config Holder parts Derived 9:16-9:22",
        ]
    );
}

#[test]
fn a_name_on_a_later_line_is_a_token_on_its_line() {
    let source = "shape Config \"Config\" {\n  root string\n}\nshape Project \"Project\" {\n  name string\n}\n\
iface X \"X\" {\n  method run() -> Result<\n    Config,\n    Project>\n}\n";
    let (_, tokens) = find(source, derived_inputs);
    assert_eq!(
        show(&tokens),
        [
            "Config X uses Derived 9:5-9:11",
            "Project X uses Derived 10:5-10:12"
        ]
    );
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "a quoted reference is written at its ID, inside its quotes"
)]
fn a_quoted_reference_is_a_token_at_its_id() {
    let source = "shape Wheel \"Wheel\" {\n  size number\n}\nshape Car \"Car\" {\n  base \"Wheel\"\n  parts [\"Wheel\", Wheel]\n}\n";
    let (_, tokens) = find(source, derived_inputs);
    assert_eq!(
        show(&tokens),
        [
            "Wheel Car base Single 5:9-5:14",
            "Wheel Car parts Item 6:11-6:16",
            "Wheel Car parts Item 6:19-6:24",
        ]
    );
}
