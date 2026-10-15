//! `FieldForms::reading`: how a (kind, field) reads, the one answer every
//! reader of a field value asks (ADR 0093).

use std::collections::HashMap;

use specforge_graph::{FieldForm, FieldForms, Reading};
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
    FieldForms::new(declared, ["open".to_string()].into_iter().collect())
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
