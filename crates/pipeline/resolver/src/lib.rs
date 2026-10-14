//! Import resolution over a compile's sources (ADR 0055): an import names a
//! source by its key, the path relative to the spec root that `specforge check`
//! names a file by. Nothing here reads, lists or stats a file.

mod resolve;

pub use resolve::{import_target, resolve_imports};
