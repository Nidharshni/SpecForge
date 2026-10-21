//! A long-lived compiled project: what watch, the LSP and MCP hold.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use specforge_common::{Diagnostic, ProjectConfig, ProjectRoot};
use specforge_graph::{Applied, GraphDelta, NodeChange};

use crate::buffers::{Buffer, Held};
use crate::compiled::CompiledProject;
use crate::diagnostics_delta::DiagnosticsDelta;
use crate::ledger::{Changes, Ledger, SessionInputs, SourceDisk};
use crate::sources::Read;
use crate::{Environment, SharedRuntime};

/// Builds the runtime of the project at a root from the config read there.
pub type BuildRuntime = Arc<dyn Fn(&Path, &ProjectConfig) -> SharedRuntime + Send + Sync>;

/// How a session gets the runtime its project's extensions run in.
#[derive(Clone)]
pub enum RuntimeSource {
    /// Built for each environment load from the config that load read,
    /// after every environment input is stamped: the project's own
    /// component runtime ([`RuntimeSource::project`]), or a test's.
    Build(BuildRuntime),
    /// The same runtime for every load (none: no extension loads).
    Fixed(Option<SharedRuntime>),
}

impl RuntimeSource {
    /// The project's own component runtime, with the per-user compile
    /// cache ([`specforge_component::ComponentRuntime::with_user_cache`]):
    /// empty, for the environment's extension load to fill.
    pub fn project() -> Self {
        RuntimeSource::Build(Arc::new(|_, _| {
            Arc::new(specforge_component::ComponentRuntime::with_user_cache())
        }))
    }

    /// The runtime of the environment load at `root` with `config`.
    pub(crate) fn runtime_for(&self, root: &Path, config: &ProjectConfig) -> Option<SharedRuntime> {
        match self {
            RuntimeSource::Build(build) => Some(build(root, config)),
            RuntimeSource::Fixed(runtime) => runtime.clone(),
        }
    }
}

/// Which checks an update runs on the updated graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckMode {
    /// Every check `specforge check` runs.
    Full,
    /// The editor's fast path while typing: when any file this update
    /// changes has parse errors the graph is broken and the checks would
    /// evaluate garbage, so they are skipped, and only the parse layer is
    /// reported until an update runs them. Otherwise, [`Self::Full`].
    SyntaxOnlyIfParseErrors,
}

/// What kind of change an [`Update`] applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateKind {
    /// Sources were re-parsed (and every check ran again).
    Sources,
    /// No source changed and the checks ran again: a check input changed, or an earlier update
    /// had skipped them.
    Checks,
    /// The environment loaded again and the project was rebuilt from disk.
    Environment,
}

/// What one update of a session did.
#[derive(Debug)]
pub struct Update {
    /// What was applied: sources, the checks alone, or the environment.
    pub kind: UpdateKind,
    /// The session's inputs changed: a file the checks read was named or
    /// dropped, a source became or stopped being a symlink or points
    /// elsewhere, or the environment loaded again with other inputs. What an
    /// adapter watches must follow [`ProjectSession::inputs`] (ADR 0030).
    pub inputs_changed: bool,
    pub delta: GraphDelta,
    /// The nodes the update moved: [`Applied::moved`]. A move changes no
    /// graph fact, so the delta does not list it; it changes what the graph
    /// documents write.
    pub moved: Vec<NodeChange>,
    /// The files re-parsed or dropped: exactly the changed ones (sorted).
    pub rebuilt_files: Vec<String>,
    /// Everything the session reports now ([`CompiledProject::diagnostics`],
    /// in its order), shared with its compiled project.
    pub diagnostics: Arc<[Diagnostic]>,
    /// Everything it reported before this update.
    pub previous_diagnostics: Arc<[Diagnostic]>,
    /// The comparison of the incremental graph with a cold rebuild, when
    /// the session verifies its updates.
    pub verification: Option<Result<(), String>>,
}

impl Update {
    /// Nothing changed: no file was applied and no check ran (the session
    /// reports the list it reported, held twice: every change of a compiled
    /// project starts its report again), and the inputs are as they were.
    /// What `release` and `apply` answer `None` for.
    fn changed_nothing(&self) -> bool {
        !self.inputs_changed && Arc::ptr_eq(&self.previous_diagnostics, &self.diagnostics)
    }

    /// The diagnostics this update added and removed, compared whole
    /// ([`DiagnosticsDelta::between`] of what the session reported before it
    /// and reports now). Computed on each call, so an update nobody asks
    /// about compares nothing (ADR 0024 D6); an update that changed nothing
    /// holds one list twice and compares nothing either.
    pub fn diagnostics_delta(&self) -> DiagnosticsDelta {
        if Arc::ptr_eq(&self.previous_diagnostics, &self.diagnostics) {
            return DiagnosticsDelta::default();
        }
        DiagnosticsDelta::between(&self.previous_diagnostics, &self.diagnostics)
    }

    /// How the incremental graph differs from a cold rebuild, when this
    /// update was verified and the two differ: what every surface reports
    /// where it reports (watch in its event, the LSP in its log, MCP as a
    /// debug assertion; ADR 0035).
    pub fn divergence(&self) -> Option<&str> {
        match &self.verification {
            Some(Err(divergence)) => Some(divergence),
            Some(Ok(())) | None => None,
        }
    }
}

/// A compiled project kept current, and what it is built from: its inputs
/// (ADR 0030), the stamps of what it last read, and where each environment
/// load gets its extension runtime. It is seeded by one cold build, then
/// kept current incrementally (ADR 0006, 0032): only the changed files are
/// re-read and re-parsed and the graph build applies them whole
/// ([`specforge_graph::GraphBuild`]), the imports of every parse are
/// resolved again (E025, I004, W113, W027), and the graph checks re-run on
/// the patched graph. After every update that runs the checks its compiled
/// project reports what a fresh compile of the same sources reports, in the
/// same order (ADR 0047).
pub struct ProjectSession {
    /// What it compiled, kept current.
    project: CompiledProject,
    /// Where each environment load gets its runtime: a built one is fresh
    /// for every load (the extensions' `.wasm` files may have changed).
    source: RuntimeSource,
    /// What the session depends on (ADR 0030) and what it read of it, as it
    /// was when read: what [`Self::stale`] compares with disk (ADR 0106).
    /// Detached (an editor with no workspace folder), files are buffers keyed
    /// by absolute path, with no spec root to resolve their imports against
    /// and no environment to reload.
    ledger: Ledger,
    /// The editor buffers the session holds: each is the truth for its file
    /// until released (ADR 0046).
    held: Held,
}

/// A project whose environment is loaded and whose sources are not read yet
/// ([`ProjectSession::begin_open`]). [`ProjectSession::reopen`] hands its
/// environment over (an editor's keyword completion is served from it) before
/// [`Self::finish_holding`] reads and builds the sources.
struct OpeningProject {
    env: Arc<Environment>,
    /// Where each environment load gets its runtime.
    source: RuntimeSource,
    /// What the loaded environment depends on, stamped; the sources are
    /// stamped by `finish`.
    ledger: Ledger,
}

impl OpeningProject {
    /// The loaded environment: what the finished session will have, until
    /// the environment is loaded again.
    fn environment(&self) -> &Arc<Environment> {
        &self.env
    }

    /// Read every source, build the graph and run the checks: the session
    /// [`ProjectSession::open`] returns. [`Self::finish_holding`] with no
    /// buffer.
    fn finish(self) -> ProjectSession {
        self.finish_holding(Vec::new())
    }

    /// [`Self::finish`], holding `buffers`: each held buffer's text is read
    /// in place of its file by the one cold build, so the checks run once.
    fn finish_holding(self, buffers: Vec<Buffer>) -> ProjectSession {
        let mut ledger = self.ledger;
        // Keys as this environment gives them: the spec root may have moved.
        let held = Held::keyed(buffers, |path| self.env.source_key(path));
        // What was stamped is what is read, except a held file: its buffer
        // is read in its place.
        let texts = held.sources(|key| !ledger.excludes(key));
        let mut project = CompiledProject::read(self.env, &texts, &mut ledger);
        project.set_verify(cfg!(debug_assertions));
        let mut session = ProjectSession {
            project,
            source: self.source,
            ledger,
            held,
        };
        session.project.check(&mut session.ledger);
        session
    }
}

impl ProjectSession {
    /// A session with no project: no config, no extension and no file
    /// until a buffer is added.
    pub fn detached() -> Self {
        ProjectSession {
            project: CompiledProject::detached(),
            source: RuntimeSource::Fixed(None),
            ledger: Ledger::detached(),
            held: Held::default(),
        }
    }

    /// Open the project at `root`, running its extensions in the project's
    /// own runtime ([`RuntimeSource::project`]).
    pub fn open(root: &ProjectRoot) -> Self {
        Self::open_from(root, RuntimeSource::project())
    }

    /// Open the project at `root` with `runtime` (none: no extension
    /// loads). A reload keeps using the same runtime (tests: production opens with [`Self::open`] or
    /// [`Self::open_through`]).
    #[cfg(feature = "testing")]
    pub fn open_with_runtime(root: &ProjectRoot, runtime: Option<SharedRuntime>) -> Self {
        Self::open_from(root, RuntimeSource::Fixed(runtime))
    }

    /// Open the project at `root`, each environment load getting its
    /// runtime from `source`.
    pub fn open_from(root: &ProjectRoot, source: RuntimeSource) -> Self {
        Self::open_through(root, source, None)
    }

    /// [`Self::open_from`], reading each source through `disk` (none: the plain file system). The
    /// environment and the checks are always read plain; a reload keeps `disk` (ADR 0145).
    pub fn open_through(
        root: &ProjectRoot,
        source: RuntimeSource,
        disk: Option<SourceDisk>,
    ) -> Self {
        Self::begin_open(root, source, disk).finish()
    }

    /// The first half of [`Self::open_from`] and [`Self::reopen`]: the
    /// environment loaded (config, extensions, registries; each load getting
    /// its runtime from `source`), no `.spec` file read.
    fn begin_open(
        root: &ProjectRoot,
        source: RuntimeSource,
        disk: Option<SourceDisk>,
    ) -> OpeningProject {
        // The ledger stamps each input in the read that reads it: the config,
        // the lock and the modules in the environment load, the sources by
        // `finish`.
        let mut ledger = Ledger::opening_through(disk);
        let env = Environment::read_through(&mut ledger, root, |config| {
            source.runtime_for(root.dir(), config)
        });
        OpeningProject {
            env: Arc::new(env),
            source,
            ledger,
        }
    }

    /// Compare every update's graph with a cold rebuild (costly). A debug
    /// build verifies every session by default; this turns it on in release
    /// (watch's `--verify-incremental`) or off.
    pub fn set_verify_incremental(&mut self, enabled: bool) {
        self.project.set_verify(enabled);
    }

    /// The editor holds these buffers now, each the truth for its file until
    /// it is released ([`Self::release`]), whatever happens to the file on
    /// disk: one update, then every check. A buffer whose text is the text the
    /// session built its file from changes nothing (its version is still
    /// kept). A buffer of a file discovery would not find (outside the spec
    /// root, excluded, not `.spec`) is held and builds nothing.
    pub fn hold(&mut self, buffers: &[Buffer]) -> Update {
        self.hold_with(buffers, CheckMode::Full)
    }

    /// [`Self::hold`], running the checks `mode` asks for.
    pub fn hold_with(&mut self, buffers: &[Buffer], mode: CheckMode) -> Update {
        let mut reads = Vec::new();
        for buffer in buffers {
            let key = self.source_key(&buffer.path);
            let read = Read::Text(buffer.text.clone());
            if !self.ledger.excludes(&key) {
                reads.push((key.clone(), read));
            }
            self.held.hold(key, buffer.clone());
        }
        let reads = self.staged(reads);
        self.update(|s| s.sources(reads, mode, false))
    }

    /// The editor no longer holds the buffers of `paths` (absolute): each
    /// file is the disk's again. A project source is stamped, then read
    /// through the session's one read: its text, its E025, or gone. Any
    /// other file (outside the spec root, excluded, not `.spec`, inside a
    /// nested project) was never built and changes nothing, unless the
    /// project holds it (a nested project took it since): then it leaves. With no project the buffer was the file's
    /// only text, so the file leaves. One update for all of them. `None`
    /// when none changed (each source's disk text is the text the session
    /// built it from) and the checks are current: a release after the
    /// typing fast path skipped them runs them.
    pub fn release(&mut self, paths: &[PathBuf]) -> Option<Update> {
        let update = self.update(|s| {
            let mut reads = Vec::new();
            for path in paths {
                let key = s.source_key(path);
                s.held.release(&key);
                // Stamped, then read (the ledger): a saved buffer leaves nothing
                // stale. With no project the buffer was the file's only text.
                let mut read = s.ledger.reread(std::slice::from_ref(&key));
                let read = read.pop().map_or(Read::Gone, |(_, read)| read);
                reads.push((key, read));
            }
            let reads = s.staged(reads);
            s.sources(reads, CheckMode::Full, false)
        });
        (!update.changed_nothing()).then_some(update)
    }

    /// The buffer the editor holds for source `key`, as the session last
    /// built from it.
    pub fn buffer(&self, key: &str) -> Option<&Buffer> {
        self.held.get(key)
    }

    /// What changed on disk among what `paths` reach (behavior
    /// `classify_project_changes`): the comparison [`Self::stale`] makes,
    /// narrowed to them. `paths` are absolute, spelled as a file watcher or
    /// an editor reports them, and may name files or directories that exist,
    /// were removed or never existed.
    ///
    /// - A path under the spec root reaches the sources at or under it. For a
    ///   directory that is every source discovery finds in it now and every
    ///   source the session read from it; for a file, itself. A source path
    ///   is compared by its directory's canonical form and its own name.
    /// - The spec root, or a directory above it, reaches every source.
    /// - An environment or check input is reached by its own path or by a
    ///   directory above it. A missing check input's directory is also
    ///   reached by any file in it.
    /// - A reached source or input is a change when it differs from its stamp
    ///   (size and modification time, the content within the timestamp
    ///   granularity). A directory created on the way to a missing input
    ///   directory outside the root is that input's change whatever the
    ///   stamps say.
    ///
    /// Every file a buffer holds is left out. A detached session returns
    /// nothing: its sources are its buffers.
    pub fn changes<'p>(&self, paths: impl IntoIterator<Item = &'p Path>) -> Changes {
        let mut changes = self.ledger.changes(paths);
        self.held.leave_out(&mut changes.sources);
        changes
    }

    /// The key of the file at `path` in this session's sources.
    pub fn source_key(&self, path: &Path) -> String {
        self.project.environment().source_key(path)
    }

    /// The one bracket every update goes through (ADR 0136): what the
    /// session reports and a copy of what it depends on are taken before
    /// `work` runs, and the `Update` is made after it, here and nowhere
    /// else. `inputs_changed` is the comparison of the two inputs. `work`
    /// reads through the ledger, changes the project (or replaces the
    /// session), and says what kind of update that was and what it applied.
    fn update(&mut self, work: impl FnOnce(&mut Self) -> (UpdateKind, Applied)) -> Update {
        let previous_diagnostics = self.project.reported();
        let inputs = self.ledger.inputs().clone();
        let (kind, applied) = work(self);
        let Applied {
            files,
            delta,
            moved,
            verification,
        } = applied;
        Update {
            kind,
            inputs_changed: *self.ledger.inputs() != inputs,
            delta,
            moved,
            rebuilt_files: files,
            diagnostics: self.project.reported(),
            previous_diagnostics,
            verification,
        }
    }

    /// The one rule for whether a read changes the project (ADR 0136 D3): it
    /// does unless the project already holds what it gives. The reads that
    /// change it, in order.
    fn staged(&self, reads: Vec<(String, Read)>) -> Vec<(String, Read)> {
        reads
            .into_iter()
            .filter(|(key, read)| !self.project.is_current(key, read))
            .collect()
    }

    /// The tail of every source update (inside the bracket): apply `reads`,
    /// resolve the imports, then run or skip the checks. With nothing to
    /// apply it runs the checks only when they are owed: a check input
    /// changed (`rechecks`, only `apply` passes it) or an earlier update
    /// skipped them; else it changes nothing (a buffer or a source names
    /// nothing else that can have moved, ADR 0055).
    fn sources(
        &mut self,
        reads: Vec<(String, Read)>,
        mode: CheckMode,
        rechecks: bool,
    ) -> (UpdateKind, Applied) {
        if reads.is_empty() {
            if rechecks || self.project.checks_skipped() {
                self.project.check(&mut self.ledger);
                return (UpdateKind::Checks, Applied::default());
            }
            return (UpdateKind::Sources, Applied::default());
        }
        let applied = self.project.apply(reads);
        if mode == CheckMode::SyntaxOnlyIfParseErrors
            && self.project.has_parse_errors_in(&applied.files)
        {
            self.project.skip_checks();
        } else {
            self.project.check(&mut self.ledger);
        }
        (UpdateKind::Sources, applied)
    }

    /// `specforge.json` or an extension changed: load the environment again
    /// and rebuild, from the sources on disk and the held buffers, in one
    /// cold build that runs the checks once.
    pub fn reload_environment(&mut self) -> Update {
        // The session keeps the root it was opened at: it is never located again. Nothing on disk
        // to load again without one.
        let Some(root) = self.project.environment().project_root().cloned() else {
            return self.update(|_| (UpdateKind::Environment, Applied::default()));
        };
        let source = self.source.clone();
        let disk = self.ledger.source_disk();
        self.reopen_through(&root, source, disk, |_| {})
    }

    /// Open the project at `root` in this session's place, each environment load getting its runtime
    /// from `source`: the environment loads first (config, lock, extensions; every input stamped before
    /// it is read) and is handed to `loaded` before any `.spec` file is read; then one cold build reads
    /// every source, each buffer this session holds in place of its file, and runs the checks once.
    /// What carries over: the held buffers (keyed again by the new environment: the spec root may have
    /// moved) and whether the session verifies its updates. Everything else is the new project's: its
    /// inputs, stamps and runtime source (later reloads use `source`). The update is the replacement:
    /// kind `Environment`, every file of the new graph rebuilt, the delta between the two graphs, the
    /// inputs changed when they differ, no verification (nothing incremental ran).
    ///
    /// `loaded` runs once and must not touch the session: it gets only the environment. A root with no
    /// `specforge.json` opens as a loose root. If the environment load or `loaded` panics, the session is
    /// unchanged (the buffers are taken after both).
    pub fn reopen(
        &mut self,
        root: &ProjectRoot,
        source: RuntimeSource,
        loaded: impl FnOnce(&Arc<Environment>),
    ) -> Update {
        self.reopen_through(root, source, None, loaded)
    }

    /// [`Self::reopen`], reading each source through `disk` (none: the plain file system), which a
    /// later reload keeps (ADR 0145).
    pub fn reopen_through(
        &mut self,
        root: &ProjectRoot,
        source: RuntimeSource,
        disk: Option<SourceDisk>,
        loaded: impl FnOnce(&Arc<Environment>),
    ) -> Update {
        self.update(|s| {
            let opening = Self::begin_open(root, source, disk);
            loaded(opening.environment());
            let buffers = std::mem::take(&mut s.held).into_buffers();
            let mut next = opening.finish_holding(buffers);
            next.project.set_verify(s.project.verifies());
            let previous = std::mem::replace(s, next);
            // The replacement: every file of the new graph rebuilt.
            (
                UpdateKind::Environment,
                s.project.replacing(&previous.project),
            )
        })
    }

    /// What the session compiled, as it is now: its graph, environment,
    /// root, report, source texts and memo. Every reader goes through it.
    pub fn project(&self) -> &CompiledProject {
        &self.project
    }

    /// What the session depends on now (ADR 0030): what a changed path is,
    /// which directories to watch, which files to report.
    pub fn inputs(&self) -> &SessionInputs {
        self.ledger.inputs()
    }

    /// Apply `changes` this session made ([`Self::changes`], [`Self::stale`]) as one update (ADR
    /// 0159). A changed environment input reloads the environment, which reads every source and
    /// runs every check, so it covers the rest of `changes`. Otherwise the named sources are read
    /// again, by path relative to the spec root: a file that is gone was deleted; one that is there
    /// and cannot be read is E025 and leaves the graph; a changed path that is no source (an
    /// `exclude` entry, a skipped directory, not `.spec`, inside a nested project) changes nothing,
    /// unless the project holds it: then it leaves the graph. A source that reads as the session
    /// holds it is re-stamped and changes nothing (ADR 0136 D3). The checks then run once if a
    /// source changed, a check input changed, or an earlier update skipped them. Every file a
    /// buffer holds is left out, including one held since `changes` were made. `None` when applying
    /// `changes` changes nothing: no source and no check input is named (once the held files are
    /// left out), or every named source reads as the session holds it, no check input is named and
    /// the inputs did not move. A detached session makes no changes (ADR 0073 D7).
    pub fn apply(&mut self, changes: &Changes) -> Option<Update> {
        if !changes.environment.is_empty() {
            return Some(self.reload_environment());
        }
        let mut sources = changes.sources.clone();
        self.held.leave_out(&mut sources);
        let rechecks = !changes.check_inputs.is_empty();
        if sources.is_empty() && !rechecks {
            return None;
        }
        let update = self.update(|s| {
            let reads = s.ledger.reread(&sources);
            let reads = s.staged(reads);
            s.sources(reads, CheckMode::Full, rechecks)
        });
        (!update.changed_nothing()).then_some(update)
    }

    /// What changed on disk since the session last built (behavior
    /// `bring_session_up_to_date`): the sources discovery finds now against
    /// those it read, and every environment and check input against what
    /// it read, every file a buffer holds left out, whatever happened to it.
    /// A session that was not opened from disk reports nothing.
    pub fn stale(&self) -> Changes {
        let mut changes = self.ledger.stale();
        self.held.leave_out(&mut changes.sources);
        changes
    }

    /// Bring the session up to date with disk, without a watcher: apply
    /// exactly what [`Self::stale`] finds. Afterwards its graph and
    /// diagnostics are what a fresh compile of the files on disk gives.
    /// `None` when nothing changed.
    pub fn ensure_fresh(&mut self) -> Option<Update> {
        let changes = self.stale();
        self.apply(&changes)
    }

    /// The runtime the project's extensions were loaded in (its
    /// environment's; none: no extension loads).
    pub fn runtime(&self) -> Option<&SharedRuntime> {
        self.project.environment().runtime()
    }
}
