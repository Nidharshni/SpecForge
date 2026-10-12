//! The inference plan: what the infer prompt's plan scope lays out, a read
//! view over the project view (CONTEXT.md "Inference plan").

use serde::Serialize;
use specforge_common::shape::Shape;

use crate::OpError;
use crate::view::ProjectView;

use super::guide::{InferenceGuide, guide};
use super::progress::{Progress, ProgressSummary, progress};

/// Files listed per page of a plan's file lists.
pub const MAX_LISTED_FILES: usize = 50;

#[derive(Debug, Clone, Copy, Default)]
pub struct InferencePlanRequest<'a> {
    /// Where the agent writes `.spec` files; the guide's spec directory
    /// when `None`.
    pub target_spec_directory: Option<&'a str>,
    /// Offset into the unanalyzed and stale file lists.
    pub cursor: usize,
}

/// One page of a file list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePage {
    /// At most [`MAX_LISTED_FILES`] files from the cursor.
    pub files: Vec<String>,
    pub total: usize,
    /// Files after this page.
    pub remaining: usize,
}

impl FilePage {
    /// The page of `files` starting at `cursor`.
    fn of(files: &[String], cursor: usize) -> Self {
        let start = cursor.min(files.len());
        let end = (start + MAX_LISTED_FILES).min(files.len());
        FilePage {
            files: files[start..end].to_vec(),
            total: files.len(),
            remaining: files.len() - end,
        }
    }
}

/// One kind in the plan's order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KindPriority {
    pub kind: String,
    pub extension: String,
    pub existing: usize,
}

#[derive(Debug, Clone)]
pub struct InferencePlan {
    pub target_spec_directory: String,
    /// The inference progress (`Progress::none()` without a root).
    pub progress: Progress,
    pub cursor: usize,
    /// The cursor of the next page of either list; `None` on the last.
    pub next_cursor: Option<usize>,
    pub unanalyzed: FilePage,
    pub stale: FilePage,
    /// Kinds with no entity first, then the others; within each group each
    /// kind after the kinds its reference fields target, else in
    /// declaration order (a cycle broken in declaration order).
    pub kind_priorities: Vec<KindPriority>,
}

impl InferencePlan {
    /// The plan as the infer prompt's plan scope prints it.
    pub fn document(&self) -> PlanDocument {
        PlanDocument {
            target_spec_directory: self.target_spec_directory.clone(),
            progress: ProgressSummary {
                files_total: self.progress.summary.files_total,
                files_analyzed: self.progress.summary.files_analyzed,
                entities_produced: self.progress.summary.entities_produced,
            },
            cursor: self.cursor,
            next_cursor: self.next_cursor,
            unanalyzed_files: self.unanalyzed.files.clone(),
            unanalyzed_total: self.unanalyzed.total,
            stale_files: self.stale.files.clone(),
            stale_total: self.stale.total,
            kind_priorities: self
                .kind_priorities
                .iter()
                .map(|priority| KindPriorityDocument {
                    kind: priority.kind.clone(),
                    extension: priority.extension.clone(),
                    existing_count: priority.existing,
                })
                .collect(),
        }
    }
}

/// The inference plan as a document: a page of each file list (files only),
/// its totals, and the cursor of the next page when one follows.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct PlanDocument {
    pub target_spec_directory: String,
    pub progress: ProgressSummary,
    pub cursor: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<usize>,
    pub unanalyzed_files: Vec<String>,
    pub unanalyzed_total: usize,
    pub stale_files: Vec<String>,
    pub stale_total: usize,
    pub kind_priorities: Vec<KindPriorityDocument>,
}

/// One kind in the plan's order, as the plan document prints it.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct KindPriorityDocument {
    pub kind: String,
    pub extension: String,
    pub existing_count: usize,
}

/// The plan `request` asks for. An unusable specforge-infer.json is E071
/// (`progress`); without a root there is no progress to plan from.
pub fn inference_plan(
    view: &ProjectView,
    request: &InferencePlanRequest,
) -> Result<InferencePlan, OpError> {
    // Nothing is planned from a specforge-infer.json that cannot be used:
    // every mark_analyzed the plan sent the agent to make would be refused.
    // Without a root there is nothing to count.
    let progress = match view.root() {
        None => Progress::none(),
        Some(_) => progress(view)?,
    };
    let guide = guide(view);
    let cursor = request.cursor;
    let unanalyzed = FilePage::of(&progress.unanalyzed, cursor);
    let stale = FilePage::of(&progress.stale, cursor);
    let next_cursor =
        (unanalyzed.remaining > 0 || stale.remaining > 0).then_some(cursor + MAX_LISTED_FILES);
    Ok(InferencePlan {
        target_spec_directory: request
            .target_spec_directory
            .map_or_else(|| guide.spec_directory.clone(), str::to_string),
        kind_priorities: priorities(&guide),
        progress,
        cursor,
        next_cursor,
        unanalyzed,
        stale,
    })
}

/// Kinds with no entity first, then the others; within each group each kind
/// after the kinds its reference fields target. Deterministic and
/// kind-neutral: the order comes from the registry's fields.
fn priorities(guide: &InferenceGuide) -> Vec<KindPriority> {
    let (unwritten, written): (Vec<usize>, Vec<usize>) =
        (0..guide.kinds.len()).partition(|&i| guide.kinds[i].existing.is_empty());
    let mut order = Vec::with_capacity(guide.kinds.len());
    for group in [unwritten, written] {
        order.extend(referenced_first(guide, group));
    }
    order
        .into_iter()
        .map(|i| {
            let kind = &guide.kinds[i];
            KindPriority {
                kind: kind.keyword.to_string(),
                extension: kind.extension.to_string(),
                existing: kind.existing.len(),
            }
        })
        .collect()
}

/// `group` (indexes into `guide.kinds`, in declaration order) reordered so a
/// kind follows the kinds of the group its reference fields target; when
/// none is ready (a cycle) the first remaining goes next.
fn referenced_first(guide: &InferenceGuide, group: Vec<usize>) -> Vec<usize> {
    let targets = |i: usize| -> Vec<&str> {
        let kind = &guide.kinds[i];
        kind.fields
            .iter()
            .filter(|field| field.field_type().is_reference())
            .filter_map(|field| field.declared().target_kind.as_deref())
            .filter(|target| *target != kind.keyword)
            .collect()
    };
    let mut remaining = group;
    let mut ordered = Vec::with_capacity(remaining.len());
    while !remaining.is_empty() {
        let ready = remaining.iter().position(|&i| {
            targets(i)
                .iter()
                .all(|target| !remaining.iter().any(|&r| guide.kinds[r].keyword == *target))
        });
        ordered.push(remaining.remove(ready.unwrap_or(0)));
    }
    ordered
}
