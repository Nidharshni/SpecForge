//! Registry compilation: the loaded declarations in, everything the
//! compiler derives from them out (ADR 0012). [`build_registries`] is
//! the one entry point: it owns the step order, and it is what every
//! test calls. The steps (declaration checks, populate, rules) are
//! private and free to change. The checks over a built graph's records
//! are `RegistryBuild::check`'s. Callers read [`RegistryBuild`]: its
//! registries, rules and passes, and the graph inputs it derives.

mod build;
mod declaration;
mod graph_inputs;
mod peers;
mod populate;
mod validate;

pub use build::{BuildPart, CHECK_PHASE, DeclaredPass, RegistryBuild, build_registries};
