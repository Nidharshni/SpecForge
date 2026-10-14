//! What an operation changed on disk.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The files an operation changed on disk: each path it created, rewrote or
/// removed and left so, recorded where it wrote, once, in path order. Paths
/// are the operation's root joined with the file (absolute when the root
/// is). A write that changed nothing (an entry already present, a file
/// already formatted, a `.gitignore` that held every entry) is not
/// recorded; a file written and then restored (a rolled-back migration) is
/// forgotten. State the registry adapter keeps for the user (trust pins,
/// credentials) is not the operation's write.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Writes(BTreeSet<PathBuf>);

impl Writes {
    /// Nothing written.
    pub fn none() -> Self {
        Self::default()
    }

    /// The files a committed change changed (or a failed one left).
    pub fn of(paths: impl IntoIterator<Item = PathBuf>) -> Self {
        paths.into_iter().collect()
    }

    /// `path` was created, rewritten or removed.
    pub fn record(&mut self, path: impl Into<PathBuf>) {
        self.0.insert(path.into());
    }

    /// `path` holds again what it held before the operation.
    pub fn forget(&mut self, path: &Path) {
        self.0.remove(path);
    }

    /// What an operation this one ran wrote (init's installs).
    pub fn merge(&mut self, other: Writes) {
        self.0.extend(other.0);
    }

    /// Every path, in path order.
    pub fn paths(&self) -> impl ExactSizeIterator<Item = &Path> {
        self.0.iter().map(PathBuf::as_path)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Every path for display, sorted: relative to `root` when it lies
    /// under it, else as recorded. A path and a root that name one place
    /// through a symbolic link (`/var` and `/private/var`) are compared as
    /// their canonical forms, a removed file's through its nearest
    /// existing ancestor.
    pub fn under(&self, root: &Path) -> Vec<PathBuf> {
        let canonical_root = canonical(root);
        let mut shown: Vec<PathBuf> = self
            .0
            .iter()
            .map(|path| {
                path.strip_prefix(root)
                    .map(Path::to_path_buf)
                    .or_else(|_| {
                        canonical(path)
                            .strip_prefix(&canonical_root)
                            .map(Path::to_path_buf)
                    })
                    .unwrap_or_else(|_| path.clone())
            })
            .collect();
        shown.sort();
        shown.dedup();
        shown
    }

    /// [`Self::under`] as display strings: what a surface lists.
    pub fn names_under(&self, root: &Path) -> Vec<String> {
        self.under(root)
            .iter()
            .map(|path| path.display().to_string())
            .collect()
    }
}

impl<P: Into<PathBuf>> FromIterator<P> for Writes {
    fn from_iter<I: IntoIterator<Item = P>>(paths: I) -> Self {
        Writes(paths.into_iter().map(Into::into).collect())
    }
}

/// Where `file` really is, when that is outside `within`: a symlink in the
/// project that leaves it (ADR 0092 D8). `None` when it is inside, and when
/// either does not exist, since nothing is read or written there.
pub fn resolves_outside(file: &Path, within: &Path) -> Option<PathBuf> {
    let (real, within) = (file.canonicalize().ok()?, within.canonicalize().ok()?);
    (!real.starts_with(within)).then_some(real)
}

/// `path` absolute and canonical as far as it exists: the canonical form
/// of its nearest existing ancestor, joined with the rest.
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
mod tests {
    use super::*;

    #[test]
    fn records_each_path_once_in_order() {
        let mut writes = Writes::none();
        assert!(writes.is_empty());
        writes.record("/p/b.spec");
        writes.record("/p/a.spec");
        writes.record("/p/b.spec");
        assert_eq!(writes.len(), 2);
        let paths: Vec<&Path> = writes.paths().collect();
        assert_eq!(paths, [Path::new("/p/a.spec"), Path::new("/p/b.spec")]);
    }

    #[test]
    fn forget_drops_a_restored_path() {
        let mut writes = Writes::from_iter(["/p/old.spec", "/p/old.spec.bak"]);
        writes.forget(Path::new("/p/old.spec"));
        writes.forget(Path::new("/p/never.spec"));
        assert_eq!(writes, Writes::from_iter(["/p/old.spec.bak"]));
    }

    #[test]
    fn merge_unions() {
        let mut writes = Writes::from_iter(["/p/specforge.json", "/p/spec/hello.spec"]);
        writes.merge(Writes::from_iter([
            "/p/specforge.json",
            "/p/specforge.lock",
        ]));
        assert_eq!(
            writes,
            Writes::from_iter([
                "/p/spec/hello.spec",
                "/p/specforge.json",
                "/p/specforge.lock"
            ])
        );
    }

    #[test]
    fn under_lists_paths_relative_to_the_root() {
        let writes =
            Writes::from_iter(["/p/spec/a.spec", "/p/specforge.json", "/elsewhere/x.wasm"]);
        assert_eq!(
            writes.names_under(Path::new("/p")),
            ["/elsewhere/x.wasm", "spec/a.spec", "specforge.json"]
        );
    }

    #[test]
    fn under_sees_through_a_linked_root() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir_all(real.join("spec")).unwrap();
        #[cfg(unix)]
        {
            let link = dir.path().join("link");
            std::os::unix::fs::symlink(&real, &link).unwrap();
            // Written through the link, shown under the real root; a removed
            // file (no longer on disk) included.
            let writes = Writes::from_iter([link.join("spec/a.spec"), link.join("gone.wasm")]);
            assert_eq!(writes.names_under(&real), ["gone.wasm", "spec/a.spec"]);
        }
    }
}
