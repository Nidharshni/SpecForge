//! User-level operations shared by the CLI, the MCP server and the LSP.
//!
//! Each surface used to orchestrate the backend crates on its own, so fixes
//! landed on one side only. An operation here takes a typed request and
//! returns a typed outcome or an [`OpError`]; the CLI and MCP only translate
//! arguments in and present the result.
//!
//! Operations never write to stdout: the MCP server owns it (it is the
//! JSON-RPC stream). The crate denies `clippy::print_stdout`.
//!
//! ## Module groups
//!
//! The top-level modules are the **base**: what every operation is written with, none of them an
//! operation. The operations live in four groups, and a group uses only itself and the groups its
//! row names (ADR 0086). `tests/operations/groups.rs` reads this table and holds every `crate::`,
//! `super::` and `self::` path in `src/` to it, tests included.
//!
//! | Group | Holds | May use |
//! |---|---|---|
//! | base | failures and verdicts, refusals, who can be asked (ADR 0110), what a caller lends (ADR 0124), writes, containment, option tables, `specforge.json`, the recorded report, the registry, solver and collector machine ports, the versioned schema, the project view, test support | |
//! | [`read`] | the read views over the project view (ADR 0015), navigation (ADR 0016), inference, explain | base |
//! | [`extension`] | what a project installs, enables and lists, what an author publishes, the commands extensions declare | base |
//! | [`analysis`] | check, analyze with its passes and the prove pass, collect, doctor | base, read, extension |
//! | [`authoring`] | the operations that write a project's sources: init, format, rename, migrate | base, read, extension |

pub mod analysis;
pub mod asking;
pub mod authoring;
mod caller;
pub mod config;
mod containment;
mod error;
pub mod extension;
mod machine;
mod noticed;
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

pub use caller::Caller;
pub use containment::{Containment, Held, OUTSIDE_ROOT, Outside, ReadFailure};
pub use error::{OpError, OpErrorKind};
pub use noticed::Noticed;
pub use refusal::Refusal;
pub use verdict::RunVerdict;
pub use writes::Writes;
