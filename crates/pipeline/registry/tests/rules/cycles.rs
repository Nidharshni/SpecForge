//! `cycle_detection` rules: the entities on a cycle of the edges of their edge
//! type, over the input's edge records (each typed by the field that wrote it).

use specforge_protocol_types::{ValidationRuleDescriptor, ValidationSeverity};
use specforge_registry::entity::{EdgeRecord, EntityRecord};
use specforge_registry::rules::NoVerdicts;
use specforge_test_macros::test as spec;

use super::{entity, messages, rules_of, with_edges};
use crate::support::declare;
use specforge_extension_sdk::prelude::*;

/// `@test` declaring `module` with `depends_on` (writing the edge
/// `ModuleDependsOn` to `module`) and `uses` (a plain reference list), plus
/// `rules`.
fn modules(rules: Vec<ValidationRuleDescriptor>) -> super::Built {
    let mut declaration = declare("@test", |c| {
        c.kind("module", |k| {
            k.description("m");
            k.field("depends_on", |f| {
                f.field_type(FieldType::ReferenceList)
                    .edge("ModuleDependsOn")
                    .target_kind("module");
            });
            k.field("uses", |f| {
                f.field_type(FieldType::ReferenceList).target_kind("module");
            });
        });
        c.kind("other", |k| {
            k.description("o");
        });
        c.edge("ModuleDependsOn", |e| {
            e.source_kind("module").target_kind("module");
        });
    });
    declaration.validation_rules = rules;
    rules_of(vec![declaration])
}

/// A cycle rule `code` on `edge_type`, targeting `target`.
pub fn cycle_rule(
    code: &str,
    target: Option<&str>,
    edge_type: Option<&str>,
) -> ValidationRuleDescriptor {
    ValidationRuleDescriptor {
        code: code.to_string(),
        severity: ValidationSeverity::Error,
        message_template: "{kind} '{id}' is on a dependency cycle".to_string(),
        check: "cycle_detection".to_string(),
        target_kind: target.map(str::to_string),
        edge_type: edge_type.map(str::to_string),
        ..Default::default()
    }
}

/// The edge `source -> target` the field `label` wrote; `edge_type` is the
/// edge type that field writes from the kind `source` is, if it writes one.
fn edge(source: &str, target: &str, label: &str, edge_type: Option<&str>) -> EdgeRecord {
    EdgeRecord {
        source: source.to_string(),
        target: target.to_string(),
        label: label.to_string(),
        edge_type: edge_type.map(str::to_string),
    }
}

/// `a ⇄ b` and `c → a` over `depends_on`, `x ⇄ y` over `uses`, and the
/// `other` entity `o`, whose `depends_on` is no field of its kind (that edge
/// is of no edge type), looped with `c`.
fn project() -> (Vec<EntityRecord>, Vec<EdgeRecord>) {
    let entities = vec![
        entity("a", "module", 0, 0),
        entity("b", "module", 0, 0),
        entity("c", "module", 0, 0),
        entity("o", "other", 0, 0),
        entity("x", "module", 0, 0),
        entity("y", "module", 0, 0),
    ];
    let edges = vec![
        edge("b", "a", "depends_on", Some("ModuleDependsOn")),
        edge("a", "b", "depends_on", Some("ModuleDependsOn")),
        edge("c", "a", "depends_on", Some("ModuleDependsOn")),
        edge("o", "c", "depends_on", None),
        edge("c", "o", "depends_on", Some("ModuleDependsOn")),
        edge("x", "y", "uses", None),
        edge("y", "x", "uses", None),
    ];
    (entities, edges)
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "cycle_detection finds cycles in edge type"
)]
fn cycle_detection_finds_cycles_in_edge_type() {
    let built = modules(vec![cycle_rule(
        "E007",
        Some("module"),
        Some("ModuleDependsOn"),
    )]);
    assert!(built.diagnostics.is_empty(), "{:?}", built.diagnostics);
    let (entities, edges) = project();

    let diagnostics = built
        .rules
        .check(&with_edges(&entities, &edges), &NoVerdicts);

    // a and b, by id; not c (it only leads into the cycle), not x and y
    // (another edge type), not the loop through the `other` entity.
    assert_eq!(
        messages(&diagnostics),
        [
            "module 'a' is on a dependency cycle",
            "module 'b' is on a dependency cycle",
        ]
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code == "E007" && d.span.is_some())
    );
}

#[test]
fn a_cycle_rule_runs_once_per_check_whatever_the_entity_order() {
    let built = modules(vec![cycle_rule(
        "E007",
        Some("module"),
        Some("ModuleDependsOn"),
    )]);
    let (mut entities, edges) = project();
    let forward = built
        .rules
        .check(&with_edges(&entities, &edges), &NoVerdicts);
    entities.reverse();
    let reversed = built
        .rules
        .check(&with_edges(&entities, &edges), &NoVerdicts);
    assert_eq!(forward, reversed);
}

// An edge type is never read as a field name: `uses` is a field, not an
// edge type, so the rule is inert (and its extension gets W021).
#[test]
fn a_cycle_rule_never_reads_its_edge_type_as_a_field_name() {
    let built = modules(vec![cycle_rule("E008", Some("module"), Some("uses"))]);
    assert!(built.rules.is_empty());
    let (entities, edges) = project();
    assert!(
        built
            .rules
            .check(&with_edges(&entities, &edges), &NoVerdicts)
            .is_empty()
    );
}

#[spec(
    behavior = "parse_validation_rule_pattern",
    verify = "a cycle_detection rule without an edge_type produces W112 and is not registered"
)]
fn a_cycle_rule_without_an_edge_type_is_w112() {
    let built = modules(vec![cycle_rule("E009", Some("module"), None)]);
    assert!(built.rules.is_empty());
    let w112 = built.coded("W112");
    assert_eq!(w112.len(), 1, "{:?}", built.diagnostics);
    assert_eq!(
        w112[0].message,
        "extension '@test': rule 'E009': check 'cycle_detection' requires an edge_type but none is set — the rule can never fire and was not registered"
    );
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "a cycle_detection rule without a target_kind reports every entity on a cycle of its edge type"
)]
fn an_untargeted_cycle_rule_reports_every_entity_on_a_cycle() {
    let built = modules(vec![cycle_rule("E010", None, Some("ModuleDependsOn"))]);
    assert!(built.diagnostics.is_empty(), "{:?}", built.diagnostics);
    let (entities, edges) = project();

    let diagnostics = built
        .rules
        .check(&with_edges(&entities, &edges), &NoVerdicts);

    // Every kind's entities, by the edges of the edge type only: `o`'s
    // `depends_on` is no field of its kind, so the loop through it is not a
    // cycle of the edge type (a field name is not an edge type).
    assert_eq!(
        messages(&diagnostics),
        [
            "module 'a' is on a dependency cycle",
            "module 'b' is on a dependency cycle",
        ]
    );
}

#[test]
fn a_cycle_rules_field_reads_as_the_default_field_and_value() {
    let mut declared = cycle_rule("E011", Some("module"), Some("ModuleDependsOn"));
    declared.field = Some("owner".to_string());
    declared.message_template = "{id} ({field}: {value}) is on a cycle".to_string();
    let built = modules(vec![declared]);
    let (mut entities, edges) = project();
    entities[0] = entities[0].clone().with_field("owner", "team-a");

    let diagnostics = built
        .rules
        .check(&with_edges(&entities, &edges), &NoVerdicts);

    assert_eq!(
        messages(&diagnostics),
        [
            "a (owner: team-a) is on a cycle",
            "b (owner: {value}) is on a cycle"
        ]
    );
}
