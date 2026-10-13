//! Lifecycle consistency: what the product kinds' statuses claim across
//! entities, and what the recorded tests prove of a feature.
//!
//! The declarative rules check one entity at a time (`W057`, `I059`, …).
//! What spans entities is checked here, by two compiler passes over the
//! snapshot the host hands them:
//!
//! - `lifecycle` (check phase: every compile, watch, the LSP, MCP) reads
//!   only what the specs declare:
//!   - W154: a completed milestone delivers a feature that is not done;
//!   - I063: a done feature depends on a feature that is not done;
//!   - I064: a milestone is due before a milestone it depends on;
//!   - I065: a shipped deliverable is tracked by a milestone not completed.
//! - `delivery_evidence` (`specforge analyze`, which reads the recorded
//!   test report): I071, a done feature the recorded tests do not prove
//!   (ADR 0039; [`crate::evidence`]).
//!
//! A kind's absent status is its vocabulary's (`vocabulary::lifecycle`), as
//! the queries read it. A deprecated feature in a completed milestone is no contradiction:
//! it was delivered, then retired.

use crate::evidence::{FeatureEvidence, IMPLEMENTER_KIND, IMPLEMENTS_FIELD};
use crate::vocabulary::{
    self, DELIVERABLE_SHIPPED, FEATURE_DEPRECATED, FEATURE_DONE, MILESTONE_COMPLETED,
};
use specforge_coverage as coverage;
use specforge_extension_sdk::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

pub fn declare(c: &mut ContributionsBuilder) {
    c.pass("lifecycle", |p| {
        p.after("resolve").phase("check").run(pass_lifecycle);
    });
    c.pass("delivery_evidence", |p| {
        p.after("resolve").run(pass_delivery_evidence);
    });
}

/// The snapshot indexed by id, with each entity's references by field.
struct Snapshot<'a> {
    entities: BTreeMap<&'a str, &'a PassEntity>,
    edges: &'a [PassEdge],
}

impl<'a> Snapshot<'a> {
    fn of(input: &'a PassInput) -> Self {
        Snapshot {
            entities: input.entities.iter().map(|e| (e.id.as_str(), e)).collect(),
            edges: &input.edges,
        }
    }

    fn kind(&self, id: &str) -> Option<&'a str> {
        self.entities.get(id).map(|e| e.kind.as_str())
    }

    /// The entities of `kind`, in id order.
    fn of_kind(&self, kind: &'a str) -> impl Iterator<Item = &'a PassEntity> + '_ {
        self.entities
            .values()
            .copied()
            .filter(move |e| e.kind == kind)
    }

    /// `entity`'s status: as written, else its kind's absent status.
    fn status(&self, entity: &'a PassEntity) -> &'a str {
        let written = entity.fields.get("status").map(String::as_str);
        match vocabulary::lifecycle(&entity.kind) {
            Some(statuses) => statuses.read(written).unwrap_or(""),
            None => written.unwrap_or(""),
        }
    }

    /// The targets of `source`'s `field` references that are of `kind`, in
    /// graph order, each once.
    fn targets(&self, source: &str, field: &str, kind: &str) -> Vec<&'a PassEntity> {
        let mut seen = BTreeSet::new();
        self.edges
            .iter()
            .filter(|e| e.source == source && e.label == field)
            .filter_map(|e| self.entities.get(e.target.as_str()).copied())
            .filter(|t| t.kind == kind && seen.insert(t.id.as_str()))
            .collect()
    }

    /// The behaviors that implement `feature`, sorted.
    fn implementers(&self, feature: &str) -> Vec<String> {
        let ids: BTreeSet<String> = self
            .edges
            .iter()
            .filter(|e| e.target == feature && e.label == IMPLEMENTS_FIELD)
            .filter(|e| self.kind(&e.source) == Some(IMPLEMENTER_KIND))
            .map(|e| e.source.clone())
            .collect();
        ids.into_iter().collect()
    }
}

/// A finding about `entity`; the host attaches its span.
fn finding(
    code: &str,
    severity: PassSeverity,
    entity: &PassEntity,
    message: String,
    suggestion: String,
) -> PassDiagnostic {
    let diagnostic = PassDiagnostic::new(code, severity, message)
        .with_suggestion(suggestion)
        .with_entity(entity.id.clone());
    match &entity.span {
        Some(span) => diagnostic.with_span(span.clone()),
        None => diagnostic,
    }
}

/// `lifecycle`: W154, I063, I064, I065, in that order, each in id order.
pub fn pass_lifecycle(input: &PassInput) -> Vec<PassDiagnostic> {
    let snap = Snapshot::of(input);
    let mut out = Vec::new();
    completed_milestones_deliver_done_features(&snap, &mut out);
    done_features_depend_on_done_features(&snap, &mut out);
    milestones_are_due_after_their_dependencies(&snap, &mut out);
    shipped_deliverables_track_completed_milestones(&snap, &mut out);
    out
}

/// W154: a `completed` milestone whose `features` names a feature that is
/// neither `done` nor `deprecated`, once per such feature.
fn completed_milestones_deliver_done_features(snap: &Snapshot<'_>, out: &mut Vec<PassDiagnostic>) {
    for milestone in snap.of_kind("milestone") {
        if snap.status(milestone) != MILESTONE_COMPLETED {
            continue;
        }
        for feature in snap.targets(&milestone.id, "features", "feature") {
            let status = snap.status(feature);
            if status == FEATURE_DONE || status == FEATURE_DEPRECATED {
                continue;
            }
            out.push(finding(
                "W154",
                PassSeverity::Warning,
                milestone,
                format!(
                    "milestone '{}' is completed but its feature '{}' is {status}",
                    milestone.id, feature.id
                ),
                format!(
                    "mark '{}' done if it was delivered; otherwise move it out of the milestone or set '{}' back to in_progress",
                    feature.id, milestone.id
                ),
            ));
        }
    }
}

/// I063: a `done` feature whose `depends_on` names a feature that is not
/// `done`, once per such dependency.
fn done_features_depend_on_done_features(snap: &Snapshot<'_>, out: &mut Vec<PassDiagnostic>) {
    for feature in snap.of_kind("feature") {
        if snap.status(feature) != FEATURE_DONE {
            continue;
        }
        for dependency in snap.targets(&feature.id, "depends_on", "feature") {
            let status = snap.status(dependency);
            if status == FEATURE_DONE {
                continue;
            }
            out.push(finding(
                "I063",
                PassSeverity::Info,
                feature,
                format!(
                    "feature '{}' is done but depends on '{}', which is {status}",
                    feature.id, dependency.id
                ),
                format!(
                    "finish '{}' (or mark it done), or set '{}' back to in_progress",
                    dependency.id, feature.id
                ),
            ));
        }
    }
}

/// A `target_date` the comparison can order: `YYYY-MM-DD`.
fn date(entity: &PassEntity) -> Option<&str> {
    let d = entity.fields.get("target_date")?.as_str();
    let b = d.as_bytes();
    let well_formed = b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit());
    well_formed.then_some(d)
}

/// I064: a milestone whose `target_date` is before that of a milestone its
/// `depends_on` names, once per such pair; a milestone or dependency
/// without a well-formed date is not compared.
fn milestones_are_due_after_their_dependencies(snap: &Snapshot<'_>, out: &mut Vec<PassDiagnostic>) {
    for milestone in snap.of_kind("milestone") {
        let Some(due) = date(milestone) else {
            continue;
        };
        for dependency in snap.targets(&milestone.id, "depends_on", "milestone") {
            let Some(dependency_due) = date(dependency) else {
                continue;
            };
            if due >= dependency_due {
                continue;
            }
            out.push(finding(
                "I064",
                PassSeverity::Info,
                milestone,
                format!(
                    "milestone '{}' is due {due}, before its dependency '{}' (due {dependency_due})",
                    milestone.id, dependency.id
                ),
                format!(
                    "move '{}'s target_date to {dependency_due} or later, or drop the dependency",
                    milestone.id
                ),
            ));
        }
    }
}

/// I065: a `shipped` deliverable whose `milestones` names a milestone that
/// is not `completed`, once per such milestone.
fn shipped_deliverables_track_completed_milestones(
    snap: &Snapshot<'_>,
    out: &mut Vec<PassDiagnostic>,
) {
    for deliverable in snap.of_kind("deliverable") {
        if snap.status(deliverable) != DELIVERABLE_SHIPPED {
            continue;
        }
        for milestone in snap.targets(&deliverable.id, "milestones", "milestone") {
            let status = snap.status(milestone);
            if status == MILESTONE_COMPLETED {
                continue;
            }
            out.push(finding(
                "I065",
                PassSeverity::Info,
                deliverable,
                format!(
                    "deliverable '{}' is shipped but its milestone '{}' is {status}",
                    deliverable.id, milestone.id
                ),
                format!(
                    "complete '{}', or set '{}' back to in_progress",
                    milestone.id, deliverable.id
                ),
            ));
        }
    }
}

/// `delivery_evidence`: I071 for each `done` feature the recorded tests do
/// not prove, with a summary of how the declared statuses and the evidence
/// compare. Without recorded test results it reports nothing and says so.
pub fn pass_delivery_evidence(input: &PassInput) -> PassOutput {
    let snap = Snapshot::of(input);
    let Some(results) = &input.test_results else {
        let mut summary = serde_json::Map::new();
        summary.insert("evidence".into(), "none".into());
        return PassOutput {
            diagnostics: Vec::new(),
            summary,
        };
    };
    let proved: BTreeSet<&str> = input
        .proved_claims
        .iter()
        .flatten()
        .map(String::as_str)
        .collect();
    // The coverage rule's score of one entity: the rule `specforge stats`
    // and the testing extension's coverage pass apply.
    let score = |id: &str| -> Option<EntityEvidence> {
        let entity = snap.entities.get(id)?;
        let rule_entity = coverage::Entity::from(*entity);
        let verdict = coverage::Verdict::of(&rule_entity, results.of(id), proved.contains(id));
        Some(EntityEvidence {
            obligations: verdict.obligations,
            proven: verdict.proven,
            failing: verdict.failing,
        })
    };

    let mut diagnostics = Vec::new();
    let (mut features, mut done, mut proven, mut done_unproven, mut proven_not_done) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    for feature in snap.of_kind("feature") {
        features += 1;
        let evidence = FeatureEvidence::of(&feature.id, &snap.implementers(&feature.id), score);
        let is_done = snap.status(feature) == FEATURE_DONE;
        done += usize::from(is_done);
        proven += usize::from(evidence.proven);
        if evidence.proven && !is_done {
            proven_not_done += 1;
        }
        if !is_done || evidence.proven {
            continue;
        }
        done_unproven += 1;
        let message = if evidence.behaviors == 0 {
            format!(
                "feature '{}' is done but no behavior implements it, so no recorded test can prove it",
                feature.id
            )
        } else {
            format!(
                "feature '{}' is done but the recorded tests prove {} of the {} behaviors implementing it ({}/{} obligations{})",
                feature.id,
                evidence.proven_behaviors,
                evidence.behaviors,
                evidence.proven_obligations,
                evidence.obligations,
                if evidence.failing > 0 {
                    format!(", {} failing tests", evidence.failing)
                } else {
                    String::new()
                }
            )
        };
        let suggestion = if evidence.behaviors == 0 {
            format!(
                "name '{}' in the `features` of the behaviors that deliver it",
                feature.id
            )
        } else {
            "link tests to the unproven obligations (#[specforge_test_macros::test(behavior = …, verify = …)]), then run `specforge collect`".to_string()
        };
        diagnostics.push(finding(
            "I071",
            PassSeverity::Info,
            feature,
            message,
            suggestion,
        ));
    }
    let mut summary = serde_json::Map::new();
    summary.insert("evidence".into(), "recorded".into());
    summary.insert("features".into(), features.into());
    summary.insert("done".into(), done.into());
    summary.insert("proven".into(), proven.into());
    summary.insert("done_unproven".into(), done_unproven.into());
    summary.insert("proven_not_done".into(), proven_not_done.into());
    PassOutput {
        diagnostics,
        summary,
    }
}
