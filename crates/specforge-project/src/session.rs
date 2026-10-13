//! A long-lived compiled project: what watch, the LSP and MCP hold.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use specforge_common::{Diagnostic, ProjectConfig, ProjectRoot, read_project_config};
use specforge_graph::{Applied, GraphDelta};

use crate::buffers::{Buffer, Held};
use crate::compiled::CompiledProject;
use crate::freshness::{DiskSnapshot, Scope};
use crate::inputs::{Changes, SessionInputs, UpdateKind};
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
    fn runtime_for(&self, root: &Path, config: &ProjectConfig) -> Option<SharedRuntime> {
        match self {
            RuntimeSource::Build(build) => Some(build(root, config)),
            RuntimeSource::Fixed(runtime) => runtime.clone(),
        }
    }
}

/// What changed in a session's sources.
pub enum SourceChange<'a> {
    /// Files changed, created or deleted on disk, by path relative to the
    /// spec root. A file that is gone was deleted; one that is there and
    /// cannot be read is E025 and leaves the graph. A file a buffer holds is
    /// left as the buffer has it.
    Disk(&'a [String]),
    /// The editor holds these buffers now, each the truth for its file until
    /// it is released ([`ProjectSession::release`]), whatever happens to the
    /// file on disk: one update, one run of the checks. A buffer whose text
    /// is the text the session built its file from changes nothing (its
    /// version is still kept). A buffer of a file discovery would not find
    /// (outside the spec root, excluded, not `.spec`) is held and builds
    /// nothing.
    Hold(&'a [Buffer]),
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

/// What one update of a session did.
#[derive(Debug)]
pub struct Update {
    /// What was applied: sources, the checks alone, or the environment.
    pub kind: UpdateKind,
    /// The session's inputs changed: a file the checks read was named or
    /// dropped, or the environment loaded again with other inputs. What an
    /// adapter watches must follow [`ProjectSession::inputs`] (ADR 0030).
    pub inputs_changed: bool,
    pub delta: GraphDelta,
    /// The files re-parsed or dropped: exactly the changed ones (sorted).
    pub rebuilt_files: Vec<String>,
    /// Files whose graph-build diagnostics changed (sorted).
    pub changed_diagnostic_files: Vec<String>,
    /// Everything the session reports now: [`ProjectSession::diagnostics`].
    pub diagnostics: Vec<Diagnostic>,
    /// The comparison of the incremental graph with a cold rebuild, when
    /// the session verifies its updates.
    pub verification: Option<Result<(), String>>,
}

impl Update {
    /// The update of `kind` that applied `applied` (a graph build's apply,
    /// [`CompiledProject::replacing`], or nothing) and now reports
    /// `diagnostics`: the one place an update is assembled.
    fn of(
        kind: UpdateKind,
        inputs_changed: bool,
        applied: Applied,
        diagnostics: Vec<Diagnostic>,
    ) -> Update {
        let Applied {
            files,
            delta,
            changed_diagnostic_files,
            verification,
        } = applied;
        Update {
            kind,
            inputs_changed,
            delta,
            rebuilt_files: files,
            changed_diagnostic_files,
            diagnostics,
            verification,
        }
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
    /// What the session depends on (ADR 0030). Detached (an editor with no
    /// workspace folder), files are buffers keyed by absolute path, with no
    /// spec root to resolve their imports against and no environment to
    /// reload.
    inputs: SessionInputs,
    /// What the session last built from, as it was when read: what
    /// [`Self::stale`] compares with disk.
    snapshot: DiskSnapshot,
    /// The editor buffers the session holds: each is the truth for its file
    /// until released (ADR 0046).
    held: Held,
    /// The last update skipped the checks (the typing fast path): the next
    /// update, or a release, runs them even when it changes no file.
    checks_skipped: bool,
}

/// A project whose environment is loaded and whose sources are not read yet
/// ([`ProjectSession::begin_open`]). Its environment is shared, so a reader
/// can serve what needs only it (an editor's keyword completion) while
/// [`Self::finish`] reads and builds the sources.
pub struct OpeningProject {
    env: Arc<Environment>,
    /// Where each environment load gets its runtime.
    source: RuntimeSource,
    /// What the loaded environment depends on.
    inputs: SessionInputs,
    /// Stamped for the environment; the sources are stamped by `finish`.
    snapshot: DiskSnapshot,
}

impl OpeningProject {
    /// The loaded environment: what the finished session will have, until
    /// the environment is loaded again.
    pub fn environment(&self) -> &Arc<Environment> {
        &self.env
    }

    /// Read every source, build the graph and run the checks: the session
    /// [`ProjectSession::open`] returns. [`Self::finish_holding`] with no
    /// buffer.
    pub fn finish(self) -> ProjectSession {
        self.finish_holding(Vec::new())
    }

    /// [`Self::finish`], holding `buffers`: each held buffer's text is read
    /// in place of its file by the one cold build, so the checks run once.
    pub fn finish_holding(self, buffers: Vec<Buffer>) -> ProjectSession {
        let mut snapshot = self.snapshot;
        let discovered = self.inputs.discover();
        snapshot.stamp_all_sources(&self.inputs, &discovered);
        // Keys as this environment gives them: the spec root may have moved.
        let held = Held::keyed(buffers, |path| self.env.source_key(path));
        // What was stamped is what is read, except a held file: its buffer
        // is read in its place.
        let texts = held.sources(|key| !self.inputs.excludes(key));
        let mut project = CompiledProject::read(self.env, &discovered, &texts);
        project.set_verify(cfg!(debug_assertions));
        let mut session = ProjectSession {
            project,
            source: self.source,
            inputs: self.inputs,
            snapshot,
            held,
            checks_skipped: false,
        };
        session.check();
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
            inputs: SessionInputs::detached(),
            snapshot: DiskSnapshot::default(),
            held: Held::default(),
            checks_skipped: false,
        }
    }

    /// Open the project at `root`, running its extensions in the project's
    /// own runtime ([`RuntimeSource::project`]).
    pub fn open(root: &ProjectRoot) -> Self {
        Self::open_from(root, RuntimeSource::project())
    }

    /// Open the project at `root` with `runtime` (none: no extension
    /// loads). A reload keeps using the same runtime.
    pub fn open_with_runtime(root: &ProjectRoot, runtime: Option<SharedRuntime>) -> Self {
        Self::open_from(root, RuntimeSource::Fixed(runtime))
    }

    /// Open the project at `root`, each environment load getting its
    /// runtime from `source`.
    pub fn open_from(root: &ProjectRoot, source: RuntimeSource) -> Self {
        Self::begin_open(root, source).finish()
    }

    /// The first half of [`Self::open_from`]: the environment loaded
    /// (config, extensions, registries; each load getting its runtime from
    /// `source`), no `.spec` file read. What needs only the environment (the
    /// kinds and fields a keyword completion offers) is served from
    /// [`OpeningProject::environment`] while [`OpeningProject::finish`]
    /// reads and builds the sources.
    pub fn begin_open(root: &ProjectRoot, source: RuntimeSource) -> OpeningProject {
        // Everything is stamped before anything reads it (crate::freshness):
        // the config before its one read, the lock and the modules before
        // the runtime and the environment read them, the sources by `finish`.
        let mut snapshot = DiskSnapshot::default();
        snapshot.stamp_config(&root.config_file());
        let read = read_project_config(root.dir());
        let inputs = SessionInputs::opened(root.dir(), &read.config);
        snapshot.stamp_environment(&inputs);
        let runtime = source.runtime_for(root.dir(), &read.config);
        let env = Environment::from_read(root, read, runtime);
        let inputs = inputs.with_check_passes(env.registries().check_passes().next().is_some());
        OpeningProject {
            env: Arc::new(env),
            source,
            inputs,
            snapshot,
        }
    }

    /// Compare every update's graph with a cold rebuild (costly). A debug
    /// build verifies every session by default; this turns it on in release
    /// (watch's `--verify-incremental`) or off.
    pub fn set_verify_incremental(&mut self, enabled: bool) {
        self.project.set_verify(enabled);
    }

    /// Apply a change to the project's sources, then run every check. An
    /// update that changes no file runs no check (unless the checks were
    /// skipped since they last ran: then it runs them). Files discovery
    /// would not find (an `exclude` entry, a skipped directory, not `.spec`)
    /// are ignored.
    pub fn update(&mut self, change: SourceChange<'_>) -> Update {
        self.update_with(change, CheckMode::Full)
    }

    /// [`Self::update`], running the checks `mode` asks for.
    pub fn update_with(&mut self, change: SourceChange<'_>, mode: CheckMode) -> Update {
        let reads: Vec<(String, Read)> = match change {
            SourceChange::Disk(paths) => {
                let mut keys: Vec<String> = paths
                    .iter()
                    .filter(|path| !self.excludes(path))
                    .cloned()
                    .collect();
                self.held.leave_out(&mut keys);
                // Stamped before they are read again (crate::freshness).
                self.snapshot.stamp_sources(&self.inputs, &keys);
                keys.into_iter()
                    .map(|key| {
                        let read = self.project.read_source(&key);
                        (key, read)
                    })
                    .collect()
            }
            SourceChange::Hold(buffers) => {
                let mut reads = Vec::new();
                for buffer in buffers {
                    let key = self.source_key(&buffer.path);
                    let read = Read::Text(buffer.text.clone());
                    if !self.excludes(&key) && !self.project.is_current(&key, &read) {
                        reads.push((key.clone(), read));
                    }
                    self.held.hold(key, buffer.clone());
                }
                reads
            }
        };
        self.applied(reads, mode)
    }

    /// The editor no longer holds the buffers of `paths` (absolute): each
    /// file is the disk's again. A project source is stamped, then read
    /// through the session's one read: its text, its E025, or gone. Any
    /// other file (outside the spec root, excluded, not `.spec`) was never
    /// built and changes nothing. With no project the buffer was the file's
    /// only text, so the file leaves. One update for all of them. `None`
    /// when none changed (each source's disk text is the text the session
    /// built it from) and the checks are current: a release after the
    /// typing fast path skipped them runs them.
    pub fn release(&mut self, paths: &[PathBuf]) -> Option<Update> {
        let mut reads = Vec::new();
        for path in paths {
            let key = self.source_key(path);
            self.held.release(&key);
            let read = if self.inputs.root().is_none() {
                // No project: the buffer was the file's only text.
                Read::Gone
            } else if self.excludes(&key) {
                // Never built: nothing to give back.
                continue;
            } else {
                // Stamped before it is read (crate::freshness): a saved
                // buffer leaves nothing stale.
                self.snapshot
                    .stamp_sources(&self.inputs, std::slice::from_ref(&key));
                self.project.read_source(&key)
            };
            if !self.project.is_current(&key, &read) {
                reads.push((key, read));
            }
        }
        if reads.is_empty() && !self.checks_skipped {
            return None;
        }
        Some(self.applied(reads, CheckMode::Full))
    }

    /// The buffer the editor holds for source `key`, as the session last
    /// built from it.
    pub fn buffer(&self, key: &str) -> Option<&Buffer> {
        self.held.get(key)
    }

    /// Every buffer the session holds, given up: what a project opened in
    /// this session's place holds next.
    pub fn into_buffers(self) -> Vec<Buffer> {
        self.held.into_buffers()
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
        let reach = self.inputs.reach(paths);
        let mut changes = self.snapshot.changes(&self.inputs, Scope::Reached(&reach));
        self.held.leave_out(&mut changes.sources);
        changes
    }

    /// The key of the file at `path` in this session's sources.
    pub fn source_key(&self, path: &Path) -> String {
        self.project.environment().source_key(path)
    }

    /// The one tail of every source update: apply `reads`, resolve the
    /// imports, then run or skip the checks. With nothing to apply it runs
    /// the checks only if they were skipped, else it changes nothing:
    /// nothing an update reads besides its sources can have moved (ADR 0055).
    fn applied(&mut self, reads: Vec<(String, Read)>, mode: CheckMode) -> Update {
        if reads.is_empty() {
            return if self.checks_skipped {
                self.recheck()
            } else {
                Update::of(
                    UpdateKind::Sources,
                    false,
                    Applied::default(),
                    self.project.diagnostics(),
                )
            };
        }
        let applied = self.project.apply(reads);
        let inputs_changed = if mode == CheckMode::SyntaxOnlyIfParseErrors
            && self.project.has_parse_errors_in(&applied.files)
        {
            self.project.skip_checks();
            self.checks_skipped = true;
            false
        } else {
            self.check()
        };
        Update::of(
            UpdateKind::Sources,
            inputs_changed,
            applied,
            self.project.diagnostics(),
        )
    }

    /// `specforge.json` or an extension changed: load the environment again
    /// and rebuild, from the sources on disk and the held buffers, in one
    /// cold build that runs the checks once.
    pub fn reload_environment(&mut self) -> Update {
        if self.inputs.root().is_none() {
            // Nothing on disk to load again.
            return Update::of(
                UpdateKind::Environment,
                false,
                Applied::default(),
                self.project.diagnostics(),
            );
        }
        // The session keeps the root it was opened at: it is never located again.
        let Some(root) = self.project.environment().project_root().cloned() else {
            return Update::of(
                UpdateKind::Environment,
                false,
                Applied::default(),
                self.project.diagnostics(),
            );
        };
        let buffers = std::mem::take(&mut self.held).into_buffers();
        let mut next = Self::begin_open(&root, self.source.clone()).finish_holding(buffers);
        next.project.set_verify(self.project.verifies());
        let previous = std::mem::replace(self, next);
        self.replaced(&previous)
    }

    /// The update that replacing `previous` with this session amounts to:
    /// every file rebuilt, the delta between the two graphs.
    pub fn replaced(&self, previous: &ProjectSession) -> Update {
        Update::of(
            UpdateKind::Environment,
            previous.inputs != self.inputs,
            self.project.replacing(&previous.project),
            self.project.diagnostics(),
        )
    }

    /// What the session compiled, as it is now: its graph, environment,
    /// root, report, source texts and memo. Every reader goes through it.
    pub fn project(&self) -> &CompiledProject {
        &self.project
    }

    /// What the session depends on now (ADR 0030): what a changed path is,
    /// which directories to watch, which files to report.
    pub fn inputs(&self) -> &SessionInputs {
        &self.inputs
    }

    /// Apply `changes`: the environment first (a reload rebuilds
    /// everything), else the sources (an update re-runs every check), else
    /// the checks alone; every file a buffer holds is left out. `None` when
    /// there is nothing to apply: `changes` is empty.
    pub fn apply(&mut self, changes: &Changes) -> Option<Update> {
        let mut sources = changes.sources.clone();
        self.held.leave_out(&mut sources);
        if !changes.environment.is_empty() && self.inputs.root().is_some() {
            Some(self.reload_environment())
        } else if !sources.is_empty() {
            Some(self.update(SourceChange::Disk(&sources)))
        } else if !changes.check_inputs.is_empty() && self.inputs.root().is_some() {
            Some(self.recheck())
        } else {
            None
        }
    }

    /// What changed on disk since the session last built (behavior
    /// `bring_session_up_to_date`): the sources discovery finds now against
    /// those it read, and every environment and check input against what
    /// it read, every file a buffer holds left out, whatever happened to it.
    /// A session that was not opened from disk reports nothing.
    pub fn stale(&self) -> Changes {
        let mut changes = self.snapshot.changes(&self.inputs, Scope::Everything);
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

    /// Whether a changed file is outside the project. A session with no
    /// project takes every buffer: it has no spec root to discover files
    /// under.
    fn excludes(&self, path: &str) -> bool {
        self.inputs.excludes(path)
    }

    /// Run every check again on the current graph: a check input changed.
    fn recheck(&mut self) -> Update {
        let inputs_changed = self.check();
        Update::of(
            UpdateKind::Checks,
            inputs_changed,
            Applied::default(),
            self.project.diagnostics(),
        )
    }

    /// Every check on the current graph, over a snapshot of it taken now,
    /// the check inputs (which the snapshot's `file_exists` rules add to)
    /// stamped first: whether the session's inputs changed.
    fn check(&mut self) -> bool {
        self.checks_skipped = false;
        let entities = self.project.snapshot_now();
        let mut changed = false;
        if self.inputs.root().is_some() {
            let next = self.inputs.with_named(
                self.project
                    .environment()
                    .registries
                    .files(&entities.rule_input()),
            );
            changed = next != self.inputs;
            self.inputs = next;
            self.snapshot.stamp_checks(&self.inputs);
        }
        self.project.check_over(entities);
        changed
    }
}
