//! Derived reference fields: edges the host fills from type names an entity
//! writes in its field types or method signatures (behavior
//! `link_derived_references`).

use specforge_graph::{
    DerivedFrom, DerivedReference, Graph, GraphConfig, GraphInputs, build_graph_with_config,
};
use specforge_parser::parse;
use specforge_test_macros::test as specforge_test;

/// The kinds and fields these tests declare, standing in for an extension:
/// `shape` derives `parts` (-> shape) from its field types and `iface`
/// derives `uses` (-> shape) from its method signatures.
pub(crate) fn derived() -> Vec<DerivedReference> {
    vec![
        DerivedReference {
            source_kind: "shape".to_string(),
            field: "parts".to_string(),
            from: DerivedFrom::TypeExpressions,
        },
        DerivedReference {
            source_kind: "iface".to_string(),
            field: "uses".to_string(),
            from: DerivedFrom::MethodSignatures,
        },
    ]
}

/// The graph inputs these tests build with.
pub(crate) fn inputs(derived: Vec<DerivedReference>) -> GraphInputs {
    GraphInputs {
        derived_references: derived,
        field_forms: specforge_graph::FieldForms::new(
            [
                (
                    ("shape".to_string(), "base".to_string()),
                    specforge_graph::FieldForm::new(
                        specforge_protocol_types::FieldType::Reference,
                        Vec::new(),
                    ),
                ),
                (
                    ("shape".to_string(), "parts".to_string()),
                    specforge_graph::FieldForm::new(
                        specforge_protocol_types::FieldType::ReferenceList,
                        Vec::new(),
                    )
                    .targeting("shape"),
                ),
                (
                    ("shape".to_string(), "tags".to_string()),
                    specforge_graph::FieldForm::new(
                        specforge_protocol_types::FieldType::StringList,
                        Vec::new(),
                    ),
                ),
                (
                    ("iface".to_string(), "uses".to_string()),
                    specforge_graph::FieldForm::new(
                        specforge_protocol_types::FieldType::ReferenceList,
                        Vec::new(),
                    )
                    .targeting("shape"),
                ),
            ]
            .into_iter()
            .collect(),
            Default::default(),
            Default::default(),
        ),
        ..Default::default()
    }
}

pub(crate) fn build(
    source: &str,
    derived: Vec<DerivedReference>,
) -> (Graph, Vec<specforge_graph::Diagnostic>) {
    let config = GraphConfig {
        inputs: inputs(derived),
        ..Default::default()
    };
    build_graph_with_config(&[parse(source, "main.spec")], &config)
}

/// The (target, label) of every edge out of `id`, sorted.
fn edges_from(graph: &Graph, id: &str) -> Vec<(String, String)> {
    let mut edges: Vec<(String, String)> = graph
        .edges_from(id)
        .iter()
        .map(|e| (e.target.to_string(), e.label.to_string()))
        .collect();
    edges.sort();
    edges
}

fn edge(target: &str, label: &str) -> (String, String) {
    (target.to_string(), label.to_string())
}

#[specforge_test(
    behavior = "link_derived_references",
    verify = "a name in a type-syntax field value links through the derived field"
)]
fn a_field_type_name_links_through_the_derived_field() {
    let source = r#"
shape Wheel "Wheel" {
  size number
}
shape Axle "Axle" {
  ends Wheel
}
shape Car "Car" {
  wheels    Wheel[]
  front     Axle | Wheel
  nested    Option<Axle>[]
}
"#;
    let (graph, diagnostics) = build(source, derived());

    assert_eq!(
        edges_from(&graph, "Car"),
        vec![edge("Axle", "parts"), edge("Wheel", "parts")]
    );
    assert_eq!(edges_from(&graph, "Axle"), vec![edge("Wheel", "parts")]);
    assert!(edges_from(&graph, "Wheel").is_empty());
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[specforge_test(
    behavior = "link_derived_references",
    verify = "a name in a method parameter or return type links through the derived field"
)]
fn a_method_signature_name_links_through_the_derived_field() {
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
    let (graph, _) = build(source, derived());

    assert_eq!(
        edges_from(&graph, "Detector"),
        vec![
            edge("Config", "uses"),
            edge("Project", "uses"),
            edge("Report", "uses"),
        ]
    );
}

#[specforge_test(
    behavior = "link_derived_references",
    verify = "a primitive, a generic wrapper or an undeclared name creates no edge"
)]
fn primitives_wrappers_and_undeclared_names_create_no_edge() {
    let source = r#"
shape Thing "Thing" {
  name    string
  count   Option<number>[]
  list    Vec<Missing>[]
}
iface Store "Store" {
  method load(id: string) -> Result<Option<Unknown>, Error>
}
"#;
    let (graph, diagnostics) = build(source, derived());

    assert!(graph.edges().is_empty(), "{:?}", graph.edges());
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[specforge_test(
    behavior = "link_derived_references",
    verify = "a name that resolves to an entity of another kind creates no edge"
)]
fn a_name_of_another_kind_creates_no_edge() {
    let source = r#"
iface Other "Other" {
  method ping() -> string
}
shape Holder "Holder" {
  peer Other
}
iface Caller "Caller" {
  method call(other: Other) -> Holder
}
"#;
    let (graph, _) = build(source, derived());

    assert!(edges_from(&graph, "Holder").is_empty());
    assert_eq!(edges_from(&graph, "Caller"), vec![edge("Holder", "uses")]);
}

#[specforge_test(
    behavior = "link_derived_references",
    verify = "quoted strings and single-reference fields derive no edge"
)]
fn quoted_strings_and_single_reference_fields_derive_no_edge() {
    let source = r#"
shape Base "Base" {
  id string
}
shape Note "Note" {
  text "mentions Base by name"
  base Base
}
"#;
    let (graph, _) = build(source, derived());

    // `base` is a single-reference field: it links as itself, not as `parts`.
    assert_eq!(edges_from(&graph, "Note"), vec![edge("Base", "base")]);
}

#[specforge_test(
    behavior = "link_derived_references",
    verify = "an entity naming itself creates no edge"
)]
fn an_entity_naming_itself_creates_no_edge() {
    let source = r#"
shape Tree "Tree" {
  children Tree[]
}
"#;
    let (graph, _) = build(source, derived());

    assert!(graph.edges().is_empty(), "{:?}", graph.edges());
}

#[specforge_test(
    behavior = "link_derived_references",
    verify = "a recursive type derives no reference cycle"
)]
fn a_recursive_type_derives_no_reference_cycle() {
    let source = r#"
shape Expr "Expr" {
  operands Term[]
}
shape Term "Term" {
  inner Expr
}
"#;
    let (graph, diagnostics) = build(source, derived());

    assert_eq!(edges_from(&graph, "Expr"), vec![edge("Term", "parts")]);
    assert_eq!(edges_from(&graph, "Term"), vec![edge("Expr", "parts")]);
    let w061: Vec<_> = diagnostics.iter().filter(|d| d.code == "W061").collect();
    assert!(w061.is_empty(), "{w061:?}");
}

#[specforge_test(
    behavior = "link_derived_references",
    verify = "a kind with no derived field derives no edge"
)]
fn a_kind_with_no_derived_field_derives_no_edge() {
    let source = r#"
shape Part "Part" {
  id string
}
shape Whole "Whole" {
  part Part
}
iface Port "Port" {
  method get() -> Part
}
"#;
    let (graph, _) = build(source, Vec::new());

    assert!(graph.edges().is_empty(), "{:?}", graph.edges());
}

#[specforge_test(
    behavior = "link_derived_references",
    verify = "a quoted union member derives no edge"
)]
fn a_quoted_union_variant_derives_no_edge() {
    let source = "shape Base {\n  id string\n}\nshape Wheel {\n  size number\n}\nshape Level = \"Base\" | Wheel\n";
    let (graph, _) = build(source, derived());

    assert_eq!(edges_from(&graph, "Level"), vec![edge("Wheel", "parts")]);
}
