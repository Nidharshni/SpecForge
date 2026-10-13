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
pub mod rpc;
pub mod spec;

pub use disk::{changed_files, files_under};
pub use events::*;
pub use extension::TestExtension;
pub use project::{Served, TestProject};
pub use rpc::*;
pub use spec::spec_files;

use serde_json::{Value, json};
use std::sync::Arc;

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
