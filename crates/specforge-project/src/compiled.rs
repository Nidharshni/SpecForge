//! The compiled project (CONTEXT.md "Compiled project", ADR 0047): what a
//! compile built and what it reports. A one-shot compile is one; a project
//! session holds one and keeps it current.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use specforge_common::{Diagnostic, ProjectRoot, codes};
use specforge_graph::{Applied, FileChange, Graph, GraphBuild, GraphConfig, compute_graph_delta};

use crate::coverage::RecordedCoverage;
use crate::snapshot::EntitySnapshot;
use crate::sources::{self, Read, SourceCache};
use crate::{Environment, SharedRuntime, SourceBuild};

/// A compiled project: an environment, the sources read in it, their graph
/// build, what resolving their imports and running the checks reported, and
/// the memo of the graph's entity snapshot and coverage (ADR 0019).
///
/// [`Self::diagnostics`] is, by definition, what `specforge check` reports,
/// in the one report order. A one-shot compile is one ([`Self::compile`]:
/// the CLI); a [`crate::ProjectSession`] holds one and keeps it current
/// ([`crate::ProjectSession::project`]). After every update that runs the
/// checks, a session's compiled project reports exactly what
/// [`Self::compile`] of the same sources reports, in the same order (ADR
/// 0047). An editor update that skipped the checks
/// ([`crate::CheckMode::SyntaxOnlyIfParseErrors`]) reports no check
/// diagnostic until the next update that runs them.
pub struct CompiledProject {
    /// Shared, so a reader keeps the environment an update started from
    /// while the session is out for it (the LSP's stand-in).
    env: Arc<Environment>,
    /// The text of every source as read, the retained trees and the
    /// sources that could not be read.
    sources: SourceCache,
    /// The graph of the sources and what building it reported (ADR 0032).
    graph: GraphBuild,
    /// E025 for the unreadable sources, then the resolver's (E025, I004,
    /// W113, W027).
    import_diagnostics: Vec<Diagnostic>,
    /// What the checks on the graph reported: the registry build's checks
    /// and the extensions' rules, then the check-phase passes. Empty when
    /// the last update skipped them.
    check_diagnostics: Vec<Diagnostic>,
    /// The graph's entity snapshot, the recorded test report and the
    /// coverage: seeded with the snapshot the checks read; made over the
    /// graph on first use after an update that skipped them.
    recorded: OnceLock<RecordedCoverage>,
    /// `false` for a detached project (no project on disk: an editor with
    /// no workspace folder, MCP while nothing is served): it has no root,
    /// and imports are not resolved, with no spec root to resolve them
    /// against (ADR 0030 D5).
    on_disk: bool,
}

impl CompiledProject {
    /// Compile the project at `root`, its extensions loaded into `runtime`
    /// (without one no extension loads), which its environment holds. It
    /// keeps no stamp (ADR 0030).
    pub fn compile(root: &ProjectRoot, runtime: Option<SharedRuntime>) -> Self {
        Self::of(Environment::load(root, runtime))
    }

    /// Compile the project of an environment already loaded (the CLI routes
    /// an extension command on it first): its sources read, its graph built
    /// and checked in the environment's runtime.
    pub fn of(env: Environment) -> Self {
        let env = Arc::new(env);
        let discovered = env.discover();
        let mut project = CompiledProject::read(env, &discovered, &BTreeMap::new());
        let entities = project.snapshot_now();
        project.check_over(entities);
        project
    }

    /// The environment it was compiled in: config, what each `extensions`
    /// entry enabled, the spec root, the registry build.
    pub fn environment(&self) -> &Environment {
        &self.env
    }

    /// [`Self::environment`], shared: it stays valid after a session
    /// replaces this project.
    pub fn shared_environment(&self) -> Arc<Environment> {
        Arc::clone(&self.env)
    }

    /// The project root it was compiled from, as the environment holds it;
    /// `None` for a detached project.
    pub fn project_root(&self) -> Option<&ProjectRoot> {
        self.on_disk.then(|| self.env.project_root()).flatten()
    }

    /// The root it was compiled from; `None` for a detached project.
    pub fn root(&self) -> Option<&Path> {
        self.on_disk.then_some(self.env.root())
    }

    /// The graph of its sources.
    pub fn graph(&self) -> &Graph {
        self.graph.graph()
    }

    /// Exactly what `specforge check` reports, in its order: the
    /// environment's (its own, then the registry build's), the imports',
    /// the graph build's, then the checks'. The one place this order is
    /// written.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.env
            .diagnostics()
            .chain(&self.import_diagnostics)
            .chain(self.graph.diagnostics())
            .chain(&self.check_diagnostics)
            .cloned()
            .collect()
    }

    /// The text the graph's spans for `path` (relative to the spec root)
    /// are positions in: the file as read, or the buffer as given, at the
    /// last change of that file. `None` for a file it does not hold.
    pub fn source_text(&self, path: &str) -> Option<Arc<str>> {
        self.sources.text(path).cloned()
    }

    /// [`Self::source_text`] of every file, shared rather than copied.
    pub fn source_texts(&self) -> HashMap<String, Arc<str>> {
        self.sources.texts()
    }

    /// How many `.spec` files it holds.
    pub fn file_count(&self) -> usize {
        self.graph.files().len()
    }

    /// The graph's entity snapshot (ADR 0019): the one its checks read, or
    /// one taken on first use after an update that skipped them.
    pub fn entities(&self) -> &EntitySnapshot {
        self.recorded().entities()
    }

    /// The entity snapshot, the recorded test report and the coverage of
    /// the graph, memoized for this state of the project.
    pub fn recorded(&self) -> &RecordedCoverage {
        self.recorded
            .get_or_init(|| RecordedCoverage::over(self.graph.graph(), &self.env))
    }
}

/// What the session asks of its compiled project, between stamps.
impl CompiledProject {
    /// No project: the empty environment, no source, no file, nothing
    /// reported. Verifies its updates in a debug build.
    pub(crate) fn detached() -> Self {
        let mut graph = GraphBuild::new(GraphConfig::default());
        graph.set_verify(cfg!(debug_assertions));
        CompiledProject {
            env: Arc::new(Environment::empty()),
            sources: SourceCache::empty(),
            graph,
            import_diagnostics: Vec::new(),
            check_diagnostics: Vec::new(),
            recorded: OnceLock::new(),
            on_disk: false,
        }
    }

    /// The cold read of `discovered` in `env` (ADR 0032's one cold
    /// pipeline, [`Environment::build_sources`]): sources read and parsed,
    /// the graph built, the imports resolved. The checks have not run. A
    /// `held` text (by source key) is read in place of its file.
    pub(crate) fn read(
        env: Arc<Environment>,
        discovered: &[PathBuf],
        held: &BTreeMap<String, &str>,
    ) -> Self {
        let SourceBuild {
            sources,
            graph,
            imports,
        } = env.build_sources(discovered, held);
        CompiledProject {
            env,
            sources,
            graph,
            import_diagnostics: imports,
            check_diagnostics: Vec::new(),
            recorded: OnceLock::new(),
            on_disk: true,
        }
    }

    /// The one read of a source (`sources::read`) under its spec root.
    pub(crate) fn read_source(&self, key: &str) -> Read {
        sources::read(self.env.spec_root(), key)
    }

    /// Whether the project already holds the state `read` gives for `key`.
    pub(crate) fn is_current(&self, key: &str, read: &Read) -> bool {
        self.sources.is_current(key, read)
    }

    /// Apply each source's new state as one change of the graph build;
    /// the memo starts again and the imports are resolved again (none
    /// when detached). The checks have not run.
    pub(crate) fn apply(&mut self, reads: Vec<(String, Read)>) -> Applied {
        let changes: Vec<FileChange> = reads
            .into_iter()
            .map(|(path, read)| self.sources.change(&path, read))
            .collect();
        let applied = self.graph.apply(changes);
        self.recorded = OnceLock::new();
        self.import_diagnostics = if self.on_disk {
            self.env.import_diagnostics(&self.sources, &self.graph)
        } else {
            Vec::new()
        };
        applied
    }

    /// A snapshot of the current graph, taken now (ADR 0019).
    pub(crate) fn snapshot_now(&self) -> Arc<EntitySnapshot> {
        Arc::new(self.env.entity_snapshot(self.graph.graph()))
    }

    /// Run every check on the current graph over `entities`, its snapshot,
    /// in the environment's runtime; the memo starts again from that
    /// snapshot.
    pub(crate) fn check_over(&mut self, entities: Arc<EntitySnapshot>) {
        self.check_diagnostics = self.env.run_checks(self.graph.graph(), &entities);
        self.recorded = OnceLock::from(RecordedCoverage::of(entities));
    }

    /// The checks are skipped for this state: none is reported, and the
    /// memo is made over the graph on first use.
    pub(crate) fn skip_checks(&mut self) {
        self.check_diagnostics = Vec::new();
        self.recorded = OnceLock::new();
    }

    /// Whether any of `paths` has a parse error (E001) in the graph build.
    pub(crate) fn has_parse_errors_in(&self, paths: &[String]) -> bool {
        paths.iter().any(|path| {
            self.graph
                .file_diagnostics(path)
                .iter()
                .any(|d| d.is(codes::E001))
        })
    }

    /// What replacing `previous` with this project amounts to, reported as
    /// an apply: every file rebuilt, the delta between the two graphs,
    /// every file with graph-build diagnostics; never verified.
    pub(crate) fn replacing(&self, previous: &CompiledProject) -> Applied {
        Applied {
            files: self
                .graph
                .files()
                .map(|(path, _)| path.to_string())
                .collect(),
            delta: compute_graph_delta(previous.graph(), self.graph()),
            changed_diagnostic_files: self.graph.diagnostic_files(),
            verification: None,
        }
    }

    /// Compare every apply with a cold build (ADR 0032, 0035).
    pub(crate) fn set_verify(&mut self, enabled: bool) {
        self.graph.set_verify(enabled);
    }

    pub(crate) fn verifies(&self) -> bool {
        self.graph.verifies()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_test_macros::test as specforge_test;

    const MISSING_IMPORT: &str = "use \"missing\"\nbehavior a \"A\" {\n}\n";

    #[test]
    fn a_detached_project_has_no_root_and_resolves_no_import() {
        let mut detached = CompiledProject::detached();
        assert_eq!(detached.root(), None);
        detached.apply(vec![(
            "a.spec".into(),
            Read::Text(MISSING_IMPORT.to_string()),
        )]);
        let imports = |p: &CompiledProject| -> Vec<Diagnostic> {
            p.diagnostics()
                .into_iter()
                .filter(|d| d.code == "W113" || d.code == "I004" || d.code == "E025")
                .collect()
        };
        assert!(
            imports(&detached).is_empty(),
            "{:?}",
            detached.diagnostics()
        );

        // The same text in a project on disk resolves its imports.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("specforge.json"), r#"{"extensions": []}"#).unwrap();
        std::fs::write(dir.path().join("a.spec"), MISSING_IMPORT).unwrap();
        let compiled = CompiledProject::compile(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            None,
        );
        assert!(
            !imports(&compiled).is_empty(),
            "{:?}",
            compiled.diagnostics()
        );
    }

    #[specforge_test(
        behavior = "snapshot_entities_once",
        verify = "the checks, the check passes and the coverage of one compile read one snapshot"
    )]
    fn skipping_the_checks_reports_none_and_scores_the_graph_on_first_use() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("specforge.json"),
            r#"{"name":"s","version":"0.1.0","extensions":["@specforge/software"]}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("a.spec"), "behavior alpha \"A\" {\n}\n").unwrap();
        let runtime = specforge_component::ComponentRuntime::with_user_cache();
        let mut project = CompiledProject::compile(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Some(Arc::new(runtime)),
        );
        let checked = project.diagnostics();
        assert!(checked.iter().any(|d| d.code == "W006"), "{checked:?}");

        project.skip_checks();
        assert!(
            project.diagnostics().iter().all(|d| d.code != "W006"),
            "{:?}",
            project.diagnostics()
        );
        assert_eq!(project.entities().kind_of("alpha"), Some("behavior"));
    }
}
