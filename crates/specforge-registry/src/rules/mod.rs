//! The validation rules a project's extensions declare, plus the host's E006
//! rules for required fields, as one typed set that runs itself (ADR 0020).
//!
//! [`Rules::build`] turns each declared descriptor into a [`Rule`] whose
//! check carries exactly what that check reads, resolved once against the
//! registries: a `matches` regex compiled, an edge rule's peer kind, the
//! graph field labels a cycle rule's edge type is written as. A descriptor
//! that cannot work as declared is W112 and is not registered. [`Rules::check`]
//! runs every rule over a [`RuleInput`] (the entity snapshot's records, its
//! edges and the spec root, ADR 0019), in a fixed order, asking
//! [`CustomVerdicts`] for `custom` rules. Rules read field text
//! (`EntityRecord::field`/`writes`), never stringify a value. Nothing
//! outside this module reads a rule's parts.

mod build;
mod check;
mod verdicts;

use std::path::PathBuf;

use specforge_common::{Diagnostic, Severity};
use specforge_protocol_types::{CheckKind, ConstraintKind, ExtensionDeclaration};

use crate::entity::RuleInput;
use crate::{EdgeRegistry, FieldRegistry, KindRegistry};
use check::{Check, EdgeScope};

pub use verdicts::{CustomCall, CustomVerdicts, NoVerdicts, Subject, Verdict, VerdictError};

/// The registries a rule resolves against: target kinds, the edge types
/// edge and cycle rules name, and the fields that write those edges or are
/// `required` (E006).
#[derive(Clone, Copy)]
pub(crate) struct Registries<'a> {
    pub kinds: &'a KindRegistry,
    pub fields: &'a FieldRegistry,
    pub edges: &'a EdgeRegistry,
}

/// Who declared a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// An extension's declared rule (its name).
    Extension(String),
    /// A rule the host generates: E006 for a `required` field.
    Host,
}

impl Origin {
    /// The declaring extension's name; `""` for the host.
    pub fn name(&self) -> &str {
        match self {
            Origin::Extension(name) => name,
            Origin::Host => "",
        }
    }
}

/// One registered rule: a declared descriptor (or a host E006 rule) after
/// its shape was checked and its references resolved.
#[derive(Debug, Clone)]
pub struct Rule {
    code: String,
    severity: Severity,
    template: String,
    target: Option<String>,
    /// The declared `field`: every check's message reads it as the default
    /// `{field}` (and its text as the default `{value}`).
    message_field: Option<String>,
    origin: Origin,
    check_kind: CheckKind,
    check: Check,
    /// The properties the rule was registered with, as declared (what
    /// [`Rule::describe`] lists).
    declared: Declared,
}

/// A rule's declared properties beyond its head, as registered.
#[derive(Debug, Clone, Default)]
struct Declared {
    edge_type: Option<String>,
    /// For an edge or cycle rule: the fields that write its edge type.
    edge_fields: Vec<String>,
    /// The extension whose kind or edge type the rule names, as declared.
    target_extension: Option<String>,
    constraint: Option<DeclaredConstraint>,
    wasm_function: Option<String>,
}

#[derive(Debug, Clone)]
struct DeclaredConstraint {
    /// `None` for a name this host does not read.
    kind: Option<ConstraintKind>,
    pattern: Option<String>,
    values: Vec<String>,
}

impl Rule {
    /// The diagnostic code its violations carry.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// The severity its violations carry.
    pub fn severity(&self) -> Severity {
        self.severity
    }

    /// The kind whose entities it checks; `None`: every entity.
    pub fn target_kind(&self) -> Option<&str> {
        self.target.as_deref()
    }

    /// Whether it applies to entities of `kind`: its target kind's, or every
    /// kind's when it names none. The one reading of `target_kind` (ADR
    /// 0019): the rules, the entity's standing and the verify-stub fix
    /// share it.
    pub fn applies_to(&self, kind: &str) -> bool {
        self.target.as_deref().is_none_or(|target| target == kind)
    }

    /// Its check kind, in the extension vocabulary.
    pub fn check_kind(&self) -> CheckKind {
        self.check_kind
    }

    /// Who declared it.
    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// The rule as the registry build snapshot lists it: the descriptor's
    /// keys (`code`, `severity`, `message_template`, `check`, `target_kind`,
    /// `edge_type`, `edge_peer_kinds`, `field`, `constraint`,
    /// `wasm_function`), plus `edge_fields` for an edge or cycle rule with
    /// an edge type (the fields that write it) and `target_extension` when
    /// the rule declares one.
    pub fn describe(&self) -> serde_json::Value {
        let edge_peer_kinds = match &self.check {
            Check::NoIncomingEdges(EdgeScope::Peers(kinds))
            | Check::NoOutgoingEdges(EdgeScope::Peers(kinds)) => Some(kinds),
            _ => None,
        };
        let mut described = serde_json::json!({
            "code": self.code,
            "severity": format!("{:?}", self.severity),
            "message_template": self.template,
            "check": self.check_kind.as_str(),
            "target_kind": self.target,
            "edge_type": self.declared.edge_type,
            "edge_peer_kinds": edge_peer_kinds,
            "field": self.message_field,
            "constraint": self.declared.constraint.as_ref().map(|c| serde_json::json!({
                "kind": c.kind.map(ConstraintKind::as_str),
                "pattern": c.pattern,
                "values": c.values,
            })),
            "wasm_function": self.declared.wasm_function,
        });
        let edge_rule = matches!(
            self.check,
            Check::NoIncomingEdges(_) | Check::NoOutgoingEdges(_) | Check::Cycle { .. }
        );
        if edge_rule && self.declared.edge_type.is_some() {
            described["edge_fields"] = serde_json::json!(self.declared.edge_fields);
        }
        if let Some(extension) = &self.declared.target_extension {
            described["target_extension"] = serde_json::json!(extension);
        }
        described
    }
}

/// Every registered rule, in execution order: the extensions' rules by code
/// (declaration order within a code), then the host's E006 rules by kind
/// and field.
#[derive(Debug, Clone, Default)]
pub struct Rules {
    rules: Vec<Rule>,
}

impl Rules {
    /// The rules `declarations` declare (load order), checked and resolved
    /// against `registries`, plus an E006 rule for every `required` field.
    ///
    /// Diagnostics, per extension and per rule in declaration order, W150,
    /// W147 or W112 (the rule cannot work as declared, not registered). Pure.
    pub(crate) fn build(
        declarations: &[ExtensionDeclaration],
        registries: Registries<'_>,
    ) -> (Rules, Vec<Diagnostic>) {
        build::build(declarations, registries)
    }

    /// W023 for a code a later extension declares again, once per repeat
    /// ([`Self::build`] does not report it: it is a collision of two
    /// extensions, which the registry build reports after the rules').
    pub(crate) fn duplicate_codes(declarations: &[ExtensionDeclaration]) -> Vec<Diagnostic> {
        build::duplicate_codes(declarations)
    }

    /// Run every rule over `input`: each rule in order, its entities by id
    /// (a cycle rule: the entities on a cycle, by id). A `custom` rule asks
    /// `verdicts`; the entities whose verdict failed are W148, once per
    /// rule, right after that rule's diagnostics, with every failure as its
    /// data; an unavailable verdict ([`NoVerdicts`]) skips the rule
    /// silently.
    pub fn check(&self, input: &RuleInput<'_>, verdicts: &dyn CustomVerdicts) -> Vec<Diagnostic> {
        self.rules
            .iter()
            .flat_map(|rule| check::run(rule, input, verdicts))
            .collect()
    }

    /// Call each `custom` rule's function once on an entity of its target
    /// kind that declares nothing: a function that cannot answer is W112,
    /// once, at load (the rule stays registered).
    pub(crate) fn probe(&self, verdicts: &dyn CustomVerdicts) -> Vec<Diagnostic> {
        self.rules
            .iter()
            .filter_map(|rule| check::probe(rule, verdicts))
            .collect()
    }

    /// The first `no_verify_statements` rule, in execution order, that
    /// applies to `kind`: the rule that requires entities of `kind` to
    /// declare obligations. A kind that accepts no `verify` statements may
    /// have one; its entities are exempt instead
    /// ([`crate::entity::Exemption::NoVerify`]).
    pub fn verify_rule_for(&self, kind: &str) -> Option<&Rule> {
        self.rules
            .iter()
            .filter(|rule| rule.check_kind == CheckKind::NoVerifyStatements)
            .find(|rule| rule.applies_to(kind))
    }

    /// Whether some `no_verify_statements` rule applies to `kind`.
    pub fn obligates(&self, kind: &str) -> bool {
        self.verify_rule_for(kind).is_some()
    }

    /// The files `file_exists` rules read on `input` (each item of a list
    /// field), resolved against `input.spec_root`, sorted and unique: check
    /// inputs of a project session, so creating one re-runs the checks.
    pub fn files(&self, input: &RuleInput<'_>) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = self
            .rules
            .iter()
            .flat_map(|rule| check::files(rule, input))
            .collect();
        files.sort();
        files.dedup();
        files
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Rule> {
        self.rules.iter()
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

impl<'a> IntoIterator for &'a Rules {
    type Item = &'a Rule;
    type IntoIter = std::slice::Iter<'a, Rule>;

    fn into_iter(self) -> Self::IntoIter {
        self.rules.iter()
    }
}
