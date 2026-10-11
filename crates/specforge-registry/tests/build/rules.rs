//! `registry_build_rules`: the build collects every extension's rules,
//! ordered by code, each with the extension that declared it, then a
//! host-generated E006 rule for every required field.

use specforge_common::Severity;
use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::rules::{Origin, Rule};
use specforge_test_macros::test as spec;

use crate::support::{build, coded, codes, declare, diagnostics, rule_codes, software};

/// An extension `name` declaring warning rules `(code, message, check)`.
fn rules(name: &str, declared: &[(&str, &str, CheckKind)]) -> ExtensionDeclaration {
    declare(name, |c| {
        for (code, message, check) in declared {
            c.rule(code, |r| {
                r.severity(ValidationSeverity::Warning)
                    .message_template(message)
                    .check(*check);
            });
        }
    })
}

/// The build's rules as `code: message template`, in order.
fn templates(build: &specforge_registry::RegistryBuild) -> Vec<String> {
    build
        .rules()
        .iter()
        .map(|rule| {
            format!(
                "{}: {}",
                rule.code(),
                rule.describe()["message_template"].as_str().unwrap()
            )
        })
        .collect()
}

/// The build's `i`th rule, in execution order.
fn nth(build: &specforge_registry::RegistryBuild, i: usize) -> &Rule {
    build.rules().iter().nth(i).unwrap()
}

#[spec(
    behavior = "registry_build_rules",
    verify = "every declared rule is in the build's rules with the extension that declared it"
)]
fn every_declared_rule_is_in_the_build_with_its_extension() {
    let a = declare("@ext/a", |c| {
        // Its rule targets software's kind: software is its peer.
        c.meta
            .peer_dependencies
            .push(crate::support::peer("@specforge/software", ">=1.0.0"));
        c.rule("W900", |r| {
            r.severity(ValidationSeverity::Warning)
                .message_template("orphan {kind} '{id}'")
                .check(CheckKind::NoIncomingEdges)
                .target_kind("behavior");
        });
    });
    let b = rules("@ext/b", &[("W901", "b rule", CheckKind::NoOutgoingEdges)]);
    let build = build([software(), a, b]);

    assert!(diagnostics(&build).is_empty(), "{:?}", diagnostics(&build));
    assert_eq!(rule_codes(&build), [("W900", "@ext/a"), ("W901", "@ext/b")]);
    let w100 = nth(&build, 0);
    assert_eq!(w100.check_kind(), CheckKind::NoIncomingEdges);
    assert_eq!(w100.check_kind().as_str(), "no_incoming_edges");
    assert_eq!(w100.severity(), Severity::Warning);
    assert_eq!(w100.describe()["message_template"], "orphan {kind} '{id}'");
    assert_eq!(w100.target_kind(), Some("behavior"));
    assert_eq!(nth(&build, 1).check_kind(), CheckKind::NoOutgoingEdges);
}

#[spec(
    behavior = "registry_build_rules",
    verify = "the extensions' rules are ordered by code"
)]
fn the_extensions_rules_are_ordered_by_code() {
    let a = rules(
        "@ext/a",
        &[
            ("W902", "a300", CheckKind::NoIncomingEdges),
            ("W900", "a100", CheckKind::NoIncomingEdges),
        ],
    );
    let b = rules(
        "@ext/b",
        &[
            ("W901", "b200", CheckKind::NoOutgoingEdges),
            ("W900", "b100", CheckKind::NoOutgoingEdges),
        ],
    );
    let build = build([a.clone(), b.clone()]);
    assert_eq!(
        rule_codes(&build),
        [
            ("W900", "@ext/a"),
            ("W900", "@ext/b"),
            ("W901", "@ext/b"),
            ("W902", "@ext/a"),
        ]
    );
    assert_eq!(
        templates(&build),
        ["W900: a100", "W900: b100", "W901: b200", "W902: a300"]
    );

    // Rules sharing a code keep load order.
    let swapped = crate::support::build([b, a]);
    assert_eq!(
        rule_codes(&swapped),
        [
            ("W900", "@ext/b"),
            ("W900", "@ext/a"),
            ("W901", "@ext/b"),
            ("W902", "@ext/a"),
        ]
    );

    // The host's E006 rules come after the extensions' rules.
    let required = declare("@ext/c", |c| {
        c.kind("Task", |k| {
            k.keyword("task");
            k.field("owner", |f| {
                f.field_type(FieldType::String).required();
            });
        });
        c.rule("W050", |r| {
            r.severity(ValidationSeverity::Warning)
                .message_template("c050")
                .check(CheckKind::NoIncomingEdges);
        });
    });
    let with_e006 = crate::support::build([required]);
    assert_eq!(rule_codes(&with_e006), [("W050", "@ext/c"), ("E006", "")]);
}

#[spec(
    behavior = "registry_build_rules",
    verify = "the build drops a rule whose target kind or edge type no loaded extension declares"
)]
fn the_build_drops_a_rule_whose_target_kind_or_edge_type_no_extension_declares() {
    let ghostly = declare("@t/e", |c| {
        c.meta
            .peer_dependencies
            .push(crate::support::peer("@specforge/software", ">=1.0.0"));
        c.rule("W101", |r| {
            r.severity(ValidationSeverity::Warning)
                .message_template("orphan {id}")
                .check(CheckKind::NoIncomingEdges)
                .target_kind("behavior")
                .edge_type("GhostEdge");
        });
    });
    // `nonexistent_kind` belongs to an optional peer that is not installed.
    let waiting = declare("@t/f", |c| {
        c.meta
            .peer_dependencies
            .push(crate::support::optional_peer("@t/absent", ">=1.0.0"));
        c.rule("W102", |r| {
            r.severity(ValidationSeverity::Warning)
                .message_template("lonely {id}")
                .check(CheckKind::NoOutgoingEdges)
                .target_kind("nonexistent_kind");
        });
    });
    let build = build([software(), ghostly, waiting]);

    // W102 (a kind nobody declares) and W101 (an edge type nobody declares)
    // are both dropped: inert (ADR 0020 D5). W101's extension is told about
    // the edge type (W021).
    assert_eq!(rule_codes(&build), []);
    let w021: Vec<&str> = coded(&build, "W021")
        .iter()
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        w021,
        [
            "extension '@t/e': rule 'W101' references edge type 'GhostEdge' not declared among its edges or its peers' edges"
        ]
    );
}

#[spec(
    behavior = "registry_build_rules",
    verify = "extensions produce E006 rules for required fields"
)]
fn required_fields_get_e006_rules() {
    let declared = declare("@specforge/software", |c| {
        c.kind("Behavior", |k| {
            k.keyword("behavior");
            k.field("contract", |f| {
                f.field_type(FieldType::String).required();
            });
            k.field("category", |f| {
                f.field_type(FieldType::String);
            });
        });
        c.kind("Invariant", |k| {
            k.keyword("invariant");
            k.field("guarantee", |f| {
                f.field_type(FieldType::String).required();
            });
        });
    });
    let build = build([declared]);

    assert_eq!(rule_codes(&build), [("E006", ""), ("E006", "")]);
    for rule in build.rules() {
        assert_eq!(
            rule.origin(),
            &Origin::Host,
            "a host rule is owned by no extension"
        );
        assert_eq!(rule.severity(), Severity::Error);
        assert_eq!(rule.check_kind(), CheckKind::MissingRequiredField);
    }
    let targets: Vec<String> = build
        .rules()
        .iter()
        .map(|rule| {
            format!(
                "{}.{}",
                rule.target_kind().unwrap(),
                rule.describe()["field"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(targets, ["behavior.contract", "invariant.guarantee"]);

    // No required field, no rule.
    assert!(crate::support::build([software()]).rules().is_empty());
}

#[spec(
    behavior = "registry_build_rules",
    verify = "Registry Build Collects Rules: rule collection holds — declarations_in_load_order, rules_collected, duplicates_warned, required_enforced, unloaded_targets_inert"
)]
fn rule_collection_holds() {
    // declarations_in_load_order: each extension's rules out of code order;
    // both declare W900; `b` has a required field and a rule (W903) for a
    // kind no extension declares, which is not registered.
    let a = rules(
        "@ext/a",
        &[
            ("W902", "a300", CheckKind::NoIncomingEdges),
            ("W900", "a100", CheckKind::NoIncomingEdges),
        ],
    );
    let b = declare("@ext/b", |c| {
        // `ghost` belongs to an optional peer that is not installed.
        c.meta
            .peer_dependencies
            .push(crate::support::optional_peer("@ext/ghosts", ">=1.0.0"));
        c.kind("Task", |k| {
            k.keyword("task");
            k.field("owner", |f| {
                f.field_type(FieldType::String).required();
            });
        });
        for (code, message) in [("W901", "b200"), ("W900", "b100")] {
            c.rule(code, |r| {
                r.severity(ValidationSeverity::Warning)
                    .message_template(message)
                    .check(CheckKind::NoOutgoingEdges);
            });
        }
        c.rule("W903", |r| {
            r.severity(ValidationSeverity::Warning)
                .message_template("b400")
                .check(CheckKind::NoIncomingEdges)
                .target_kind("ghost");
        });
    });
    let build = build([a, b]);

    // rules_collected: one set, ordered by code, each with its extension.
    assert_eq!(
        rule_codes(&build),
        [
            ("W900", "@ext/a"),
            ("W900", "@ext/b"),
            ("W901", "@ext/b"),
            ("W902", "@ext/a"),
            ("E006", ""),
        ]
    );

    // duplicates_warned: one W023 naming the code and both extensions.
    let w023 = coded(&build, "W023");
    assert_eq!(w023.len(), 1, "{:?}", diagnostics(&build));
    assert_eq!(w023[0].severity, Severity::Warning);
    for part in ["'W900'", "'@ext/a'", "'@ext/b'"] {
        assert!(w023[0].message.contains(part), "{}", w023[0].message);
    }
    assert_eq!(codes(&build), ["W023"]);

    // required_enforced: the required field has its E006 rule.
    let e006 = build.rules().iter().last().unwrap();
    assert_eq!(e006.target_kind(), Some("task"));
    assert_eq!(e006.describe()["field"], "owner");

    // unloaded_targets_inert: the rule for `ghost` costs no diagnostic.
    assert_eq!(diagnostics(&build).len(), 1, "{:?}", diagnostics(&build));

    // A code one extension repeats is not a duplicate across extensions.
    let repeated = crate::support::build([rules(
        "@ext/solo",
        &[
            ("W900", "first", CheckKind::NoIncomingEdges),
            ("W900", "again", CheckKind::NoIncomingEdges),
        ],
    )]);
    assert!(coded(&repeated, "W023").is_empty());
    assert_eq!(repeated.rules().len(), 2);
}
