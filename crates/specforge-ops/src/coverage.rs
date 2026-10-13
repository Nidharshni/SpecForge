//! The coverage view: each entity's coverage under the one rule
//! `analyze coverage` applies, over the project view (ADR 0015).
//!
//! The MCP coverage tool, inspect's coverage, query and the review prompt read
//! it;
//! with no entity named it lists exactly the entities stats counts as
//! testable, so its rows and stats' numbers cannot disagree.

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_project::coverage::{Status, Summary, Verdict};
use specforge_project::snapshot::{EntityRecord, Standing};

use crate::OpError;
use crate::options::{Choice, OptionTable};
use crate::view::ProjectView;

/// Which rows to list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CoverageQuery<'q> {
    /// That entity alone, whether it counts toward coverage or not.
    pub entity_id: Option<&'q str>,
    /// Without `entity_id`: only entities of this kind.
    pub kind: Option<&'q str>,
    /// Only rows with this status.
    pub status: Option<Status>,
}

/// One entity's coverage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageRow {
    pub entity_id: String,
    pub kind: String,
    /// Its kind is testable.
    pub testable: bool,
    /// A testable-kind entity that owes no obligations and declares none
    /// (W004 exempts it): it does not count toward coverage
    /// (`Standing::exempt`).
    pub exempt: bool,
    pub verdict: Verdict,
}

impl CoverageRow {
    fn of(record: &EntityRecord, standing: &Standing, verdict: &Verdict) -> Self {
        CoverageRow {
            entity_id: record.id.clone(),
            kind: record.kind.clone(),
            testable: standing.testable,
            exempt: standing.exempt(),
            verdict: verdict.clone(),
        }
    }

    pub fn status(&self) -> Status {
        self.verdict.status()
    }

    /// One row as every surface writes it (`McpCoverageResult`):
    /// `entity_id`, `kind`, `status` (a [`STATUS`] name), `declared`,
    /// `linked`, `evidence_collected`, `obligations`, `proven`, `unproven`,
    /// `exempt`.
    pub fn document(&self) -> CoverageRowDocument {
        CoverageRowDocument {
            entity_id: self.entity_id.clone(),
            kind: self.kind.clone(),
            status: STATUS.name_of(self.status()).to_string(),
            declared: self.declared(),
            linked: self.linked(),
            evidence_collected: self.linked(),
            obligations: self.verdict.obligations,
            proven: self.verdict.proven,
            unproven: self.verdict.unproven.clone(),
            exempt: self.exempt,
        }
    }

    /// It declares at least one obligation.
    pub fn declared(&self) -> bool {
        self.verdict.obligations > 0
    }

    /// Tests are recorded for it.
    pub fn linked(&self) -> bool {
        self.verdict.tests > 0
    }

    /// It counts toward coverage and is not proven
    /// (`ProjectCoverage::is_unverified`).
    pub fn unverified(&self) -> bool {
        self.testable && !self.exempt && !self.verdict.is_proven()
    }
}

/// One row as every surface writes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
pub struct CoverageRowDocument {
    pub entity_id: String,
    pub kind: String,
    #[shape(names = STATUS)]
    pub status: String,
    pub declared: bool,
    pub linked: bool,
    /// Always equals `linked`.
    pub evidence_collected: bool,
    pub obligations: usize,
    pub proven: usize,
    pub unproven: Vec<String>,
    pub exempt: bool,
}

/// The rows a query selects, and the project's coverage summary.
#[derive(Debug, Clone)]
pub struct CoverageOutcome {
    pub rows: Vec<CoverageRow>,
    pub summary: Summary,
}

/// The coverage view. With no `entity_id`: the entities that count toward
/// coverage, so `rows.len() == summary.testable_total`, narrowed by kind
/// and status, in entity id order. With `entity_id`: that entity, counted
/// or not (an exempt row says so), or no row when the graph lacks it. A
/// recorded report that cannot be read is the error.
pub fn coverage(view: &ProjectView, query: &CoverageQuery) -> Result<CoverageOutcome, OpError> {
    let coverage = view.coverage()?;
    let rows = view
        .entities()
        .iter()
        .filter(|(record, standing)| match query.entity_id {
            Some(entity_id) => record.id == entity_id,
            None => standing.counts() && query.kind.is_none_or(|kind| record.kind == kind),
        })
        .filter_map(|(record, standing)| {
            Some(CoverageRow::of(
                record,
                standing,
                coverage.verdict(&record.id)?,
            ))
        })
        .filter(|row| query.status.is_none_or(|status| row.status() == status))
        .collect();
    Ok(CoverageOutcome {
        rows,
        summary: coverage.summary.clone(),
    })
}

/// One entity's row, counted or not; `None` when the graph lacks it.
pub fn row(view: &ProjectView, entity_id: &str) -> Result<Option<CoverageRow>, OpError> {
    let coverage = view.coverage()?;
    Ok(view
        .entities()
        .get(entity_id)
        .zip(coverage.verdict(entity_id))
        .map(|((record, standing), verdict)| CoverageRow::of(record, standing, verdict)))
}

/// `specforge.coverage`'s `status_filter`: a coverage status as the
/// results spell it (its serialized form). A filter, so no default.
pub const STATUS: OptionTable<Status> = OptionTable {
    argument: "coverage status",
    choices: &[
        Choice {
            name: "covered",
            aliases: &[],
            help: "every obligation proven",
            value: Status::Covered,
        },
        Choice {
            name: "partial",
            aliases: &[],
            help: "some obligations proven",
            value: Status::Partial,
        },
        Choice {
            name: "uncovered",
            aliases: &[],
            help: "none proven",
            value: Status::Uncovered,
        },
    ],
    default: None,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_names_parse_and_others_are_refused_with_the_closest() {
        for status in [Status::Covered, Status::Partial, Status::Uncovered] {
            assert_eq!(STATUS.parse(STATUS.name_of(status)), Ok(status));
            assert_eq!(
                serde_json::to_value(status).unwrap(),
                STATUS.name_of(status),
                "the name is the serialized form"
            );
        }
        assert_eq!(STATUS.parse_optional(None), Ok(None), "a filter");
        let error = STATUS.parse("coverd").unwrap_err();
        assert_eq!(error.code, "invalid_input");
        assert_eq!(
            error.message,
            "Unknown coverage status: coverd. Expected: covered, partial, uncovered"
        );
        assert_eq!(error.suggestion.as_deref(), Some("did you mean 'covered'?"));
    }
}
