//! The linker and the finder read every block alike (ADR 0163, round 18 amendment): each edge is a token its source
//! holds, and each token a placed block holds that names an entity is that block's edge.

use specforge_graph::{
    DerivedFrom, DerivedReference, FieldForm, FieldForms, GraphConfig, GraphInputs, WrittenToken,
    build_graph_with_config, written_tokens,
};
use specforge_parser::parse;
use specforge_protocol_types::FieldType;
use specforge_test_macros::test as specforge_test;

/// `shape.parts` (reference list, derived from type expressions), `shape.tags` (string list), `shape.base`,
/// `iface.uses` (derived from method signatures) and `behavior.ports` (-> port), every kind declared so defaults
/// are checked.
fn inputs() -> GraphInputs {
    let list = |t: FieldType, target: Option<&str>| {
        let form = FieldForm::new(t, Vec::new());
        target.map_or(form.clone(), |k| form.targeting(k))
    };
    let form = |kind: &str, field: &str, form| ((kind.to_string(), field.to_string()), form);
    GraphInputs {
        derived_references: vec![
            DerivedReference {
                source_kind: "shape".into(),
                field: "parts".into(),
                from: DerivedFrom::TypeExpressions,
            },
            DerivedReference {
                source_kind: "iface".into(),
                field: "uses".into(),
                from: DerivedFrom::MethodSignatures,
            },
        ],
        field_forms: FieldForms::new(
            [
                form("shape", "base", list(FieldType::Reference, Some("shape"))),
                form(
                    "shape",
                    "parts",
                    list(FieldType::ReferenceList, Some("shape")),
                ),
                form("shape", "tags", list(FieldType::StringList, None)),
                form(
                    "iface",
                    "uses",
                    list(FieldType::ReferenceList, Some("shape")),
                ),
                form(
                    "behavior",
                    "ports",
                    list(FieldType::ReferenceList, Some("port")),
                ),
            ]
            .into_iter()
            .collect(),
            Default::default(),
            ["shape", "iface", "behavior", "port", "ref"]
                .into_iter()
                .map(String::from)
                .collect(),
        ),
        ..Default::default()
    }
}

/// Assert the two walks agree over `source`. A duplicate declaration's second block is a token with no edge of its
/// own, so `placed` is false there and only the edge -> token direction holds.
fn assert_agree(name: &str, source: &str, placed: bool) {
    let file = parse(source, "main.spec");
    let config = GraphConfig {
        inputs: inputs(),
        ..Default::default()
    };
    let (graph, _) = build_graph_with_config(std::slice::from_ref(&file), &config);
    let tokens: Vec<WrittenToken> =
        written_tokens(&file, &config.inputs, |n| graph.node(n).map(|n| n.kind.raw));
    for edge in graph.edges() {
        assert!(
            tokens.iter().any(|t| t.target == edge.target
                && t.field == edge.label
                && t.held_by.contains(&edge.source)),
            "{name}: edge {edge:?} is held by no token: {tokens:?}"
        );
    }
    if !placed {
        return;
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
                "{name}: {token:?} held by {holder} has no edge"
            );
        }
    }
}

const WHEEL: &str = "shape Wheel \"Wheel\" {\n  size number\n}\n\n";
const REPO: &str = "port Repo \"Repo\" {\n}\n";

#[specforge_test(
    behavior = "link_derived_references",
    verify = "the linker and the finder read every block alike"
)]
fn the_linker_and_the_finder_agree_on_every_case() {
    let with_wheel = |body: &str| format!("{WHEEL}{body}");
    let rows: Vec<(&str, String, bool)> = vec![
        (
            "unions, generics, a quoted member, a signature",
            with_wheel(
                "shape Axle \"Axle\" {\n  ends Wheel\n}\n\nshape Car \"Car\" {\n  wheels Wheel[]\n  front  Axle | Wheel\n  \
                 nested Option<Axle>[]\n  quoted \"Wheel\" | Axle\n}\n\niface Detector \"Detector\" {\n  \
                 method detect(Wheel: Axle) -> Result<Wheel, Error>\n}\n",
            ),
            true,
        ),
        (
            "a reference list written as one value",
            with_wheel("shape Car \"Car\" {\n  parts Wheel\n}\n"),
            true,
        ),
        (
            "a string list word",
            with_wheel("shape Car \"Car\" {\n  tags Wheel\n}\n"),
            true,
        ),
        (
            "a default taken by several blocks",
            format!(
                "defaults behavior {{\n  ports [Repo]\n}}\n\nbehavior a \"A\" {{\n}}\n\nbehavior b \"B\" {{\n}}\n\n{REPO}"
            ),
            true,
        ),
        (
            "a default no block takes",
            format!("defaults behavior {{\n  ports [Repo]\n}}\n\n{REPO}"),
            true,
        ),
        (
            "a default a block overrides",
            format!(
                "defaults behavior {{\n  ports [Repo]\n}}\n\nbehavior a \"A\" {{\n}}\n\nbehavior c \"C\" {{\n  ports [Other]\n}}\n\n\
                 {REPO}\nport Other \"Other\" {{\n}}\n"
            ),
            true,
        ),
        (
            "a string-list word in a default",
            with_wheel("defaults shape {\n  tags Wheel\n}\n\nshape Car \"Car\" {\n}\n"),
            true,
        ),
        (
            "a derived name in a default",
            with_wheel("defaults shape {\n  slot Wheel\n}\n\nshape Car \"Car\" {\n}\n"),
            true,
        ),
        (
            "an E002 loser",
            with_wheel(
                "shape Car \"Car\" {\n  parts [Wheel]\n}\n\nshape Car \"Again\" {\n  parts [Axle]\n}\n\nshape Axle \"Axle\" {\n}\n",
            ),
            false,
        ),
        (
            "a W060 loser",
            with_wheel(
                "shape Car \"Car\" {\n  parts [Wheel]\n}\n\niface Car \"Car\" {\n  method m() -> Wheel\n}\n",
            ),
            false,
        ),
        (
            "a mistyped reference (E022)",
            with_wheel("behavior b \"B\" {\n  ports [Wheel]\n}\n"),
            true,
        ),
        (
            "an unresolved reference (E003)",
            "shape Car \"Car\" {\n  parts [Ghost]\n}\n".to_string(),
            true,
        ),
        (
            "the block's own name",
            "shape Car \"Car\" {\n  parts [Car]\n  me Car[]\n}\n".to_string(),
            true,
        ),
        (
            "a scheme ref in a string list",
            "ref gh.issue:1 \"I\"\n\nshape Car \"Car\" {\n  tags [gh.issue:1]\n}\n".to_string(),
            true,
        ),
    ];
    for (name, source, placed) in rows {
        assert_agree(name, &source, placed);
    }
}
