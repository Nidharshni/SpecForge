//! What an operation changed on disk.

use specforge_common::ProjectPath;
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

    /// Every path as a document names it ([`ProjectPath`]), sorted and listed once: relative to `root` with `/`,
    /// absolute and canonical outside it; with no root, absolute and canonical.
    pub fn named(&self, root: Option<&Path>) -> Vec<ProjectPath> {
        let mut named: Vec<ProjectPath> = self
            .0
            .iter()
            .map(|path| match root {
                Some(root) => ProjectPath::of(root, path),
                None => ProjectPath::absolute(path),
            })
            .collect();
        named.sort();
        named.dedup();
        named
    }
}

impl<P: Into<PathBuf>> FromIterator<P> for Writes {
    fn from_iter<I: IntoIterator<Item = P>>(paths: I) -> Self {
        Writes(paths.into_iter().map(Into::into).collect())
    }
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
    fn named_lists_paths_relative_to_the_root() {
        let writes =
            Writes::from_iter(["/p/spec/a.spec", "/p/specforge.json", "/elsewhere/x.wasm"]);
        let named: Vec<String> = writes
            .named(Some(Path::new("/p")))
            .into_iter()
            .map(ProjectPath::into_string)
            .collect();
        assert_eq!(
            named,
            ["/elsewhere/x.wasm", "spec/a.spec", "specforge.json"]
        );
    }

    #[test]
    fn named_without_a_root_is_absolute() {
        let writes = Writes::from_iter(["/p/spec/a.spec", "/p/b.spec"]);
        let named: Vec<String> = writes
            .named(None)
            .into_iter()
            .map(ProjectPath::into_string)
            .collect();
        assert_eq!(named, ["/p/b.spec", "/p/spec/a.spec"]);
    }
}
