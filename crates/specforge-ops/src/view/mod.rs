//! The project view: what every operation over a compiled project reads
//! (CONTEXT.md "Project view", ADR 0015).
//!
//! A surface builds one from the compiled project it holds (a one-shot
//! compile in the CLI, a project session's in MCP and the LSP,
//! `ProjectSession::project`) and hands it to an operation; the operation returns a typed outcome the
//! surface only renders. The view borrows the environment the project was
//! compiled in and says what its surface reports for the project. It owns
//! the project's recorded test report and the coverage computed from it,
//! both read at the root the project was compiled from and never in an
//! ancestor directory.

mod connectivity;
mod kinds;

use std::path::Path;
use std::sync::Arc;

use specforge_common::Diagnostic;
use specforge_emitter::{GraphProtocolSchema, generate_schema};
use specforge_graph::Graph;
use specforge_installed::{Installed, LockState};
use specforge_project::coverage::{ProjectCoverage, Recorded, RecordedCoverage, TestReport};
use specforge_project::snapshot::EntitySnapshot;
use specforge_project::{CompiledProject, Environment, SharedRuntime};
#[cfg(test)]
use specforge_project::{ExtensionLoad, Loaded};
use specforge_registry::RegistryBuild;

pub use connectivity::{Connectivity, Degree};
pub use kinds::{KnownKinds, UNKNOWN_KIND};

use crate::OpError;
use crate::schema_cache::SchemaCache;

/// The compiled project as one surface sees it, borrowed: what every
/// operation over a project reads (CONTEXT.md "Project view", ADR 0015).
///
/// `root` is the root the project was compiled from: the recorded test
/// report, the schema cache, `specforge.lock`, the installed binaries and
/// the source files are read there, never in an ancestor. Without a root
/// (a graph built in memory) there is no report, no schema cache, no
/// extension pass, and an operation that reads or writes the project on
/// disk refuses with `no_project` ([`Self::project_root`]).
#[derive(Clone, Copy)]
pub struct ProjectView<'a> {
    graph: &'a Graph,
    env: &'a Environment,
    root: Option<&'a Path>,
    /// The memo of the graph's entity snapshot, the recorded report and the
    /// coverage, owned by whoever owns `graph`.
    recorded: &'a RecordedCoverage,
    /// Where what the surface reports for the project comes from.
    reported: Reported<'a>,
    /// What the surface reports after what the compile reported (MCP's
    /// I017 notices).
    also_reported: &'a [Diagnostic],
}

/// Where a view's reported diagnostics come from: its owner, asked when
/// an operation needs them.
#[derive(Clone, Copy)]
enum Reported<'a> {
    /// A compiled project, one-shot or a session's: what `specforge check`
    /// reports for it.
    Compiled(&'a CompiledProject),
    /// A listed slice: a graph built in memory, a test.
    Listed(&'a [Diagnostic]),
}

impl<'a> ProjectView<'a> {
    /// The view of `graph`, compiled in `env`, rooted at `root`. `recorded`
    /// must belong to the owner of `graph` (`RecordedCoverage::over(graph,
    /// env)` for a graph assembled in a test). It reports nothing until
    /// [`Self::reporting`].
    pub fn new(
        graph: &'a Graph,
        env: &'a Environment,
        root: Option<&'a Path>,
        recorded: &'a RecordedCoverage,
    ) -> Self {
        ProjectView {
            graph,
            env,
            root,
            recorded,
            reported: Reported::Listed(&[]),
            also_reported: &[],
        }
    }

    /// The view of a compiled project (a one-shot compile, or a session's
    /// `ProjectSession::project`), rooted where it was compiled (none for
    /// a detached project); it reports what `specforge check` reports for
    /// it.
    pub fn of(project: &'a CompiledProject) -> Self {
        ProjectView {
            reported: Reported::Compiled(project),
            ..Self::new(
                project.graph(),
                project.environment(),
                project.root(),
                project.recorded(),
            )
        }
    }

    /// The project's graph.
    pub fn graph(&self) -> &'a Graph {
        self.graph
    }

    /// What `specforge.json` and the loaded extensions gave the compile:
    /// the config, what each `extensions` entry enabled, the spec root,
    /// the registry build. Operations read them here, never from disk
    /// again.
    pub fn env(&self) -> &'a Environment {
        self.env
    }

    /// The runtime the view's extensions were loaded in (its environment's):
    /// every operation that calls an extension over the view calls it there.
    /// None: the environment loaded no extension.
    pub fn runtime(&self) -> Option<&'a SharedRuntime> {
        self.env.runtime()
    }

    /// The registry build of the environment: kinds, fields, edges, rules,
    /// the extension declarations and their ordered passes. The one way to
    /// reach it (`env().registries()` is the same value).
    pub fn registries(&self) -> &'a RegistryBuild {
        self.env.registries()
    }

    /// The root the project was compiled from; `None` for a graph built in
    /// memory ([`Self::project_root`] refuses instead).
    pub fn root(&self) -> Option<&'a Path> {
        self.root
    }

    /// What `specforge.lock` held when the compile read the environment:
    /// the one read every operation uses (`list`, `doctor`, `remove`).
    /// Absent without a root: a graph built in memory has no project on
    /// disk to have locked anything.
    pub fn lock(&self) -> &'a LockState {
        self.installed().lock()
    }

    /// The project's installed extensions as the compile read them: its
    /// root and lock, once. None without a root: a graph built in memory
    /// has no project on disk to have installed anything.
    pub fn installed(&self) -> &'a Installed {
        static NONE: Installed = Installed::none();
        match self.root {
            Some(_) => self.env.installed(),
            None => &NONE,
        }
    }

    /// This view, reporting `diagnostics` in place of its owner's (a graph
    /// built in memory, a test).
    pub fn reporting(self, diagnostics: &'a [Diagnostic]) -> Self {
        ProjectView {
            reported: Reported::Listed(diagnostics),
            ..self
        }
    }

    /// This view, reporting `extra` after what its compile reported: MCP's
    /// surface registration notices (I017) for the served project.
    pub fn also_reporting(self, extra: &'a [Diagnostic]) -> Self {
        ProjectView {
            also_reported: extra,
            ..self
        }
    }

    /// What this surface reports for the project: what `specforge check`
    /// reports for the compile behind the view, in its order, then what
    /// the surface added ([`Self::also_reporting`]).
    pub fn reported(&self) -> Vec<Diagnostic> {
        let mut diagnostics = match self.reported {
            Reported::Compiled(project) => project.diagnostics(),
            Reported::Listed(listed) => listed.to_vec(),
        };
        diagnostics.extend(self.also_reported.iter().cloned());
        diagnostics
    }

    /// The root, for an operation that reads or writes the project on
    /// disk: `no_project` without one.
    pub fn project_root(&self) -> Result<&'a Path, OpError> {
        self.root.ok_or_else(|| {
            OpError::no_project(
                "this operation needs the project on disk, and this project has none",
            )
        })
    }

    /// `<root>/specforge-report.json`, what `specforge collect` last wrote:
    /// `Ok(None)` without a root or a file; an error (E045) when it is there
    /// but unusable.
    pub fn test_report(&self) -> Result<Option<Arc<TestReport>>, OpError> {
        self.recorded
            .report(self.root)
            .map_err(crate::report::unusable)
    }

    /// The coverage rule over the graph's entity snapshot and the recorded
    /// report, computed once per compile and report content.
    pub fn coverage(&self) -> Result<Arc<ProjectCoverage>, OpError> {
        self.recorded().map(|recorded| recorded.coverage)
    }

    /// The recorded report ([`Self::test_report`]) and the coverage computed
    /// from it ([`Self::coverage`]), read together: one read of the report
    /// file for a view that needs both.
    pub fn recorded(&self) -> Result<Recorded, OpError> {
        self.recorded.at(self.root).map_err(crate::report::unusable)
    }

    /// The graph's entity snapshot (ADR 0019): every entity with what it
    /// writes and its standing, the one the project's checks read. It is
    /// the one the view's memo was made with: its owner's, so the view
    /// cannot read another graph's.
    pub fn entities(&self) -> &'a EntitySnapshot {
        self.recorded.entities()
    }

    /// The Graph Protocol schema the loaded extensions produce, unversioned
    /// (what the model diagram renders).
    pub fn schema(&self) -> GraphProtocolSchema {
        let registries = self.registries();
        generate_schema(
            registries.kinds(),
            registries.edges(),
            registries.fields(),
            &registries
                .extension_info()
                .map(|(name, version)| (name.to_string(), version.to_string()))
                .collect::<Vec<_>>(),
        )
    }

    /// [`Self::schema`], versioned against the schema cache at the root
    /// (`specforge export`'s rule); without a root, unversioned. Only reads
    /// the cache.
    pub fn versioned_schema(&self) -> GraphProtocolSchema {
        let mut schema = self.schema();
        if let Some(cache) = self.schema_cache() {
            cache.version(&mut schema);
        }
        schema
    }

    /// The schema cache at `<root>/.specforge/`; `None` without a root.
    pub(crate) fn schema_cache(&self) -> Option<SchemaCache> {
        self.root.map(SchemaCache::of_root)
    }
}

/// Projects assembled in a test, for the operations' unit tests.
#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use tempfile::TempDir;

    /// A project assembled in a test: a temp root, a graph, an environment
    /// and what its surface reports. Only what a test sets is on disk.
    ///
    /// The fixture holds what the compile read and loaded (the config read,
    /// the installed extensions, what each entry enabled, the declarations,
    /// the runtime) and assembles its environment from them at every
    /// builder call ([`Environment::assemble`]), so the order of the calls
    /// does not matter and the environment is one `check` could make.
    pub(crate) struct Fixture {
        pub dir: TempDir,
        pub graph: Graph,
        read: specforge_common::ConfigRead,
        installed: Installed,
        /// What the compile read each entry as enabling, in place of what
        /// the config's entries name.
        enabled: Option<Vec<specforge_project::EnabledExtension>>,
        declarations: Vec<specforge_protocol_types::ExtensionDeclaration>,
        runtime: Option<SharedRuntime>,
        env: Environment,
        /// Made over the graph and environment as they are at the first
        /// view ([`Self::recorded`]).
        recorded: std::sync::OnceLock<RecordedCoverage>,
        pub reported: Vec<Diagnostic>,
    }

    impl Fixture {
        /// An empty project rooted at a fresh temp directory: the compile
        /// found a `specforge.json` there (not written to disk) with the
        /// default config, nothing enabled, no extension, nothing reported.
        pub fn new() -> Self {
            let dir = TempDir::new().unwrap();
            let installed = Installed::unread(dir.path());
            Fixture {
                read: specforge_common::ConfigRead {
                    found: true,
                    ..Default::default()
                },
                installed,
                enabled: None,
                declarations: Vec::new(),
                runtime: None,
                env: Environment::empty(),
                graph: Graph::new(),
                recorded: std::sync::OnceLock::new(),
                reported: Vec::new(),
                dir,
            }
            .assembled()
        }

        /// The environment the fixture assembled from what it holds.
        pub fn env(&self) -> &Environment {
            &self.env
        }

        /// Assemble the environment from what the fixture says the compile
        /// read and loaded: the one place its environment is made.
        fn assembled(mut self) -> Self {
            let load =
                if self.runtime.is_none() && self.enabled.is_none() && self.declarations.is_empty()
                {
                    ExtensionLoad::NoRuntime
                } else {
                    ExtensionLoad::In {
                        runtime: self.runtime.clone().unwrap_or_else(|| {
                            std::sync::Arc::new(specforge_wasm::testing::InProcessRuntime::new())
                        }),
                        loaded: Loaded {
                            enabled: self.enabled.clone().unwrap_or_else(|| {
                                Loaded::unloaded(&self.read.config.extensions).enabled
                            }),
                            declarations: self.declarations.clone(),
                            diagnostics: Vec::new(),
                        },
                    }
                };
            self.env = Environment::assemble(self.read.clone(), self.installed.clone(), load);
            self.recorded = std::sync::OnceLock::new();
            self
        }

        /// The compile loaded its extensions in `runtime`, which the
        /// environment holds.
        pub fn runtime(mut self, runtime: SharedRuntime) -> Self {
            self.runtime = Some(runtime);
            self.assembled()
        }

        /// The coverage memo of the graph and environment the test built.
        fn recorded(&self) -> &RecordedCoverage {
            self.recorded
                .get_or_init(|| RecordedCoverage::over(&self.graph, &self.env))
        }

        /// The compile read `config` as `specforge.json` (not written to
        /// disk): its `extensions` entries, each enabling what its text
        /// names ([`EnabledExtension::unloaded`]), and its `providers`,
        /// registered against the loaded declarations.
        pub fn config_json(mut self, config: serde_json::Value) -> Self {
            self.read = specforge_common::parse_project_config(
                &self.dir.path().join("specforge.json"),
                &config.to_string(),
            );
            self.assembled()
        }

        /// [`Self::config_json`] of a config enabling `entries`.
        pub fn config(self, entries: &[&str]) -> Self {
            self.config_json(serde_json::json!({ "extensions": entries }))
        }

        /// What the compile read each entry as enabling, in place of what
        /// the config's entries name.
        pub fn enabled(mut self, enabled: Vec<specforge_project::EnabledExtension>) -> Self {
            self.enabled = Some(enabled);
            self.assembled()
        }

        /// The compile loaded `declarations`, in this order.
        pub fn declarations(
            mut self,
            declarations: Vec<specforge_protocol_types::ExtensionDeclaration>,
        ) -> Self {
            self.declarations = declarations;
            self.assembled()
        }

        /// An extension `name` at `version` that contributes nothing.
        pub fn declaration(
            name: &str,
            version: &str,
        ) -> specforge_protocol_types::ExtensionDeclaration {
            specforge_protocol_types::ExtensionDeclaration {
                handshake: specforge_protocol_types::HandshakeResponse {
                    name: name.into(),
                    version: version.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }

        /// `<dir>/specforge.lock` locks each `(name, version, source)`; the
        /// compile read it.
        pub fn lock(mut self, entries: &[(&str, &str, &str)]) -> Self {
            let lock = specforge_installed::LockFile {
                lockfile_version: 1,
                entries: entries
                    .iter()
                    .map(
                        |(name, version, source)| specforge_installed::LockFileEntry {
                            name: specforge_protocol_types::PackageName::parse(name).unwrap(),
                            version: version.to_string(),
                            source: specforge_installed::LockSource::parse(source),
                            wasm_hash: format!("hash-{name}"),
                            key_id: None,
                            peer_dependencies: Vec::new(),
                        },
                    )
                    .collect(),
            };
            specforge_installed::write_lock_file(
                &lock,
                &specforge_installed::lock_path(self.dir.path()),
            )
            .unwrap();
            self.installed = Installed::with_lock(self.dir.path(), LockState::Read(lock));
            self.assembled()
        }

        /// The compile read exactly `lock` (not written to disk), for a lock
        /// a test edits after `lock(..)` built it.
        pub fn with_lock_file(mut self, lock: specforge_installed::LockFile) -> Self {
            self.installed = Installed::with_lock(self.dir.path(), LockState::Read(lock));
            self.assembled()
        }

        /// The compile read the lock at the root as it is on disk now.
        pub fn read_lock(mut self) -> Self {
            self.installed = Installed::at(self.dir.path());
            self.assembled()
        }

        /// The compile found no `specforge.json` at the root.
        pub fn without_config_file(mut self) -> Self {
            self.read.found = false;
            self.assembled()
        }

        /// The compile could not use `specforge.json` as written, for
        /// `problems` (what the surface reports for them is the test's to
        /// set).
        pub fn config_problems(mut self, problems: Vec<specforge_common::ConfigProblem>) -> Self {
            self.read.problems = problems;
            self.assembled()
        }

        /// The project's surface reports `diagnostics`.
        pub fn reporting(mut self, diagnostics: Vec<Diagnostic>) -> Self {
            self.reported = diagnostics;
            self
        }

        /// The view rooted at the temp directory, reporting what the
        /// fixture reports.
        pub fn view(&self) -> ProjectView<'_> {
            ProjectView::new(
                &self.graph,
                &self.env,
                Some(self.dir.path()),
                self.recorded(),
            )
            .reporting(&self.reported)
        }

        /// The same project without a root.
        pub fn rootless_view(&self) -> ProjectView<'_> {
            ProjectView::new(&self.graph, &self.env, None, self.recorded())
                .reporting(&self.reported)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_common::{Severity, codes};
    use specforge_project::ProjectSession;
    use specforge_test_macros::test as specforge_test;

    #[specforge_test(
        behavior = "snapshot_entities_once",
        verify = "the checks, the check passes and the coverage of one compile read one snapshot"
    )]
    fn a_view_reads_the_snapshot_its_compile_took() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("specforge.json"), r#"{"name": "v"}"#).unwrap();
        std::fs::write(dir.path().join("a.spec"), "behavior a \"A\" {\n}\n").unwrap();
        let compiled = CompiledProject::compile(dir.path(), None);
        let view = ProjectView::of(&compiled);
        assert!(std::ptr::eq(view.entities(), compiled.entities()));
        assert!(std::ptr::eq(
            view.coverage().unwrap().entities(),
            compiled.entities()
        ));
        assert_eq!(view.entities().kind_of("a"), Some("behavior"));

        // A graph assembled without a compile: the view takes one, once,
        // with the environment's spec root.
        let graph = Graph::new();
        let env = Environment::assemble(
            specforge_common::parse_project_config(
                &dir.path().join("specforge.json"),
                r#"{"spec_root": "spec"}"#,
            ),
            Installed::unread(dir.path()),
            ExtensionLoad::NoRuntime,
        );
        let recorded = RecordedCoverage::over(&graph, &env);
        let view = ProjectView::new(&graph, &env, None, &recorded);
        assert!(view.entities().is_empty());
        assert!(std::ptr::eq(view.entities(), view.entities()));
        assert_eq!(view.entities().spec_root(), env.spec_root());
        assert!(std::ptr::eq(
            view.coverage().unwrap().entities(),
            view.entities()
        ));
    }

    #[specforge_test(
        behavior = "management_operations_over_the_project_view",
        verify = "an operation that reads or writes the project on disk refuses a view without a root"
    )]
    fn a_view_of_a_session_is_the_view_of_its_compiled_project() {
        // A detached session: no root, nothing reported, no project on disk.
        let detached = ProjectSession::detached();
        let view = ProjectView::of(detached.project());
        assert_eq!(view.root(), None);
        assert!(view.reported().is_empty());
        assert_eq!(view.project_root().unwrap_err().code, "no_project");

        // A session opened from disk is viewed as the compile of the same disk.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("specforge.json"), r#"{"extensions": []}"#).unwrap();
        std::fs::write(
            dir.path().join("a.spec"),
            "behavior a \"A\" {\n  invariants [ghost]\n}\n",
        )
        .unwrap();
        let opened = ProjectSession::open_with_runtime(dir.path(), None);
        let compiled = CompiledProject::compile(dir.path(), None);
        let (held, of) = (
            ProjectView::of(opened.project()),
            ProjectView::of(&compiled),
        );
        assert_eq!(held.root(), of.root());
        assert_eq!(held.reported(), of.reported());
        assert_eq!(held.entities().len(), of.entities().len());
    }

    #[specforge_test(
        behavior = "read_views_over_the_project_view",
        verify = "the recorded test report is read at the view's root, never an ancestor's"
    )]
    fn the_report_is_read_at_the_view_root_not_an_ancestor() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path();
        std::fs::write(project.join("specforge.json"), "{}").unwrap();
        std::fs::write(project.join("specforge-report.json"), "{not json").unwrap();
        let sub = project.join("sub");
        std::fs::create_dir(&sub).unwrap();
        let graph = Graph::new();
        let env = Environment::with_registries(RegistryBuild::default(), None);

        let recorded = RecordedCoverage::over(&graph, &env);
        let at_sub = ProjectView::new(&graph, &env, Some(&sub), &recorded);
        assert!(at_sub.test_report().unwrap().is_none());
        assert!(at_sub.coverage().unwrap().summary.test_results.is_none());

        let recorded = RecordedCoverage::over(&graph, &env);
        let at_root = ProjectView::new(&graph, &env, Some(project), &recorded);
        let error = at_root.test_report().unwrap_err();
        assert_eq!(error.code, "E045");
        assert_eq!(error.kind, crate::OpErrorKind::SchemaMismatch);
        assert!(at_root.coverage().is_err());

        let recorded = RecordedCoverage::over(&graph, &env);
        let rootless = ProjectView::new(&graph, &env, None, &recorded);
        assert!(rootless.test_report().unwrap().is_none());
    }

    fn codes(diagnostics: &[Diagnostic]) -> Vec<&str> {
        diagnostics.iter().map(|d| d.code.as_str()).collect()
    }

    #[specforge_test(
        behavior = "read_views_over_the_project_view",
        verify = "a view reports what its compile reported, then what its surface adds"
    )]
    fn a_view_reports_what_its_compile_reported_then_what_its_surface_adds() {
        // A compile reports what `specforge check` reports for it.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("specforge.json"), r#"{"extensions": []}"#).unwrap();
        std::fs::write(dir.path().join("main.spec"), "behavior b \"B\" {\n}\n").unwrap();
        let compiled = CompiledProject::compile(dir.path(), None);
        let of = ProjectView::of(&compiled);
        assert_eq!(of.reported(), compiled.diagnostics());
        assert_eq!(of.root(), Some(dir.path()));
        // One registry build: the environment's, reached through one accessor.
        assert!(std::ptr::eq(
            of.registries(),
            compiled.environment().registries()
        ));

        // A view built in memory reports nothing until it is told what.
        let graph = Graph::new();
        let env = Environment::with_registries(RegistryBuild::default(), None);
        let recorded = RecordedCoverage::over(&graph, &env);
        let bare = ProjectView::new(&graph, &env, None, &recorded);
        assert!(bare.reported().is_empty());
        let warning = [Diagnostic::untyped("W002", Severity::Warning, "unused")];
        let listed = bare.reporting(&warning);
        assert_eq!(codes(&listed.reported()), ["W002"]);

        // What the surface adds comes after, whatever the view reports.
        let i017 = [Diagnostic::new(codes::I017, "not auto-promoted")];
        assert_eq!(
            codes(&listed.also_reporting(&i017).reported()),
            ["W002", "I017"]
        );
        let mut expected = compiled.diagnostics();
        expected.extend(i017.iter().cloned());
        assert_eq!(of.also_reporting(&i017).reported(), expected);
    }

    #[specforge_test(
        behavior = "management_operations_over_the_project_view",
        verify = "an operation that reads or writes the project on disk refuses a view without a root"
    )]
    fn a_view_without_a_root_has_no_project_root() {
        let fixture = testing::Fixture::new();

        let error = fixture.rootless_view().project_root().unwrap_err();

        assert_eq!(error.code, "no_project");
        assert_eq!(
            fixture.view().project_root().unwrap(),
            fixture.dir.path(),
            "a rooted view's project root is its root"
        );
    }
}
