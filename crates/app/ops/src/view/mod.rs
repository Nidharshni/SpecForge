//! The project view: what every operation over a compiled project reads
//! (CONTEXT.md "Project view", ADR 0015).
//!
//! A surface builds one from the compiled project it holds and hands it to
//! an operation; the operation returns a typed outcome the surface only
//! renders. `ProjectView::of` is the only constructor, and the view holds
//! the compiled project it reads. A compiled project is one of four things:
//! - a one-shot compile (the CLI: `CompiledProject::compile`);
//! - a project session's, kept current (MCP, watch, the LSP:
//!   `ProjectSession::project`);
//! - a copy of a session's, which the LSP holds while the session is out for
//!   an update (`CompiledProject`'s `Clone`);
//! - one a test assembled in memory (`specforge_project::testing::Assembled`).
//!
//! The view says what its surface reports for the project. The project's
//! recorded test report and the coverage computed from it are read at the
//! root the project was compiled from and never in an ancestor directory;
//! over MCP only from inside that root ([`ProjectView::contained`], ADR 0139).

mod connectivity;
mod entity;
mod kinds;

use std::path::Path;
use std::sync::Arc;

use specforge_common::Diagnostic;
use specforge_emitter::{GraphProtocolSchema, generate_schema};
use specforge_graph::Graph;
use specforge_installed::{Installed, LockState};
use specforge_project::coverage::{ProjectCoverage, Recorded, ReportError, TestReport};
use specforge_project::{CompiledProject, Environment, SharedRuntime};
use specforge_registry::{Disk, EntitySnapshot, PlainDisk, RegistryBuild};

pub use connectivity::{Connectivity, Degree};
pub use entity::{EntityField, not_found};
pub use kinds::{KnownKinds, UNKNOWN_KIND};

use crate::containment::ContainedDisk;
use crate::schema_version::{SchemaCache, VersionedSchema};
use crate::{Containment, OpError};

/// The compiled project as one surface sees it, borrowed: what every
/// operation over a project reads (CONTEXT.md "Project view", ADR 0015).
///
/// `root` is the root the project was compiled from: the recorded test
/// report, the schema cache, `specforge.lock`, the installed binaries and
/// the source files are read there, never in an ancestor. Over MCP the report is read only from
/// inside that root ([`Self::contained`], ADR 0139). Without a root
/// (a detached project, one assembled in memory) there is no report, no
/// schema cache, no extension pass, and an operation that reads or writes
/// the project on disk refuses with `no_project` ([`Self::project_root`]).
#[derive(Clone, Copy)]
pub struct ProjectView<'a> {
    project: &'a CompiledProject,
    /// What the surface reports in place of what the compile reported (the
    /// LSP hover, MCP's context prompt).
    listed: Option<&'a [Diagnostic]>,
    /// What the surface reports after what the compile reported (MCP's
    /// I017 notices).
    also_reported: &'a [Diagnostic],
    /// Whether the project's files it reads are held inside its root (MCP, ADR 0139).
    contained: bool,
}

impl<'a> ProjectView<'a> {
    /// The view of a compiled project: a one-shot compile, a session's
    /// `ProjectSession::project`, the copy the LSP holds while its session
    /// is out, or an assembled project in a test. The only constructor: its
    /// graph, environment, root and memo agree because they are the
    /// project's (ADR 0108). Rooted where it was compiled (none for a
    /// detached project); it reports what `specforge check` reports for it.
    pub fn of(project: &'a CompiledProject) -> Self {
        ProjectView {
            project,
            listed: None,
            also_reported: &[],
            contained: false,
        }
    }

    /// The compiled project this view reads.
    pub fn project(&self) -> &'a CompiledProject {
        self.project
    }

    /// The project's graph.
    pub fn graph(&self) -> &'a Graph {
        self.project.graph()
    }

    /// What `specforge.json` and the loaded extensions gave the compile:
    /// the config, what each `extensions` entry enabled, the spec root,
    /// the registry build. Operations read them here, never from disk
    /// again.
    pub fn env(&self) -> &'a Environment {
        self.project.environment()
    }

    /// The runtime the view's extensions were loaded in (its environment's):
    /// every operation that calls an extension over the view calls it there.
    /// None: the environment loaded no extension.
    pub fn runtime(&self) -> Option<&'a SharedRuntime> {
        self.env().runtime()
    }

    /// The registry build of the environment: kinds, fields, edges, rules,
    /// the extension declarations and their ordered passes. The one way to
    /// reach it (`env().registries()` is the same value).
    pub fn registries(&self) -> &'a RegistryBuild {
        self.env().registries()
    }

    /// The root the project was compiled from; `None` for a detached
    /// project ([`Self::project_root`] refuses instead).
    pub fn root(&self) -> Option<&'a Path> {
        self.project.root()
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
        match self.root() {
            Some(_) => self.env().installed(),
            None => &NONE,
        }
    }

    /// This view, reporting `diagnostics` in place of its compile's (the
    /// LSP hover reports what it published; a test).
    pub fn reporting(self, diagnostics: &'a [Diagnostic]) -> Self {
        ProjectView {
            listed: Some(diagnostics),
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
        let mut diagnostics = self
            .listed
            .map(<[Diagnostic]>::to_vec)
            .unwrap_or_else(|| self.project.diagnostics());
        diagnostics.extend(self.also_reported.iter().cloned());
        diagnostics
    }

    /// The root, for an operation that reads or writes the project on
    /// disk: `no_project` without one.
    pub fn project_root(&self) -> Result<&'a Path, OpError> {
        self.root().ok_or_else(|| {
            OpError::no_project(
                "this operation needs the project on disk, and this project has none",
            )
        })
    }

    /// `<root>/specforge-report.json`, what `specforge collect` last wrote:
    /// `Ok(None)` without a root or a file; an error (E045) when it is there
    /// but unusable.
    pub fn test_report(&self) -> Result<Option<Arc<TestReport>>, OpError> {
        self.read_report(|disk| self.project.test_report(disk))
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
        self.read_report(|disk| self.project.recorded(disk))
    }

    /// `read` over the disk this view reads the report through: held inside the root when
    /// contained, else the file system as the user sees it.
    fn read_report<T>(
        &self,
        read: impl FnOnce(&dyn Disk) -> Result<T, ReportError>,
    ) -> Result<T, OpError> {
        match self.root().filter(|_| self.contained) {
            Some(root) => read(&ContainedDisk(Containment::within(root))),
            None => read(&PlainDisk),
        }
        .map_err(crate::report::unusable)
    }

    /// This view, reading the project's recorded report only from inside its root: a report
    /// that lands outside (a symlink out) is E045, `invalid_input`. MCP's call target bounds
    /// every view it builds (ADR 0092 D2); the CLI and the LSP do not (ADR 0092 D6) and follow
    /// the user's link.
    pub fn contained(self) -> Self {
        ProjectView {
            contained: true,
            ..self
        }
    }

    /// The graph's entity snapshot (ADR 0019): every entity with what it
    /// writes and its standing, the one the project's checks read. It is
    /// the one the view's memo was made with: its owner's, so the view
    /// cannot read another graph's.
    pub fn entities(&self) -> &'a EntitySnapshot {
        self.project.entities()
    }

    /// The Graph Protocol schema the loaded extensions produce, unversioned
    /// (what the model diagram renders).
    pub fn schema(&self) -> GraphProtocolSchema {
        generate_schema(self.registries())
    }

    /// [`Self::schema`], versioned against the schema record at the root
    /// (`specforge export`'s rule); without a root, unrecorded. Only reads
    /// the record.
    pub fn versioned_schema(&self) -> VersionedSchema {
        match self.schema_cache() {
            Some(cache) => cache.version(self.schema()),
            None => VersionedSchema::unrecorded(self.schema()),
        }
    }

    /// The schema record at `<root>/.specforge/`; `None` without a root.
    pub(crate) fn schema_cache(&self) -> Option<SchemaCache> {
        self.root().map(SchemaCache::of_root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Viewed;
    use specforge_common::{Severity, codes};
    use specforge_project::ProjectSession;
    use specforge_project::testing::Assembled;
    use specforge_test_macros::test as specforge_test;

    #[specforge_test(
        behavior = "snapshot_entities_once",
        verify = "the checks, the check passes and the coverage of one compile read one snapshot"
    )]
    fn a_view_reads_the_snapshot_its_compile_took() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("specforge.json"), r#"{"name": "v"}"#).unwrap();
        std::fs::write(dir.path().join("a.spec"), "behavior a \"A\" {\n}\n").unwrap();
        let compiled = CompiledProject::compile(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            None,
        );
        let view = ProjectView::of(&compiled);
        assert!(std::ptr::eq(view.entities(), compiled.entities()));
        let coverage = view.coverage().unwrap();
        assert!(
            coverage
                .verdicts
                .keys()
                .eq(view.entities().records().iter().map(|r| &r.id)),
            "the coverage scores the view's snapshot"
        );
        assert_eq!(view.entities().kind_of("a"), Some("behavior"));

        // A graph assembled without a compile: the view takes one, once,
        // with the environment's spec root.
        let project = Assembled::new().config_json(serde_json::json!({"spec_root": "spec"}));
        let view = project.view();
        assert!(view.entities().is_empty());
        assert!(std::ptr::eq(view.entities(), view.entities()));
        assert_eq!(view.entities().spec_root(), view.env().spec_root());
        let coverage = view.coverage().unwrap();
        assert!(
            coverage
                .verdicts
                .keys()
                .eq(view.entities().records().iter().map(|r| &r.id)),
            "the coverage scores the view's snapshot"
        );
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
        let opened = ProjectSession::open_with_runtime(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            None,
        );
        let compiled = CompiledProject::compile(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            None,
        );
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

        let sub_project = Assembled::at(&sub);
        let at_sub = sub_project.view();
        assert!(at_sub.test_report().unwrap().is_none());
        assert!(at_sub.coverage().unwrap().summary.test_results.is_none());

        let root_project = Assembled::at(project);
        let at_root = root_project.view();
        let error = at_root.test_report().unwrap_err();
        assert_eq!(error.code, "E045");
        assert_eq!(error.kind, crate::OpErrorKind::SchemaMismatch);
        assert!(at_root.coverage().is_err());

        let rootless = root_project.rootless_view();
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
        let compiled = CompiledProject::compile(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            None,
        );
        let of = ProjectView::of(&compiled);
        assert_eq!(of.reported(), compiled.diagnostics());
        assert_eq!(of.root(), Some(dir.path()));
        // One registry build: the environment's, reached through one accessor.
        assert!(std::ptr::eq(
            of.registries(),
            compiled.environment().registries()
        ));

        // A view built in memory reports nothing until it is told what.
        let project = Assembled::over(RegistryBuild::default());
        let bare = project.rootless_view();
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
        let fixture = Assembled::new();

        let error = fixture.rootless_view().project_root().unwrap_err();

        assert_eq!(error.code, "no_project");
        assert_eq!(
            fixture.view().project_root().unwrap(),
            fixture.root(),
            "a rooted view's project root is its root"
        );
    }
}
