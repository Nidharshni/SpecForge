//! `specforge stats` and `specforge.stats`: the project's statistics, one
//! operation over the project view (ADR 0015) whose outcome is the one
//! document both surfaces print (`McpStatsResult`, ADR 0052).

use serde::Serialize;
use specforge_common::Counts;
use specforge_common::Diagnostic;
use specforge_common::shape::Shape;
use specforge_project::coverage::Summary;

use crate::OpError;
use crate::view::ProjectView;

/// The project's statistics: what `specforge stats --format json` and
/// `specforge.stats` print.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct Stats {
    /// Every entity of the graph.
    pub entity_count: usize,
    /// The entities of each kind, in kind order: every kind an entity is
    /// written with, counted once ([`ProjectView::entities_by_kind`]).
    pub entity_counts: Vec<KindCount>,
    pub edge_count: usize,
    /// Entities no edge links to another entity (CONTEXT.md "Unconnected
    /// entity").
    pub unconnected_count: usize,
    /// The entities that count toward coverage.
    pub testable_count: usize,
    /// Testable entities that declare at least one obligation.
    pub declared_count: usize,
    /// Entities, of any kind, that declare at least one obligation.
    pub verified_count: usize,
    /// `declared_count` over `testable_count`, in percent (0 when nothing
    /// is testable): declared intent.
    pub declared_pct: f64,
    /// The share of testable entities proven, in percent: the
    /// `analyze coverage --min` gate's figure. Absent without recorded
    /// test results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof_pct: Option<f64>,
    /// What the view's surface reports for the project, by severity
    /// ([`ProjectView::reported`]).
    pub diagnostic_summary: Counts,
}

/// One kind and how many of the graph's entities are written with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
pub struct KindCount {
    pub kind: String,
    pub count: usize,
}

/// The project's statistics. The testable, declared and proven counts are
/// the coverage rule's over the view's recorded report, so stats and
/// `analyze coverage` report the same numbers: testable entities are those
/// that count toward coverage (the entities W004 exempts that declare
/// nothing are left out). The diagnostic counts are of what the view's
/// surface reports for the project ([`ProjectView::reported`]: the CLI what
/// `specforge check` reports, MCP that plus its surface registration
/// notices). A recorded report that cannot be read is the error.
pub fn stats(view: &ProjectView) -> Result<Stats, OpError> {
    let coverage = view.coverage()?;
    Ok(tally(view, &coverage.summary, &view.reported()))
}

fn tally(view: &ProjectView, coverage: &Summary, diagnostics: &[Diagnostic]) -> Stats {
    let graph = view.graph();
    let entity_counts = view
        .entities_by_kind()
        .into_iter()
        .map(|(kind, count)| KindCount {
            kind: kind.to_string(),
            count,
        })
        .collect();
    let verified_count = coverage.discharge_funnel.entities_with_obligations;
    let testable_count = coverage.testable_total;
    let testable_verified = coverage.testable_verified;

    let unconnected_count = view.connectivity().unconnected().count();

    let declared_pct = if testable_count > 0 {
        (testable_verified as f64 / testable_count as f64) * 100.0
    } else {
        0.0
    };
    let proof_pct = coverage.test_results.as_ref().map(|_| coverage.proof_pct());

    Stats {
        entity_count: graph.node_count(),
        entity_counts,
        edge_count: graph.edge_count(),
        unconnected_count,
        testable_count,
        declared_count: testable_verified,
        verified_count,
        declared_pct,
        proof_pct,
        diagnostic_summary: Counts::of(diagnostics),
    }
}
