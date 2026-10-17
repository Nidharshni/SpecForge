//! The read ledger: what a project session depends on, what it read of it and
//! when (behavior `classify_project_changes`, ADR 0030, ADR 0073; behavior
//! `bring_session_up_to_date`).
//!
//! A session is built from its sources (the `.spec` files discovery finds
//! under the spec root, each read through the recording run, each link hop
//! included, ADR 0129), its environment inputs (the files the environment
//! load read: `specforge.json`, `specforge.lock`, the extension modules, and
//! each hop of a symlink among them, ADR 0114) and
//! its check inputs (files the checks read but no parse does: the build
//! cache check-phase passes read, the files `file_reference` fields and
//! `file_exists` rules name). [`SessionInputs`] is that set as one value;
//! the session asks its inputs what a changed path reaches
//! (`ProjectSession::changes`), and watch and the LSP ask them which
//! directories to watch and what to report. The stamps (what each input
//! looked like when it was read) are compared with disk by
//! `ProjectSession::stale` and `changes`, and are taken in the call that
//! reads it, before the read, so a write that races the read is seen next time.
//!
//! The record is one table of recorded inputs (ADR 0133): every run is recorded by
//! [`Run::finish`], told what it recorded ([`Recording`]), and [`Ledger::record`] is the
//! one writer of the rows and the stamps it covers.

mod disk;
mod inputs;
mod key;
mod reach;
mod recorded;
mod stamps;
#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use specforge_common::{ProjectRoot, SourceRoot};
use specforge_registry::{Disk, PlainDisk};

use crate::Environment;
use crate::sources::{self, Read};

use disk::RunDisk;
pub use disk::SourceDisk;
pub use inputs::{SessionInputs, WatchRoot, Watched};
pub use key::source_key;
use recorded::{Class, Recording};
use stamps::{Entry, Scope, Stamps};

/// What a batch of changed paths, or the disk since the last read, means to
/// a session: the input to [`crate::ProjectSession::apply`]. Made only by
/// [`crate::ProjectSession::changes`] and [`crate::ProjectSession::stale`]
/// (ADR 0100 D5); a reader reads it through its accessors.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Changes {
    /// Changed sources, by key relative to the spec root: sorted, deduplicated.
    pub(crate) sources: Vec<String>,
    /// The environment must load again. Each path of the batch that reached
    /// an environment input which changed, as the batch spelled it (for
    /// `stale`, each changed input as the session recorded it): sorted,
    /// deduplicated.
    pub(crate) environment: Vec<PathBuf>,
    /// The checks must run again: the paths that reached a changed check
    /// input, named the same way.
    pub(crate) check_inputs: Vec<PathBuf>,
}

impl Changes {
    /// Nothing the session is built from changed.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty() && self.environment.is_empty() && self.check_inputs.is_empty()
    }

    /// The changed sources, by key relative to the spec root: sorted, deduplicated.
    pub fn sources(&self) -> &[String] {
        &self.sources
    }

    /// The paths that reached a changed environment input: sorted, deduplicated.
    pub fn environment(&self) -> &[PathBuf] {
        &self.environment
    }

    /// The paths that reached a changed check input: sorted, deduplicated.
    pub fn check_inputs(&self) -> &[PathBuf] {
        &self.check_inputs
    }
}

/// How a compile reads the disk (ADR 0106). A project session reads through its
/// [`Ledger`], which stamps each file in the call that reads it and records it as
/// an input; a one-shot compile reads through [`Unrecorded`], which only reads.
/// Both run the same steps in the same order: the environment load (the config
/// once, the lock and modules after it), the sources after discovery, the check
/// inputs before the checks. A step never fails: an unreadable file is in what it
/// returns (a `ConfigRead` with its problems, `Read::Unreadable`, `Read::Gone`).
pub(crate) trait Reads {
    /// Run `load` (the environment load: `specforge.json` read once, the
    /// runtime built from it, the lock and the modules) with the disk it reads
    /// through (ADR 0114): the ledger stamps each read and records it as an
    /// environment input, each link hop included, and the session's inputs
    /// become those of the environment `load` returns; `Unrecorded`'s only
    /// reads (`PlainDisk`).
    fn environment(
        &mut self,
        root: &ProjectRoot,
        load: impl FnOnce(&dyn Disk) -> Environment,
    ) -> Environment;
    /// The keys of the sources discovery finds under `sources` now, sorted.
    fn discover(&mut self, sources: &SourceRoot) -> Vec<String>;
    /// Source `key` under `spec_root`: its text, its E025, or gone.
    fn source(&mut self, spec_root: &Path, key: &str) -> Read;
    /// Run `check` (the checks over one snapshot) with the disk they read
    /// through (ADR 0107): the ledger's records and stamps each read as a
    /// check input; `Unrecorded`'s only reads.
    fn run_checks(&mut self, check: impl FnOnce(&dyn Disk));
}

/// A one-shot compile's reads: the same steps, nothing recorded, no stamp (ADR
/// 0030, ADR 0047 D1).
pub(crate) struct Unrecorded;

impl Reads for Unrecorded {
    fn environment(
        &mut self,
        _root: &ProjectRoot,
        load: impl FnOnce(&dyn Disk) -> Environment,
    ) -> Environment {
        load(&PlainDisk)
    }

    fn discover(&mut self, sources: &SourceRoot) -> Vec<String> {
        walked_keys(sources)
    }

    fn source(&mut self, spec_root: &Path, key: &str) -> Read {
        sources::read(&PlainDisk, spec_root, key)
    }

    fn run_checks(&mut self, check: impl FnOnce(&dyn Disk)) {
        check(&PlainDisk);
    }
}

/// The keys of the sources discovery finds under `sources`, in its order,
/// and of the links it finds that point nowhere: the session reads both
/// (ADR 0141), sorted.
fn walked_keys(sources: &SourceRoot) -> Vec<String> {
    key::discovered_keys(sources, &sources.walk_at(sources.dir()))
}

impl Reads for Ledger {
    /// Each file the load reads is stamped, then read, and is an environment
    /// input; a file rewritten after its stamp is seen next time. Starts the
    /// environment record, and the check inputs, again.
    fn environment(
        &mut self,
        root: &ProjectRoot,
        load: impl FnOnce(&dyn Disk) -> Environment,
    ) -> Environment {
        let run = self.environment_run();
        let environment = load(&RunDisk(&run, &PlainDisk));
        run.finish(Recording::Environment {
            root: root.dir(),
            config: environment.config(),
        });
        environment
    }

    /// Starts the record of the sources again: no source is stamped and no
    /// link hop recorded until it is read ([`Self::source`]); a held source is
    /// never read here (ADR 0106 D2, ADR 0129).
    fn discover(&mut self, sources: &SourceRoot) -> Vec<String> {
        self.record(Recording::Sources, Vec::new());
        walked_keys(sources)
    }

    /// Stamped, then read, each link hop included: a source written after its
    /// stamp is seen next time.
    fn source(&mut self, spec_root: &Path, key: &str) -> Read {
        self.read_source(spec_root, key, |disk| sources::read(disk, spec_root, key))
    }

    /// The checks' reads, each stamped and recorded as a check input when
    /// the run finishes.
    fn run_checks(&mut self, check: impl FnOnce(&dyn Disk)) {
        let run = self.checks();
        check(&RunDisk(&run, &PlainDisk));
        run.finish(Recording::Checks);
    }
}

/// What a project session read and when: its inputs (ADR 0030) and the stamp of
/// each, taken before the read it describes. The one module that touches the
/// stamps (ADR 0106).
pub(crate) struct Ledger {
    inputs: SessionInputs,
    stamps: Stamps,
    /// The disk a source is read through; none reads the plain file system (ADR 0145).
    source_disk: Option<SourceDisk>,
}

impl Ledger {
    /// A detached session's: no root; every method reads nothing and records
    /// nothing; `changes` and `stale` are empty; `reread` gives `Read::Gone`
    /// for every key (the buffer was the only text).
    pub(crate) fn detached() -> Self {
        Ledger {
            inputs: SessionInputs::detached(),
            stamps: Stamps::default(),
            source_disk: None,
        }
    }

    /// About to open a project: nothing read yet, its sources read through `disk` (none: plain).
    pub(crate) fn opening_through(disk: Option<SourceDisk>) -> Self {
        Ledger {
            source_disk: disk,
            ..Self::detached()
        }
    }

    /// The disk the sources are read through, so a reload keeps it.
    pub(crate) fn source_disk(&self) -> Option<SourceDisk> {
        self.source_disk.clone()
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
    /// read, in order.
    pub(crate) fn reread(&mut self, keys: &[String]) -> Vec<(String, Read)> {
        let Some(spec_root) = self.inputs.spec_root().map(Path::to_path_buf) else {
            return keys.iter().map(|key| (key.clone(), Read::Gone)).collect();
        };
        keys.iter()
            .map(|key| {
                let read = if self.inputs.excludes(key) {
                    // No source (any more): a record kept would hide its return.
                    self.forget_source(&spec_root, key);
                    Read::Gone
                } else {
                    self.read_source(&spec_root, key, |disk| sources::read(disk, &spec_root, key))
                };
                (key.clone(), read)
            })
            .collect()
    }

    /// One run of the checks' reads: each is stamped and recorded as a check
    /// input when the run finishes (ADR 0106 D4).
    pub(crate) fn checks(&mut self) -> Run<'_> {
        self.run()
    }

    /// One run of reads, recorded when the ledger has a root.
    fn run(&mut self) -> Run<'_> {
        Run {
            records: self.inputs.root().is_some(),
            ledger: self,
            read: RefCell::new(Vec::new()),
        }
    }

    /// One run of the environment load's reads: each is stamped and recorded
    /// as an environment input when the run finishes (ADR 0114). The inputs
    /// are still detached when it starts, so it records regardless.
    fn environment_run(&mut self) -> Run<'_> {
        Run {
            records: true,
            ledger: self,
            read: RefCell::new(Vec::new()),
        }
    }

    /// The one read of source `key` a session makes, for the cold read and
    /// every reread (ADR 0129): `read` runs on the recording run's disk,
    /// which stamps each link hop the read follows, then the file, before
    /// reading it; the source's stamp (by key) and its hops are then exactly
    /// what this read saw. A detached ledger records nothing. `read` is a
    /// parameter so a test can write between the stamp and the read.
    pub(crate) fn read_source(
        &mut self,
        spec_root: &Path,
        key: &str,
        read: impl FnOnce(&dyn Disk) -> Read,
    ) -> Read {
        let path = spec_root.join(key);
        let disk = self.source_disk();
        let run = self.run();
        let plain: &dyn Disk = &PlainDisk;
        let text = read(&RunDisk(&run, disk.as_deref().map_or(plain, |d| d)));
        run.finish(Recording::Source { key, path: &path });
        text
    }

    /// Source `key` is no source (any more): its stamp and hops are
    /// forgotten.
    fn forget_source(&mut self, spec_root: &Path, key: &str) {
        let path = spec_root.join(key);
        self.record(Recording::Source { key, path: &path }, Vec::new());
    }

    /// The one writer of the record: the inputs and the stamps `recording`
    /// covers, in step (ADR 0133). Each path is recorded once per class, with
    /// its first stamp.
    fn record(&mut self, recording: Recording<'_>, read: Vec<(Reading, PathBuf, Option<Entry>)>) {
        let mut rows: Vec<(Class, PathBuf)> = Vec::new();
        let mut seen: BTreeSet<(Class, PathBuf)> = BTreeSet::new();
        let mut stamps: BTreeMap<(Class, PathBuf), Option<Entry>> = BTreeMap::new();
        let mut source: Option<Option<Entry>> = None;
        for (reading, path, entry) in read {
            let Some(class) = recording.class_of(reading, &path) else {
                // The source's own file: its first stamp.
                source.get_or_insert(entry);
                continue;
            };
            if class.stamped() {
                stamps.entry((class.clone(), path.clone())).or_insert(entry);
            }
            // A set, not a scan of `rows`: a check run can read thousands of files.
            if seen.insert((class.clone(), path.clone())) {
                rows.push((class, path));
            }
        }
        if matches!(recording, Recording::Checks) {
            // A set: the order the checks asked in is no change.
            rows.sort();
        }
        self.stamps.replace(&recording, stamps, source);
        self.inputs.replace(&recording, rows);
    }

    /// A ledger over `inputs`, nothing stamped.
    #[cfg(test)]
    pub(crate) fn over(inputs: SessionInputs) -> Self {
        Ledger {
            inputs,
            stamps: Stamps::default(),
            source_disk: None,
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

/// What a read the ledger records is (how a later change to it is reached).
pub(crate) enum Reading {
    /// A file: its bytes or its existence (the build cache, a file a
    /// `file_reference` field or a `file_exists` rule names). Reached by its
    /// own path or a directory above it.
    File,
    /// A directory's entries (the E016 suggestion lists a missing file's
    /// directory). Also reached by any file directly in it.
    Listing,
}

/// One run of reads the ledger records (ADR 0106 D4, ADR 0114). Reading needs
/// only `&self`, so a reader held by shared reference can record through it.
#[must_use = "a run's reads are recorded only when it finishes"]
pub(crate) struct Run<'l> {
    ledger: &'l mut Ledger,
    /// Whether a read is stamped and kept: false for a detached ledger's run,
    /// which still runs each read.
    records: bool,
    read: RefCell<Vec<(Reading, PathBuf, Option<Entry>)>>,
}

impl Run<'_> {
    /// Stamp `path`, then run `read` on it (the read the caller performs)
    /// and return its result. A detached ledger's run stamps nothing and
    /// still runs `read`.
    pub(crate) fn read<T>(&self, input: Reading, path: &Path, read: impl FnOnce(&Path) -> T) -> T {
        if self.records {
            let entry = Entry::stamp(path);
            self.read
                .borrow_mut()
                .push((input, path.to_path_buf(), entry));
        }
        read(path)
    }

    /// Record what this run read as `recording` (ADR 0133): the rows and
    /// stamps it covers become exactly these reads, each path once per class,
    /// each with its first stamp. A detached ledger's run records nothing.
    pub(crate) fn finish(self, recording: Recording<'_>) {
        let Run {
            ledger,
            records,
            read,
        } = self;
        if records {
            ledger.record(recording, read.into_inner());
        }
    }
}
