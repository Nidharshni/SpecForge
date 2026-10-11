//! Inference: the manifest an agent's inference is recorded in
//! (`specforge-infer.json`), its sessions, its progress and gaps, and the
//! `inferred` lint. `specforge infer-status`, the MCP
//! `specforge.infer_progress`, `specforge.infer_gaps` and
//! `specforge.infer_session` tools and the infer prompt's plan read the
//! project here.
//!
//! Progress compares the source files the enabled analyzers would scan
//! with the manifest's index; gaps scans those files for public items and
//! keeps the ones no graph entity names; the lint reports analyzed files
//! that changed or are over-dense; [`session`] records one step of an
//! inference session. The inference guide (what to look for to infer each
//! kind's entities, [`guide`] and [`kind_guide`]) is a read view over the
//! project view that the infer prompt, `specforge infer-guide` and the
//! LSP's keyword completion render. The inference plan ([`inference_plan`]:
//! progress, paged file lists and the kinds in the order to write them) is
//! the infer prompt's plan scope.

mod discovery;
mod gaps;
mod guide;
mod lint;
mod manifest;
mod plan;
mod progress;
mod session;

pub use crate::scan::ScanFailure;
pub use gaps::{
    DirectoryGaps, GapItem, Gaps, GapsDocument, ScanFailureRow, SourceItem, directory_of, gaps,
};
pub use guide::{InferenceGuide, KindGuide, guide, kind_guide};
pub use lint::lint;
pub(crate) use manifest::read_manifest;
pub use manifest::{
    InferenceManifest, InferenceSession, InferenceSummary, MANIFEST_FILENAME,
    MANIFEST_WRITE_FAILED, SessionStatus, SourceFileEntry,
};
pub use plan::{
    FilePage, InferencePlan, InferencePlanRequest, KindPriority, MAX_LISTED_FILES, inference_plan,
};
pub use progress::{
    Progress, ProgressDocument, ProgressSummary, SessionRow, UnanalyzedDirectory, progress,
};
pub use session::{
    END_STATUS, EndStatus, Recorded, SESSION_ACTION, SESSION_ACTIVE, SESSION_NOT_ACTIVE,
    SOURCE_OUTSIDE_ROOT, SOURCE_UNREADABLE, SessionAction, SessionOutcome, SessionStep,
    UNKNOWN_SESSION, session,
};
