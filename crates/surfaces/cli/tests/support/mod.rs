//! What the CLI's tests share: `specforge` as a user runs it ([`cli::Cli`]), the children and clients the harnesses
//! drive, and the registry and written-file helpers. Nothing else in the suite spells the binary, `--path`,
//! `--format json` or the environment a run has (ADR 0078).
#![allow(dead_code)]

pub mod child;
pub mod cli;
pub mod lsp;
pub mod mcp_process;
pub mod registry;
pub mod written;

#[allow(unused_imports)]
pub use cli::{Cli, Document, Run};
pub use specforge_project::testing::TestProject;
