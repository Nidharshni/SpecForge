//! Shared discovery policy (C14-16): one walker, one skip-list, one
//! symlink rule for every surface that scans a workspace (resolver, LSP,
//! formatter, CLI). Divergent per-crate walkers made the same job run at
//! three speeds with three results. `.spec` discovery and inference's
//! sources both walk with it.

use std::path::{Path, PathBuf};

use crate::{Exclude, ProjectRoot};

/// Directories never entered below a walked root: build artifacts,
/// dependency trees, and VCS internals. A walked root is entered whatever
/// its own name.
const SKIP_DIRS: &[&str] = &["target", "node_modules", ".git", ".hg", "dist", "build"];

/// What one walk of a directory found.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Walked {
    /// The files reached that `keep` accepts, sorted. Only files, through a
    /// symlink too: a directory named like one, or a symlink to one, is not a
    /// source; nor is a symlink that points nowhere.
    pub files: Vec<PathBuf>,
    /// The directories below the root that the walk reached and did not
    /// enter because `stop` held for them, sorted.
    pub stopped: Vec<PathBuf>,
}

/// The one walk, under the shared policy, for the files `keep` accepts:
/// - `root` is entered whatever its name;
/// - below it, no [`SKIP_DIRS`] directory is entered, nor one whose (UTF-8)
///   name `skip` holds for;
/// - a symlink is not followed into a directory; one that resolves to a file
///   is reached as that file, one to a directory or to nothing is no source;
/// - a directory below `root` for which `stop` holds is not entered but is
///   listed in [`Walked::stopped`];
/// - a reached file is listed when `keep` holds for its path.
///
/// No `exclude` entry and no project boundary applies here; a project's
/// sources are [`SourceRoot`].
pub fn walk_files(
    root: &Path,
    skip: impl Fn(&str) -> bool,
    keep: impl Fn(&Path) -> bool,
    mut stop: impl FnMut(&Path) -> bool,
) -> Walked {
    let mut stopped: Vec<PathBuf> = Vec::new();
    let mut files: Vec<PathBuf> = walkdir::WalkDir::new(root)
        .follow_links(false) // symlink cycles and out-of-tree traversal are never wanted here
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 || !entry.file_type().is_dir() {
                return true;
            }
            if entry
                .file_name()
                .to_str()
                .is_some_and(|name| SKIP_DIRS.contains(&name) || skip(name))
            {
                return false;
            }
            if stop(entry.path()) {
                stopped.push(entry.path().to_path_buf());
                return false;
            }
            true
        })
        .filter_map(Result::ok)
        .filter(|entry| {
            let ft = entry.file_type();
            is_source_entry(ft, entry.path()) && keep(entry.path())
        })
        .map(walkdir::DirEntry::into_path)
        .collect();
    files.sort();
    stopped.sort();
    Walked { files, stopped }
}

/// [`walk_files`] for `.spec` files, skipping only [`SKIP_DIRS`].
pub fn walk_spec_files(root: &Path, stop: impl FnMut(&Path) -> bool) -> Walked {
    walk_files(
        root,
        |_| false,
        |path| path.extension().is_some_and(|ext| ext == "spec"),
        stop,
    )
}

/// Whether an entry of a walk of type `ft` at `path` can be a source: not a
/// directory, and a symlink only when it resolves to a file (not to a
/// directory, not to nothing).
fn is_source_entry(ft: std::fs::FileType, path: &Path) -> bool {
    if ft.is_symlink() {
        path.is_file()
    } else {
        !ft.is_dir()
    }
}

/// Where a project's sources are (CONTEXT "Project sources"): its spec root
/// and what its `exclude` entries leave out. The one answer every compile,
/// session, format, migrate and rename asks for "which files are the
/// sources" ([`Self::files`]), "which are at this path" ([`Self::files_at`])
/// and "is this one of them" ([`Self::holds`]).
///
/// The walk enters the spec root whatever its own name and whatever it
/// holds, and below it enters no [`SKIP_DIRS`] directory, no symlink, and no
/// directory that holds `specforge.json`: that directory is a nested
/// project, whose files are its own project's sources and none of this
/// one's (ADR 0080). Only files are sources, a symlink only when it
/// resolves to a file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceRoot {
    dir: PathBuf,
    exclude: Exclude,
}

impl SourceRoot {
    /// The sources under `dir` (a spec root) that `exclude` does not leave out.
    pub fn new(dir: PathBuf, exclude: Exclude) -> Self {
        SourceRoot { dir, exclude }
    }

    /// The spec root.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The sources on disk now, sorted: the `.spec` files [`walk_spec_files`]
    /// reaches under the spec root that no `exclude` entry leaves out (the
    /// project's `specforge.json` `exclude`, ADR 0055 D7).
    pub fn files(&self) -> Vec<PathBuf> {
        walk_spec_files(&self.dir, ProjectRoot::is_root)
            .files
            .into_iter()
            .filter(|path| self.keeps(path))
            .collect()
    }

    /// The sources [`Self::files`] lists at `path`, now: when `path` is a
    /// directory (not a symlink), every one under it; when it is a source
    /// file (a file, through a symlink too, that [`Self::holds`] keeps),
    /// itself; else none. Equal to [`Self::files`] filtered to `path` and what
    /// lies under it.
    pub fn files_at(&self, path: &Path) -> Vec<PathBuf> {
        use std::path::Component;
        let Ok(relative) = path.strip_prefix(&self.dir) else {
            return Vec::new();
        };
        if relative.as_os_str().is_empty() {
            return self.files();
        }
        let names: Vec<&std::ffi::OsStr> = relative
            .components()
            .filter_map(|c| match c {
                Component::Normal(name) => Some(name),
                _ => None,
            })
            .collect();
        if names.len() != relative.components().count()
            || names
                .iter()
                .any(|name| name.to_str().is_some_and(|n| SKIP_DIRS.contains(&n)))
        {
            return Vec::new();
        }
        // No directory on the way is a symlink or a nested project: the walk
        // never enters one.
        let mut walked = self.dir.clone();
        for name in &names[..names.len() - 1] {
            walked.push(name);
            if std::fs::symlink_metadata(&walked).is_ok_and(|m| m.file_type().is_symlink())
                || ProjectRoot::is_root(&walked)
            {
                return Vec::new();
            }
        }
        match std::fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_dir() => {
                if ProjectRoot::is_root(path) {
                    return Vec::new();
                }
                walk_spec_files(path, ProjectRoot::is_root)
                    .files
                    .into_iter()
                    .filter(|file| self.keeps(file))
                    .collect()
            }
            Ok(meta)
                if is_source_entry(meta.file_type(), path)
                    && self.holds(&relative.to_string_lossy()) =>
            {
                vec![path.to_path_buf()]
            }
            _ => Vec::new(),
        }
    }

    /// Whether `key`, a path relative to the spec root, is one [`Self::files`]
    /// lists once the file exists:
    /// - a `.spec` path inside the spec root (no `..`, not absolute);
    /// - under no [`SKIP_DIRS`] directory;
    /// - in no directory below the spec root that is a symlink or holds
    ///   `specforge.json` now (it reads whether each directory on the way is
    ///   one or holds one, nothing else): the walk never enters either;
    /// - not left out by an `exclude` entry.
    ///
    /// A surface told about one changed path applies the walk's policy
    /// through it.
    pub fn holds(&self, key: &str) -> bool {
        use std::path::Component;
        let path = Path::new(key);
        let lexical = path.extension().is_some_and(|ext| ext == "spec")
            && path.parent().is_none_or(|dir| {
                dir.components().all(|c| match c {
                    Component::Normal(name) => {
                        !name.to_str().is_some_and(|n| SKIP_DIRS.contains(&n))
                    }
                    Component::CurDir => true,
                    _ => false,
                })
            })
            && !self.exclude.excludes(path);
        lexical
            && path
                .ancestors()
                .skip(1)
                .filter(|dir| !dir.as_os_str().is_empty())
                .all(|dir| {
                    let at = self.dir.join(dir);
                    !ProjectRoot::is_root(&at)
                        && !std::fs::symlink_metadata(&at).is_ok_and(|m| m.file_type().is_symlink())
                })
    }

    /// Whether no `exclude` entry leaves out `file`, a path under the spec root.
    fn keeps(&self, file: &Path) -> bool {
        !self
            .exclude
            .excludes(file.strip_prefix(&self.dir).unwrap_or(file))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_test_macros::test as specforge_test;
    use std::fs;

    #[test]
    fn skips_build_dirs_and_filters_by_extension() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.spec"), "").unwrap();
        fs::write(root.join("src/notes.txt"), "").unwrap();
        fs::write(root.join("target/generated.spec"), "").unwrap();

        let found = sources(root, Exclude::default()).files();
        let names: Vec<String> = found
            .iter()
            .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["src/main.spec".to_string()]);
    }

    #[test]
    fn exclude_patterns_drop_matching_relative_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("specs/legacy")).unwrap();
        fs::write(root.join("specs/current.spec"), "").unwrap();
        fs::write(root.join("specs/legacy/old.spec"), "").unwrap();

        let found = sources(root, Exclude::parse(["legacy"]).unwrap()).files();
        let names: Vec<String> = found
            .iter()
            .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["specs/current.spec".to_string()]);
    }

    #[test]
    fn holds_applies_the_walk_policy_to_one_path() {
        let tmp = tempfile::tempdir().unwrap();
        let exclude = sources(tmp.path(), Exclude::parse(["drafts/"]).unwrap());
        assert!(exclude.holds("a.spec"));
        assert!(exclude.holds("src/deep/a.spec"));
        assert!(!exclude.holds("notes.txt"));
        assert!(!exclude.holds("target/generated.spec"));
        assert!(!exclude.holds("src/node_modules/x.spec"));
        assert!(!exclude.holds("drafts/a.spec"));
        assert!(!exclude.holds("../outside.spec"));
        // A file merely named like a skipped directory is still found.
        assert!(exclude.holds("build.spec"));
    }

    #[cfg(unix)]
    #[test]
    fn holds_refuses_a_path_under_a_symlinked_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("sub/a.spec"), "").unwrap();
        std::os::unix::fs::symlink(root.join("sub"), root.join("dirlink")).unwrap();
        let held = sources(root, Exclude::default());
        assert!(held.holds("sub/a.spec"));
        assert!(!held.holds("dirlink/a.spec"));
        assert!(!held.holds("dirlink/new.spec"));
    }

    fn found(relative: &str, exclude: &[&str]) -> bool {
        let tmp = tempfile::tempdir().unwrap();
        sources(tmp.path(), Exclude::parse(exclude.iter().copied()).unwrap()).holds(relative)
    }

    fn sources(dir: &Path, exclude: Exclude) -> SourceRoot {
        SourceRoot::new(dir.to_path_buf(), exclude)
    }

    #[specforge_test(
        behavior = "resolve_use_imports",
        verify = "an exclude entry without a slash matches a name at any depth"
    )]
    fn an_exclude_entry_without_a_slash_matches_a_name_at_any_depth() {
        assert!(!found("drafts/a.spec", &["drafts"]));
        assert!(!found("sub/drafts/b.spec", &["drafts"]));
        assert!(found("mydrafts/c.spec", &["drafts"]));
        assert!(found("drafts.spec", &["drafts"]));
        assert!(found("general.spec", &["gen"]));
        assert!(!found("x.draft.spec", &["*.draft.spec"]));
        assert!(!found("sub/y.draft.spec", &["*.draft.spec"]));
        assert!(!found("drafts/a.spec", &["drafts/"]));
        assert!(!found("sub/drafts/a.spec", &["drafts/"]));
    }

    #[specforge_test(
        behavior = "resolve_use_imports",
        verify = "an exclude entry with a slash is anchored at the spec root"
    )]
    fn an_exclude_entry_with_a_slash_is_anchored_at_the_spec_root() {
        assert!(!found("sub/drafts/a.spec", &["sub/drafts"]));
        assert!(found("x/sub/drafts/a.spec", &["sub/drafts"]));
        assert!(!found("drafts/a.spec", &["/drafts"]));
        assert!(found("sub/drafts/a.spec", &["/drafts"]));
    }

    #[specforge_test(
        behavior = "resolve_use_imports",
        verify = "exclude entries are glob patterns matched against a file and its directories"
    )]
    fn exclude_entries_are_glob_patterns_over_a_file_and_its_directories() {
        assert!(!found("drafts/a.spec", &["drafts/**"]));
        assert!(!found("drafts/x/y.spec", &["drafts/**"]));
        assert!(found("sub/drafts/a.spec", &["drafts/**"]));
        assert!(!found("v1/old/a.spec", &["v?/old"]));
        assert!(!found("a/x/y.spec", &["[ab]/x"]));
        assert!(found("c/x/y.spec", &["[ab]/x"]));
        assert!(!found("gen/a.spec", &["**/gen"]));
        assert!(!found("p/q/gen/a.spec", &["**/gen"]));
        // A trailing slash matches directories only.
        assert!(found("x.spec", &["x.spec/"]));
    }

    #[specforge_test(
        behavior = "resolve_use_imports",
        verify = "a spec root named like a skipped directory is compiled"
    )]
    fn a_walked_root_is_walked_whatever_its_name() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("build")).unwrap();
        fs::write(root.join("build/a.spec"), "").unwrap();

        assert_eq!(
            sources(&root.join("build"), Exclude::default()).files(),
            vec![root.join("build/a.spec")]
        );
        // The `build` directory as a child is still skipped.
        assert!(sources(root, Exclude::default()).files().is_empty());
    }

    #[specforge_test(
        behavior = "resolve_use_imports",
        verify = "a directory named like a .spec file is no source"
    )]
    fn only_files_are_sources() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::write(root.join("a.spec"), "").unwrap();
        fs::create_dir_all(root.join("archive.spec")).unwrap();

        assert_eq!(
            sources(root, Exclude::default()).files(),
            vec![root.join("a.spec")]
        );
    }

    #[cfg(unix)]
    #[specforge_test(
        behavior = "resolve_use_imports",
        verify = "a symlink is a source only when it resolves to a file"
    )]
    fn a_symlink_is_a_source_only_when_it_resolves_to_a_file() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let sibling = tmp.path().join("sibling");
        fs::create_dir_all(root.join("realdir")).unwrap();
        fs::create_dir_all(&sibling).unwrap();
        fs::write(root.join("a.spec"), "").unwrap();
        fs::write(sibling.join("other.spec"), "").unwrap();
        std::os::unix::fs::symlink(root.join("realdir"), root.join("d.spec")).unwrap();
        std::os::unix::fs::symlink("nowhere.spec", root.join("dangling.spec")).unwrap();
        std::os::unix::fs::symlink(sibling.join("other.spec"), root.join("link.spec")).unwrap();

        assert_eq!(
            sources(&root, Exclude::default()).files(),
            vec![root.join("a.spec"), root.join("link.spec")]
        );
    }

    #[test]
    fn a_walk_stops_where_told_and_lists_where() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        for (path, text) in [
            ("a.spec", ""),
            ("p/specforge.json", "{}"),
            ("p/b.spec", ""),
            ("q/c.spec", ""),
        ] {
            fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
            fs::write(root.join(path), text).unwrap();
        }

        let walked = walk_spec_files(root, ProjectRoot::is_root);
        assert_eq!(
            walked.files,
            vec![root.join("a.spec"), root.join("q/c.spec")]
        );
        assert_eq!(walked.stopped, vec![root.join("p")]);
        // A root `stop` would hold for is still walked.
        let inside = walk_spec_files(&root.join("p"), ProjectRoot::is_root);
        assert_eq!(inside.files, vec![root.join("p/b.spec")]);
    }

    #[specforge_test(
        behavior = "resolve_use_imports",
        verify = "discovery at a path finds what the whole walk finds there"
    )]
    fn discovery_at_a_path_is_the_walk_under_it() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        for path in [
            "a.spec",
            "notes.txt",
            "sub/b.spec",
            "sub/deep/c.spec",
            "target/gen.spec",
            "drafts/d.spec",
            "archive.spec/inner.spec",
            "nested/specforge.json",
            "nested/n.spec",
            "nested/deep/m.spec",
            "sub/inner/specforge.json",
            "sub/inner/i.spec",
        ] {
            fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
            fs::write(root.join(path), "").unwrap();
        }
        #[cfg(unix)]
        {
            let other = tmp.path().join("other");
            fs::create_dir_all(&other).unwrap();
            fs::write(other.join("o.spec"), "").unwrap();
            std::os::unix::fs::symlink(other.join("o.spec"), root.join("sub/link.spec")).unwrap();
            std::os::unix::fs::symlink(root.join("sub"), root.join("dirlink")).unwrap();
            std::os::unix::fs::symlink("nowhere.spec", root.join("dangling.spec")).unwrap();
        }
        let exclude = Exclude::parse(["drafts/"]).unwrap();
        let sources = sources(&root, exclude);
        let whole = sources.files();
        let mut paths: Vec<PathBuf> = walkdir::WalkDir::new(&root)
            .into_iter()
            .filter_map(Result::ok)
            .map(walkdir::DirEntry::into_path)
            .collect();
        paths.push(root.join("sub/missing/x.spec"));
        paths.push(root.join("dirlink/b.spec"));
        paths.push(root.clone());
        for path in paths {
            let expected: Vec<PathBuf> = whole
                .iter()
                .filter(|f| **f == path || f.starts_with(&path))
                .cloned()
                .collect();
            assert_eq!(sources.files_at(&path), expected, "{}", path.display());
        }
    }

    /// `a.spec`, `q/c.spec`, and a nested project `p` (`specforge.json`,
    /// `b.spec`, `deep/d.spec`) under a fresh root.
    fn tree_with_a_nested_project() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        for (path, text) in [
            ("a.spec", ""),
            ("q/c.spec", ""),
            ("p/specforge.json", "{}"),
            ("p/b.spec", ""),
            ("p/deep/d.spec", ""),
        ] {
            fs::create_dir_all(tmp.path().join(path).parent().unwrap()).unwrap();
            fs::write(tmp.path().join(path), text).unwrap();
        }
        tmp
    }

    #[specforge_test(
        behavior = "resolve_use_imports",
        verify = "a .spec file inside a nested project below the spec root is no source"
    )]
    fn a_projects_sources_end_at_a_nested_project() {
        let tmp = tree_with_a_nested_project();
        let root = tmp.path();
        assert_eq!(
            sources(root, Exclude::default()).files(),
            vec![root.join("a.spec"), root.join("q/c.spec")]
        );
    }

    #[specforge_test(
        behavior = "resolve_use_imports",
        verify = "a .spec file inside a nested project below the spec root is no source"
    )]
    fn a_path_in_a_nested_project_is_no_source() {
        let tmp = tree_with_a_nested_project();
        let held = sources(tmp.path(), Exclude::default());
        assert!(!held.holds("p/b.spec"));
        assert!(!held.holds("p/deep/d.spec"));
        assert!(held.holds("q/c.spec"));
        assert!(held.holds("p.spec"));
        assert!(held.holds("gone/x.spec"));
    }

    #[test]
    fn walk_files_keeps_what_it_is_told_and_skips_what_it_is_told() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        for file in ["a.rs", "a.spec", "gen/b.rs", "target/c.rs"] {
            let path = root.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "").unwrap();
        }
        let walked = walk_files(
            root,
            |name| name == "gen",
            |path| path.extension() == Some("rs".as_ref()),
            |_| false,
        );
        assert_eq!(walked.files, [root.join("a.rs")]);
    }

    #[cfg(unix)]
    #[test]
    fn walk_files_enters_no_symlinked_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let other = tmp.path().join("other");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&other).unwrap();
        fs::write(root.join("a.rs"), "").unwrap();
        fs::write(other.join("x.rs"), "").unwrap();
        std::os::unix::fs::symlink(".", root.join("loop")).unwrap();
        std::os::unix::fs::symlink(&other, root.join("out")).unwrap();
        let walked = walk_files(
            &root,
            |_| false,
            |path| path.extension() == Some("rs".as_ref()),
            |_| false,
        );
        assert_eq!(walked.files, [root.join("a.rs")]);
    }
}
