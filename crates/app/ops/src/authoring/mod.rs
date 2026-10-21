//! The operations that write a project's sources: init (a new project, its config and starter, its
//! extensions through the lock plan, ADR 0065), format (ADR 0021), rename (the occurrences
//! navigation finds, ADR 0016), migrate, and the fixes offered for a diagnostic (ADR 0016). The group uses the base, the read views and the
//! extension group. A plan's texts are written as one planned change (ADR 0184).

pub mod edit;
pub mod fix;
pub mod format;
pub mod init;
pub mod migrate;
pub mod planned;
pub mod rename;
