//! The review: the coverage gaps of a neighbourhood, a read view over the
//! project view (ADR 0015, "Prompt read views"). The review prompt and
//! `specforge review` render it.

use std::collections::BTreeSet;

use serde::Serialize;
use specforge_common::shape::Shape;

use crate::Refusal;
use crate::read::coverage::{CoverageQuery, CoverageRow, CoverageRowDocument, selected};
use crate::view::{EntityField, ProjectView, Select};

/// Hops around the reviewed entity when a request names none.
pub const DEFAULT_DEPTH: usize = 1;

/// What to review.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReviewRequest<'a> {
    /// The entity whose neighbourhood is reviewed; the whole project when
    /// `None`.
    pub entity_id: Option<&'a str>,
    /// Hops around `entity_id`, [`DEFAULT_DEPTH`] when `None`; refused
    /// without `entity_id` (the selection's `depth_without_entity`).
    pub depth: Option<usize>,
}

/// What a finding says is missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewGap {
    /// The entity declares no obligation (`CoverageRow::declared` is false).
    NoObligations,
    /// No edge links the entity to another entity (`Degree::is_unconnected`).
    Unconnected,
}

impl ReviewGap {
    /// `"warning"` for [`Self::NoObligations`], `"info"` for [`Self::Unconnected`].
    pub fn severity(self) -> &'static str {
        match self {
            ReviewGap::NoObligations => "warning",
            ReviewGap::Unconnected => "info",
        }
    }
}

/// One gap of one reviewed entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewFinding {
    pub entity_id: String,
    pub gap: ReviewGap,
}

impl ReviewFinding {
    /// `Entity '<id>' has no verify declarations` /
    /// `Entity '<id>' is unconnected: no edge links it to another entity`.
    pub fn message(&self) -> String {
        match self.gap {
            ReviewGap::NoObligations => {
                format!("Entity '{}' has no verify declarations", self.entity_id)
            }
            ReviewGap::Unconnected => format!(
                "Entity '{}' is unconnected: no edge links it to another entity",
                self.entity_id
            ),
        }
    }
}

/// What a review answers.
#[derive(Debug, Clone)]
pub struct Review {
    /// The reviewed entity; `None` for the whole project.
    pub entity_id: Option<String>,
    /// The coverage view's rows (the entities that count toward coverage)
    /// within the neighbourhood, in id order.
    pub rows: Vec<CoverageRow>,
    /// The hops around `entity_id` the review reached; `None` without it.
    pub depth: Option<usize>,
    /// Per row, in row order: its missing obligations, then its
    /// unconnectedness.
    pub findings: Vec<ReviewFinding>,
}

impl Review {
    /// The review as the review prompt's payload and `specforge review
    /// --format json` write it.
    pub fn document(&self) -> ReviewDocument {
        ReviewDocument {
            entity_id: self.entity_id.as_deref().unwrap_or("*").to_string(),
            findings: self
                .findings
                .iter()
                .map(|finding| FindingDocument {
                    entity_id: finding.entity_id.clone(),
                    severity: finding.gap.severity().to_string(),
                    message: finding.message(),
                })
                .collect(),
            coverage_summary: self.rows.iter().map(CoverageRow::document).collect(),
        }
    }
}

/// The review as `specforge review --format json` and the review prompt
/// print it (`McpReviewPromptResult`).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct ReviewDocument {
    /// `*` for the whole project.
    pub entity_id: String,
    pub findings: Vec<FindingDocument>,
    pub coverage_summary: Vec<CoverageRowDocument>,
}

/// One gap of one reviewed entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
pub struct FindingDocument {
    pub entity_id: String,
    pub severity: String,
    pub message: String,
}

/// The review `request` asks for. An `entity_id` the graph lacks is
/// `view::not_found` (E003), and a `depth` without one
/// the selection's `depth_without_entity`; a recorded report that cannot be read is
/// its E045 failure (`ProjectView::coverage`).
pub fn review(view: &ProjectView, request: &ReviewRequest) -> Result<Review, Refusal<EntityField>> {
    let selection = view.select(&Select {
        entity: request.entity_id,
        named_by: EntityField::EntityId,
        depth: request.depth,
        kinds: &[],
    })?;
    let depth = request
        .entity_id
        .map(|_| request.depth.unwrap_or(DEFAULT_DEPTH));
    let neighbourhood: Option<BTreeSet<&'static str>> = selection
        .reach(Some(DEFAULT_DEPTH))
        .map(|reached| reached.iter().map(|r| r.id.as_str()).collect());
    let rows: Vec<CoverageRow> = selected(view, &CoverageQuery::default())?
        .rows
        .into_iter()
        .filter(|row| {
            neighbourhood
                .as_ref()
                .is_none_or(|ids| ids.contains(row.entity_id.as_str()))
        })
        .collect();

    let connectivity = view.connectivity();
    let mut findings = Vec::new();
    for row in &rows {
        if !row.declared() {
            findings.push(ReviewFinding {
                entity_id: row.entity_id.clone(),
                gap: ReviewGap::NoObligations,
            });
        }
        if connectivity.degree(&row.entity_id).is_unconnected() {
            findings.push(ReviewFinding {
                entity_id: row.entity_id.clone(),
                gap: ReviewGap::Unconnected,
            });
        }
    }
    Ok(Review {
        entity_id: request.entity_id.map(str::to_string),
        rows,
        depth,
        findings,
    })
}
