//! The read ledger: what a project session depends on, what it read of it and
//! when (behavior `classify_project_changes`, ADR 0030, ADR 0073; behavior
//! `bring_session_up_to_date`).
//!
//! A session is built from its sources (the `.spec` files discovery finds
//! under the spec root), its environment inputs (`specforge.json`,
//! `specforge.lock` and the extension modules the environment loads) and
//! its check inputs (files the checks read but no parse does: the build
//! cache check-phase passes read, the files `file_reference` fields and
//! `file_exists` rules name). [`SessionInputs`] is that set as one value;
//! the session asks its inputs what a changed path reaches
//! (`ProjectSession::changes`), and watch and the LSP ask them which
//! directories to watch and what to report. The stamps (what each input
//! looked like when it was read) are compared with disk by
//! `ProjectSession::stale` and `changes`, and are taken before the read
//! they describe, so a write that races the read is seen next time.

mod inputs;
mod key;
mod reach;
mod stamps;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

pub use inputs::{SessionInputs, WatchRoot, Watched};
pub use key::source_key;
pub(crate) use stamps::{Scope, Stamps};

/// What a batch of changed paths, or the disk since the last read, means to
/// a session: the input to [`crate::ProjectSession::apply`]. Produced only
/// by [`crate::ProjectSession::changes`] and [`crate::ProjectSession::stale`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Changes {
    /// Changed sources, by key relative to the spec root: sorted, deduplicated.
    pub sources: Vec<String>,
    /// The environment must load again. Each path of the batch that reached
    /// an environment input which changed, as the batch spelled it (for
    /// `stale`, each changed input as the session recorded it): sorted,
    /// deduplicated.
    pub environment: Vec<PathBuf>,
    /// The checks must run again: the paths that reached a changed check
    /// input, named the same way.
    pub check_inputs: Vec<PathBuf>,
}

impl Changes {
    /// Nothing the session is built from changed.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty() && self.environment.is_empty() && self.check_inputs.is_empty()
    }
}
