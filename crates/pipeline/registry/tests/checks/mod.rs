//! The checks over a built graph's entity records, run the way production
//! runs them: through `RegistryBuild::check`, behind its gate and in its
//! order (ADR 0031). No test names a check.

mod fields;
mod files;
mod identifiers;
mod kinds;
mod order;
mod references;
mod refs;
mod values;
