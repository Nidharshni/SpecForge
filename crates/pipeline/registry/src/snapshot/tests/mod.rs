//! The snapshot's tests: the field text, and the obligation rule over graphs built from text.

mod standing;
mod text;
mod validator;

use std::path::Path;

use specforge_graph::{Graph, GraphBuild, GraphConfig};
use specforge_protocol_types::{
    EntityKindDescriptor, ExtensionDeclaration, FieldDescriptor, ValidationRuleDescriptor,
    ValidationSeverity,
};

use crate::compilation::{RegistryBuild, build_registries};
use crate::disk::MemoryDisk;
use crate::rules::NoVerdicts;
use crate::snapshot::EntitySnapshot;

/// `source` built with the graph inputs of `registries`: how the session pairs them, so a value
/// is stored as the build coerced it (ADR 0115).
fn graph_of(source: &str, registries: &RegistryBuild) -> Graph {
    let config = GraphConfig {
        inputs: registries.graph_inputs(),
        ..GraphConfig::default()
    };
    GraphBuild::of([specforge_parser::parse(source, "test.spec")], config)
        .into_parts()
        .0
}

fn w004(kind: &str) -> ValidationRuleDescriptor {
    ValidationRuleDescriptor {
        code: "W004".into(),
        severity: ValidationSeverity::Warning,
        message_template: "{kind} '{id}' is testable but declares no verify obligations".into(),
        check: "no_verify_statements".into(),
        target_kind: Some(kind.into()),
        field: Some("verify".into()),
        ..Default::default()
    }
}

/// The registry build of one extension declaring `kinds` as
/// `(name, testable, supports_verify)`, the `abstract` flag on
/// `behavior` (as @specforge/formal does) when `abstract_behaviors`, and
/// `rules`.
fn declared(
    kinds: &[(&str, bool, bool)],
    abstract_behaviors: bool,
    rules: Vec<ValidationRuleDescriptor>,
) -> RegistryBuild {
    let mut declaration = ExtensionDeclaration {
        validation_rules: rules,
        entities: kinds
            .iter()
            .map(|(name, testable, supports_verify)| EntityKindDescriptor {
                name: name.to_string(),
                keyword: Some(name.to_string()),
                testable: *testable,
                supports_verify: *supports_verify,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    declaration.handshake.name = "@specforge/testing".into();
    declaration.handshake.version = "1.0.0".into();
    if abstract_behaviors {
        declaration
            .entities
            .iter_mut()
            .find(|kind| kind.name == "behavior")
            .expect("a behavior kind")
            .fields
            .push(FieldDescriptor {
                name: "abstract".into(),
                field_type: "bool".into(),
                exempts_obligations: true,
                ..Default::default()
            });
    }
    let build = build_registries(vec![declaration]);
    assert!(
        build.diagnostics().next().is_none(),
        "{:?}",
        build.diagnostics().collect::<Vec<_>>()
    );
    build
}

fn snapshot(source: &str, registries: &RegistryBuild) -> EntitySnapshot {
    EntitySnapshot::of(&graph_of(source, registries), registries, Path::new(""))
}

/// The ids W004 reports on `source` (rules on `behavior` and `type`).
fn w004_ids(source: &str, abstract_behaviors: bool) -> Vec<String> {
    // A rule for a kind nobody declares is inert (ADR 0020 D5): declare
    // the two the rules target.
    let registries = declared(
        &[("behavior", true, true), ("type", true, true)],
        abstract_behaviors,
        vec![w004("behavior"), w004("type")],
    );
    let snapshot = snapshot(source, &registries);
    let mut ids: Vec<String> = registries
        .rules()
        .check(&snapshot.rule_input(&MemoryDisk::new()), &NoVerdicts)
        .into_iter()
        .map(|d| d.message.split('\'').nth(1).unwrap().to_string())
        .collect();
    ids.sort();
    ids
}
