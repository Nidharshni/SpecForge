//! What each check reads, resolved, and how it runs over the entity
//! snapshot's records: per entity for every check but cycles, which walk
//! the input's edges.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use specforge_common::cycles::{CycleOptions, find_cycles};
use specforge_common::{CustomRuleFailure, Diagnostic, DiagnosticData, codes};

use super::verdicts::{CustomCall, CustomVerdicts, Subject, Verdict, VerdictError};
use super::{Origin, Rule};
use crate::entity::{Direction, EntityRecord, RuleInput};

/// One variant per check kind; each carries what it reads, resolved.
#[derive(Debug, Clone)]
pub(crate) enum Check {
    NoIncomingEdges(EdgeScope),
    NoOutgoingEdges(EdgeScope),
    NoEdges,
    /// `missing_field_when_flag_set` (`skip_owing_nothing` when the field is
    /// `verify`: an entity that owes no statements is not missing them) and
    /// `missing_required_field`.
    MissingField {
        field: String,
        skip_owing_nothing: bool,
    },
    FieldValue {
        field: String,
        constraint: ValueConstraint,
    },
    /// When `when_field` holds one of `equals`, `field` must be present and
    /// non-empty.
    ConditionalFieldRequired {
        field: String,
        when_field: String,
        equals: Vec<String>,
    },
    /// The entities on a cycle of the edges labelled one of `labels` (the
    /// fields that write the edge type). No labels: it reports nothing.
    Cycle {
        labels: Vec<String>,
    },
    FileExists {
        field: String,
    },
    VerifyKindAllowlist {
        allowed: Vec<String>,
    },
    NoVerifyStatements {
        obligations: Obligations,
    },
    Custom {
        extension: String,
        function: String,
    },
}

/// Which edges an edge rule counts.
#[derive(Debug, Clone)]
pub(crate) enum EdgeScope {
    /// Every edge.
    Any,
    /// Only edges to (outgoing) or from (incoming) an entity of one of
    /// these kinds: the far end of the rule's edge type (its declared kind,
    /// else every kind that writes it without declaring it).
    Peers(Vec<String>),
}

/// A `field_value_constraint`'s predicate on the field's text.
#[derive(Debug, Clone)]
pub(crate) enum ValueConstraint {
    NonEmpty,
    OneOf(Vec<String>),
    /// Compiled once when the rule is built.
    Matches(regex::Regex),
}

/// Where a `no_verify_statements` rule reads an entity's obligations.
#[derive(Debug, Clone)]
pub(crate) enum Obligations {
    /// Its `verify` statements.
    Statements,
    /// A field the declaring extension names instead.
    Field(String),
}

/// A violation's `{field}` and `{value}`, when the check names them.
#[derive(Default)]
struct Violation {
    field: Option<String>,
    value: Option<String>,
}

impl Violation {
    fn value(value: impl Into<String>) -> Self {
        Violation {
            field: None,
            value: Some(value.into()),
        }
    }
}

/// `rule` over `input`: its violations, entities by id.
pub(super) fn run(
    rule: &Rule,
    input: &RuleInput<'_>,
    verdicts: &dyn CustomVerdicts,
) -> Vec<Diagnostic> {
    if let Check::Cycle { labels } = &rule.check {
        return cycles(rule, labels, input);
    }
    if let Check::FileExists { field } = &rule.check {
        return missing_files(rule, field, input);
    }
    let mut diagnostics = Vec::new();
    let mut failures = Vec::new();
    for record in input.entities.iter().filter(|e| rule.applies_to(&e.kind)) {
        let violation = match evaluate(rule, record, verdicts) {
            Ok(Some(violation)) => violation,
            Ok(None) => continue,
            // No runtime: the rule checks nothing, and says nothing.
            Err(VerdictError::Unavailable) => continue,
            // The function failed on it: it is not checked, and the rule
            // says so once, after its own diagnostics (W148).
            Err(VerdictError::Failed(error)) => {
                failures.push(CustomRuleFailure {
                    entity: record.id.clone(),
                    error,
                });
                continue;
            }
        };
        diagnostics.push(report(rule, record, violation));
    }
    if let Check::Custom {
        extension,
        function,
    } = &rule.check
        && !failures.is_empty()
    {
        diagnostics.push(not_checked(rule, extension, function, failures));
    }
    diagnostics
}

/// W148: `rule`'s `function` (an export of `extension`) failed on
/// `failures`' entities during this check. One per rule per check: the
/// message names how many and the first, the data every one, by id.
fn not_checked(
    rule: &Rule,
    extension: &str,
    function: &str,
    mut failures: Vec<CustomRuleFailure>,
) -> Diagnostic {
    failures.sort_by(|a, b| a.entity.cmp(&b.entity));
    let first = &failures[0];
    let (entities, they) = if failures.len() == 1 {
        ("entity", "it was")
    } else {
        ("entities", "they were")
    };
    Diagnostic::new(
        codes::W148,
        format!(
            "extension '{extension}': rule '{}': wasm_function '{function}' failed on {} {entities} ('{}': {}) — {they} not checked",
            rule.code,
            failures.len(),
            first.entity,
            first.error
        ),
    )
    .with_data(DiagnosticData::CustomRuleFailures {
        rule: rule.code.clone(),
        function: function.to_string(),
        failures,
    })
}

/// Whether `record` violates `rule`, and what the violation names. Err: a
/// custom rule got no verdict.
fn evaluate(
    rule: &Rule,
    record: &EntityRecord,
    verdicts: &dyn CustomVerdicts,
) -> Result<Option<Violation>, VerdictError> {
    let violated = |violated: bool| Ok(violated.then(Violation::default));
    match &rule.check {
        Check::NoIncomingEdges(scope) => violated(scope.count(record, Direction::Incoming) == 0),
        Check::NoOutgoingEdges(scope) => violated(scope.count(record, Direction::Outgoing) == 0),
        Check::NoEdges => violated(
            record.edges(Direction::Incoming, None) == 0
                && record.edges(Direction::Outgoing, None) == 0,
        ),
        Check::MissingField {
            field,
            skip_owing_nothing,
        } => {
            // An entity that owes no obligations (a union, which has no
            // body to hold them, one an extension's flag exempts, or one
            // whose kind accepts no `verify`) is not missing `verify`.
            violated(!(*skip_owing_nothing && record.exempts_statements()) && !record.writes(field))
        }
        Check::FieldValue { field, constraint } => violated(
            // A field that is not written breaks no constraint.
            record.field(field).is_some_and(|value| match constraint {
                ValueConstraint::NonEmpty => value.is_empty(),
                ValueConstraint::OneOf(allowed) => !allowed.iter().any(|a| a == value),
                ValueConstraint::Matches(regex) => !regex.is_match(value),
            }),
        ),
        Check::ConditionalFieldRequired {
            field,
            when_field,
            equals,
        } => violated(
            record
                .field(when_field)
                .is_some_and(|condition| equals.iter().any(|v| v == condition))
                && record.field(field).is_none_or(str::is_empty),
        ),
        // Run over the input by `cycles` and `missing_files`.
        Check::Cycle { .. } | Check::FileExists { .. } => Ok(None),
        Check::VerifyKindAllowlist { allowed } => Ok(record
            .obligations
            .iter()
            .map(|o| &o.kind)
            // A bare `verify "..."` has no kind.
            .filter(|kind| !kind.is_empty())
            .find(|kind| !allowed.contains(kind))
            .map(|kind| Violation::value(kind.clone()))),
        Check::NoVerifyStatements { obligations } => violated(match obligations {
            // A struct member named `verify` is a field, not a statement,
            // so it never stands in for one; what exempts an entity is its
            // record's exemption, never a field's name.
            Obligations::Statements => {
                !record.exempts_statements() && record.obligations.is_empty()
            }
            Obligations::Field(field) => !record.exempts_fields() && !record.writes(field),
        }),
        Check::Custom {
            extension,
            function,
        } => match verdicts.verdict(CustomCall {
            extension,
            function,
            subject: Subject::Entity(record),
        })? {
            Verdict::Pass => Ok(None),
            Verdict::Fail { field, value } => Ok(Some(Violation { field, value })),
        },
    }
}

impl EdgeScope {
    /// The edges of `record` in `direction` it counts.
    fn count(&self, record: &EntityRecord, direction: Direction) -> usize {
        match self {
            EdgeScope::Any => record.edges(direction, None),
            EdgeScope::Peers(kinds) => kinds
                .iter()
                .map(|kind| record.edges(direction, Some(kind)))
                .sum(),
        }
    }
}

/// The diagnostic of `rule` on `record`: `{field}` and `{value}` are the
/// violation's when it names them, else the rule's field and its text.
fn report(rule: &Rule, record: &EntityRecord, violation: Violation) -> Diagnostic {
    let default_field = rule.message_field.as_deref();
    let default_value = record.field(default_field.unwrap_or(""));
    let field = violation.field.as_deref().or(default_field);
    let value = violation.value.as_deref().or(default_value);
    let allowed = match &rule.check {
        Check::VerifyKindAllowlist { allowed } => Some(allowed.join(", ")),
        _ => None,
    };
    let message = interpolate(
        &rule.template,
        &record.id,
        &record.kind,
        field,
        value,
        allowed.as_deref(),
    );
    diagnostic(rule, message, record)
}

/// What `rule` reports on `record`. A rule an extension declared names that
/// extension as the diagnostic's origin; the host's own (E006) has none.
fn diagnostic(rule: &Rule, message: String, record: &EntityRecord) -> Diagnostic {
    let mut diagnostic = match &rule.origin {
        Origin::Extension(extension) => {
            Diagnostic::from_extension(extension, rule.code.clone(), rule.severity, message)
        }
        Origin::Host => Diagnostic::untyped(rule.code.clone(), rule.severity, message),
    };
    diagnostic.span = Some(record.span.clone());
    diagnostic
}

/// `template` with `{id}`, `{kind}`, and `{field}`, `{value}`, `{allowed}`
/// when given; a placeholder with nothing to put in stays as written.
fn interpolate(
    template: &str,
    id: &str,
    kind: &str,
    field: Option<&str>,
    value: Option<&str>,
    allowed: Option<&str>,
) -> String {
    let mut result = template.replace("{id}", id).replace("{kind}", kind);
    if let Some(f) = field {
        result = result.replace("{field}", f);
    }
    if let Some(v) = value {
        result = result.replace("{value}", v);
    }
    if let Some(a) = allowed {
        result = result.replace("{allowed}", a);
    }
    result
}

/// A cycle rule over `input`: each entity it applies to (every entity when
/// it names no target kind) that sits on a cycle of the edges labelled one
/// of `labels` between such entities, by id. `{kind}` is the member's own
/// kind; `{field}` and `{value}` default to the rule's field, as for every
/// check.
fn cycles(rule: &Rule, labels: &[String], input: &RuleInput<'_>) -> Vec<Diagnostic> {
    if labels.is_empty() {
        return Vec::new();
    }
    let members: BTreeMap<&str, &EntityRecord> = input
        .entities
        .iter()
        .filter(|e| rule.applies_to(&e.kind))
        .map(|e| (e.id.as_str(), e))
        .collect();
    if members.is_empty() {
        return Vec::new();
    }
    let mut adjacency: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for edge in input.edges {
        if labels.contains(&edge.label)
            && members.contains_key(edge.source.as_str())
            && members.contains_key(edge.target.as_str())
        {
            adjacency
                .entry(edge.source.clone())
                .or_default()
                .insert(edge.target.clone());
        }
    }
    // Seeds in id order (R-6: the walk must not depend on hash order).
    let seeds: Vec<String> = members.keys().map(|id| id.to_string()).collect();
    let (on_cycle, _) = find_cycles(&seeds, &adjacency, CycleOptions::default());
    on_cycle
        .iter()
        .filter_map(|id| members.get(id.as_str()))
        .map(|record| report(rule, record, Violation::default()))
        .collect()
}

/// The W112 of a custom rule whose function cannot answer the load-time
/// probe; `None` for any other rule, or when it answers (or no runtime can).
pub(super) fn probe(rule: &Rule, verdicts: &dyn CustomVerdicts) -> Option<Diagnostic> {
    let Check::Custom {
        extension,
        function,
    } = &rule.check
    else {
        return None;
    };
    let call = CustomCall {
        extension,
        function,
        subject: Subject::Probe {
            kind: rule.target.as_deref(),
        },
    };
    match verdicts.verdict(call) {
        Ok(_) | Err(VerdictError::Unavailable) => None,
        Err(VerdictError::Failed(error)) => Some(
            Diagnostic::new(
                codes::W112,
                format!(
                    "extension '{extension}': rule '{}': wasm_function '{function}' could not be resolved ({error}) — the rule will not fire",
                    rule.code
                ),
            )
            .with_suggestion(format!(
                "export '{function}' from '{extension}', or fix the rule's wasm_function"
            )),
        ),
    }
}

/// The paths `record` names in `field`: each item of a list field, else
/// the field's text (nothing when it does not write the field).
fn named_paths<'r>(record: &'r EntityRecord, field: &str) -> Vec<&'r str> {
    match record.fields.iter().rev().find(|f| f.key == field) {
        Some(written) => match &written.items {
            Some(items) => items.iter().map(String::as_str).collect(),
            None => vec![written.text.as_str()],
        },
        None => Vec::new(),
    }
}

/// A `file_exists` rule over `input`: each path an entity it applies to
/// names in `field` that does not exist, by entity id then path order. A
/// relative path is the spec root's; an absolute one is checked as written
/// (`join` keeps it). A list field's items are each a path, reported on
/// their own (`{value}` is the missing item).
fn missing_files(rule: &Rule, field: &str, input: &RuleInput<'_>) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for record in input.entities.iter().filter(|e| rule.applies_to(&e.kind)) {
        let listed = record
            .fields
            .iter()
            .rev()
            .find(|f| f.key == field)
            .is_some_and(|f| f.items.is_some());
        for path in named_paths(record, field) {
            if input.spec_root.join(path).exists() {
                continue;
            }
            let violation = if listed {
                Violation::value(path)
            } else {
                Violation::default()
            };
            diagnostics.push(report(rule, record, violation));
        }
    }
    diagnostics
}

/// The paths a `file_exists` rule reads on `input` (every item of a list
/// field), against the spec root.
pub(super) fn files(rule: &Rule, input: &RuleInput<'_>) -> Vec<PathBuf> {
    let Check::FileExists { field } = &rule.check else {
        return Vec::new();
    };
    input
        .entities
        .iter()
        .filter(|e| rule.applies_to(&e.kind))
        .flat_map(|e| named_paths(e, field))
        .map(|path| input.spec_root.join(path))
        .collect()
}
