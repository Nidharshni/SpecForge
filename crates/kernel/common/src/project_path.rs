//! How a surface document names a file an operation opened or wrote (ADR 0102).

use serde::Serialize;
use std::fmt;
use std::path::{Component, Path, PathBuf};

/// A file of a project as a document names it: relative to the project root with `/` between components, or, for a
/// file outside the root (only the CLI can name one, ADR 0092 D6), absolute and canonical as far as it exists.
/// It serializes as that string.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, crate::shape::Shape)]
pub struct ProjectPath(String);

impl ProjectPath {
    /// `path` named from `root`. A path and a root that name one place through a symbolic link (`/var` and
    /// `/private/var`) are compared as their canonical forms, a removed file's through its nearest existing ancestor.
    /// A relative `path` is read against the process's working directory, as the CLI's arguments are.
    pub fn of(root: &Path, path: &Path) -> ProjectPath {
        if let Some(relative) = within(path, root) {
            return relative;
        }
        let canonical_root = canonical(root);
        match within(&canonical(path), &canonical_root) {
            Some(relative) => relative,
            None => ProjectPath::absolute(path),
        }
    }

    /// `path` named with no root: absolute and canonical as far as it exists.
    pub fn absolute(path: &Path) -> ProjectPath {
        ProjectPath(canonical(path).display().to_string())
    }

    /// The name as the document prints it.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The name, owned.
    pub fn into_string(self) -> String {
        self.0
    }

    /// The file this names, joined onto `root` (an absolute name is its own path): what a consumer that opens the
    /// named file again reads.
    pub fn under(&self, root: &Path) -> PathBuf {
        root.join(&self.0)
    }
}

impl PartialEq<&str> for ProjectPath {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<str> for ProjectPath {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl fmt::Display for ProjectPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `path` relative to `root` as spelled, `/`-joined; `None` when it is not under it, or reaches out and back
/// through a `..`.
fn within(path: &Path, root: &Path) -> Option<ProjectPath> {
    let relative = path.strip_prefix(root).ok()?;
    let mut names = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(name) => names.push(name.to_string_lossy()),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(ProjectPath(if names.is_empty() {
        ".".to_string()
    } else {
        names.join("/")
    }))
}

/// `path` absolute and canonical as far as it exists: the canonical form of its nearest existing ancestor, joined
/// with the rest.
fn canonical(path: &Path) -> PathBuf {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut existing = path.clone();
    let mut rest = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name().map(|n| n.to_os_string()) else {
            break;
        };
        rest.push(name);
        if !existing.pop() {
            break;
        }
    }
    let mut canonical = std::fs::canonicalize(&existing).unwrap_or(existing);
    canonical.extend(rest.into_iter().rev());
    canonical
}

#[cfg(test)]
mod tests;
