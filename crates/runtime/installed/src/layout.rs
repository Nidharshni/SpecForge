//! Where a project's installed extensions live. The only place these
//! strings are spelled.

use std::path::{Path, PathBuf};

/// The lock file's name at a project root.
pub const LOCK_FILE: &str = "specforge.lock";

/// The directory, under the project root, that holds installed extensions.
pub(crate) const EXTENSIONS_DIR: &str = ".specforge/extensions";

/// An installed extension's binary, under its package directory.
pub(crate) const MODULE_FILE: &str = "extension.wasm";

/// Under the extensions directory, where a change moves modules while it
/// commits. A package name cannot start with `.`, so no extension is named
/// like it.
pub(crate) const STAGING: &str = ".staging";

/// `specforge.lock` at the project root `root`: the one definition of where
/// the lock lives.
pub fn lock_path(root: &Path) -> PathBuf {
    root.join(LOCK_FILE)
}

/// `.specforge/extensions` at the project root `root`.
pub(crate) fn extensions_dir(root: &Path) -> PathBuf {
    root.join(EXTENSIONS_DIR)
}
