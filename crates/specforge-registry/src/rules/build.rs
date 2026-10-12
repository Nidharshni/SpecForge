//! Declared descriptor → [`Rule`]: its shape checked (W112 for a rule that
//! cannot work as declared), its references resolved against the
//! registries, then the host's E006 rules. ([`duplicate_codes`] is W023, which the
//! registry build reports as a collision.)

use std::collections::{BTreeSet, HashMap, HashSet};

use specforge_common::{Diagnostic, Severity, codes};
use specforge_diagnostics::{Level, check_extension_code};
use specforge_protocol_types::{
    CheckKind, ConstraintKind, ExtensionDeclaration, ValidationRuleDescriptor, ValidationSeverity,
};

use super::check::{Check, EdgeScope, Obligations, ValueConstraint};
use super::{Declared, DeclaredConstraint, Origin, Registries, Rule, Rules};

/// The statement that declares an entity's obligations.
const VERIFY_FIELD: &str = "verify";

pub(super) fn build(
    declarations: &[ExtensionDeclaration],
    registries: Registries<'_>,
) -> (Rules, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let mut rules = Vec::new();
    for declaration in declarations {
        let extension = declaration.name();
        // The (code, severity) pairs already reported as W150: a code an
        // extension declares for several target kinds is reported once.
        let mut misused = HashSet::new();
        for descriptor in &declaration.validation_rules {
            diagnostics.extend(code_misuse(extension, descriptor, &mut misused));
            let built = shape(descriptor, extension).and_then(|mut rule| {
                let unread = ignore_unread(descriptor, &mut rule);
                Ok((resolve(rule, registries)?, unread))
            });
            match built {
                Ok((rule, unread)) => {
                    diagnostics.extend(unread);
                    rules.extend(rule);
                }
                Err(w112) => diagnostics.push(w112),
            }
        }
    }
    // Code order (stable: a code declared for several kinds keeps its
    // declaration order).
    rules.sort_by(|a: &Rule, b: &Rule| a.code.cmp(&b.code));
    rules.extend(required_field_rules(registries));
    (Rules { rules }, diagnostics)
}

/// W150 when `extension` may not report `descriptor`'s code at its
/// severity (`check_extension_code`), once per (code, severity) of an
/// extension. The rule is registered as declared.
fn code_misuse(
    extension: &str,
    descriptor: &ValidationRuleDescriptor,
    reported: &mut HashSet<(String, Level)>,
) -> Option<Diagnostic> {
    let level = match descriptor.severity {
        ValidationSeverity::Error => Level::Error,
        ValidationSeverity::Warning => Level::Warning,
        ValidationSeverity::Info => Level::Info,
    };
    let misuse = check_extension_code(extension, &descriptor.code, level).err()?;
    if !reported.insert((descriptor.code.clone(), level)) {
        return None;
    }
    Some(
        Diagnostic::new(
            codes::W150,
            format!(
                "extension '{extension}': rule '{}' uses {misuse}",
                descriptor.code
            ),
        )
        .with_suggestion(
            "renumber the rule in the extension's range, or, for a first-party extension, \
             catalogue the code",
        ),
    )
}

/// W112 for `extension`'s rule `code`: it cannot work as declared (`why`),
/// so it is not registered.
fn cannot_fire(extension: &str, code: &str, why: &str) -> Diagnostic {
    Diagnostic::new(
        codes::W112,
        format!(
            "extension '{extension}': rule '{code}': {why} — the rule can never fire and was not registered"
        ),
    )
}

/// W147 for each property `descriptor` sets that `rule`'s check does not
/// read, which `rule` is then registered without: an `edge_type` on a
/// check that counts no edges, a `constraint` on one that reads none, a
/// `wasm_function` on a declarative check, a constraint `pattern` or
/// `values` its check does not read, or a constraint kind other than the
/// one its check reads (read as that one). `field` is never one: every
/// check's message reads it.
fn ignore_unread(descriptor: &ValidationRuleDescriptor, rule: &mut Rule) -> Vec<Diagnostic> {
    let check = rule.check_kind;
    let head = format!("extension '{}': rule '{}'", rule.origin.name(), rule.code);
    let unread = |property: &str| {
        Diagnostic::new(
            codes::W147,
            format!(
                "{head}: {property} is not read by check '{check}' — the rule was registered without it"
            ),
        )
    };
    let mut diagnostics = Vec::new();
    let declared = &mut rule.declared;

    let reads_edge_type = matches!(
        check,
        CheckKind::NoIncomingEdges | CheckKind::NoOutgoingEdges | CheckKind::CycleDetection
    );
    let reads_constraint = matches!(
        check,
        CheckKind::FieldValueConstraint
            | CheckKind::ConditionalFieldRequired
            | CheckKind::VerifyKindAllowlist
    );
    if !reads_edge_type && declared.edge_type.take().is_some() {
        diagnostics.push(unread("edge_type"));
    }
    if !reads_constraint && declared.constraint.take().is_some() {
        diagnostics.push(unread("constraint"));
    }
    if check != CheckKind::Custom && declared.wasm_function.take().is_some() {
        diagnostics.push(unread("wasm_function"));
    }

    let Some(constraint) = declared.constraint.as_mut() else {
        return diagnostics;
    };
    // The constraint kind the check reads, when it reads only one.
    let expected = match check {
        CheckKind::ConditionalFieldRequired => Some(ConstraintKind::WhenFieldEquals),
        CheckKind::VerifyKindAllowlist => Some(ConstraintKind::OneOf),
        _ => None,
    };
    if let Some(expected) = expected
        && constraint.kind != Some(expected)
    {
        let written = descriptor
            .constraint
            .as_ref()
            .map(|c| c.kind.as_str())
            .unwrap_or_default();
        diagnostics.push(Diagnostic::new(
            codes::W147,
            format!("{head}: constraint kind '{written}' is not read by check '{check}' (it reads {expected}) — read as {expected}"),
        ));
        constraint.kind = Some(expected);
    }
    // `pattern` and `values` as the (now expected) constraint kind reads them.
    let (reads_pattern, reads_values) = match (check, constraint.kind) {
        (CheckKind::FieldValueConstraint, Some(ConstraintKind::NonEmpty)) => (false, false),
        (CheckKind::FieldValueConstraint, Some(ConstraintKind::OneOf)) => (false, true),
        (CheckKind::FieldValueConstraint, Some(ConstraintKind::Matches)) => (true, false),
        (CheckKind::ConditionalFieldRequired, _) => (true, true),
        (CheckKind::VerifyKindAllowlist, _) => (false, true),
        _ => (true, true),
    };
    if !reads_pattern && constraint.pattern.take().is_some() {
        diagnostics.push(unread("constraint.pattern"));
    }
    if !reads_values && !constraint.values.is_empty() {
        constraint.values.clear();
        diagnostics.push(unread("constraint.values"));
    }
    diagnostics
}

/// The rule `descriptor` declares, with the check its kind names and the
/// properties that check reads; W112 when it cannot work as declared.
/// Edge rules are resolved afterwards ([`resolve`]).
#[allow(clippy::result_large_err)]
fn shape(descriptor: &ValidationRuleDescriptor, extension: &str) -> Result<Rule, Diagnostic> {
    let r = descriptor;
    let Some(check_kind) = CheckKind::parse(&r.check) else {
        return Err(Diagnostic::new(
            codes::W112,
            format!(
                "extension '{extension}': unrecognized validation pattern kind '{}'",
                r.check
            ),
        ));
    };
    let fail = |why: &str| cannot_fire(extension, &r.code, why);
    let requires_field = || {
        r.field.clone().ok_or_else(|| {
            fail(&format!(
                "check '{check_kind}' requires a field but none is set"
            ))
        })
    };

    let check = match check_kind {
        // Resolved against the edge registry by `resolve`.
        CheckKind::NoIncomingEdges => Check::NoIncomingEdges(EdgeScope::Any),
        CheckKind::NoOutgoingEdges => Check::NoOutgoingEdges(EdgeScope::Any),
        CheckKind::NoEdges => Check::NoEdges,
        CheckKind::MissingFieldWhenFlagSet => {
            let field = requires_field()?;
            Check::MissingField {
                skip_owing_nothing: field == VERIFY_FIELD,
                field,
            }
        }
        CheckKind::MissingRequiredField => Check::MissingField {
            field: requires_field()?,
            skip_owing_nothing: false,
        },
        CheckKind::FileExists => Check::FileExists {
            field: requires_field()?,
        },
        CheckKind::FieldValueConstraint => {
            let Some(c) = r.constraint.as_ref() else {
                return Err(fail(
                    "check 'field_value_constraint' requires a constraint but none is set",
                ));
            };
            let constraint = match ConstraintKind::parse(&c.kind) {
                Some(ConstraintKind::NonEmpty) => ValueConstraint::NonEmpty,
                Some(ConstraintKind::OneOf) if c.values.is_empty() => {
                    return Err(fail(
                        "one_of constraint has an empty values list — every field value would be flagged as a violation",
                    ));
                }
                Some(ConstraintKind::OneOf) => ValueConstraint::OneOf(c.values.clone()),
                Some(ConstraintKind::Matches) => {
                    let Some(pattern) = c.pattern.as_deref() else {
                        return Err(fail(
                            "matches constraint has no pattern — no value can ever be checked",
                        ));
                    };
                    match regex::Regex::new(pattern) {
                        Ok(regex) => ValueConstraint::Matches(regex),
                        Err(err) => {
                            return Err(Diagnostic::new(
                                codes::W112,
                                format!(
                                    "extension '{extension}': rule '{}': invalid regex pattern '{pattern}': {err}",
                                    r.code
                                ),
                            ));
                        }
                    }
                }
                Some(ConstraintKind::WhenFieldEquals) | None => {
                    return Err(fail(&format!(
                        "unknown constraint kind '{}' for check 'field_value_constraint' (expected non_empty, one_of, or matches)",
                        c.kind
                    )));
                }
            };
            Check::FieldValue {
                field: requires_field()?,
                constraint,
            }
        }
        CheckKind::ConditionalFieldRequired => {
            let Some(c) = r.constraint.as_ref() else {
                return Err(fail(
                    "check 'conditional_field_required' requires a constraint but none is set",
                ));
            };
            let Some(when_field) = c.pattern.clone() else {
                return Err(fail(
                    "conditional_field_required requires constraint.pattern (the condition field) — without it the condition can never be met",
                ));
            };
            if c.values.is_empty() {
                return Err(fail(
                    "conditional_field_required has an empty condition values list — the condition can never be met",
                ));
            }
            Check::ConditionalFieldRequired {
                field: requires_field()?,
                when_field,
                equals: c.values.clone(),
            }
        }
        // Resolved against the field registry by `resolve`. Every edge
        // already makes a reference cycle (W061): without an edge type the
        // rule has nothing of its own to check.
        CheckKind::CycleDetection if r.edge_type.is_none() => {
            return Err(fail(
                "check 'cycle_detection' requires an edge_type but none is set",
            ));
        }
        CheckKind::CycleDetection => Check::Cycle { labels: Vec::new() },
        CheckKind::VerifyKindAllowlist => match r.constraint.as_ref() {
            Some(c) if !c.values.is_empty() => Check::VerifyKindAllowlist {
                allowed: c.values.clone(),
            },
            _ => {
                return Err(fail(
                    "verify_kind_allowlist requires a constraint with values — every verify kind would be flagged",
                ));
            }
        },
        CheckKind::NoVerifyStatements => Check::NoVerifyStatements {
            obligations: match r.field.as_deref().unwrap_or(VERIFY_FIELD) {
                VERIFY_FIELD => Obligations::Statements,
                field => Obligations::Field(field.to_string()),
            },
        },
        CheckKind::Custom => Check::Custom {
            extension: extension.to_string(),
            function: r
                .wasm_function
                .clone()
                .ok_or_else(|| fail("check 'custom' requires a wasm_function but none is set"))?,
        },
    };

    Ok(Rule {
        code: r.code.clone(),
        severity: match r.severity {
            ValidationSeverity::Error => Severity::Error,
            ValidationSeverity::Warning => Severity::Warning,
            ValidationSeverity::Info => Severity::Info,
        },
        template: r.message_template.clone(),
        target: r.target_kind.clone(),
        message_field: r.field.clone(),
        origin: Origin::Extension(extension.to_string()),
        check_kind,
        check,
        declared: Declared {
            edge_type: r.edge_type.clone(),
            edge_fields: Vec::new(),
            target_extension: r.target_extension.clone(),
            constraint: r.constraint.as_ref().map(|c| DeclaredConstraint {
                kind: ConstraintKind::parse(&c.kind),
                pattern: c.pattern.clone(),
                values: c.values.clone(),
            }),
            wasm_function: r.wasm_function.clone(),
        },
    })
}

/// `rule` resolved against the registries; `None` when it cannot apply to
/// this project, W112 when it can never fire.
///
/// A rule whose target kind no loaded extension declares (and that is not
/// structural) is dropped (inert), as ADR 0020 D5 says: it belongs to an
/// optional peer that is not installed, and its entities are E024.
///
/// A rule that reads `verify` statements (an allowlist, or an obligation
/// rule whose obligations are statements) on a declared kind that accepts
/// none can neither be satisfied nor violated: W112.
///
/// An edge rule (`no_*_edges` with an edge type) counts only edges to the
/// edge type's target kind (from its source kind for `no_incoming_edges`):
/// a `no_outgoing_edges` rule on `BehaviorImplementsFeature` asks whether a
/// behavior implements a feature, not whether it references anything. When
/// no loaded extension declares that kind, the edge can't exist in the
/// project and the rule is dropped. A cycle rule follows every field that
/// writes its edge type. An edge or cycle rule whose edge type no loaded
/// extension declares is dropped (inert).
#[allow(clippy::result_large_err)]
fn resolve(mut rule: Rule, registries: Registries<'_>) -> Result<Option<Rule>, Diagnostic> {
    if let Some(kind) = rule.target.as_deref()
        && !registries.kinds.contains(kind)
        && !specforge_common::structural::is_structural(kind)
    {
        return Ok(None);
    }
    let reads_statements = matches!(
        rule.check,
        Check::VerifyKindAllowlist { .. }
            | Check::NoVerifyStatements {
                obligations: Obligations::Statements
            }
    );
    if reads_statements
        && let Some(kind) = rule.target.as_deref()
        && registries
            .kinds
            .get(kind)
            .is_some_and(|entry| !entry.supports_verify)
    {
        return Err(cannot_fire(
            rule.origin.name(),
            &rule.code,
            &format!(
                "check '{}' reads verify statements, which kind '{kind}' does not accept",
                rule.check_kind
            ),
        ));
    }
    let edge_rule = matches!(
        rule.check,
        Check::NoIncomingEdges(_) | Check::NoOutgoingEdges(_) | Check::Cycle { .. }
    );
    let Some(edge_type) = rule.declared.edge_type.clone().filter(|_| edge_rule) else {
        return Ok(Some(rule));
    };
    // An edge type resolves through the edge registry only, never as a
    // field name: one no loaded extension declares belongs to an optional
    // peer that is not installed, and the rule is inert (W021 tells the
    // author when neither the extension nor its peers declare it).
    let Some(edge) = registries.edges.get(&edge_type) else {
        return Ok(None);
    };
    // The fields an edge of this type is written as (its graph labels).
    let writing: Vec<String> = registries
        .fields
        .iter()
        .filter(|(_, _, entry)| entry.declared().edge.as_deref() == Some(&edge_type))
        .map(|(_, field, _)| field.to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    rule.declared.edge_fields = writing.clone();
    match &mut rule.check {
        Check::NoIncomingEdges(scope) | Check::NoOutgoingEdges(scope) => {
            // The far end: its declared kind, else every kind that writes
            // it without declaring it. Open: every edge counts.
            let far_end = if rule.check_kind == CheckKind::NoIncomingEdges {
                edge.source_kinds()
            } else {
                edge.target_kinds()
            };
            if !far_end.is_empty() {
                let loaded: Vec<String> = far_end
                    .into_iter()
                    .filter(|kind| registries.kinds.contains(kind))
                    .map(str::to_string)
                    .collect();
                // No loaded kind at its far end: the rule can't be met.
                if loaded.is_empty() {
                    return Ok(None);
                }
                *scope = EdgeScope::Peers(loaded);
            }
        }
        Check::Cycle { labels } => *labels = writing,
        _ => {}
    }
    Ok(Some(rule))
}

/// One E006 rule per `required` field, owned by the host, by (kind, field).
fn required_field_rules(registries: Registries<'_>) -> Vec<Rule> {
    let mut required: Vec<(String, String)> = registries
        .fields
        .iter()
        .filter(|(_, _, entry)| entry.declared().required)
        .map(|(kind, field, _)| (kind.to_string(), field.to_string()))
        .collect();
    required.sort();
    required
        .into_iter()
        .map(|(kind, field)| Rule {
            code: codes::E006.id().to_string(),
            severity: Severity::of(codes::E006),
            template: format!("{kind} '{{id}}' is missing required field '{field}'"),
            target: Some(kind),
            message_field: Some(field.clone()),
            origin: Origin::Host,
            check_kind: CheckKind::MissingRequiredField,
            check: Check::MissingField {
                field,
                skip_owing_nothing: false,
            },
            declared: Declared::default(),
        })
        .collect()
}

/// W023: a validation rule code a later extension declares again, once
/// per repeat, naming the code, that extension and the first one. A code
/// one extension repeats is not reported. Every declared rule counts,
/// including one W112 rejects.
pub(super) fn duplicate_codes(declarations: &[ExtensionDeclaration]) -> Vec<Diagnostic> {
    let mut first: HashMap<&str, &str> = HashMap::new();
    let mut diagnostics = Vec::new();
    for declaration in declarations {
        for rule in &declaration.validation_rules {
            match first.get(rule.code.as_str()) {
                None => {
                    first.insert(&rule.code, declaration.name());
                }
                Some(owner) if *owner != declaration.name() => {
                    diagnostics.push(Diagnostic::new(
                        codes::W023,
                        format!(
                            "validation rule code '{}' from '{}' duplicates code from '{}'",
                            rule.code,
                            declaration.name(),
                            owner
                        ),
                    ));
                }
                Some(_) => {}
            }
        }
    }
    diagnostics
}
