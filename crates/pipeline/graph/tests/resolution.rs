//! What one written reference is (ADR 0117): an edge, E003 or I004, decided in the linker against its
//! field's target kind (behavior `link_entity_references`).

use specforge_graph::{
    FieldForm, FieldForms, Graph, GraphConfig, GraphInputs, build_graph_with_config,
};
use specforge_parser::parse;
use specforge_protocol_types::FieldType;
use specforge_test_macros::test as specforge_test;

/// `behavior.features` names `feature`s; `behavior.peers` names `behavior`s; `behavior.note` names nothing in
/// particular. `loaded` are the kinds a loaded extension declares; `owners` are the absent kinds' providers.
fn build(
    source: &str,
    loaded: &[&str],
    owners: &[(&str, &str)],
) -> (Graph, Vec<specforge_graph::Diagnostic>) {
    let form = |field: &str, target: Option<&str>| {
        let form = FieldForm::new(FieldType::ReferenceList, Vec::new());
        (
            ("behavior".to_string(), field.to_string()),
            match target {
                Some(kind) => form.targeting(kind),
                None => form,
            },
        )
    };
    let config = GraphConfig {
        inputs: GraphInputs {
            field_forms: FieldForms::new(
                [
                    form("features", Some("feature")),
                    form("peers", Some("behavior")),
                    form("note", None),
                ]
                .into_iter()
                .collect(),
                Default::default(),
                loaded.iter().map(|k| k.to_string()).collect(),
            ),
            target_owners: owners
                .iter()
                .map(|(kind, owner)| (kind.to_string(), owner.to_string()))
                .collect(),
            ..Default::default()
        },
        ..Default::default()
    };
    build_graph_with_config(&[parse(source, "main.spec")], &config)
}

fn codes(diagnostics: &[specforge_graph::Diagnostic]) -> Vec<&str> {
    diagnostics.iter().map(|d| d.code.as_str()).collect()
}

const MISSING: &str = "behavior b1 \"B1\" { features [f_missing] }\n";

#[specforge_test(
    behavior = "link_entity_references",
    verify = "an unresolved reference in a field whose target kind no extension declares is I004 naming the extension"
)]
fn a_missing_id_of_an_absent_kind_is_i004_naming_its_provider() {
    let (graph, diagnostics) = build(MISSING, &["behavior"], &[("feature", "@specforge/product")]);
    assert_eq!(codes(&diagnostics), ["I004"], "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].suggestion.as_deref(),
        Some(
            "enable the extension that declares 'feature' with `specforge add @specforge/product`, or drop the field"
        )
    );
    assert_eq!(
        diagnostics[0].message,
        "reference 'f_missing' in field 'features' of 'b1' targets kind 'feature', which no enabled extension provides"
    );
    assert_eq!(graph.edge_count(), 0);
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "an unresolved reference in a field whose target kind no extension declares is I004 naming the extension"
)]
fn a_missing_id_of_an_absent_kind_with_no_provider_suggests_a_search() {
    let (_, diagnostics) = build(MISSING, &["behavior"], &[]);
    assert_eq!(codes(&diagnostics), ["I004"], "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].suggestion.as_deref(),
        Some(
            "find an extension that declares 'feature' with `specforge search feature`, or drop the field"
        )
    );
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "an unresolved reference to a loaded or undeclared target kind is E003 alone"
)]
fn a_missing_id_is_e003_alone() {
    // `feature` is loaded: the field's target exists, so the id is simply missing.
    let (_, diagnostics) = build(MISSING, &["behavior", "feature"], &[]);
    assert_eq!(codes(&diagnostics), ["E003"], "{diagnostics:?}");
    // A field with no target kind: E003 as well.
    let (_, diagnostics) = build("behavior b1 \"B1\" { note [ghost] }\n", &["behavior"], &[]);
    assert_eq!(codes(&diagnostics), ["E003"], "{diagnostics:?}");
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "a reference naming an existing entity of the absent target kind links without a diagnostic"
)]
fn an_id_of_the_absent_kind_links_silently() {
    let (graph, diagnostics) = build(
        "behavior b1 \"B1\" { features [f1] }\nfeature f1 \"F1\" { }\n",
        &["behavior"],
        &[("feature", "@specforge/product")],
    );
    assert_eq!(codes(&diagnostics), Vec::<&str>::new(), "{diagnostics:?}");
    assert_eq!(graph.edge_count(), 1);
}
