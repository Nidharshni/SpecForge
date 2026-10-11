//! The coverage rule of `@specforge/testing` (ADR 0004, D2-f): which of an
//! entity's `verify` obligations its recorded tests prove, whether the entity
//! is proven, and the project summary the `coverage` pass reports.
//!
//! It is one pure library, owned by the testing extension and linked twice:
//! the extension's Wasm `coverage` pass (the authority for the A-codes and
//! the `--min` gate) calls [`assess`], and host surfaces link the same crate
//! for per-entity views ([`Verdict::of`], [`Verdict::status`]). Neither side
//! re-implements the rule, and both are held to the golden vectors in
//! `tests/fixtures/coverage-cases.json`.
//!
//! It reads a pass's entities through `Entity::from(&PassEntity)`, the one
//! reading the host and the extensions share, and it states the counting rule
//! once ([`Counting`]).
//!
//! It depends on serde and the guest wire's `PassEntity` only, so it builds
//! for `wasm32-wasip2`. Keep these types out of the extension SDK and
//! protocol-types: every builtin blob is built from those, and a change there
//! makes all of them stale. It reads them; they never read it.

use serde::{Deserialize, Serialize};
use specforge_protocol_types::PassEntity;
use std::collections::{BTreeMap, BTreeSet};

/// How a recorded test's status names a passing test.
pub const PASSING_STATUS: &str = "pass";
/// The verify kind a formal claim the prove pass entailed discharges.
pub const PROPERTY_VERIFY_KIND: &str = "property";
/// The summary's name for obligations written without a kind (`verify "..."`).
const UNTYPED_OBLIGATION: &str = "untyped";
/// A graded entity's risk when it declares none.
const UNSPECIFIED_RISK: &str = "unspecified";

/// The coverage owner's risk policy for one kind (ADR 0009, B): its
/// entities' risk is tallied, those nothing references are counted as
/// orphans, and one with no obligations is A002, an error at `error_at`
/// and a warning otherwise. `@specforge/testing` supplies it; without it
/// no kind is graded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskGrading {
    /// The graded kind.
    pub kind: String,
    /// The risk at which one of its entities without obligations is an error.
    pub error_at: String,
}

/// Whether an entity counts toward coverage, from the three facts the rule
/// reads (ADR 0004 D2-b, ADR 0019): the one statement of the counting rule.
/// The host's `Standing` and a pass's [`Entity`] both answer through it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counting {
    /// Its kind is testable (its registry entry says so).
    pub testable: bool,
    /// It owes obligations of its own: a `no_verify_statements` rule
    /// applies to its kind and nothing exempts it.
    pub owes_obligations: bool,
    /// How many obligations (`verify` statements) it declares.
    pub declared: usize,
}

impl Counting {
    /// It counts toward coverage (the testable totals and the `--min`
    /// gate's denominator): testable, and it owes obligations or declares
    /// some.
    pub fn counts(self) -> bool {
        self.testable && (self.owes_obligations || self.declared > 0)
    }

    /// Testable, yet it owes none and declares none, so it does not count:
    /// the summary's `testable_exempt`, the coverage view's and inspect's
    /// `exempt`. Not [`Entity::exempt`], which says only that it owes none.
    pub fn exempt(self) -> bool {
        self.testable && !self.counts()
    }
}

/// An entity as the rule sees it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub kind: String,
    /// The kind counts toward coverage (its registry entry is `testable`).
    #[serde(default)]
    pub testable: bool,
    /// The entity owes no obligations of its own (ADR 0004, D2-b): what
    /// W004 exempts, a union type, an `abstract true` entity, or one of a
    /// kind no rule requires obligations of (governance). Decided by the
    /// host from the registry. An exempt entity that declares none is
    /// left out of the testable count and is not A001; one that declares
    /// some counts like any other. This is "owes none", not
    /// [`Counting::exempt`]: an entity that owes none and declares some
    /// still counts.
    #[serde(default)]
    pub exempt: bool,
    /// One entry per `verify` statement, in order: its kind, or `""` for a
    /// bare `verify "..."`.
    #[serde(default)]
    pub verify_kinds: Vec<String>,
    /// The obligations' texts, parallel to `verify_kinds`.
    #[serde(default)]
    pub verify_texts: Vec<String>,
    /// A graded entity's declared risk, if any: filled by the caller for
    /// the kind its [`RiskGrading`] names.
    #[serde(default)]
    pub risk: Option<String>,
    /// Something references the entity. A graded entity nothing references is
    /// unreferenced: counted in the summary, reported by software's W003.
    #[serde(default)]
    pub referenced: bool,
}

impl Entity {
    /// How many obligations (`verify` statements) the entity declares.
    pub fn obligations(&self) -> usize {
        self.verify_texts.len()
    }

    /// The facts the counting rule reads.
    pub fn counting(&self) -> Counting {
        Counting {
            testable: self.testable,
            owes_obligations: !self.exempt,
            declared: self.obligations(),
        }
    }

    /// Whether the entity counts toward coverage ([`Counting::counts`]).
    pub fn counts_toward_coverage(&self) -> bool {
        self.counting().counts()
    }
}

impl From<&PassEntity> for Entity {
    /// A pass's entity as the rule reads it. The host's per-entity views and
    /// every extension's pass share this one reading:
    /// - its testable flag, `exempt` (it owes none) and obligations, as the
    ///   host decided them;
    /// - referenced when any edge comes in;
    /// - `risk` unset: a caller that grades a kind fills it ([`RiskGrading`]).
    fn from(entity: &PassEntity) -> Self {
        Entity {
            id: entity.id.clone(),
            kind: entity.kind.clone(),
            testable: entity.testable,
            exempt: entity.exempt,
            verify_kinds: entity.verify_kinds.clone(),
            verify_texts: entity.verify_texts.clone(),
            risk: None,
            referenced: entity.incoming_edge_count > 0,
        }
    }
}

/// One recorded test for an entity (a `specforge-report.json` entry).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedTest {
    #[serde(default)]
    pub name: Option<String>,
    pub status: String,
    /// The obligation text the test names, when it names one.
    #[serde(default)]
    pub verify: Option<String>,
}

impl RecordedTest {
    pub fn passed(&self) -> bool {
        self.status == PASSING_STATUS
    }
}

/// The project's recorded tests, per entity id.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestResults {
    #[serde(default)]
    pub runner: Option<String>,
    #[serde(default)]
    pub entities: BTreeMap<String, Vec<RecordedTest>>,
}

/// An entity's coverage at a glance, for per-entity views.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// At least one obligation, every one proven, and no test fails.
    Covered,
    /// Some obligation is proven, or a test fails.
    Partial,
    /// Nothing proven, including an entity with no obligations.
    Uncovered,
}

/// How an entity's recorded tests (and formal proofs) cover its obligations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    /// Obligations the entity declares.
    pub obligations: usize,
    /// Obligations a passing test names, or a formal proof discharges.
    pub proven: usize,
    /// Obligation texts nothing proves, in declaration order (A015).
    pub unproven: Vec<String>,
    /// Texts tests name that the entity doesn't declare, sorted (A016).
    pub undeclared: Vec<String>,
    /// Recorded tests for the entity.
    pub tests: usize,
    /// Recorded tests that did not pass (A014).
    pub failing: usize,
}

impl Verdict {
    /// The proof rule. A test proves an obligation by naming its text; a
    /// test that names none counts toward the entity but proves no
    /// particular obligation. When the entity's formal claims were entailed
    /// (`formally_proved`), its `verify property` obligations are
    /// discharged without a test.
    pub fn of(entity: &Entity, tests: &[RecordedTest], formally_proved: bool) -> Self {
        let passing: BTreeSet<&str> = tests
            .iter()
            .filter(|t| t.passed())
            .filter_map(|t| t.verify.as_deref())
            .collect();
        let declared: BTreeSet<&str> = entity.verify_texts.iter().map(String::as_str).collect();
        let mut proven = 0;
        let mut unproven = Vec::new();
        for (i, text) in entity.verify_texts.iter().enumerate() {
            let property =
                entity.verify_kinds.get(i).map(String::as_str) == Some(PROPERTY_VERIFY_KIND);
            if passing.contains(text.as_str()) || (formally_proved && property) {
                proven += 1;
            } else {
                unproven.push(text.clone());
            }
        }
        let undeclared: BTreeSet<&str> = tests
            .iter()
            .filter_map(|t| t.verify.as_deref())
            .filter(|text| !declared.contains(text))
            .collect();
        Verdict {
            obligations: entity.obligations(),
            proven,
            unproven,
            undeclared: undeclared.into_iter().map(str::to_string).collect(),
            tests: tests.len(),
            failing: tests.iter().filter(|t| !t.passed()).count(),
        }
    }

    /// Whether the entity counts as proven (the `--min` gate's numerator):
    /// it declares at least one obligation, every one is proven, and none
    /// of its recorded tests fails (ADR 0004, D2-a). A test that proves
    /// nothing declared is not proof, so an entity with no obligations is
    /// never proven; an entity whose obligations are all formally
    /// discharged is proven without a test.
    pub fn is_proven(&self) -> bool {
        self.obligations > 0 && self.unproven.is_empty() && self.failing == 0
    }

    /// The per-entity view, from the same facts as [`Verdict::is_proven`].
    pub fn status(&self) -> Status {
        if self.is_proven() {
            Status::Covered
        } else if self.proven > 0 || self.failing > 0 {
            Status::Partial
        } else {
            Status::Uncovered
        }
    }
}

/// A finding's severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

/// One coverage finding (A001, A002, A014, A015, A016).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub code: &'static str,
    pub severity: Severity,
    /// The entity it is about: an index into the slice given to [`assess`].
    pub entity: usize,
    pub message: String,
    pub suggestion: Option<String>,
}

/// The discharge funnel: from intent to proof.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Funnel {
    pub entities_with_obligations: usize,
    /// Entities [`Verdict::is_proven`] holds for (0 without recorded tests).
    pub entities_proven: usize,
    pub report_failures: usize,
    pub formally_discharged: usize,
}

/// What the recorded tests add up to; present only when there are some.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestResultsSummary {
    pub runner: Option<String>,
    /// Entities the report records tests for, whether or not the graph has them.
    pub entities_recorded: usize,
    pub tests_recorded: usize,
    pub tests_failed: usize,
    pub entities_proven: usize,
    pub obligations_proven: usize,
}

/// Invariants of one risk level, and how many declare no obligations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskTally {
    pub risk: String,
    pub total: usize,
    pub unverified: usize,
}

/// The `coverage` pass summary. Its serialized form is the pass's summary
/// JSON, which `specforge analyze` prints and the `--min` gate reads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    /// Entities that count toward coverage
    /// ([`Entity::counts_toward_coverage`]).
    pub testable_total: usize,
    /// Testable entities that declare at least one obligation.
    pub testable_verified: usize,
    /// Testable entities [`Verdict::is_proven`] holds for: the `--min`
    /// gate's numerator. (`discharge_funnel.entities_proven` counts proven
    /// entities of every kind.)
    pub testable_proven: usize,
    /// Entities of a testable kind left out of `testable_total` because
    /// they owe no obligations and declare none (unions, abstract
    /// entities, governance kinds), listed so the exclusion is visible.
    pub testable_exempt: usize,
    pub obligations: usize,
    /// Obligations per verify kind (`untyped` for a bare `verify`).
    pub obligation_kinds: BTreeMap<String, usize>,
    pub invariant_enforced: usize,
    pub invariant_unreferenced: usize,
    pub discharge_funnel: Funnel,
    pub test_results: Option<TestResultsSummary>,
    /// Per risk level, in risk order.
    pub invariants: Vec<RiskTally>,
}

impl Summary {
    /// The `--min` gate's percentage: proven testable entities over
    /// testable ones. Nothing testable satisfies any threshold (100%).
    pub fn proof_pct(&self) -> f64 {
        if self.testable_total == 0 {
            100.0
        } else {
            self.testable_proven as f64 * 100.0 / self.testable_total as f64
        }
    }
}

/// The whole project's coverage.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Assessment {
    /// Per entity id.
    pub verdicts: BTreeMap<String, Verdict>,
    pub summary: Summary,
    /// In entity id order; per entity A015, A016, A014, A001, A002.
    pub findings: Vec<Finding>,
}

/// Score a project at three layers.
///
/// - Intent: a testable entity with no obligations is A001; an invariant
///   of the graded kind with none is A002, an error at the grading's error
///   level.
/// - Enforcement: graded entities nothing references are counted, not
///   reported.
/// - Proof, only with recorded `results`: an obligation nothing proves is
///   A015, a test naming an obligation the entity doesn't declare is A016,
///   and a failing test is A014.
///
/// `proved` holds the entities whose formal claims the prove pass entailed
/// (`None` when it did not run). `grading` names the risk-graded kind;
/// without it nothing is tallied by risk and nothing is A002.
pub fn assess(
    entities: &[Entity],
    results: Option<&TestResults>,
    proved: Option<&BTreeSet<String>>,
    grading: Option<&RiskGrading>,
) -> Assessment {
    let mut findings = Vec::new();
    let mut verdicts = BTreeMap::new();
    let mut obligation_kinds: BTreeMap<String, usize> = BTreeMap::new();
    let (mut testable_total, mut testable_verified, mut testable_proven) = (0usize, 0usize, 0usize);
    let mut testable_exempt = 0usize;
    let mut funnel = Funnel::default();
    let (mut invariant_unreferenced, mut obligations_proven) = (0usize, 0usize);
    // risk -> (invariants, invariants without obligations)
    let mut invariants: BTreeMap<String, (usize, usize)> = BTreeMap::new();

    let mut order: Vec<usize> = (0..entities.len()).collect();
    order.sort_by(|&a, &b| entities[a].id.cmp(&entities[b].id));
    for index in order {
        let entity = &entities[index];
        let (kind, id) = (entity.kind.as_str(), entity.id.as_str());
        let obligations = entity.obligations();
        let mut finding = |code, severity, message: String, suggestion: Option<&str>| {
            findings.push(Finding {
                code,
                severity,
                entity: index,
                message,
                suggestion: suggestion.map(str::to_string),
            });
        };

        for verify_kind in &entity.verify_kinds {
            let key = if verify_kind.is_empty() {
                UNTYPED_OBLIGATION
            } else {
                verify_kind
            };
            *obligation_kinds.entry(key.to_string()).or_default() += 1;
        }
        if obligations > 0 {
            funnel.entities_with_obligations += 1;
        }

        let tests: &[RecordedTest] = results
            .and_then(|r| r.entities.get(id))
            .map_or(&[], Vec::as_slice);
        let formally_proved = proved.is_some_and(|ids| ids.contains(id));
        let verdict = Verdict::of(entity, tests, formally_proved);
        if results.is_some() {
            obligations_proven += verdict.proven;
            if !verdict.unproven.is_empty() {
                finding(
                    "A015",
                    Severity::Warning,
                    format!(
                        "{kind} '{id}' has {} obligation(s) no passing test proves: {}",
                        verdict.unproven.len(),
                        quoted(&verdict.unproven)
                    ),
                    Some(
                        "link a test to each obligation by its text (`verify = \"...\"` in the test's annotation)",
                    ),
                );
            }
            if !verdict.undeclared.is_empty() {
                finding(
                    "A016",
                    Severity::Warning,
                    format!(
                        "tests name obligation(s) {kind} '{id}' does not declare: {}",
                        quoted(&verdict.undeclared)
                    ),
                    Some(
                        "fix the test's `verify` text to match the spec's statement exactly, or add the statement",
                    ),
                );
            }
            if verdict.is_proven() {
                funnel.entities_proven += 1;
                if entity.counts_toward_coverage() {
                    testable_proven += 1;
                }
            }
        }
        let failed: Vec<&str> = tests
            .iter()
            .filter(|t| !t.passed())
            .map(|t| t.name.as_deref().unwrap_or("<unnamed>"))
            .collect();
        if !failed.is_empty() {
            funnel.report_failures += failed.len();
            finding(
                "A014",
                Severity::Error,
                format!(
                    "{kind} '{id}' has {} failing test(s) in the test results: {}",
                    failed.len(),
                    failed.join(", ")
                ),
                None,
            );
        }

        if formally_proved
            && entity
                .verify_kinds
                .iter()
                .any(|k| k == PROPERTY_VERIFY_KIND)
        {
            funnel.formally_discharged += 1;
        }

        let counting = entity.counting();
        if counting.exempt() {
            testable_exempt += 1;
        } else if counting.counts() {
            testable_total += 1;
            if obligations == 0 {
                finding(
                    "A001",
                    Severity::Warning,
                    format!("{kind} '{id}' declares no verify obligations"),
                    Some("add a `verify unit` or `verify property` statement"),
                );
            } else {
                testable_verified += 1;
            }
        }

        if let Some(grading) = grading
            && kind == grading.kind
        {
            let risk = entity.risk.as_deref().unwrap_or(UNSPECIFIED_RISK);
            let error_level = risk == grading.error_at;
            let tally = invariants.entry(risk.to_string()).or_insert((0, 0));
            tally.0 += 1;
            if !entity.referenced {
                invariant_unreferenced += 1;
            }
            if obligations == 0 {
                tally.1 += 1;
                let (severity, suggestion) = if error_level {
                    (
                        Severity::Error,
                        format!(
                            "{}-risk {kind}: add at least one `verify property` obligation",
                            grading.error_at
                        ),
                    )
                } else {
                    (
                        Severity::Warning,
                        "add a `verify property` or `verify unit` obligation".to_string(),
                    )
                };
                finding(
                    "A002",
                    severity,
                    format!("{kind} '{id}' declares no verify obligations"),
                    Some(suggestion.as_str()),
                );
            }
        }

        verdicts.insert(entity.id.clone(), verdict);
    }

    let invariant_total: usize = invariants.values().map(|(total, _)| total).sum();
    let test_results = results.map(|report| {
        let tests = report.entities.values().flatten();
        TestResultsSummary {
            runner: report.runner.clone(),
            entities_recorded: report.entities.len(),
            tests_recorded: tests.clone().count(),
            tests_failed: tests.filter(|t| !t.passed()).count(),
            entities_proven: funnel.entities_proven,
            obligations_proven,
        }
    });
    let summary = Summary {
        testable_total,
        testable_verified,
        testable_proven,
        testable_exempt,
        obligations: obligation_kinds.values().sum(),
        obligation_kinds,
        invariant_enforced: invariant_total - invariant_unreferenced,
        invariant_unreferenced,
        discharge_funnel: funnel,
        test_results,
        invariants: invariants
            .into_iter()
            .map(|(risk, (total, unverified))| RiskTally {
                risk,
                total,
                unverified,
            })
            .collect(),
    };
    Assessment {
        verdicts,
        summary,
        findings,
    }
}

fn quoted(texts: &[String]) -> String {
    texts
        .iter()
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests;
