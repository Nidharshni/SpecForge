//! The project a path is in (CONTEXT "Project root", ADR 0053): the one
//! rule every surface opens a project by.

use std::path::{Component, Path, PathBuf};

/// The file that makes a directory a project: its configuration.
pub const PROJECT_FILE: &str = "specforge.json";

/// The root of the project a path is in.
///
/// Walking up from the path's directory (a file's parent, or the directory
/// itself), symlinks resolved, the first directory holding `specforge.json`
/// is the project root (the nearest wins). With none at or above it, the
/// path's own directory is a *loose* root: a project with the default
/// configuration and no extension. Nothing else marks a project: the file that
/// holds the `spec` block is a source like any other.
///
/// It is spelled as the path spells it where that spelling reaches the root
/// (an editor keeps naming its files the way it named its workspace),
/// else canonically; two roots are equal when they are the same directory.
/// The compile, the session open, the environment load and every operation
/// that reads a project from disk take one. Only `init` names a directory
/// as given, and it opens nothing.
#[derive(Debug, Clone)]
pub struct ProjectRoot {
    /// The root as the caller's path spells it, absolute.
    dir: PathBuf,
    /// The same directory, canonical: what equality and hashing compare.
    canonical: PathBuf,
    /// `specforge.json` is there (false: a loose root).
    configured: bool,
}

/// A path that names no directory to find a project from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoSuchPath {
    /// The path as given.
    pub path: PathBuf,
    /// Why it could not be resolved (`NotFound`, `PermissionDenied`, …).
    pub kind: std::io::ErrorKind,
}

impl ProjectRoot {
    /// The project `path` is in (any file or directory inside it).
    /// `NoSuchPath` when `path` does not exist or can't be resolved.
    pub fn of(path: &Path) -> Result<ProjectRoot, NoSuchPath> {
        let resolved = std::fs::canonicalize(path).map_err(|e| NoSuchPath {
            path: path.to_path_buf(),
            kind: e.kind(),
        })?;
        let is_file = !resolved.is_dir();
        let start = match (is_file, resolved.parent()) {
            (true, Some(parent)) => parent.to_path_buf(),
            _ => resolved,
        };
        // Canonical ancestors cannot loop: a circular symlink chain already
        // failed to canonicalize.
        let (canonical, configured) = match start.ancestors().find(|dir| Self::is_root(dir)) {
            Some(root) => (root.to_path_buf(), true),
            None => (start, false),
        };
        let dir = spelled(path, is_file, &canonical).unwrap_or_else(|| canonical.clone());
        Ok(ProjectRoot {
            dir,
            canonical,
            configured,
        })
    }

    /// The project `path` is in, only when it is configured: `None` for a
    /// loose root or a path that does not exist. What a formatted file's
    /// configuration is bounded by, and what init notes.
    pub fn enclosing(path: &Path) -> Option<ProjectRoot> {
        Self::of(path).ok().filter(ProjectRoot::is_configured)
    }

    /// Whether `dir` itself is a project root (holds `specforge.json`): the
    /// predicate a walk down stops at (format's nested projects).
    pub fn is_root(dir: &Path) -> bool {
        dir.join(PROJECT_FILE).exists()
    }

    /// The root directory, spelled as the caller's path spells it.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The root directory, canonical.
    pub fn canonical(&self) -> &Path {
        &self.canonical
    }

    /// Whether `specforge.json` is there; false for a loose root.
    pub fn is_configured(&self) -> bool {
        self.configured
    }

    /// This root spelled canonically: what MCP serves and compares (ADR 0014 D6).
    pub fn canonicalized(self) -> ProjectRoot {
        ProjectRoot {
            dir: self.canonical.clone(),
            ..self
        }
    }

    /// `<root>/specforge.json`.
    pub fn config_file(&self) -> PathBuf {
        self.dir.join(PROJECT_FILE)
    }
}

/// The ancestor of `path`'s directory, as `path` spells it (absolute), that
/// is the directory `canonical`; `None` when the spelling holds a `..` or no
/// ancestor reaches it (a symlink that leaves the tree).
fn spelled(path: &Path, is_file: bool, canonical: &Path) -> Option<PathBuf> {
    let absolute = std::path::absolute(path).ok()?;
    // `components` drops the `.`s; a `..` is not followed lexically.
    if absolute
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return None;
    }
    let lexical: PathBuf = absolute.components().collect();
    let lexical = match is_file {
        true => lexical.parent()?.to_path_buf(),
        false => lexical,
    };
    lexical
        .ancestors()
        .find(|dir| std::fs::canonicalize(dir).is_ok_and(|c| c == canonical))
        .map(Path::to_path_buf)
}

impl PartialEq for ProjectRoot {
    fn eq(&self, other: &Self) -> bool {
        self.canonical == other.canonical
    }
}

impl Eq for ProjectRoot {}

impl std::hash::Hash for ProjectRoot {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.canonical.hash(state);
    }
}

impl std::fmt::Display for NoSuchPath {
    /// `path not found: <path>` for `NotFound`, else `cannot resolve <path>: <kind>`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            std::io::ErrorKind::NotFound => write!(f, "path not found: {}", self.path.display()),
            kind => write!(f, "cannot resolve {}: {kind}", self.path.display()),
        }
    }
}

impl std::error::Error for NoSuchPath {}
