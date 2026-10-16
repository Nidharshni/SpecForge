//! `FieldForms::reading`: how a (kind, field) reads, the one answer every
//! reader of a field value asks (ADR 0093).

use std::collections::HashMap;

use specforge_graph::{FieldForm, FieldForms, Reading, Target};
use specforge_protocol_types::FieldType;

fn forms() -> FieldForms {
    let declared: HashMap<(String, String), FieldForm> = [
        ("owner", FieldType::Reference),
        ("see_also", FieldType::ReferenceList),
        ("aliases", FieldType::StringList),
    ]
    .into_iter()
    .map(|(field, ty)| {
        (
            ("doc".to_string(), field.to_string()),
            FieldForm::new(ty, vec![]),
        )
    })
    .collect();
    FieldForms::new(
        declared,
        ["open".to_string()].into_iter().collect(),
        Default::default(),
    )
}

#[test]
fn a_declared_field_reads_its_form() {
    let forms = forms();
    let Reading::Declared(form) = forms.reading("doc", "aliases") else {
        panic!("aliases is declared");
    };
    assert_eq!(form.field_type(), FieldType::StringList);
}

#[test]
fn an_open_kinds_undeclared_field_is_a_member() {
    assert_eq!(forms().reading("open", "levels"), Reading::Member);
}

#[test]
fn a_closed_kinds_undeclared_field_is_undeclared() {
    let forms = forms();
    assert_eq!(forms.reading("doc", "unknown"), Reading::Undeclared);
    assert_eq!(forms.reading("unregistered", "x"), Reading::Undeclared);
}

#[test]
fn only_a_declared_reference_is_a_single_reference() {
    let forms = forms();
    assert!(forms.reading("doc", "owner").is_single_reference());
    assert!(!forms.reading("doc", "see_also").is_single_reference());
    assert!(!forms.reading("doc", "aliases").is_single_reference());
    assert!(!forms.reading("open", "owner").is_single_reference());
    assert!(!forms.reading("doc", "unknown").is_single_reference());
}

#[test]
fn references_of_a_reference_list_are_its_items_with_their_spans() {
    use specforge_common::{SourceSpan, Sym};
    use specforge_parser::{FieldEntry, FieldValue, ListItem, Written};

    let span = |col| SourceSpan {
        file: Sym::new("t.spec"),
        start_line: 1,
        start_col: col,
        end_line: 1,
        end_col: col + 1,
    };
    let entry = FieldEntry {
        key: Sym::new("see_also"),
        value: FieldValue::List(vec![
            ListItem {
                text: "a".into(),
                span: span(3),
                written: Written::Bare,
            },
            ListItem {
                text: "b".into(),
                span: span(6),
                written: Written::Quoted,
            },
        ]),
        annotations: vec![],
        value_span: None,
    };
    let forms = forms();
    let references = forms.reading("doc", "see_also").references(&entry);
    let read: Vec<(&str, usize)> = references
        .iter()
        .map(|r| (r.id, r.span.unwrap().start_col))
        .collect();
    assert_eq!(read, [("a", 3), ("b", 6)]);
}

#[test]
fn single_references_are_not_in_cycles() {
    use specforge_graph::{GraphConfig, GraphInputs, build_graph_with_config};

    let single = |kind: &str| {
        (
            (kind.to_string(), "other".to_string()),
            FieldForm::new(FieldType::Reference, vec![]),
        )
    };
    let config = GraphConfig {
        inputs: GraphInputs {
            field_forms: FieldForms::new(
                [single("alpha"), single("beta")].into_iter().collect(),
                Default::default(),
                Default::default(),
            ),
            ..Default::default()
        },
        ..Default::default()
    };
    let source = "alpha a1 \"A\" { other b1 }\nbeta b1 \"B\" { other a1 }\n";
    let (graph, diagnostics) =
        build_graph_with_config(&[specforge_parser::parse(source, "main.spec")], &config);
    assert!(
        diagnostics.iter().all(|d| d.code != "W061"),
        "{diagnostics:?}"
    );
    assert_eq!(graph.edge_count(), 2, "both edges are linked");
}

/// `doc.owner` -> `team` (declared by a loaded extension), `doc.sponsor` -> `org` (declared by none),
/// `doc.note` with no target, and a member kind.
fn targeting_forms() -> FieldForms {
    let declared = [
        ("owner", Some("team")),
        ("sponsor", Some("org")),
        ("note", None),
    ]
    .into_iter()
    .map(|(field, target)| {
        let form = FieldForm::new(FieldType::Reference, vec![]);
        let form = match target {
            Some(kind) => form.targeting(kind),
            None => form,
        };
        (("doc".to_string(), field.to_string()), form)
    })
    .collect();
    FieldForms::new(
        declared,
        ["open".to_string()].into_iter().collect(),
        ["doc".to_string(), "team".to_string()]
            .into_iter()
            .collect(),
    )
}

#[test]
fn a_declared_target_kind_is_loaded_when_a_loaded_extension_declares_it() {
    assert_eq!(
        targeting_forms().target("doc", "owner"),
        Some(Target::Loaded("team"))
    );
}

#[test]
fn a_target_kind_no_loaded_extension_declares_is_absent() {
    let forms = targeting_forms();
    assert_eq!(forms.target("doc", "sponsor"), Some(Target::Absent("org")));
    assert_eq!(
        forms.targets(),
        [
            (("doc", "owner"), Target::Loaded("team")),
            (("doc", "sponsor"), Target::Absent("org")),
        ]
    );
}

#[test]
fn a_member_or_undeclared_field_has_no_target() {
    let forms = targeting_forms();
    assert_eq!(forms.target("doc", "note"), None, "no target kind declared");
    assert_eq!(forms.target("open", "anything"), None, "a member");
    assert_eq!(forms.target("doc", "missing"), None, "undeclared");
    assert_eq!(FieldForms::default().target("doc", "owner"), None);
}

#[test]
fn a_target_admits_only_its_own_kind() {
    assert!(Target::Loaded("team").admits("team"));
    assert!(Target::Absent("org").admits("org"));
    assert!(!Target::Loaded("team").admits("doc"));
    assert_eq!(Target::Absent("org").kind(), "org");
}
