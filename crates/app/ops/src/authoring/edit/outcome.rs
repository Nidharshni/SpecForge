//! What a list of typed edits did (ADR 0162): [`run`] plans it and writes it unless it previews; [`EditDocument`]
//! is what both surfaces print.

use super::{Edit, EditField, EditReport, apply, plan};
use crate::view::ProjectView;
use crate::{Caller, Refusal, Wrote};
use serde::Serialize;
use specforge_common::shape::Shape;

/// What a list of typed edits did: what it plans (or planned) and whether it wrote.
#[derive(Debug, Clone)]
pub struct Edited {
    pub report: EditReport,
    pub wrote: Wrote,
}

/// Plan `edits` over `view` ([`plan`]) and write them as one project change ([`apply`]) unless `dry_run`. An empty
/// plan is applied too: it stages no file and answers no writes. Refused as [`plan`] refuses, or with [`apply`]'s
/// error (its writes: the files it could not put back).
pub fn run(
    view: &ProjectView<'_>,
    edits: &[Edit],
    dry_run: bool,
    caller: Caller<'_>,
) -> Result<Edited, Refusal<EditField>> {
    let planned = plan(view, edits)?;
    let wrote = Wrote::unless_preview(dry_run, || apply(&planned, caller))?;
    Ok(Edited {
        report: planned.report().clone(),
        wrote,
    })
}

/// What `specforge set|unset|link|unlink|delete|new --format json` and `specforge.edit` print (`McpEditResult`):
/// the report, and whether the run previewed (ADR 0052 D1, ADR 0162).
#[derive(Debug, Clone, Serialize, Shape)]
pub struct EditDocument {
    #[serde(flatten)]
    pub report: EditReport,
    /// The edits were only planned: nothing was written.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub dry_run: bool,
}

impl Edited {
    /// The document both surfaces print.
    pub fn document(&self) -> EditDocument {
        EditDocument {
            report: self.report.clone(),
            dry_run: self.wrote.previewed(),
        }
    }
}
