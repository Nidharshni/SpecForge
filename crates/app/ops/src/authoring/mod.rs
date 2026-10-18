//! The operations that write a project's sources: init (a new project, its config and starter, its
//! extensions through the lock plan, ADR 0065), format (ADR 0021), rename (the occurrences
//! navigation finds, ADR 0016) and migrate. The group uses the base, the read views and the
//! extension group.

pub mod edit;
pub mod format;
pub mod init;
pub mod migrate;
pub mod rename;
