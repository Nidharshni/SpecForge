//! User-level operations shared by the CLI, the MCP server and the LSP.
//!
//! Each surface used to orchestrate the backend crates on its own, so fixes
//! landed on one side only. An operation here takes a typed request and
//! returns a typed outcome or an [`OpError`]; the CLI and MCP only translate
//! arguments in and present the result.
//!
//! Operations never write to stdout: the MCP server owns it (it is the
//! JSON-RPC stream). The crate denies `clippy::print_stdout`.

pub mod analysis;
pub mod authoring;
pub mod config;
mod error;
pub mod extension;
pub mod options;
pub mod read;
mod refusal;
pub mod registry;
mod report;
pub mod schema_version;
pub mod solver;
#[cfg(feature = "testing")]
pub mod testing;
mod verdict;
pub mod view;
mod writes;

pub use error::{OpError, OpErrorKind};
pub use refusal::Refusal;
pub use verdict::RunVerdict;
pub use writes::{Writes, resolves_outside};
