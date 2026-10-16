//! How a source is keyed: by its path relative to the spec root (as `specforge check` names it).

use std::path::{Path, PathBuf};

use crate::Environment;

impl Environment {
    /// A `.spec` path's key: relative to the spec root when the file is
    /// under it (as `specforge check` names it), else the path itself.
    /// With no spec root (no project), every key is the path itself.
    pub fn source_key(&self, path: &Path) -> String {
        source_key(self.sources.dir(), path)
    }
}

/// `path`'s key under `spec_root` (see [`Environment::source_key`]). The
/// editor and the workspace may spell one directory differently (a
/// symlinked temp dir), so when the plain forms differ the path is compared
/// by its directory's canonical form and its own name: a symlinked file is
/// keyed by its own name, never its target's, and a deleted file is keyed
/// the same way.
pub fn source_key(spec_root: &Path, path: &Path) -> String {
    if spec_root.as_os_str().is_empty() {
        return path.to_string_lossy().into_owned();
    }
    if let Ok(relative) = path.strip_prefix(spec_root) {
        return relative.to_string_lossy().into_owned();
    }
    if let Ok(canonical_root) = std::fs::canonicalize(spec_root)
        && let Ok(relative) = in_canonical_dir(path).strip_prefix(&canonical_root)
    {
        return relative.to_string_lossy().into_owned();
    }
    path.to_string_lossy().into_owned()
}

/// `path` with its directory canonical and its own name kept: a symlink stays
/// itself. `path` as given when it has no directory or no name.
fn in_canonical_dir(path: &Path) -> PathBuf {
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => {
            canonical(parent).join(name)
        }
        _ => path.to_path_buf(),
    }
}

/// `path`, canonical when it can be: the path itself when it exists, else
/// its directory's canonical form joined with its name (a deleted file),
/// else as given.
pub(crate) fn canonical(path: &Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => {
            canonical(parent).join(name)
        }
        _ => path.to_path_buf(),
    }
}
