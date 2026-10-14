//! User-level operations shared by the CLI, the MCP server and the LSP.
//!
//! Each surface used to orchestrate the backend crates on its own, so fixes
//! landed on one side only. An operation here takes a typed request and
//! returns a typed outcome or an [`OpError`]; the CLI and MCP only translate
//! arguments in and present the result.
//!
//! Operations never write to stdout: the MCP server owns it (it is the
//! JSON-RPC stream). The crate denies `clippy::print_stdout`.

pub mod analyze;
mod builtin_passes;
pub mod check;
pub mod collect;
pub mod command;
pub mod config;
pub mod coverage;
pub mod doctor;
mod error;
pub mod explain;
pub mod explore;
pub mod export;
pub mod extension;
pub mod format;
pub mod infer;
pub mod init;
pub mod inspect;
pub mod migrate;
pub mod model;
pub mod navigate;
pub mod options;
pub mod plan;
mod prove;
pub mod publish;
pub mod query;
mod refusal;
pub mod registry;
pub mod rename;
mod report;
pub mod review;
mod scan;
pub mod schema;
pub mod schema_version;
pub mod solver;
pub mod stats;
#[cfg(feature = "testing")]
pub mod testing;
pub mod trace;
mod verdict;
pub mod view;
mod writes;

pub use error::{OpError, OpErrorKind};
pub use refusal::Refusal;
pub use verdict::RunVerdict;
pub use writes::{Writes, resolves_outside};
