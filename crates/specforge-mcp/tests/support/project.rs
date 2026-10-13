//! A project on disk, served by the real `initialize`.

use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::sync::Arc;

use serde_json::{Value, json};
use specforge_mcp::McpServer;
use specforge_mcp::events::testing::EventLog;
use specforge_project::SharedRuntime;
use specforge_wasm::testing::{InProcessRuntime, RecordedCall};
use tempfile::TempDir;

use super::events::{Logged, Logs};
use super::extension::{EXT, TestExtension, runtime_of};
use super::rpc;

/// A project on disk: its `specforge.json` and the files a test writes.
/// The directory lives as long as the value (no leaked temp dirs).
pub struct TestProject {
    dir: TempDir,
    /// `specforge.json` without its `extensions`.
    config: Value,
    /// The extensions `specforge.json` enables, in load order, when a test
    /// chose them ([`Self::enabling`]); else the ones served.
    enabled: Option<Vec<String>>,
}

impl Default for TestProject {
    fn default() -> Self {
        Self::new()
    }
}

impl TestProject {
    /// `{"name": "t", "version": "0.1.0"}` and no file. Unless a test
    /// chooses ([`Self::enabling`]), it enables the extensions it is served
    /// with ([`Self::serve`]), `@test/ext` under [`Self::serve_in`], and
    /// none under [`Self::serve_components`].
    pub fn new() -> Self {
        TestProject {
            dir: TempDir::new().expect("a temp dir"),
            config: json!({"name": "t", "version": "0.1.0"}),
            enabled: None,
        }
    }

    /// Enable `extensions` (in load order) instead.
    pub fn enabling(mut self, extensions: &[&str]) -> Self {
        self.enabled = Some(extensions.iter().map(|e| e.to_string()).collect());
        self
    }

    /// Apply `edit` to its `specforge.json` (`spec_root`, `inference`, …).
    pub fn config(mut self, edit: impl FnOnce(&mut Value)) -> Self {
        edit(&mut self.config);
        self
    }

    /// Write `text` at `path`, relative to the root.
    pub fn file(self, path: &str, text: &str) -> Self {
        write(self.root(), path, text);
        self
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    /// An initialized server over this project, its extensions running in
    /// one in-process runtime serving `extensions`: `initialize` with this
    /// root opens the session from disk and runs the registry build.
    pub fn serve(mut self, extensions: &[TestExtension]) -> Served {
        if self.enabled.is_none() {
            self.enabled = Some(extensions.iter().map(|e| e.name().to_string()).collect());
        }
        let runtime = Arc::new(runtime_of(extensions));
        let mut served = self.serve_in(Arc::clone(&runtime) as SharedRuntime);
        served.runtime = Some(runtime);
        served
    }

    /// As [`Self::serve`], the extensions running in `runtime` (the
    /// surface tests' `FakeExtension`).
    pub fn serve_in(mut self, runtime: SharedRuntime) -> Served {
        self.enabled.get_or_insert_with(|| vec![EXT.to_string()]);
        // What an in-process runtime serves, the project has installed: it
        // loads through the production path.
        specforge_installed::testing::install_enabled(
            self.root(),
            self.enabled.as_deref().unwrap_or_default(),
            &specforge_installed::Builtins(specforge_component::builtins::BUILTIN_EXTENSIONS),
        );
        self.initialized(
            McpServer::new()
                .with_solver(super::z3_ready())
                .with_extension_runtime(runtime),
        )
    }

    /// An initialized server over this project with no host runtime: the
    /// project's own component runtime loads its builtins and installed
    /// extensions, as `specforge mcp <root>` does. For tests of add,
    /// remove, doctor and the builtins.
    pub fn serve_components(mut self) -> Served {
        self.enabled.get_or_insert_with(Vec::new);
        self.initialized(McpServer::new().with_solver(super::z3_ready()))
    }

    /// The project as files only, its `specforge.json` written (enabling
    /// what [`Self::enabling`] chose, else nothing): for a test that drives
    /// `initialize` itself.
    pub fn into_dir(self) -> TempDir {
        self.write_config();
        self.dir
    }

    fn write_config(&self) {
        let mut config = self.config.clone();
        config["extensions"] = json!(self.enabled.clone().unwrap_or_default());
        write(self.root(), "specforge.json", &config.to_string());
    }

    /// Write `specforge.json`, then initialize `server` over the root, its
    /// events recorded into the log the returned value keeps.
    fn initialized(self, server: McpServer) -> Served {
        let log = EventLog::new();
        let mut server = server.with_events(log.clone());
        self.write_config();
        let reply = rpc::call(
            &mut server,
            "initialize",
            json!({"projectRoot": self.root().to_str().expect("a UTF-8 root")}),
        );
        assert!(reply["error"].is_null(), "initialize: {reply}");
        assert!(server.state().session().project().root().is_some());
        Served {
            server,
            log,
            project: self,
            runtime: None,
        }
    }
}

/// An initialized MCP server serving a [`TestProject`]: what a client of
/// `specforge mcp <root>` talks to. Dereferences to the server.
pub struct Served {
    server: McpServer,
    /// What the server recorded.
    log: EventLog,
    project: TestProject,
    runtime: Option<Arc<InProcessRuntime>>,
}

impl Served {
    pub fn root(&self) -> &Path {
        self.project.root()
    }

    /// Write `text` at `path` under the root: the next request that reads
    /// the project brings the served session up to date (ADR 0014), as an
    /// edit in a client's editor would.
    pub fn write(&self, path: &str, text: &str) {
        write(self.root(), path, text);
    }

    /// The server (with its log) and the project's directory, apart: for a
    /// helper that hands them out separately (`fake_extension::initialized`).
    pub fn into_parts(self) -> (Logged, TempDir) {
        let logged = Logged {
            server: self.server,
            log: self.log,
        };
        (logged, self.project.dir)
    }

    /// Remove `path` under the root.
    pub fn remove(&self, path: &str) {
        std::fs::remove_file(self.root().join(path))
            .unwrap_or_else(|e| panic!("remove {path}: {e}"));
    }

    /// The extension calls the in-process runtime answered, oldest first,
    /// handshakes and describes left out (empty under
    /// [`TestProject::serve_components`] and [`TestProject::serve_in`]).
    pub fn extension_calls(&self) -> Vec<RecordedCall> {
        self.runtime
            .as_ref()
            .map(|runtime| runtime.calls())
            .unwrap_or_default()
            .into_iter()
            .filter(|c| c.export != "__handshake" && c.export != "__describe")
            .collect()
    }
}

impl Logs for Served {
    fn log(&self) -> &EventLog {
        &self.log
    }
}

impl Deref for Served {
    type Target = McpServer;

    fn deref(&self) -> &McpServer {
        &self.server
    }
}

impl DerefMut for Served {
    fn deref_mut(&mut self) -> &mut McpServer {
        &mut self.server
    }
}

/// Write `text` at `path` under `root`, its directories created.
fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("{}: {e}", parent.display()));
    }
    std::fs::write(&path, text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}

// What the fixture serves is what the real build makes of its sources and
// declarations.

/// `alpha`, a behavior on lines 1–4 of `test.spec`.
const ALPHA: &str = "behavior alpha \"Alpha\" {\n    contract \"The system MUST do alpha\"\n    verify unit \"does alpha\"\n}\n";

#[test]
fn the_fixture_serves_its_declaration_through_the_registry_build() {
    let served = TestProject::new()
        .file(
            "test.spec",
            "behavior alpha \"Alpha\" {\n}\ninvariant gamma \"Gamma\" {\n}\n",
        )
        .serve(&[TestExtension::software().obligating("behavior")]);

    assert!(served.state().session().project().root().is_some());
    let registries = served.state().registries();
    let declared: Vec<&str> = registries.declarations().iter().map(|d| d.name()).collect();
    assert_eq!(declared, [EXT]);
    for kind in ["behavior", "invariant", "feature"] {
        let entry = registries
            .kinds()
            .get(kind)
            .unwrap_or_else(|| panic!("{kind} is not registered"));
        assert_eq!(entry.source_extension, EXT, "{kind}");
    }
    // One W004 rule, targeting `behavior`: the behavior that declares no
    // obligation is reported, the invariant (a testable kind no rule
    // obligates) is not.
    let w004: Vec<String> = served
        .state()
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "W004")
        .map(|d| d.message)
        .collect();
    assert_eq!(
        w004,
        ["behavior 'alpha' is testable but declares no verify obligations"],
        "{w004:?}"
    );
}

#[test]
fn the_declaration_is_what_the_builders_declare() {
    let declaration = TestExtension::named("@test/other")
        .kind("task", true)
        .headline("task")
        .string_field("task", "owner")
        .reference("task", "blocks", "task")
        .declaring(|c| {
            c.kind("note", |_| {});
        })
        .declaration();

    assert_eq!(declaration.name(), "@test/other");
    let kinds: Vec<&str> = declaration
        .entities
        .iter()
        .map(|k| k.name.as_str())
        .collect();
    assert_eq!(kinds, ["task", "note"]);
    let task = &declaration.entities[0];
    assert!(task.testable && task.supports_verify);
    let fields: Vec<(&str, &str, bool, bool)> = task
        .fields
        .iter()
        .map(|f| {
            (
                f.name.as_str(),
                f.field_type.as_str(),
                f.normative,
                f.headline,
            )
        })
        .collect();
    assert_eq!(
        fields,
        [
            ("contract", "string", true, true),
            ("status", "string", false, true),
            ("owner", "string", false, false),
            ("blocks", "reference_list", false, false),
        ]
    );
    let blocks = &task.fields[3];
    assert_eq!(blocks.target_kind.as_deref(), Some("task"));
    assert_eq!(blocks.edge.as_deref(), Some("blocks"));
    let edges: Vec<&str> = declaration.edges.iter().map(|e| e.label.as_str()).collect();
    assert_eq!(edges, ["blocks"]);
    assert!(declaration.validation_rules.is_empty());
    assert!(declaration.passes.is_empty());
}

#[test]
fn the_served_graph_is_the_sources_on_disk() {
    let served = TestProject::new()
        .file("test.spec", ALPHA)
        .file(
            "features.spec",
            "feature beta \"Beta\" {\n    behaviors [alpha]\n}\n",
        )
        .serve(&[TestExtension::software()]);

    let graph = served.state().graph();
    let alpha = graph.node("alpha").expect("alpha is served");
    assert_eq!(alpha.kind.raw, "behavior");
    assert_eq!(alpha.title.as_deref(), Some("Alpha"));
    // The parser's spans: 1-based lines and columns.
    assert_eq!(alpha.source_span.file, "test.spec");
    assert_eq!(
        (alpha.source_span.start_line, alpha.source_span.start_col),
        (1, 1)
    );
    assert_eq!(alpha.source_span.end_line, 4);
    let beta = graph.node("beta").expect("beta is served");
    assert_eq!(beta.kind.raw, "feature");
    assert_eq!(beta.source_span.file, "features.spec");
    let edges: Vec<(&str, &str)> = graph
        .edges_from("beta")
        .iter()
        .map(|e| (e.target.as_str(), e.label.as_str()))
        .collect();
    assert_eq!(edges, [("alpha", "behaviors")]);
    assert!(
        served.state().diagnostics().is_empty(),
        "{:?}",
        served.state().diagnostics()
    );
}

#[test]
fn a_reported_diagnostic_joins_the_compile() {
    use specforge_common::DiagnosticData;
    use specforge_extension_sdk::prelude::PassDiagnostic;

    let served = TestProject::new()
        .file("test.spec", ALPHA)
        .serve(&[TestExtension::software()
            .reporting(PassDiagnostic::warning("W900", "x").with_entity("alpha"))]);

    let reported: Vec<_> = served
        .state()
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "W900")
        .collect();
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert_eq!(reported[0].message, "x");
    assert_eq!(
        reported[0].data.as_deref(),
        Some(&DiagnosticData::Subject {
            entity: "alpha".into()
        })
    );
    // A spanless diagnostic that names an entity is placed at it.
    let span = reported[0].span.as_ref().expect("alpha's span");
    assert_eq!((span.file.as_str(), span.start_line), ("test.spec", 1));
    // The pass ran in the extension's runtime.
    assert!(
        served
            .extension_calls()
            .iter()
            .any(|c| c.extension == EXT && c.export == "__pass_report"),
        "{:?}",
        served.extension_calls()
    );
}

#[test]
fn a_write_is_served_by_the_next_request() {
    let mut served = TestProject::new()
        .file("test.spec", ALPHA)
        .serve(&[TestExtension::software()]);
    let ids = |served: &mut Served| -> Vec<String> {
        rpc::tool(served, "specforge.list", json!({}))["entities"]
            .clone()
            .as_array()
            .expect("a list")
            .iter()
            .map(|e| e["id"].as_str().expect("an id").to_string())
            .collect()
    };
    assert_eq!(ids(&mut served), ["alpha"]);

    served.write("more/delta.spec", "invariant delta \"Delta\" {\n}\n");
    assert_eq!(ids(&mut served), ["alpha", "delta"]);

    served.remove("more/delta.spec");
    assert_eq!(ids(&mut served), ["alpha"]);
}

#[test]
fn serve_components_loads_the_builtins() {
    let served = TestProject::new()
        .enabling(&["@specforge/software"])
        .file("test.spec", ALPHA)
        .serve_components();

    let behavior = served
        .state()
        .registries()
        .kinds()
        .get("behavior")
        .expect("behavior is registered");
    assert_eq!(behavior.source_extension, "@specforge/software");
    assert!(served.state().graph().node("alpha").is_some());
    assert!(served.extension_calls().is_empty());
}
