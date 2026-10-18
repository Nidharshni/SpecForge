use specforge_common::{SourceSpan, Sym};
use specforge_emitter::{EmitFormat, EmitOptions, document_schema, publish};
use specforge_graph::{Edge, FieldValue, Graph, Node};
use specforge_parser::{ListItem, VerifyStatement, Written};
use specforge_test_macros::test as specforge_test;

fn span(line: usize) -> SourceSpan {
    SourceSpan {
        file: Sym::new("test.spec"),
        start_line: line,
        start_col: 0,
        end_line: line,
        end_col: 0,
    }
}

fn node(id: &str, kind: &str, title: Option<&str>) -> Node {
    let mut node = Node::new(id, kind, span(3));
    node.title = title.map(str::to_string);
    node
}

fn string(node: &mut Node, key: &str, text: &str) {
    node.fields
        .push(Sym::new(key), FieldValue::String(text.into()));
}

fn edge(source: &str, target: &str, label: &str) -> Edge {
    Edge {
        source: Sym::new(source),
        target: Sym::new(target),
        label: Sym::new(label),
    }
}

fn item(text: &str, written: Written) -> ListItem {
    ListItem {
        text: text.into(),
        span: span(1),
        written,
    }
}

fn graph_of(nodes: Vec<Node>, edges: Vec<Edge>) -> Graph {
    let mut graph = Graph::new();
    for node in nodes {
        graph.add_node(node);
    }
    for edge in edges {
        graph.add_edge(edge);
    }
    graph
}

fn markdown(graph: &Graph, options: EmitOptions<'_>) -> String {
    specforge_emitter::emit(
        graph,
        &EmitOptions {
            format: EmitFormat::Markdown,
            ..options
        },
    )
    .expect("a markdown export of a scope that exists cannot fail")
}

fn plain(graph: &Graph) -> String {
    markdown(graph, EmitOptions::default())
}

#[specforge_test(
    behavior = "export_review_document",
    verify = "a section is headed by its kind, id and title"
)]
fn a_section_is_headed_by_kind_id_and_title() {
    let graph = graph_of(
        vec![
            node("alpha", "behavior", Some("Alpha Behavior")),
            node("Bare", "type", None),
            node("pipes", "type", Some("A|B <c>")),
        ],
        vec![],
    );
    let doc = plain(&graph);
    assert!(
        doc.contains("## behavior `alpha` — Alpha Behavior\n"),
        "{doc}"
    );
    assert!(doc.contains("## type `Bare`\n"), "{doc}");
    assert!(doc.contains("## type `pipes` — A\\|B \\<c>\n"), "{doc}");
    assert!(doc.contains("`test.spec:3`"), "{doc}");
}

#[specforge_test(
    behavior = "export_review_document",
    verify = "a normative field is quoted whole"
)]
fn a_normative_field_is_quoted_whole() {
    let mut alpha = node("alpha", "behavior", None);
    string(&mut alpha, "contract", "line one\nline two");
    string(&mut alpha, "description", "first\nsecond");
    let graph = graph_of(vec![alpha], vec![]);
    let registry = crate::support::headline_registry(&["behavior"]);

    let doc = markdown(
        &graph,
        EmitOptions {
            field_registry: Some(&registry),
            ..Default::default()
        },
    );
    assert!(
        doc.contains("**contract**\n\n> line one\n> line two"),
        "{doc}"
    );
    assert!(doc.contains("**description**\n\nfirst\nsecond"), "{doc}");
    assert!(!doc.contains("> first"), "{doc}");

    // Without a registry nothing is quoted: the string is a paragraph by shape.
    let bare = plain(&graph);
    assert!(!bare.contains("> line one"), "{bare}");
    assert!(
        bare.contains("**contract**\n\nline one\nline two"),
        "{bare}"
    );
}

#[specforge_test(
    behavior = "export_review_document",
    verify = "links name each linked entity's kind and title, outside the selection too"
)]
fn links_name_kind_and_title_outside_the_selection() {
    let mut a = node("a", "behavior", Some("A"));
    a.fields.push(
        Sym::new("types"),
        FieldValue::List(vec![item("t", Written::Bare)]),
    );
    let graph = graph_of(
        vec![a, node("t", "type", Some("T title"))],
        vec![edge("a", "t", "types")],
    );
    let doc = markdown(
        &graph,
        EmitOptions {
            scope: Some("a"),
            depth: Some(0),
            ..Default::default()
        },
    );
    assert_eq!(doc.matches("\n## ").count(), 1, "{doc}");
    assert!(doc.contains("| types | `t` | type | T title |"), "{doc}");
}

#[specforge_test(
    behavior = "export_review_document",
    verify = "a reference that does not resolve is listed as unresolved"
)]
fn a_reference_that_does_not_resolve_is_listed() {
    let project = specforge_project::testing::TestProject::new()
        .config(|c| c["spec_root"] = "spec".into())
        .file(
            "spec/main.spec",
            "behavior a \"A\" {\n  category command\n  invariants [ghost]\n}\n",
        )
        .enabling(&["@specforge/software"]);
    let ctx = project.compile();
    let registries = ctx.environment().registries();
    let doc = markdown(
        ctx.graph(),
        EmitOptions {
            field_registry: Some(registries.fields()),
            field_forms: Some(registries.field_forms()),
            ..Default::default()
        },
    );
    assert!(
        doc.contains("| invariants | `ghost` | — | *unresolved* |"),
        "{doc}"
    );
    assert!(!doc.contains("| invariants | ghost |"), "{doc}");
    assert!(doc.contains("| category | command |"), "{doc}");
}

#[specforge_test(
    behavior = "export_review_document",
    verify = "the entities that reference it are listed"
)]
fn the_entities_that_reference_it_are_listed() {
    let graph = graph_of(
        vec![
            node("f", "feature", Some("F")),
            node("b2", "behavior", Some("B two")),
            node("b1", "behavior", Some("B one")),
            node("p", "persona", Some("P")),
        ],
        vec![
            edge("p", "f", "key_features"),
            edge("b2", "f", "features"),
            edge("b1", "f", "features"),
        ],
    );
    let doc = plain(&graph);
    let section = doc
        .split("\n## ")
        .find(|s| s.starts_with("feature `f`"))
        .unwrap();
    let rows: Vec<&str> = section
        .lines()
        .filter(|l| l.starts_with("| features") || l.starts_with("| key_features"))
        .collect();
    assert_eq!(
        rows,
        [
            "| features | `b1` | behavior | B one |",
            "| features | `b2` | behavior | B two |",
            "| key_features | `p` | persona | P |",
        ]
    );
}

#[specforge_test(
    behavior = "export_review_document",
    verify = "obligations are numbered in declaration order"
)]
fn obligations_are_numbered_in_declaration_order() {
    let mut alpha = node("alpha", "behavior", None);
    alpha.obligations = vec![
        VerifyStatement {
            kind: "unit".into(),
            description: "x".into(),
        },
        VerifyStatement {
            kind: String::new(),
            description: "y".into(),
        },
    ];
    let doc = plain(&graph_of(vec![alpha], vec![]));
    assert!(
        doc.contains("| # | Kind | Obligation |\n|---|---|---|\n| 1 | unit | x |\n| 2 | — | y |"),
        "{doc}"
    );
}

#[specforge_test(
    behavior = "export_review_document",
    verify = "sections are ordered scope first, then by kind and id"
)]
fn sections_are_ordered_scope_first_then_kind_and_id() {
    let graph = graph_of(
        vec![
            node("z", "behavior", None),
            node("a", "type", None),
            node("m", "behavior", None),
        ],
        vec![edge("m", "a", "types"), edge("z", "a", "types")],
    );
    let doc = markdown(
        &graph,
        EmitOptions {
            scope: Some("a"),
            depth: Some(1),
            ..Default::default()
        },
    );
    let headings: Vec<&str> = doc.lines().filter(|l| l.starts_with("## ")).collect();
    assert_eq!(
        headings,
        ["## type `a`", "## behavior `m`", "## behavior `z`"]
    );
}

#[test]
fn derived_edges_follow_the_written_references() {
    let mut a = node("a", "behavior", None);
    a.fields.push(
        Sym::new("types"),
        FieldValue::List(vec![item("x", Written::Bare)]),
    );
    let graph = graph_of(
        vec![a, node("x", "type", None), node("y", "type", None)],
        vec![
            edge("a", "x", "types"),
            edge("a", "y", "types"),
            edge("a", "y", "composed_types"),
        ],
    );
    let doc = plain(&graph);
    let section = doc
        .split("\n## ")
        .find(|s| s.starts_with("behavior `a`"))
        .unwrap();
    let rows: Vec<&str> = section
        .lines()
        .filter(|l| l.starts_with("| types") || l.starts_with("| composed"))
        .collect();
    assert_eq!(
        rows,
        [
            "| types | `x` | type | — |",
            "| types | `y` | type | — |",
            "| composed_types | `y` | type | — |",
        ]
    );
}

#[test]
fn fields_prose_and_blocks_render_by_shape() {
    let mut a = node("a", "behavior", None);
    a.fields.push(
        Sym::new("acceptance"),
        FieldValue::List(vec![item("a", Written::Quoted), item("b", Written::Quoted)]),
    );
    let mut block = specforge_graph::FieldMap::new();
    block.push(Sym::new("k"), FieldValue::String("v".into()));
    a.fields
        .push(Sym::new("requires"), FieldValue::Block(block));
    string(&mut a, "items", "string[]");
    string(&mut a, "variants", "open | done");
    let doc = plain(&graph_of(vec![a], vec![]));
    assert!(doc.contains("**acceptance**\n\n- a\n- b"), "{doc}");
    assert!(doc.contains("**requires**\n\n- k: v"), "{doc}");
    assert!(doc.contains("| items | string[] |"), "{doc}");
    assert!(doc.contains("| variants | open \\| done |"), "{doc}");
}

#[test]
fn the_header_states_the_selection() {
    let graph = graph_of(
        vec![node("a", "type", None), node("b", "behavior", None)],
        vec![edge("b", "a", "types")],
    );
    let header = |options: EmitOptions<'_>| {
        markdown(&graph, options)
            .lines()
            .nth(2)
            .unwrap()
            .to_string()
    };
    assert_eq!(header(EmitOptions::default()), "2 entities.");
    assert_eq!(
        header(EmitOptions {
            scope: Some("a"),
            depth: Some(1),
            ..Default::default()
        }),
        "2 entities around `a` (depth 1)."
    );
    assert_eq!(
        header(EmitOptions {
            kind_filter: vec!["type", "behavior", "type"],
            ..Default::default()
        }),
        "2 entities, of kinds behavior, type."
    );
    assert_eq!(
        header(EmitOptions {
            kind_filter: vec!["type"],
            ..Default::default()
        }),
        "1 entity, of kind type."
    );
    assert_eq!(plain(&Graph::new()).lines().nth(2).unwrap(), "0 entities.");
}

#[test]
fn the_review_document_is_deterministic() {
    let nodes = || {
        vec![
            node("b", "behavior", Some("B")),
            node("a", "type", Some("A")),
            node("c", "behavior", Some("C")),
        ]
    };
    let edges = || vec![edge("b", "a", "types"), edge("c", "a", "types")];
    let forward = graph_of(nodes(), edges());
    let (mut n, mut e) = (nodes(), edges());
    n.reverse();
    e.reverse();
    assert_eq!(plain(&forward), plain(&graph_of(n, e)));
}

#[test]
fn markdown_has_no_document_schema() {
    let mut defs = specforge_common::shape::Definitions::default();
    assert!(document_schema(EmitFormat::Markdown, &mut defs).is_none());
    let schema = specforge_emitter::GraphProtocolSchema::empty();
    assert!(publish(&schema, EmitFormat::Markdown).is_none());
}
