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

mod disk;
mod inputs;
mod key;
mod reach;
mod stamps;
#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use specforge_common::{ConfigRead, ProjectConfig, ProjectRoot, SourceRoot, read_project_config};
use specforge_registry::{Disk, PlainDisk};

use crate::sources::{self, Read};

use disk::RunDisk;
pub use inputs::{SessionInputs, WatchRoot, Watched};
pub use key::source_key;
use stamps::{Entry, Scope, Stamps};

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

/// How a compile reads the disk (ADR 0106). A project session reads through its
/// [`Ledger`], which stamps each file in the call that reads it and records it as
/// an input; a one-shot compile reads through [`Unrecorded`], which only reads.
/// Both run the same steps in the same order: the config once, the lock and
/// modules before the extension load, the sources after discovery, the check
/// inputs before the checks. A step never fails: an unreadable file is in what it
/// returns (a `ConfigRead` with its problems, `Read::Unreadable`, `Read::Gone`).
pub(crate) trait Reads {
    /// `specforge.json` at `root`, read once (`read_project_config`).
    fn config(&mut self, root: &ProjectRoot) -> ConfigRead;
    /// Before the extension runtime and the environment read them: the lock and
    /// the module of every extension `config` enables (a prediction until a
    /// reader reaches the installed crate, ADR 0030 D3).
    fn before_load(&mut self, root: &ProjectRoot, config: &ProjectConfig);
    /// The keys of the sources discovery finds under `sources` now, sorted.
    fn discover(&mut self, sources: &SourceRoot) -> Vec<String>;
    /// Source `key` under `spec_root`: its text, its E025, or gone.
    fn source(&mut self, spec_root: &Path, key: &str) -> Read;
    /// Run `check` (the checks over one snapshot) with the disk they read
    /// through (ADR 0107): the ledger's records and stamps each read as a
    /// check input; `Unrecorded`'s only reads. Whether the session's inputs
    /// moved (always `false` unrecorded).
    fn run_checks(&mut self, check: impl FnOnce(&dyn Disk)) -> bool;
}

/// A one-shot compile's reads: the same steps, nothing recorded, no stamp (ADR
/// 0030, ADR 0047 D1).
pub(crate) struct Unrecorded;

impl Reads for Unrecorded {
    fn config(&mut self, root: &ProjectRoot) -> ConfigRead {
        read_project_config(root.dir())
    }

    fn before_load(&mut self, _root: &ProjectRoot, _config: &ProjectConfig) {}

    fn discover(&mut self, sources: &SourceRoot) -> Vec<String> {
        walked_keys(sources)
    }

    fn source(&mut self, spec_root: &Path, key: &str) -> Read {
        sources::read(spec_root, key)
    }

    fn run_checks(&mut self, check: impl FnOnce(&dyn Disk)) -> bool {
        check(&PlainDisk);
        false
    }
}

/// The keys of the files discovery finds under `sources`, in its order.
fn walked_keys(sources: &SourceRoot) -> Vec<String> {
    sources
        .files()
        .iter()
        .map(|path| key::discovered(sources.dir(), path))
        .collect()
}

impl Reads for Ledger {
    /// Stamped, then read once. A config rewritten after this is seen next
    /// time, whatever the rest was read from. Starts the environment record
    /// again.
    fn config(&mut self, root: &ProjectRoot) -> ConfigRead {
        self.stamps.stamp_config(&root.config_file());
        read_project_config(root.dir())
    }

    /// Records the inputs of the environment `config` names, and stamps the
    /// lock and each extension module.
    fn before_load(&mut self, root: &ProjectRoot, config: &ProjectConfig) {
        self.inputs = SessionInputs::opened(root.dir(), config);
        self.stamps.stamp_environment(&self.inputs);
    }

    /// Records the keys and each symlink's target, and forgets the stamps of
    /// the sources: each is stamped when it is read ([`Self::source`]).
    fn discover(&mut self, sources: &SourceRoot) -> Vec<String> {
        let files = sources.files();
        self.stamps.forget_sources();
        self.inputs.set_links(&files);
        files
            .iter()
            .map(|path| key::discovered(sources.dir(), path))
            .collect()
    }

    /// Stamped, then read: a source written after its stamp is seen next time.
    fn source(&mut self, spec_root: &Path, key: &str) -> Read {
        self.source_with(spec_root, key, |_| sources::read(spec_root, key))
    }

    /// The checks' reads, each stamped and recorded as a check input when
    /// the run finishes.
    fn run_checks(&mut self, check: impl FnOnce(&dyn Disk)) -> bool {
        let run = self.checks();
        check(&RunDisk(&run));
        run.finish()
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
        let moved = self.inputs.relink(keys);
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

    /// One run of the checks' reads: each is stamped and recorded as a check
    /// input when the run finishes (ADR 0106 D4).
    pub(crate) fn checks(&mut self) -> CheckRun<'_> {
        CheckRun {
            ledger: self,
            read: RefCell::new(Vec::new()),
        }
    }

    /// Stamp source `key`, then run `read` on its file (the read the caller
    /// performs): the one place a source is stamped for the cold read. The
    /// test hook of [`Reads::source`].
    pub(crate) fn source_with(
        &mut self,
        spec_root: &Path,
        key: &str,
        read: impl FnOnce(&Path) -> Read,
    ) -> Read {
        let path = spec_root.join(key);
        if self.inputs.root().is_some() {
            self.stamps.stamp_source(key, &path);
        }
        read(&path)
    }

    /// A ledger over `inputs`, nothing stamped.
    #[cfg(test)]
    pub(crate) fn over(inputs: SessionInputs) -> Self {
        Ledger {
            inputs,
            stamps: Stamps::default(),
        }
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

/// What a read of a check input is (how a later change to it is reached).
pub(crate) enum CheckInput {
    /// A file: its bytes or its existence (the build cache, a file a
    /// `file_reference` field or a `file_exists` rule names). Reached by its
    /// own path or a directory above it.
    File,
    /// A directory's entries (the E016 suggestion lists a missing file's
    /// directory). Also reached by any file directly in it.
    Listing,
}

/// One run of the checks' reads. Reading needs only `&self`, so a reader the
/// checks hold by shared reference can record through it.
#[must_use = "a run's reads are recorded only when it finishes"]
pub(crate) struct CheckRun<'l> {
    ledger: &'l mut Ledger,
    read: RefCell<Vec<(CheckInput, PathBuf, Option<Entry>)>>,
}

impl CheckRun<'_> {
    /// Stamp `path`, then run `read` on it (the read the caller performs)
    /// and return its result. A detached ledger's run stamps nothing and
    /// still runs `read`.
    pub(crate) fn read<T>(
        &self,
        input: CheckInput,
        path: &Path,
        read: impl FnOnce(&Path) -> T,
    ) -> T {
        if self.ledger.inputs.root().is_some() {
            let entry = Entry::stamp(path);
            self.read
                .borrow_mut()
                .push((input, path.to_path_buf(), entry));
        }
        read(path)
    }

    /// The check inputs are now exactly what this run read (ADR 0030 D2: renewed each time the checks run). Their canonical
    /// forms and missing directories are taken again; nothing else in the
    /// record changes. Whether the inputs moved.
    pub(crate) fn finish(self) -> bool {
        let CheckRun { ledger, read } = self;
        if ledger.inputs.root().is_none() {
            return false;
        }
        let before = ledger.inputs.clone();
        let mut files: Vec<PathBuf> = Vec::new();
        let mut listings: Vec<PathBuf> = Vec::new();
        let mut stamps: BTreeMap<PathBuf, Option<Entry>> = BTreeMap::new();
        for (input, path, entry) in read.into_inner() {
            let list = match input {
                CheckInput::File => &mut files,
                CheckInput::Listing => &mut listings,
            };
            if !list.contains(&path) {
                list.push(path.clone());
            }
            stamps.entry(path).or_insert(entry);
        }
        // A set: the order the checks asked in is no change.
        files.sort();
        listings.sort();
        ledger.inputs.set_checks(files, listings);
        ledger.stamps.set_checks(stamps);
        ledger.inputs != before
    }
}
