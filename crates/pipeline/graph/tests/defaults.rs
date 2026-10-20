//! File defaults (ADR 0149): a `defaults <kind> { ... }` block gives its
//! fields to every block of that kind in its file (behavior
//! `apply_file_defaults`).

use specforge_common::Sym;
use specforge_graph::{
    FieldForm, FieldForms, FileChange, FileDefaults, Graph, GraphBuild, GraphConfig, GraphInputs,
    Holder, build_graph_with_config,
};
use specforge_parser::parse;
use specforge_protocol_types::FieldType;
use specforge_test_macros::test as specforge_test;

/// Kinds `behavior` (`ports`, `features`: reference lists; `owner` is not
/// declared), `port` and `feature`, standing in for an extension.
pub(crate) fn forms() -> FieldForms {
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

const BEHAVIORS: &str = "behavior a \"A\" {\n}\n\nbehavior b \"B\" {\n}\n\nbehavior c \"C\" {\n}\n\n\
                         port Repo \"Repo\" {\n}\n";

/// `defaults behavior { ports [Repo] }` above three behaviors and a port.
fn blocks() -> String {
    format!("defaults behavior {{\n  ports [Repo]\n}}\n\n{BEHAVIORS}")
}

fn coded<'d>(diagnostics: &'d [specforge_graph::Diagnostic], code: &str) -> Vec<&'d str> {
    diagnostics
        .iter()
        .filter(|d| d.code == code)
        .map(|d| d.message.as_str())
        .collect()
}

fn ports(graph: &Graph, id: &str) -> Vec<String> {
    match graph.node(id).unwrap().fields.get("ports") {
        Some(specforge_graph::FieldValue::List(items)) => {
            items.iter().map(|i| i.text.clone()).collect()
        }
        other => panic!("{id}: {other:?}"),
    }
}

#[specforge_test(
    behavior = "apply_file_defaults",
    verify = "a default reaches every block of its kind in its file"
)]
fn a_default_reaches_every_block_of_its_kind_in_its_file() {
    let (graph, _) = build(&[("a.spec", &blocks())]);
    for id in ["a", "b", "c"] {
        assert_eq!(ports(&graph, id), ["Repo"], "{id}");
        assert_eq!(graph.edges_from(id).len(), 1, "{id}");
    }
}

#[specforge_test(
    behavior = "apply_file_defaults",
    verify = "a defaults block is never a node"
)]
fn a_defaults_block_is_no_node() {
    let (graph, diagnostics) = build(&[
        ("a.spec", "defaults behavior {\n  ports [Repo]\n}\n"),
        ("b.spec", "defaults behavior {\n  ports [Repo]\n}\n"),
    ]);
    assert!(graph.node("behavior").is_none());
    assert!(coded(&diagnostics, "E002").is_empty(), "{diagnostics:?}");
}

#[specforge_test(
    behavior = "apply_file_defaults",
    verify = "a block's own field replaces the default whole"
)]
fn a_blocks_own_field_replaces_the_default_whole() {
    let text = "defaults behavior {\n  ports [Repo]\n}\n\nbehavior a \"A\" {\n  ports [Other]\n}\n\n\
         behavior b \"B\" {\n  ports []\n}\n\nbehavior c \"C\" {\n}\n\nport Repo \"Repo\" {\n}\n";
    let (graph, _) = build(&[("a.spec", text)]);
    assert_eq!(ports(&graph, "a"), ["Other"]);
    assert!(ports(&graph, "b").is_empty());
    assert!(graph.edges_from("b").is_empty());
    assert_eq!(ports(&graph, "c"), ["Repo"]);
}

#[specforge_test(
    behavior = "apply_file_defaults",
    verify = "a default reaches no other file and no other kind"
)]
fn a_default_reaches_no_other_file_and_no_other_kind() {
    let (graph, _) = build(&[
        (
            "a.spec",
            "defaults behavior {\n  ports [Repo]\n}\n\nport Repo \"Repo\" {\n}\n",
        ),
        ("b.spec", "behavior b \"B\" {\n}\n"),
    ]);
    assert!(graph.node("b").unwrap().fields.get("ports").is_none());
    assert!(graph.node("Repo").unwrap().fields.get("ports").is_none());
}

#[specforge_test(
    behavior = "apply_file_defaults",
    verify = "an inherited reference is written at the default"
)]
fn an_inherited_reference_is_written_at_the_default() {
    let (graph, _) = build(&[("a.spec", &blocks())]);
    let inputs = GraphInputs {
        field_forms: forms(),
        ..Default::default()
    };
    let tokens = specforge_graph::written_tokens(&parse(&blocks(), "a.spec"), &inputs, |n| {
        graph.node(n).map(|n| n.kind.raw)
    });
    assert_eq!(tokens.len(), 1, "{tokens:?}");
    let token = &tokens[0];
    let span = &token.span;
    assert_eq!(
        (span.start_line, span.start_col, span.end_line, span.end_col),
        (2, 10, 2, 14)
    );
    assert_eq!(token.holder, Holder::Defaults(Sym::new("behavior")));
    assert_eq!(token.held_by, ["a", "b", "c"].map(Sym::new));
    for id in ["a", "b", "c"] {
        let edge = graph.edges_from(id)[0];
        assert_eq!(
            (edge.target, edge.label),
            (Sym::new("Repo"), Sym::new("ports")),
            "{id}"
        );
    }
}

#[specforge_test(
    behavior = "apply_file_defaults",
    verify = "a default for a field its kind does not declare is E077 and is not applied"
)]
fn a_default_for_an_undeclared_field_is_e077() {
    let text = format!("defaults behavior {{\n  owner \"x\"\n}}\n\n{BEHAVIORS}");
    let (graph, diagnostics) = build(&[("a.spec", &text)]);
    assert_eq!(coded(&diagnostics, "E077").len(), 1, "{diagnostics:?}");
    assert!(coded(&diagnostics, "W161").is_empty());
    assert!(graph.node("a").unwrap().fields.get("owner").is_none());

    for kind in ["widget", "ref"] {
        let text = format!("defaults {kind} {{\n  owner \"x\"\n}}\n\n{BEHAVIORS}");
        let (_, diagnostics) = build(&[("a.spec", &text)]);
        assert_eq!(
            coded(&diagnostics, "E077").len(),
            1,
            "{kind}: {diagnostics:?}"
        );
    }
}

#[test]
fn without_declared_kinds_a_default_applies_unchecked() {
    let text = format!("defaults behavior {{\n  owner \"x\"\n}}\n\n{BEHAVIORS}");
    let parsed = [parse(&text, "a.spec")];
    let (graph, diagnostics) = build_graph_with_config(&parsed, &GraphConfig::default());
    assert!(coded(&diagnostics, "E077").is_empty(), "{diagnostics:?}");
    assert!(graph.node("a").unwrap().fields.get("owner").is_some());
}

#[specforge_test(
    behavior = "apply_file_defaults",
    verify = "a malformed defaults block is E078"
)]
fn a_malformed_defaults_block_is_e078() {
    let messages = |text: String| {
        let (graph, diagnostics) = build(&[("a.spec", &text)]);
        (graph, coded(&diagnostics, "E078").len())
    };
    let (graph, n) = messages(format!(
        "defaults behavior \"B\" {{\n  ports [Repo]\n}}\n\n{BEHAVIORS}"
    ));
    assert_eq!(n, 1, "a title");
    assert_eq!(ports(&graph, "a"), ["Repo"]);

    let (_, n) = messages(format!(
        "defaults behavior {{\n  verify unit \"x\"\n}}\n\n{BEHAVIORS}"
    ));
    assert_eq!(n, 1, "a verify member");

    let (graph, n) = messages(format!(
        "defaults behavior {{\n  ports [Repo]\n  ports [Other]\n}}\n\n{BEHAVIORS}"
    ));
    assert_eq!(n, 1, "a key twice");
    assert_eq!(ports(&graph, "a"), ["Repo"], "the first is applied");

    let (graph, n) = messages(format!(
        "{}\n\ndefaults behavior {{\n  ports [Other]\n}}\n",
        blocks()
    ));
    assert_eq!(n, 1, "a second block");
    assert_eq!(ports(&graph, "a"), ["Repo"], "the first wins");

    let (graph, n) = messages(format!(
        "{BEHAVIORS}\ndefaults behavior {{\n  ports [Repo]\n}}\n"
    ));
    assert_eq!(n, 1, "after the first block");
    assert_eq!(ports(&graph, "a"), ["Repo"], "it still applies");
}

#[specforge_test(
    behavior = "apply_file_defaults",
    verify = "a default that changes nothing is W161"
)]
fn a_default_that_changes_nothing_is_w161() {
    let text = "defaults behavior {\n  ports [Repo]\n}\n\nbehavior a \"A\" {\n  ports [Repo]\n}\n\n\
         behavior b \"B\" {\n}\n\nport Repo \"Repo\" {\n}\n";
    let (_, diagnostics) = build(&[("a.spec", text)]);
    let w = coded(&diagnostics, "W161");
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("restates"), "{w:?}");

    let (_, diagnostics) = build(&[("a.spec", "defaults behavior {\n  ports [Repo]\n}\n")]);
    let w = coded(&diagnostics, "W161");
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("reaches no block"), "{w:?}");
}

#[specforge_test(
    behavior = "apply_file_defaults",
    verify = "changing a default changes every block that inherits it"
)]
fn changing_a_default_changes_every_block_that_inherits_it() {
    let config = GraphConfig {
        inputs: GraphInputs {
            field_forms: forms(),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut build = GraphBuild::of([parse(&blocks(), "a.spec")], config);
    build.set_verify(true);
    let changed = blocks().replace("[Repo]", "[Store]");
    let applied = build.apply([FileChange::Parsed(parse(&changed, "a.spec"))]);
    assert_eq!(applied.verification, Some(Ok(())));
    let modified: Vec<_> = applied
        .delta
        .modified_nodes
        .iter()
        .map(|n| (n.id.to_string(), n.changed_fields.clone()))
        .collect();
    assert_eq!(
        modified,
        ["a", "b", "c"].map(|id| (id.to_string(), vec!["ports".to_string()]))
    );
}

const ASKED: &str = "defaults behavior {\n  ports [Repo]\n  ports [Store]\n  owner x\n}\n\n\
     defaults behavior {\n  features [f]\n}\n\n\
     behavior a \"A\" {\n  ports [\"Repo\"]\n}\n\n\
     behavior b \"B\" {\n  ports [Store]\n}\n\n\
     behavior c \"C\" {\n  ports []\n}\n";

#[specforge_test(
    behavior = "apply_file_defaults",
    verify = "the defaults answer which entry applies and whether a field restates it"
)]
fn the_defaults_answer_which_entry_applies_and_whether_a_field_restates_it() {
    let file = parse(ASKED, "a.spec");
    let (defaults, _) = FileDefaults::of(&file, &forms());
    let first = defaults
        .entry("behavior", "ports")
        .expect("the first ports");
    assert_eq!(first.value_span.as_ref().map(|s| s.start_line), Some(2));
    assert!(defaults.entry("behavior", "owner").is_none(), "E077");
    assert!(
        defaults.entry("behavior", "features").is_none(),
        "a second block"
    );
    assert!(defaults.entry("port", "ports").is_none());

    let own = |id: &str| {
        let block = file.entities.iter().find(|e| e.id.raw == id).unwrap();
        block.fields.entries()[0].clone()
    };
    assert!(
        defaults.restates("behavior", &own("a")),
        "quoted, same value"
    );
    assert!(!defaults.restates("behavior", &own("b")));
    assert!(!defaults.restates("behavior", &own("c")), "[] says none");
}

#[test]
fn w161_flags_exactly_what_restates_answers() {
    let file = parse(ASKED, "a.spec");
    let (defaults, _) = FileDefaults::of(&file, &forms());
    let answered: Vec<&str> = file
        .entities
        .iter()
        .filter(|e| e.kind.raw == "behavior")
        .filter(|e| defaults.restates("behavior", &e.fields.entries()[0]))
        .map(|e| e.id.raw.as_str())
        .collect();
    let (_, diagnostics) = build(&[("a.spec", ASKED)]);
    let flagged: Vec<&str> = coded(&diagnostics, "W161")
        .into_iter()
        .filter(|m| m.contains("restates"))
        .map(|m| {
            if m.contains("'a'") {
                "a"
            } else if m.contains("'b'") {
                "b"
            } else {
                "c"
            }
        })
        .collect();
    assert_eq!(answered, ["a"]);
    assert_eq!(flagged, answered);
}
