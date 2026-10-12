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
