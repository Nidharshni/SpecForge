//! @specforge/testing — the runner-agnostic test vocabulary (ADR 0002).
//!
//! Which kinds accept `verify` obligations, which obligation kinds each
//! allows, and the rules over them (W004 untested testable entity, W009
//! verify kind outside the allowlist) live here rather than in the extensions
//! that own the kinds. Every testable kind is contributed as an enhancement;
//! each kind's extension is an optional peer, so a kind whose extension a
//! project doesn't use is skipped silently. Test runners (`@specforge/cargo-test`,
//! `@specforge/vitest`, …) build on this vocabulary.
//!
//! The `coverage` pass scores it: intent (verify obligations on testable
//! entities), enforcement (invariants something references), and proof
//! (recorded test results, or formal claims the prove pass entailed). The
//! rule it applies is the `specforge-coverage` crate, which the host links
//! too (ADR 0004, D2-f).

use specforge_coverage as coverage;
use specforge_extension_sdk::prelude::*;
use std::collections::BTreeSet;

/// A kind that accepts `verify` obligations.
struct Testable {
    kind: &'static str,
    /// Extension that owns the kind.
    owner: &'static str,
    /// Obligation kinds `verify <kind> "..."` may use (W009).
    verify_kinds: &'static [&'static str],
    /// Warn (W004) when an entity of this kind declares no obligations.
    requires_obligations: bool,
    /// The kind is risk-graded by the coverage rule: the field holding an
    /// entity's risk, and the risk at which one without obligations is an
    /// A002 error (ADR 0009, B).
    risk: Option<Risk>,
}

/// Where a graded kind's risk lives and when its omission is an error.
struct Risk {
    field: &'static str,
    error_at: &'static str,
}

/// The reserved statement whose meaning this extension supplies.
const VERIFY_FIELD: &str = "verify";

const SOFTWARE: &str = "@specforge/software";
const GOVERNANCE: &str = "@specforge/governance";

const TESTABLE: &[Testable] = &[
    Testable {
        kind: "behavior",
        owner: SOFTWARE,
        verify_kinds: &["unit", "contract", "integration", "property", "performance"],
        requires_obligations: true,
        risk: None,
    },
    Testable {
        kind: "invariant",
        owner: SOFTWARE,
        verify_kinds: &["unit", "integration", "property", "performance", "mutation"],
        requires_obligations: true,
        risk: Some(Risk {
            field: "risk",
            error_at: "high",
        }),
    },
    Testable {
        kind: "event",
        owner: SOFTWARE,
        verify_kinds: &["integration", "unit", "deadlock_free", "liveness"],
        requires_obligations: true,
        risk: None,
    },
    Testable {
        kind: "type",
        owner: SOFTWARE,
        verify_kinds: &["unit", "property"],
        requires_obligations: true,
        risk: None,
    },
    Testable {
        kind: "port",
        owner: SOFTWARE,
        verify_kinds: &["integration", "unit"],
        requires_obligations: true,
        risk: None,
    },
    Testable {
        kind: "constraint",
        owner: GOVERNANCE,
        verify_kinds: &["unit", "integration", "property", "load", "contract"],
        requires_obligations: false,
        risk: None,
    },
    Testable {
        kind: "failure_mode",
        owner: GOVERNANCE,
        verify_kinds: &["unit", "integration", "property"],
        requires_obligations: false,
        risk: None,
    },
];

#[specforge_extension_sdk::extension(
    name = "@specforge/testing",
    version = "1.0.0",
    description = "The runner-agnostic test vocabulary: which kinds owe verify obligations, the rules over them and the coverage pass"
)]
struct Testing;

impl Contributions for Testing {
    fn contribute(c: &mut ContributionsBuilder) {
        // Each kind's extension is an optional peer: the host skips the
        // enhancement of a kind no loaded extension declares, silently.
        let mut owners: Vec<&str> = Vec::new();
        for t in TESTABLE {
            if !owners.contains(&t.owner) {
                owners.push(t.owner);
                c.optional_peer(t.owner, "^1.0");
            }
        }

        for t in TESTABLE {
            c.enhance(t.kind, |e| {
                e.verify_kinds(t.verify_kinds);
            });
            if t.requires_obligations {
                c.rule("W004", |r| {
                    r.check(CheckKind::NoVerifyStatements)
                        .target_kind(t.kind)
                        .field(VERIFY_FIELD)
                        .severity(ValidationSeverity::Warning)
                        .message_template(
                            "{kind} '{id}' is testable but declares no verify obligations",
                        );
                });
            }
            c.rule("W009", |r| {
                r.check(CheckKind::VerifyKindAllowlist)
                    .target_kind(t.kind)
                    .severity(ValidationSeverity::Warning)
                    .message_template(
                        "entity '{id}' has verify kind '{value}' not in allowed set {allowed}",
                    )
                    .constraint(|k| {
                        k.kind(ConstraintKind::OneOf).values(t.verify_kinds);
                    });
            });
        }

        c.pass("coverage", |p| {
            p.after("resolve").run(pass_coverage);
        });
    }
}

/// `coverage` — proof obligations, enforcement, and discharge per entity.
///
/// - A001: testable entity with no verify obligations (no intent)
/// - A002: a risk-graded entity (an invariant) with no verify obligations
///   (an error when high-risk); the grading is [`TESTABLE`]'s
/// - A014: an entity's recorded tests include failures
/// - A015: obligations no passing test names (with recorded results)
/// - A016: tests name obligations the entity doesn't declare
///
/// The rule itself is `specforge-coverage` (ADR 0004, D2-f), which the host
/// links too; this pass only adapts the pass input to it and its findings
/// to pass diagnostics. Whether an entity is exempt is the host's call,
/// read from the snapshot (`PassEntity::owes_none`).
fn pass_coverage(input: &PassInput) -> PassOutput {
    // The one kind TESTABLE grades by risk.
    let graded = TESTABLE
        .iter()
        .find_map(|t| t.risk.as_ref().map(|r| (t.kind, r)));
    let grading = graded.map(|(kind, risk)| coverage::RiskGrading {
        kind: kind.to_string(),
        error_at: risk.error_at.to_string(),
    });
    let entities: Vec<coverage::Entity> = input
        .entities
        .iter()
        .map(|e| coverage::Entity {
            risk: graded
                .filter(|(kind, _)| *kind == e.kind)
                .and_then(|(_, risk)| e.fields.get(risk.field).cloned()),
            ..coverage::Entity::from(e)
        })
        .collect();
    let proved: Option<BTreeSet<String>> = input
        .proved_claims
        .as_ref()
        .map(|ids| ids.iter().cloned().collect());

    let assessment = coverage::assess(
        &entities,
        input.test_results.as_ref(),
        proved.as_ref(),
        grading.as_ref(),
    );
    let diagnostics = assessment
        .findings
        .into_iter()
        .map(|finding| {
            let severity = match finding.severity {
                coverage::Severity::Error => PassSeverity::Error,
                coverage::Severity::Warning => PassSeverity::Warning,
            };
            let mut diagnostic = PassDiagnostic::new(finding.code, severity, finding.message);
            if let Some(suggestion) = finding.suggestion {
                diagnostic = diagnostic.with_suggestion(suggestion);
            }
            match &input.entities[finding.entity].span {
                Some(span) => diagnostic.with_span(span.clone()),
                None => diagnostic,
            }
        })
        .collect();
    PassOutput {
        diagnostics,
        summary: match serde_json::to_value(&assessment.summary) {
            Ok(serde_json::Value::Object(summary)) => summary,
            _ => serde_json::Map::new(),
        },
    }
}

specforge_extension_sdk::component_guest!(build = specforge_extension_build);

#[cfg(test)]
mod tests;
