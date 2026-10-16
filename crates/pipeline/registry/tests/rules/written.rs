//! Rules whose answer is the snapshot's: the edge type of each edge, what exempts an entity, and
//! the text of a field. Every test writes its entities as `.spec` text and reads them through the
//! build's own graph inputs (ADR 0115); the rule engine's own tests stay over hand-built records.

use specforge_common::Diagnostic;
use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::ValidationRuleDescriptor;
use specforge_registry::rules::NoVerdicts;
use specforge_test_macros::test as spec;

use super::cycles::cycle_rule;
use super::{Built, constraint, declaring_with_kinds, messages, rule, rules, rules_of};
use crate::support::written::snapshot_of;
use crate::support::{NoDisk, declare};

/// What `built`'s rules report over `text`, written as one file.
fn over_text(built: &Built, text: &str) -> Vec<Diagnostic> {
    let snapshot = snapshot_of(&built.build, &[("t.spec", text)]);
    built
        .rules
        .check(&snapshot.rule_input(&NoDisk), &NoVerdicts)
}

/// `@test` declaring `behavior`, `feature` (only when `with_feature`), `event` and the edges
/// `BehaviorImplementsFeature` and `BehaviorReferencesEvent` between them, plus `rule`.
fn implementing(rule: ValidationRuleDescriptor, with_feature: bool) -> Built {
    let mut declaration = declare("@test", |c| {
        c.kind("behavior", |k| {
            k.keyword("behavior");
            k.field("features", |f| {
                f.field_type(FieldType::ReferenceList)
                    .edge("BehaviorImplementsFeature")
                    .target_kind("feature");
            });
            k.field("events", |f| {
                f.field_type(FieldType::ReferenceList)
                    .edge("BehaviorReferencesEvent")
                    .target_kind("event");
            });
        });
        if with_feature {
            c.kind("feature", |k| {
                k.keyword("feature");
            });
        }
        c.kind("event", |k| {
            k.keyword("event");
        });
        c.edge("BehaviorImplementsFeature", |e| {
            e.source_kind("behavior").target_kind("feature");
        });
        c.edge("BehaviorReferencesEvent", |e| {
            e.source_kind("behavior").target_kind("event");
        });
    });
    declaration.validation_rules = vec![rule];
    rules_of(vec![declaration])
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "an edge rule counts only edges of its edge type and is dropped when no extension declares the kind at its far end"
)]
fn an_edge_rule_counts_only_its_edge_type() {
    let mut declared = rule("W001", "no_outgoing_edges");
    declared.edge_type = Some("BehaviorImplementsFeature".to_string());
    declared.message_template = "behavior '{id}' does not implement any feature".to_string();

    // b1 references an event but no feature; b2 implements a feature.
    let text = "behavior b1 \"B1\" {\n  events [e1]\n}\n\n\
                behavior b2 \"B2\" {\n  features [f1]\n}\n\n\
                event e1 \"E1\" {\n}\n\n\
                feature f1 \"F1\" {\n}\n";
    let built = implementing(declared.clone(), true);
    assert_eq!(
        built.rules.iter().next().unwrap().describe()["edge_writers"],
        serde_json::json!(["behavior.features"])
    );
    let diagnostics = over_text(&built, text);
    assert_eq!(
        messages(&diagnostics),
        ["behavior 'b1' does not implement any feature"]
    );

    // Without an extension declaring `feature`, the rule can't be met: dropped.
    let built = implementing(declared, false);
    assert!(built.rules.is_empty());
    assert!(built.diagnostics.is_empty(), "{:?}", built.diagnostics);
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "an edge rule over an edge type several kinds write without declaring it counts the edges of every writing kind"
)]
fn an_edge_rule_over_an_undeclared_edge_counts_every_writing_kind() {
    // `person` is assigned by `task` (from `@test`) and `ticket` (from
    // `@test/more`), through the `assigned_to` edge neither declares.
    let mut declared = rule("W001", "no_incoming_edges");
    declared.target_kind = Some("person".to_string());
    declared.edge_type = Some("assigned_to".to_string());
    declared.message_template = "person '{id}' is assigned nothing".to_string();
    let mut first = declare("@test", |c| {
        c.kind("task", |k| {
            k.keyword("task");
            k.field("assignee", |f| {
                f.field_type(FieldType::Reference)
                    .edge("assigned_to")
                    .target_kind("person");
            });
        });
        c.kind("person", |k| {
            k.keyword("person");
        });
    });
    first.validation_rules = vec![declared];
    let second = declare("@test/more", |c| {
        c.kind("ticket", |k| {
            k.keyword("ticket");
            k.field("owner", |f| {
                f.field_type(FieldType::Reference)
                    .edge("assigned_to")
                    .target_kind("person");
            });
            k.field("reviewer", |f| {
                f.field_type(FieldType::Reference)
                    .edge("reviews")
                    .target_kind("person");
            });
        });
    });
    let built = rules_of(vec![first, second]);
    assert_eq!(
        built.rules.iter().next().unwrap().describe()["edge_writers"],
        serde_json::json!(["task.assignee", "ticket.owner"])
    );

    // p1 is assigned by a task, p2 by a ticket, p3 only reviewed by one.
    let text = "person p1 \"P1\" {\n}\n\nperson p2 \"P2\" {\n}\n\nperson p3 \"P3\" {\n}\n\n\
                task t1 \"T\" {\n  assignee p1\n}\n\n\
                ticket k1 \"K\" {\n  owner p2\n}\n\n\
                ticket k2 \"K\" {\n  reviewer p3\n}\n";
    let diagnostics = over_text(&built, text);
    assert_eq!(messages(&diagnostics), ["person 'p3' is assigned nothing"]);
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "cycle_detection follows every field that writes its edge type"
)]
fn cycle_detection_follows_every_field_that_writes_its_edge_type() {
    // `module.depends_on` and `service.needs` both write `Requires`.
    let mut declaration = declare("@test", |c| {
        c.kind("module", |k| {
            k.keyword("module");
            k.field("depends_on", |f| {
                f.field_type(FieldType::ReferenceList)
                    .edge("Requires")
                    .target_kind("service");
            });
        });
        c.kind("service", |k| {
            k.keyword("service");
            k.field("needs", |f| {
                f.field_type(FieldType::ReferenceList)
                    .edge("Requires")
                    .target_kind("module");
            });
        });
        c.edge("Requires", |e| {
            e.description("a dependency");
        });
    });
    declaration.validation_rules = vec![cycle_rule("E012", None, Some("Requires"))];
    let built = rules_of(vec![declaration]);
    assert!(built.diagnostics.is_empty(), "{:?}", built.diagnostics);
    let rule = built.rules.iter().next().unwrap();
    assert_eq!(
        rule.describe()["edge_writers"],
        serde_json::json!(["module.depends_on", "service.needs"])
    );
    // m1 -> s1 -> m1 closes only through both fields; m2 -> s1 leads in.
    let text = "module m1 \"M\" {\n  depends_on [s1]\n}\n\n\
                module m2 \"M\" {\n  depends_on [s1]\n}\n\n\
                service s1 \"S\" {\n  needs [m1]\n}\n";

    let diagnostics = over_text(&built, text);

    assert_eq!(
        messages(&diagnostics),
        [
            "module 'm1' is on a dependency cycle",
            "service 's1' is on a dependency cycle",
        ]
    );
}

/// A `no_verify_statements` rule `code` on `target` (every kind when `None`), reading `field`.
fn obligation_rule(code: &str, target: Option<&str>, field: &str) -> ValidationRuleDescriptor {
    ValidationRuleDescriptor {
        code: code.to_string(),
        severity: specforge_protocol_types::ValidationSeverity::Warning,
        message_template: "{kind} '{id}' has no verify".to_string(),
        check: "no_verify_statements".to_string(),
        target_kind: target.map(str::to_string),
        field: Some(field.to_string()),
        ..Default::default()
    }
}

#[spec(
    behavior = "te_validate_unverified_testable",
    verify = "a field named like an obligation or an exemption exempts nothing from W004"
)]
fn w004_reads_statements_and_the_exemption_not_field_names() {
    // A rule for `behavior` and one for `type` (a union has no body to hold obligations).
    let built = rules(vec![
        obligation_rule("W004", Some("behavior"), "verify"),
        obligation_rule("W005", Some("type"), "verify"),
    ]);
    let over = |text: &str| over_text(&built, text);
    assert_eq!(over("behavior b1 \"B\" {\n}\n").len(), 1);
    assert!(over("behavior b2 \"B\" {\n  verify unit \"it works\"\n}\n").is_empty());

    // Members named like a statement or an exemption are fields: none of
    // them stands in for an obligation or exempts the entity.
    for (name, value) in [
        ("verify", "string"),
        ("gherkin", "\"string\""),
        ("abstract", "true"),
        ("variants", "open | done"),
    ] {
        let named = over(&format!("behavior b3 \"B\" {{\n  {name} {value}\n}}\n"));
        assert_eq!(named.len(), 1, "a field named {name} exempts nothing");
    }

    // The union syntax exempts; a type with a body does not.
    assert!(over("type Status = active | inactive\n").is_empty());
    assert_eq!(over("type Plain \"Plain\" {\n  id string\n}\n").len(), 1);
}

#[spec(
    behavior = "snapshot_entities_once",
    verify = "a kind that accepts no verify statements owes no obligations, whatever rule applies to it"
)]
fn a_kind_without_verify_owes_no_statements_but_still_owes_other_fields() {
    let memo_rules = |rules: Vec<ValidationRuleDescriptor>| -> Built {
        let mut declaration = declare("@test", |c| {
            c.kind("memo", |k| {
                k.keyword("memo").testable(true).supports_verify(false);
            });
        });
        declaration.validation_rules = rules;
        rules_of(vec![declaration])
    };
    let memo = "memo m \"M\" {\n}\n";
    // Statement obligations: exempt.
    let statements = memo_rules(vec![obligation_rule("P300", None, "verify")]);
    assert!(over_text(&statements, memo).is_empty());
    let mut flagged = obligation_rule("P301", None, "verify");
    flagged.check = "missing_field_when_flag_set".to_string();
    assert!(over_text(&memo_rules(vec![flagged]), memo).is_empty());
    // An obligation declared in another field: not exempt.
    let gherkin = memo_rules(vec![obligation_rule("P302", None, "gherkin")]);
    assert_eq!(over_text(&gherkin, memo).len(), 1);
    // A union body exempts from both.
    assert!(over_text(&gherkin, "memo u = a | b\n").is_empty());
}

#[test]
fn a_non_empty_constraint_flags_an_empty_value() {
    // An empty list and an empty block are written, with empty text (ADR 0019).
    for (field, field_type, empty, filled) in [
        ("tags", FieldType::StringList, "[]", "[x]"),
        (
            "requires",
            FieldType::Block,
            "{\n  }",
            "{\n    k \"v\"\n  }",
        ),
    ] {
        let mut declared = rule("I068", "field_value_constraint");
        declared.field = Some(field.to_string());
        declared.constraint = Some(constraint("non_empty", None, &[]));
        let mut declaration = declaring_with_kinds(vec![declared]);
        declaration.entities[0]
            .fields
            .push(specforge_protocol_types::FieldDescriptor {
                name: field.to_string(),
                field_type: field_type.to_string(),
                ..Default::default()
            });
        let built = rules_of(vec![declaration]);
        let text = format!(
            "behavior b1 \"B\" {{\n  {field} {empty}\n}}\n\nbehavior b2 \"B\" {{\n  {field} {filled}\n}}\n"
        );
        assert_eq!(
            messages(&over_text(&built, &text)),
            ["orphan behavior 'b1'"],
            "{field}"
        );
    }
}

#[spec(
    behavior = "emit_diagnostic_from_pattern",
    verify = "message template interpolates {field} and {value}"
)]
fn message_template_interpolates_field_and_value() {
    let constrained = |field: &str, field_type: FieldType| {
        let mut declared = rule("W103", "field_value_constraint");
        declared.message_template = "{kind} '{id}' has {field}='{value}'".to_string();
        declared.field = Some(field.to_string());
        declared.constraint = Some(constraint("one_of", None, &["active"]));
        let mut declaration = declaring_with_kinds(vec![declared]);
        declaration.entities[0]
            .fields
            .push(specforge_protocol_types::FieldDescriptor {
                name: field.to_string(),
                field_type: field_type.to_string(),
                ..Default::default()
            });
        rules_of(vec![declaration])
    };
    let status = constrained("status", FieldType::String);
    let diagnostics = over_text(&status, "behavior b1 \"B\" {\n  status \"invalid\"\n}\n");
    assert_eq!(
        messages(&diagnostics),
        ["behavior 'b1' has status='invalid'"]
    );

    // An expression group is its conjuncts joined by ", " (the field text, ADR 0019), where the
    // written value of every output puts one per line (ADR 0064 D3).
    let metric = constrained("metric", FieldType::String);
    let diagnostics = over_text(
        &metric,
        "behavior b2 \"B\" {\n  metric expr { a < 1, b > 2 }\n}\n",
    );
    assert_eq!(
        messages(&diagnostics),
        ["behavior 'b2' has metric='a < 1, b > 2'"]
    );
}
