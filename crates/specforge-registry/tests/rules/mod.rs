//! The rule set (`specforge_registry::rules`), tested through its
//! interface: descriptors in (`support::declare`, then the build), a
//! `Rules` and its diagnostics out, then `rules.check` over entity records.

mod checks;
mod custom;
mod cycles;
mod order;
mod shape;

use std::path::Path;

use specforge_common::{Diagnostic, SourceSpan, Sym};
use specforge_protocol_types::{
    ExtensionDeclaration, FieldConstraintDescriptor, ValidationRuleDescriptor, ValidationSeverity,
};
use specforge_registry::RegistryBuild;
use specforge_registry::entity::{EdgeRecord, EntityRecord, RuleInput};
use specforge_registry::rules::{NoVerdicts, Rules};

use crate::support::{build, declare};

/// What the build made of some declared rules: the rule set and the
/// diagnostics its rules step reported (W112 and W147, then W023).
pub struct Built {
    pub build: RegistryBuild,
    pub rules: Rules,
    pub diagnostics: Vec<Diagnostic>,
}

impl Built {
    /// The diagnostics with `code`.
    pub fn coded(&self, code: &str) -> Vec<&Diagnostic> {
        self.diagnostics.iter().filter(|d| d.code == code).collect()
    }

    /// The registered rules' codes, in execution order.
    pub fn codes(&self) -> Vec<&str> {
        self.rules.iter().map(|rule| rule.code()).collect()
    }
}

/// The rule set of `declarations`: the build's rules, and its rules step's
/// diagnostics (W112, W147 and W023, the only ones of those codes the build
/// reports).
pub fn rules_of(declarations: Vec<ExtensionDeclaration>) -> Built {
    let build = build(declarations);
    let diagnostics = build
        .diagnostics()
        .filter(|d| ["W112", "W147", "W023"].contains(&d.code.as_str()))
        .cloned()
        .collect();
    Built {
        rules: build.rules.clone(),
        build,
        diagnostics,
    }
}

/// An extension `@test` declaring `rules` and nothing else.
pub fn declaring(rules: Vec<ValidationRuleDescriptor>) -> ExtensionDeclaration {
    let mut declaration = declare("@test", |_| {});
    declaration.validation_rules = rules;
    declaration
}

/// An extension `@test` declaring `rules` and, for each kind they target,
/// that kind (a rule for a kind nobody declares is inert, ADR 0020 D5).
pub fn declaring_with_kinds(rules: Vec<ValidationRuleDescriptor>) -> ExtensionDeclaration {
    let kinds: std::collections::BTreeSet<String> = rules
        .iter()
        .filter_map(|rule| rule.target_kind.clone())
        .collect();
    let mut declaration = declare("@test", |c| {
        for kind in &kinds {
            c.kind(kind, |k| {
                k.keyword(kind).testable(true).supports_verify(true);
            });
        }
    });
    declaration.validation_rules = rules;
    declaration
}

/// The rule set of an extension `@test` declaring `rules` and the kinds
/// they target.
pub fn rules(rules: Vec<ValidationRuleDescriptor>) -> Built {
    rules_of(vec![declaring_with_kinds(rules)])
}

/// The rule set of the one rule `rule`.
pub fn one(rule: ValidationRuleDescriptor) -> Built {
    rules(vec![rule])
}

/// A warning rule `code` with `check` on `behavior`, template
/// `orphan {kind} '{id}'`.
pub fn rule(code: &str, check: &str) -> ValidationRuleDescriptor {
    ValidationRuleDescriptor {
        code: code.to_string(),
        severity: ValidationSeverity::Warning,
        message_template: "orphan {kind} '{id}'".to_string(),
        check: check.to_string(),
        target_kind: Some("behavior".to_string()),
        edge_type: None,
        field: None,
        constraint: None,
        wasm_function: None,
        target_extension: None,
    }
}

/// A constraint `kind` with `pattern` and `values`.
pub fn constraint(kind: &str, pattern: Option<&str>, values: &[&str]) -> FieldConstraintDescriptor {
    FieldConstraintDescriptor {
        kind: kind.to_string(),
        pattern: pattern.map(str::to_string),
        values: values.iter().map(|v| v.to_string()).collect(),
    }
}

pub fn span() -> SourceSpan {
    SourceSpan {
        file: Sym::new("test.spec"),
        start_line: 1,
        start_col: 0,
        end_line: 1,
        end_col: 0,
    }
}

/// A record of `kind` `id` with `incoming` and `outgoing` edges in total.
pub fn entity(id: &str, kind: &str, incoming: usize, outgoing: usize) -> EntityRecord {
    let mut entity = EntityRecord::new(kind, id, &span());
    entity.incoming.total = incoming;
    entity.outgoing.total = outgoing;
    entity
}

/// The rules' input over `entities`, with no edges and no spec root.
pub fn over(entities: &[EntityRecord]) -> RuleInput<'_> {
    RuleInput {
        entities,
        edges: &[],
        spec_root: Path::new(""),
    }
}

/// The rules' input over `entities` and `edges`.
pub fn with_edges<'a>(entities: &'a [EntityRecord], edges: &'a [EdgeRecord]) -> RuleInput<'a> {
    RuleInput {
        entities,
        edges,
        spec_root: Path::new(""),
    }
}

/// `rules` checked over `entities` without a runtime.
pub fn check(rules: &Built, entities: &[EntityRecord]) -> Vec<Diagnostic> {
    rules.rules.check(&over(entities), &NoVerdicts)
}

/// The messages of `diagnostics`.
pub fn messages(diagnostics: &[Diagnostic]) -> Vec<&str> {
    diagnostics.iter().map(|d| d.message.as_str()).collect()
}
