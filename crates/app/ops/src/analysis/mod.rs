//! The operations that judge a project: check (the diagnostic policy and the run verdict, ADR
//! 0018), analyze with its built-in passes and the prove pass (ADR 0079), collect (the test
//! results analysis reads, ADR 0002) and doctor (the setup's health). The group uses the base, the
//! read views (check reports the inferred lint) and the extension group (doctor lists the
//! project's extensions).

pub mod analyze;
mod builtin_passes;
pub mod check;
pub mod collect;
pub mod doctor;
mod prove;
