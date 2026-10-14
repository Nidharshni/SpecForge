//! The rule set's order and ownership: the extensions' rules by code
//! (declaration order within a code), then the host's E006 rules by kind
//! and field; W023 for a code two extensions declare.

use specforge_common::Severity;
use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::CheckKind;
use specforge_registry::rules::Origin;

use super::{declaring_with_kinds, entity, rule, rules_of};
use crate::support::declare;

#[test]
fn rules_from_several_extensions_are_one_set_in_code_order() {
    let mut a = declaring_with_kinds(vec![
        rule("W300", "no_incoming_edges"),
        rule("W100", "no_incoming_edges"),
    ]);
    a.handshake.name = "@ext/a".to_string();
    let mut b = declaring_with_kinds(vec![rule("W200", "no_outgoing_edges")]);
    b.handshake.name = "@ext/b".to_string();

    let built = rules_of(vec![a, b]);

    assert!(built.diagnostics.is_empty(), "{:?}", built.diagnostics);
    let owned: Vec<(&str, &str)> = built
        .rules
        .iter()
        .map(|r| (r.code(), r.origin().name()))
        .collect();
    assert_eq!(
        owned,
        [("W100", "@ext/a"), ("W200", "@ext/b"), ("W300", "@ext/a")]
    );
}

#[test]
fn a_code_declared_for_several_kinds_keeps_declaration_order() {
    let mut declared = Vec::new();
    for kind in ["type", "behavior", "event"] {
        let mut w004 = rule("W004", "no_verify_statements");
        w004.target_kind = Some(kind.to_string());
        declared.push(w004);
    }
    declared.insert(1, rule("A001", "no_edges"));
    let built = rules_of(vec![declaring_with_kinds(declared)]);
    let order: Vec<(&str, Option<&str>)> = built
        .rules
        .iter()
        .map(|r| (r.code(), r.target_kind()))
        .collect();
    assert_eq!(
        order,
        [
            ("A001", Some("behavior")),
            ("W004", Some("type")),
            ("W004", Some("behavior")),
            ("W004", Some("event")),
        ]
    );
}

#[test]
fn a_code_two_extensions_declare_is_w023_and_both_rules_are_kept() {
    let mut a = declaring_with_kinds(vec![rule("W100", "no_incoming_edges")]);
    a.handshake.name = "@ext/a".to_string();
    let mut b = declaring_with_kinds(vec![
        rule("W100", "no_outgoing_edges"),
        // A rule W112 rejects still counts for W023.
        rule("W101", "bogus"),
    ]);
    b.handshake.name = "@ext/b".to_string();
    let mut c = declaring_with_kinds(vec![rule("W101", "no_edges"), rule("W101", "no_edges")]);
    c.handshake.name = "@ext/c".to_string();

    let built = rules_of(vec![a, b, c]);

    let reported: Vec<(&str, &str)> = built
        .diagnostics
        .iter()
        .map(|d| (d.code.as_str(), d.message.as_str()))
        .collect();
    assert_eq!(
        reported,
        [
            (
                "W112",
                "extension '@ext/b': unrecognized validation pattern kind 'bogus'"
            ),
            (
                "W023",
                "validation rule code 'W100' from '@ext/b' duplicates code from '@ext/a'"
            ),
            // Once per repeat.
            (
                "W023",
                "validation rule code 'W101' from '@ext/c' duplicates code from '@ext/b'"
            ),
            (
                "W023",
                "validation rule code 'W101' from '@ext/c' duplicates code from '@ext/b'"
            ),
        ]
    );
    assert_eq!(built.codes(), ["W100", "W100", "W101", "W101"]);
}

#[test]
fn the_hosts_e006_rules_follow_the_extensions_by_kind_and_field() {
    let mut declaration = declare("@test", |c| {
        c.kind("behavior", |k| {
            k.keyword("behavior");
        });
        c.kind("zeta", |k| {
            k.description("z");
            k.field("b", |f| {
                f.field_type(FieldType::String).required();
            });
            k.field("a", |f| {
                f.field_type(FieldType::String).required();
            });
        });
        c.kind("alpha", |k| {
            k.description("a");
            k.field("name", |f| {
                f.field_type(FieldType::String).required();
            });
            k.field("note", |f| {
                f.field_type(FieldType::String);
            });
        });
    });
    declaration.validation_rules = vec![rule("Z999", "no_edges")];

    let built = rules_of(vec![declaration]);

    let order: Vec<(&str, Option<&str>, &Origin)> = built
        .rules
        .iter()
        .map(|r| (r.code(), r.target_kind(), r.origin()))
        .collect();
    let ext = Origin::Extension("@test".to_string());
    assert_eq!(
        order,
        [
            ("Z999", Some("behavior"), &ext),
            ("E006", Some("alpha"), &Origin::Host),
            ("E006", Some("zeta"), &Origin::Host),
            ("E006", Some("zeta"), &Origin::Host),
        ]
    );
    let e006 = built.rules.iter().nth(1).unwrap();
    assert_eq!(e006.check_kind(), CheckKind::MissingRequiredField);
    assert_eq!(e006.severity(), Severity::Error);
    assert_eq!(e006.describe()["field"], "name");
    let fields: Vec<serde_json::Value> = built
        .rules
        .iter()
        .skip(2)
        .map(|r| r.describe()["field"].clone())
        .collect();
    assert_eq!(fields, [serde_json::json!("a"), serde_json::json!("b")]);

    let diagnostics = super::check(
        &built,
        &[
            entity("a1", "alpha", 0, 0),
            entity("a2", "alpha", 0, 0).with_field("name", ""),
        ],
    );
    let messages: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == "E006")
        .map(|d| d.message.as_str())
        .collect();
    // Written, even empty, is present.
    assert_eq!(messages, ["alpha 'a1' is missing required field 'name'"]);
}
