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

use std::path::{Path, PathBuf};

use specforge_common::{ConfigRead, ProjectConfig, ProjectRoot, read_project_config};

use crate::sources::{self, Read};

pub use inputs::{SessionInputs, WatchRoot, Watched};
pub(crate) use key::discovered;
pub use key::source_key;
use stamps::{Scope, Stamps};

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

/// What a project session read and when: its inputs (ADR 0030) and the stamp of
/// each, taken before the read it describes. The one module that touches the
/// stamps (ADR 0106).
pub(crate) struct Ledger {
    inputs: SessionInputs,
    stamps: Stamps,
}

impl Ledger {
    /// A detached session's: no root; every method reads nothing and records
    /// nothing; `changes` and `stale` are empty; `reread` gives `Read::Gone`
    /// for every key (the buffer was the only text).
    pub(crate) fn detached() -> Self {
        Ledger {
            inputs: SessionInputs::detached(),
            stamps: Stamps::default(),
        }
    }

    /// About to open a project: nothing read yet.
    pub(crate) fn opening() -> Self {
        Self::detached()
    }

    /// What the session depends on now; its equality is what
    /// `Update::inputs_changed` compares.
    pub(crate) fn inputs(&self) -> &SessionInputs {
        &self.inputs
    }

    /// Whether `key` is no source of the project (never, detached).
    pub(crate) fn excludes(&self, key: &str) -> bool {
        self.inputs.excludes(key)
    }

    /// `specforge.json` at `root`: stamped, then read once. A config
    /// rewritten after this is seen next time, whatever the rest was read
    /// from. Starts the environment record again.
    pub(crate) fn config(&mut self, root: &ProjectRoot) -> ConfigRead {
        self.stamps.stamp_config(&root.config_file());
        read_project_config(root.dir())
    }

    /// Before the extension runtime and the environment read them: record the
    /// inputs of the environment `config` names, and stamp the lock and each
    /// extension module (a prediction until a reader reaches the installed
    /// crate, ADR 0030 D3).
    pub(crate) fn before_load(&mut self, root: &ProjectRoot, config: &ProjectConfig) {
        self.inputs = SessionInputs::opened(root.dir(), config);
        self.stamps.stamp_environment(&self.inputs);
    }

    /// With the build cache recorded, when the loaded environment declares
    /// check-phase passes (temporary: folded into the check run).
    pub(crate) fn with_check_passes(&mut self, check_passes: bool) {
        self.inputs = self.inputs.clone().with_check_passes(check_passes);
    }

    /// The `.spec` files discovery finds now: every one stamped, and each
    /// symlink read, before the sources are read.
    pub(crate) fn discover(&mut self) -> Vec<PathBuf> {
        let discovered = self.inputs.discover();
        self.stamps.stamp_all_sources(&self.inputs, &discovered);
        self.inputs = self.inputs.linked(&discovered);
        discovered
    }

    /// Read the sources `keys` names again, each stamped, its link read,
    /// then read. A key that is no source any more reads as gone and its
    /// stamp and link are forgotten (so its return is seen). Each key's
    /// read, in order, and whether a link moved.
    pub(crate) fn reread(&mut self, keys: &[String]) -> (Vec<(String, Read)>, bool) {
        let Some(spec_root) = self.inputs.spec_root().map(Path::to_path_buf) else {
            return (
                keys.iter().map(|key| (key.clone(), Read::Gone)).collect(),
                false,
            );
        };
        self.stamps.stamp_sources(&self.inputs, keys);
        let relinked = self.inputs.relinked(keys);
        let moved = relinked != self.inputs;
        self.inputs = relinked;
        let reads = keys
            .iter()
            .map(|key| {
                let read = if self.inputs.excludes(key) {
                    Read::Gone
                } else {
                    sources::read(&spec_root, key)
                };
                (key.clone(), read)
            })
            .collect();
        (reads, moved)
    }

    /// The files the checks about to run read, named: stamped, and recorded
    /// (the missing ones' directories with them). Whether the inputs moved.
    pub(crate) fn before_checks(&mut self, named: Vec<PathBuf>) -> bool {
        if self.inputs.root().is_none() {
            return false;
        }
        let next = self.inputs.with_named(named);
        let changed = next != self.inputs;
        self.inputs = next;
        self.stamps.stamp_checks(&self.inputs);
        changed
    }

    /// What changed on disk among what `paths` reach (ADR 0073).
    pub(crate) fn changes<'p>(&self, paths: impl IntoIterator<Item = &'p Path>) -> Changes {
        let reach = self.inputs.reach(paths);
        self.stamps.changes(&self.inputs, Scope::Reached(&reach))
    }

    /// Everything that differs from what was read.
    pub(crate) fn stale(&self) -> Changes {
        self.stamps.changes(&self.inputs, Scope::Everything)
    }
}
