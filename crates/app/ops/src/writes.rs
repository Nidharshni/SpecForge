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

/// What a mutation did to disk (ADR 0162): it only previewed, because its request asked for a preview (`dry_run`;
/// format's check or diff), and wrote nothing; or it ran and wrote these files, none when it found nothing to
/// change. Copied from the request, never inferred from what the operation answered (ADR 0022 M2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wrote {
    Previewed,
    Ran(Writes),
}

impl Wrote {
    /// A run its request asked as `dry_run`, having written `writes`. A preview that wrote is a bug: debug builds
    /// panic.
    pub fn of(dry_run: bool, writes: Writes) -> Wrote {
        if dry_run {
            debug_assert!(writes.is_empty(), "a preview wrote: {writes:?}");
            Wrote::Previewed
        } else {
            Wrote::Ran(writes)
        }
    }

    /// `write` run unless `dry_run`; a preview runs nothing.
    pub(crate) fn unless_preview<E>(
        dry_run: bool,
        write: impl FnOnce() -> Result<Writes, E>,
    ) -> Result<Wrote, E> {
        if dry_run {
            Ok(Wrote::Previewed)
        } else {
            write().map(Wrote::Ran)
        }
    }

    pub fn previewed(&self) -> bool {
        matches!(self, Wrote::Previewed)
    }

    /// The files written: none for a preview.
    pub fn into_files(self) -> Writes {
        match self {
            Wrote::Previewed => Writes::none(),
            Wrote::Ran(writes) => writes,
        }
    }

    /// `files_written` as both surfaces print it: `None` for a preview, else every file written, named from `root`
    /// ([`Writes::named`]); `Some([])` for a run that wrote nothing.
    pub fn named(&self, root: Option<&Path>) -> Option<Vec<ProjectPath>> {
        match self {
            Wrote::Previewed => None,
            Wrote::Ran(writes) => Some(writes.named(root)),
        }
    }
}

/// An operation that cannot preview ran (init, a rollback, an inference step).
impl From<Writes> for Wrote {
    fn from(writes: Writes) -> Wrote {
        Wrote::Ran(writes)
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

    #[test]
    fn wrote_names_no_file_for_a_preview_and_every_file_for_a_run() {
        assert_eq!(Wrote::Previewed.named(None), None);
        assert_eq!(Wrote::Ran(Writes::none()).named(None), Some(vec![]));
        let two = Wrote::Ran(Writes::from_iter(["/p/specforge.json", "/p/a.spec"]));
        let named: Vec<String> = two
            .named(Some(Path::new("/p")))
            .unwrap()
            .into_iter()
            .map(ProjectPath::into_string)
            .collect();
        assert_eq!(named, ["a.spec", "specforge.json"]);
    }

    #[test]
    fn unless_preview_runs_nothing_for_a_preview() {
        let ran = Wrote::unless_preview(true, || -> Result<Writes, ()> { panic!("ran") });
        assert_eq!(ran, Ok(Wrote::Previewed));
        let ran = Wrote::unless_preview(false, || -> Result<Writes, ()> { Ok(Writes::none()) });
        assert_eq!(ran, Ok(Wrote::Ran(Writes::none())));
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "a preview wrote")]
    fn a_preview_that_wrote_is_a_bug() {
        let _ = Wrote::of(true, Writes::from_iter(["/p/a.spec"]));
    }
}
