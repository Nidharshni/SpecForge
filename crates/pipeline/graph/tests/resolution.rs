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

const WRONG_KIND: &str = "behavior b1 \"B1\" {\n  features [b2]\n}\nbehavior b2 \"B2\" {\n}\n";

#[specforge_test(
    behavior = "link_entity_references",
    verify = "a reference to an existing entity of another kind than its field's target kind is E022 at the reference"
)]
fn a_reference_to_another_kind_is_e022_at_the_reference_and_links() {
    let (graph, diagnostics) = build(WRONG_KIND, &["behavior", "feature"], &[]);
    assert_eq!(codes(&diagnostics), ["E022"], "{diagnostics:?}");
    let span = diagnostics[0].span.as_ref().unwrap();
    assert_eq!(
        (span.start_line, span.start_col, span.end_col),
        (2, 13, 15),
        "the token"
    );
    assert_eq!(
        graph.edge_count(),
        1,
        "it still links, so navigation reaches it"
    );
    assert_eq!(
        diagnostics[0].message,
        "reference 'b2' in field 'features' of behavior 'b1' targets a behavior, but this field expects feature"
    );
    assert_eq!(
        diagnostics[0].suggestion.as_deref(),
        Some("name a feature here, or drop the reference")
    );
}

#[test]
fn a_loaded_target_kind_suggests_a_close_id_of_that_kind() {
    let (_, diagnostics) = build(
        "behavior b1 \"B1\" {\n  features [f_one]\n}\nbehavior f_one \"B\" {\n}\nfeature f_on \"F\" {\n}\n",
        &["behavior", "feature"],
        &[],
    );
    assert_eq!(codes(&diagnostics), ["E022"], "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].suggestion.as_deref(),
        Some("did you mean 'f_on'?")
    );
}

#[test]
fn a_reference_to_its_target_kind_links_silently() {
    let (graph, diagnostics) = build(
        "behavior b1 \"B1\" { peers [b2] }\nbehavior b2 \"B2\" { }\n",
        &["behavior"],
        &[],
    );
    assert_eq!(codes(&diagnostics), Vec::<&str>::new(), "{diagnostics:?}");
    assert_eq!(graph.edge_count(), 1);
}

#[test]
fn a_field_without_a_target_kind_links_any_kind() {
    let (graph, diagnostics) = build(
        "behavior b1 \"B1\" { note [f1] }\nfeature f1 \"F\" { }\n",
        &["behavior", "feature"],
        &[],
    );
    assert_eq!(codes(&diagnostics), Vec::<&str>::new(), "{diagnostics:?}");
    assert_eq!(graph.edge_count(), 1);
}

#[test]
fn an_undeclared_kinds_field_has_no_target() {
    let (graph, diagnostics) = build(
        "gadget g1 \"G\" { features [b2] }\nbehavior b2 \"B2\" { }\n",
        &["behavior", "feature"],
        &[],
    );
    assert_eq!(codes(&diagnostics), Vec::<&str>::new(), "{diagnostics:?}");
    assert_eq!(graph.edge_count(), 1);
}

#[test]
fn each_wrong_kind_reference_is_its_own_e022_in_written_order() {
    let (_, diagnostics) = build(
        "behavior b1 \"B1\" { features [b3, b2] }\nbehavior b2 \"B2\" { }\nbehavior b3 \"B3\" { }\n",
        &["behavior", "feature"],
        &[],
    );
    assert_eq!(codes(&diagnostics), ["E022", "E022"], "{diagnostics:?}");
    assert!(diagnostics[0].message.contains("'b3'"));
    assert!(diagnostics[1].message.contains("'b2'"));
}

#[test]
fn e022_names_the_target_the_field_the_kinds_and_the_source() {
    let (_, diagnostics) = build(WRONG_KIND, &["behavior", "feature"], &[]);
    let message = &diagnostics[0].message;
    for word in ["b2", "features", "behavior", "feature", "b1"] {
        assert!(message.contains(word), "{word}: {message}");
    }
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "a reference to an existing entity of another kind than its field's target kind is E022 at the reference"
)]
fn a_wrong_kind_id_of_an_absent_kind_is_e022_naming_its_provider() {
    let (graph, diagnostics) = build(
        WRONG_KIND,
        &["behavior"],
        &[("feature", "@specforge/product")],
    );
    assert_eq!(codes(&diagnostics), ["E022"], "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].suggestion.as_deref(),
        Some(
            "name a feature here (enable the extension that declares it with `specforge add @specforge/product`), or drop the reference"
        )
    );
    assert_eq!(graph.edge_count(), 1);

    let (_, diagnostics) = build(WRONG_KIND, &["behavior"], &[]);
    assert_eq!(
        diagnostics[0].suggestion.as_deref(),
        Some(
            "name a feature here (find an extension that declares it with `specforge search feature`), or drop the reference"
        )
    );
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "a quoted reference is written at its ID, inside its quotes"
)]
fn a_quoted_reference_is_e003_and_e022_at_its_id() {
    let source = "behavior b1 \"B1\" {\n  features [\"ghost\"]\n  peers \"ghost\"\n}\n\n\
                  behavior b2 \"B2\" {\n  features [\"b1\"]\n}\n";
    let (_, diagnostics) = build(source, &["behavior", "feature"], &[]);
    let mut at: Vec<String> = diagnostics
        .iter()
        .filter_map(|d| {
            let s = d.span.as_ref()?;
            Some(format!(
                "{} {}:{}-{}",
                d.code, s.start_line, s.start_col, s.end_col
            ))
        })
        .collect();
    at.sort();
    // A list item, a single value coerced to a list, and a mistyped id (b1 is a behavior).
    assert_eq!(
        at,
        ["E003 2:14-19", "E003 3:10-15", "E022 7:14-16"],
        "{diagnostics:?}"
    );
}
