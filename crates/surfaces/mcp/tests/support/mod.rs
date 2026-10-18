//! What every MCP test serves: a project on disk whose extensions are
//! declared with the SDK builders and run in process (`InProcessRuntime`,
//! the test adapter of the `WasmRuntime` port), loaded when the server
//! initializes over it — `ProjectSession::open_with_runtime`,
//! `Environment::load`, `build_registries`, a full compile — exactly as
//! `specforge mcp <root>` loads a project. No test writes a registry entry,
//! a graph or a diagnostic into the server's state: what a test serves is
//! what its sources and its extension declare.

pub mod disk;
pub mod documents;
pub mod events;
pub mod extension;
pub mod project;
pub mod replies;
pub mod spec;

pub use disk::{changed_files, files_under};
pub use events::*;
pub use extension::TestExtension;
pub use project::{Serve, TestProject};
pub use spec::spec_files;
pub use specforge_mcp::resource::ResourceSpec;
pub use specforge_mcp::testing::Served;
pub use specforge_mcp::testing::rpc::*;

use serde_json::{Value, json};
use std::sync::Arc;

/// An `AgentPlan` of entries naming `ids`, each to implement.
pub fn agent_plan(ids: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "plan_id": "p",
        "entries": ids
            .iter()
            .map(|id| serde_json::json!({"entity_id": id, "action": "implement"}))
            .collect::<Vec<_>>(),
    })
}

/// The solver every served test project's doctor sees: z3 runs (ADR 0079 D5).
pub fn z3_ready() -> Arc<dyn specforge_ops::solver::Solver> {
    Arc::new(specforge_ops::testing::ScriptedSolver::ready())
}

/// What each tool that takes a `path` is called with to act on a project
/// with `behavior alpha` (its path added by the caller).
pub fn path_tool_calls() -> Vec<(&'static str, Value)> {
    vec![
        ("specforge.validate", json!({})),
        ("specforge.analyze", json!({"pass": "contracts"})),
        ("specforge.collect", json!({})),
        ("specforge.format", json!({"check": true})),
        (
            "specforge.rename",
            json!({"entity_id": "alpha", "new_name": "gamma"}),
        ),
        (
            "specforge.edit",
            json!({"edits": [{"op": "set", "entity_id": "alpha", "field": "contract", "value": "The system MUST work"}]}),
        ),
        (
            "specforge.add_extension",
            json!({"specifiers": ["@specforge/software"], "dry_run": true}),
        ),
        (
            "specforge.remove_extension",
            json!({"name": "@specforge/software", "dry_run": true}),
        ),
        ("specforge.migrate", json!({"dry_run": true})),
    ]
}

/// `blob` copied into `project_root` under its own file name (a local
/// install names the extension after that stem), and that copy's path: a
/// path a request names stays inside the served project.
pub fn blob_in(project_root: &std::path::Path, blob: std::path::PathBuf) -> std::path::PathBuf {
    let to = project_root.join(blob.file_name().expect("a blob has a file name"));
    if !to.exists() {
        std::fs::copy(&blob, &to).expect("copy the blob into the project");
    }
    to
}

/// A project a test built in a directory of its own, moved inside the
/// project a server serves: a call may name only projects there (ADR 0092).
pub struct Nested(std::path::PathBuf);

impl Nested {
    pub fn path(&self) -> &std::path::Path {
        &self.0
    }
}

/// `other` (its directory moved, so nothing of it is left behind) inside
/// `served`, the root of the project the server serves.
pub fn within(served: &std::path::Path, other: tempfile::TempDir) -> Nested {
    let name = other
        .path()
        .file_name()
        .expect("a temp dir has a name")
        .to_os_string();
    let to = served.join("nested").join(name);
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::fs::rename(other.path(), &to).expect("move the project inside the served one");
    Nested(to)
}

/// A server that serves nothing and has no root, recording its events: no
/// path is bounded yet (ADR 0092 D2), and `init` creates the first project
/// of the session wherever it is told, then serves it. With a project served
/// `init` creates none.
pub fn unserved() -> Logged {
    let mut server = logged(specforge_mcp::McpServer::new());
    let reply = call(&mut server, "initialize", json!({}));
    assert!(reply["error"].is_null(), "{reply}");
    server
}
