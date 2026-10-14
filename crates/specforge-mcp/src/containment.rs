//! Where a path an MCP request names may land (ADR 0092).
//!
//! A client names paths: a call's `path`, format's `paths`, render's
//! `out_dir`, the directory `init` creates. Each resolves inside one
//! directory, or the request is refused: the served project's root (while
//! none is served, the server's root) for the target, the call's project
//! for an argument that names a file or directory of it. This module owns
//! that rule: nothing else compares a client's path with a root.
//!
//! A name is resolved the way the file system would: a relative one is read
//! against the directory (never the process's), `.` and `..` are applied
//! to the real directory reached so far, and every symlink on the way is
//! followed, as far as the path exists. What does not exist yet (a directory
//! about to be created) is the rest of the name, joined on.

use std::path::{Component, Path, PathBuf};

use crate::state::McpState;
use crate::tool::{ErrorCode, McpError};

/// A directory the paths a request names must resolve inside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Containment {
    root: PathBuf,
}

/// A name that resolves outside its [`Containment`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outside {
    named: PathBuf,
    resolved: PathBuf,
    root: PathBuf,
}

impl Containment {
    /// Names resolve inside `root`, as it is on disk now.
    pub(crate) fn at(root: &Path) -> Self {
        Containment {
            root: resolved(root),
        }
    }

    /// The bound of a request: the served project's root; while none is
    /// served, the server's root; `None` when the server has neither (a
    /// server made with no root names nothing to stay inside).
    pub(crate) fn of(state: &McpState) -> Option<Self> {
        state
            .project_root()
            .or(state.root.as_deref())
            .map(Containment::at)
    }

    /// Where `named` lands, canonical as far as it exists: a relative name
    /// is read against the root.
    pub(crate) fn resolve(&self, named: &Path) -> Result<PathBuf, Outside> {
        let landed = resolved(&self.root.join(named));
        if landed.starts_with(&self.root) {
            Ok(landed)
        } else {
            Err(Outside {
                named: named.to_path_buf(),
                resolved: landed,
                root: self.root.clone(),
            })
        }
    }

    /// Whether `canonical`, a path already resolved, is inside the root.
    pub(crate) fn holds(&self, canonical: &Path) -> bool {
        canonical.starts_with(&self.root)
    }

    /// `named` as the call spelled it, absolute: joined onto the root when
    /// relative.
    pub(crate) fn spelled(&self, named: &Path) -> PathBuf {
        self.root.join(named)
    }

    /// The refusal for `named`, which resolved to `resolved`.
    pub(crate) fn outside(&self, named: &Path, resolved: PathBuf) -> Outside {
        Outside {
            named: named.to_path_buf(),
            resolved,
            root: self.root.clone(),
        }
    }
}

impl Outside {
    /// `invalid_input` on `argument`: what the name resolves to and what
    /// bounds it.
    pub(crate) fn refusal(self, argument: &str) -> McpError {
        McpError::new(
            ErrorCode::InvalidInput,
            format!(
                "{argument} '{}' resolves to {}, outside {}: a path a request names stays inside the served project",
                self.named.display(),
                self.resolved.display(),
                self.root.display()
            ),
        )
        .with_argument(argument)
    }
}

impl Outside {
    /// `invalid_input` on `file`, a file of the project (not an argument of
    /// the call): a source that is a symlink leaving the project.
    pub(crate) fn refusal_of_file(self, file: &str) -> McpError {
        McpError::new(
            ErrorCode::InvalidInput,
            format!(
                "{file} resolves to {}, outside {}: a symlink that leaves the project is not followed",
                self.resolved.display(),
                self.root.display()
            ),
        )
        .with_file(file)
    }
}

/// `path` made absolute and resolved component by component: `..` leaves
/// the real directory reached so far (after its symlinks), so no name can
/// pass through a symlink or a `..` unseen. Canonical as far as it exists.
pub(crate) fn resolved(path: &Path) -> PathBuf {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut landed = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => landed.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                landed.pop();
            }
            Component::Normal(name) => {
                landed.push(name);
                if let Ok(real) = std::fs::canonicalize(&landed) {
                    landed = real;
                }
            }
        }
    }
    landed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_resolved_against_the_root_and_cannot_leave_it() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join("proj");
        std::fs::create_dir_all(root.join("spec")).unwrap();
        std::fs::create_dir_all(dir.path().join("outside")).unwrap();
        let containment = Containment::at(&root);

        for inside in ["spec", "./spec/../spec", "not/yet/made", "."] {
            assert!(containment.resolve(Path::new(inside)).is_ok(), "{inside}");
        }
        let outside = dir.path().join("outside");
        for leaving in [
            "..",
            "../outside",
            "spec/../../outside",
            "not/yet/../../../outside",
            outside.to_str().unwrap(),
        ] {
            assert!(
                containment.resolve(Path::new(leaving)).is_err(),
                "{leaving}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_that_leaves_the_root_is_followed_before_the_check() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join("proj");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(dir.path().join("outside/deep")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("outside"), root.join("link")).unwrap();
        std::os::unix::fs::symlink(&root, dir.path().join("outside/back")).unwrap();
        let containment = Containment::at(&root);

        assert!(containment.resolve(Path::new("link")).is_err());
        assert!(containment.resolve(Path::new("link/deep")).is_err());
        // `..` after a symlink is the symlink target's parent, not the root.
        assert!(containment.resolve(Path::new("link/deep/..")).is_err());
        // A symlink that comes back inside is inside.
        assert!(containment.resolve(Path::new("link/back")).is_ok());
    }
}
