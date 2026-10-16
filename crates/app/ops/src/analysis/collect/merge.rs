//! Merging an extension's answer into `specforge-report.json`.

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{Diagnostic, codes};
use specforge_project::coverage::{ReportedEntity, ReportedTest, TestReport};
use specforge_protocol_types::{CollectOutput, TestStatus};
use std::collections::{BTreeMap, BTreeSet};

// ── merging ─────────────────────────────────────────────────────────────────

/// The entities results may name, each with its obligation texts (the
/// `verify` descriptions a convention-linked test may prove).
#[derive(Debug, Clone, Default)]
pub(super) struct KnownEntities(BTreeMap<String, Vec<String>>);

impl KnownEntities {
    /// Every entity of the view's entity snapshot, with its obligation texts.
    pub(super) fn of(entities: &specforge_registry::EntitySnapshot) -> Self {
        entities
            .records()
            .iter()
            .map(|record| {
                let texts = record.obligations.iter().map(|o| o.text.clone()).collect();
                (record.id.clone(), texts)
            })
            .collect()
    }

    pub(super) fn contains(&self, id: &str) -> bool {
        self.0.contains_key(id)
    }

    /// The entity's obligation texts, empty for an unknown entity.
    pub(super) fn obligations(&self, id: &str) -> &[String] {
        self.0.get(id).map(Vec::as_slice).unwrap_or_default()
    }
}

impl FromIterator<(String, Vec<String>)> for KnownEntities {
    fn from_iter<I: IntoIterator<Item = (String, Vec<String>)>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// Counts from one merge.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Shape)]
pub struct MergeStats {
    pub entities: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
}

/// Replace `runner`'s results in `report` with `collected`. Other runners'
/// results are kept, so several runners can report on one project. Tests
/// that name an unknown entity become W115 warnings, and skipped tests are
/// counted but not recorded: a skipped test proves nothing.
pub(super) fn merge(
    report: &mut TestReport,
    runner: &str,
    collected: &CollectOutput,
    known: &KnownEntities,
) -> (MergeStats, Vec<Diagnostic>) {
    for entity in report.results.values_mut() {
        entity
            .tests
            .retain(|t| t.runner.as_deref().is_some_and(|r| r != runner));
    }
    report.results.retain(|_, e| !e.tests.is_empty());

    let mut stats = MergeStats::default();
    let mut diagnostics = Vec::new();
    let mut unknown = BTreeSet::new();
    for entity in &collected.entity_results {
        if !known.contains(&entity.entity_id) {
            unknown.insert(entity.entity_id.as_str());
            continue;
        }
        let mut recorded = Vec::new();
        for test in &entity.test_results {
            match test.status {
                TestStatus::Passed => stats.passed += 1,
                TestStatus::Failed => stats.failed += 1,
                TestStatus::Skipped => {
                    stats.skipped += 1;
                    continue;
                }
            }
            recorded.push(ReportedTest {
                result: test.clone(),
                runner: Some(runner.to_string()),
            });
        }
        if recorded.is_empty() {
            continue;
        }
        stats.entities += 1;
        report
            .results
            .entry(entity.entity_id.clone())
            .or_insert_with(|| ReportedEntity { tests: Vec::new() })
            .tests
            .extend(recorded);
    }
    for id in unknown {
        diagnostics.push(
            Diagnostic::new(
                codes::W115,
                format!("{runner} reported tests for unknown entity '{id}'"),
            )
            .with_suggestion("check the test's entity annotation for a rename or typo".to_string()),
        );
    }

    let runners: BTreeSet<&str> = report
        .results
        .values()
        .flat_map(|e| e.tests.iter().filter_map(|t| t.runner.as_deref()))
        .collect();
    report.runner =
        (!runners.is_empty()).then(|| runners.into_iter().collect::<Vec<_>>().join(", "));
    (stats, diagnostics)
}
