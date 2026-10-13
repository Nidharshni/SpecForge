//! Shared `.spec` discovery policy (C14-16): one walker, one skip-list, one
//! symlink rule for every surface that scans a workspace (resolver, LSP,
//! formatter, CLI). Divergent per-crate walkers made the same job run at
//! three speeds with three results.

use std::path::{Path, PathBuf};

use crate::Exclude;

/// Directories never entered below a walked root: build artifacts,
/// dependency trees, and VCS internals. A walked root is entered whatever
/// its own name.
const SKIP_DIRS: &[&str] = &["target", "node_modules", ".git", ".hg", "dist", "build"];

/// What one walk of a directory found.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Walked {
    /// The `.spec` files reached, sorted. Only files, through a symlink too: a
    /// directory named like one, or a symlink to one, is not a source; nor is
    /// a symlink that points nowhere.
    pub files: Vec<PathBuf>,
    /// The directories below the root that the walk reached and did not
    /// enter because `stop` held for them, sorted.
    pub stopped: Vec<PathBuf>,
}

/// The one `.spec` walk, under the shared policy:
/// - `root` is entered whatever its name;
/// - no [`SKIP_DIRS`] directory below it is entered;
/// - a symlink is not followed into a directory; one that resolves to a file
///   is reached as that file, one to a directory or to nothing is no source;
/// - a directory below `root` for which `stop` holds is not entered but is
///   listed in [`Walked::stopped`].
///
/// No `exclude` entry applies here; a project's sources are
/// [`discover_spec_files`].
pub fn walk_spec_files(root: &Path, mut stop: impl FnMut(&Path) -> bool) -> Walked {
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
                .is_some_and(|name| SKIP_DIRS.contains(&name))
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
            is_source_entry(ft, entry.path())
                && entry.path().extension().is_some_and(|ext| ext == "spec")
        })
        .map(walkdir::DirEntry::into_path)
        .collect();
    files.sort();
    stopped.sort();
    Walked { files, stopped }
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

/// A project's sources under `root` (its spec root): the `.spec` files
/// [`walk_spec_files`] reaches that no `exclude` entry leaves out (the
/// project's `specforge.json` `exclude`, ADR 0055 D7). Sorted for
/// deterministic downstream behavior.
pub fn discover_spec_files(root: &Path, exclude: &Exclude) -> Vec<PathBuf> {
    walk_spec_files(root, |_| false)
        .files
        .into_iter()
        .filter(|path| !exclude.excludes(path.strip_prefix(root).unwrap_or(path)))
        .collect()
}

/// The sources [`discover_spec_files`] finds at `path` under `root`, now: when
/// `path` is a directory (not a symlink), every one under it; when it is a
/// source file (a file, through a symlink too, that [`is_discovered`] keeps),
/// itself; else none. Equal to `discover_spec_files(root, exclude)` filtered to
/// `path` and what lies under it.
pub fn discover_spec_files_at(root: &Path, path: &Path, exclude: &Exclude) -> Vec<PathBuf> {
    use std::path::Component;
    let Ok(relative) = path.strip_prefix(root) else {
        return Vec::new();
    };
    if relative.as_os_str().is_empty() {
        return discover_spec_files(root, exclude);
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
    // No directory on the way is a symlink: the walk never enters one.
    let mut walked = root.to_path_buf();
    for name in &names[..names.len() - 1] {
        walked.push(name);
        if std::fs::symlink_metadata(&walked).is_ok_and(|m| m.file_type().is_symlink()) {
            return Vec::new();
        }
    }
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_dir() => walk_spec_files(path, |_| false)
            .files
            .into_iter()
            .filter(|file| !exclude.excludes(file.strip_prefix(root).unwrap_or(file)))
            .collect(),
        Ok(meta)
            if is_source_entry(meta.file_type(), path)
                && is_discovered(&relative.to_string_lossy(), exclude) =>
        {
            vec![path.to_path_buf()]
        }
        _ => Vec::new(),
    }
}

/// Whether [`discover_spec_files`] finds `relative` (a path relative to
/// the walked root): a `.spec` file inside the root, under no
/// [`SKIP_DIRS`] directory, that no `exclude` entry leaves out. A surface
/// told about one changed file applies the policy the walk would.
pub fn is_discovered(relative: &str, exclude: &Exclude) -> bool {
    use std::path::Component;
    let path = Path::new(relative);
    path.extension().is_some_and(|ext| ext == "spec")
        && path.parent().is_none_or(|dir| {
            dir.components().all(|c| match c {
                Component::Normal(name) => !name.to_str().is_some_and(|n| SKIP_DIRS.contains(&n)),
                Component::CurDir => true,
                _ => false,
            })
        })
        && !exclude.excludes(path)
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

        let found = discover_spec_files(root, &Exclude::default());
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

        let found = discover_spec_files(root, &Exclude::parse(["legacy"]).unwrap());
        let names: Vec<String> = found
            .iter()
            .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["specs/current.spec".to_string()]);
    }

    #[test]
    fn is_discovered_applies_the_walk_policy_to_one_path() {
        let exclude = Exclude::parse(["drafts/"]).unwrap();
        assert!(is_discovered("a.spec", &exclude));
        assert!(is_discovered("src/deep/a.spec", &exclude));
        assert!(!is_discovered("notes.txt", &exclude));
        assert!(!is_discovered("target/generated.spec", &exclude));
        assert!(!is_discovered("src/node_modules/x.spec", &exclude));
        assert!(!is_discovered("drafts/a.spec", &exclude));
        assert!(!is_discovered("../outside.spec", &exclude));
        // A file merely named like a skipped directory is still found.
        assert!(is_discovered("build.spec", &exclude));
    }

    fn found(relative: &str, exclude: &[&str]) -> bool {
        is_discovered(relative, &Exclude::parse(exclude.iter().copied()).unwrap())
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
            discover_spec_files(&root.join("build"), &Exclude::default()),
            vec![root.join("build/a.spec")]
        );
        // The `build` directory as a child is still skipped.
        assert!(discover_spec_files(root, &Exclude::default()).is_empty());
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
            discover_spec_files(root, &Exclude::default()),
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
            discover_spec_files(&root, &Exclude::default()),
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

        let walked = walk_spec_files(root, crate::ProjectRoot::is_root);
        assert_eq!(
            walked.files,
            vec![root.join("a.spec"), root.join("q/c.spec")]
        );
        assert_eq!(walked.stopped, vec![root.join("p")]);
        // A root `stop` would hold for is still walked.
        let inside = walk_spec_files(&root.join("p"), crate::ProjectRoot::is_root);
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
        let whole = discover_spec_files(&root, &exclude);
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
            assert_eq!(
                discover_spec_files_at(&root, &path, &exclude),
                expected,
                "{}",
                path.display()
            );
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

    #[ignore = "fixed by T2"]
    #[specforge_test(
        behavior = "resolve_use_imports",
        verify = "a .spec file inside a nested project below the spec root is no source"
    )]
    fn a_projects_sources_end_at_a_nested_project() {
        let tmp = tree_with_a_nested_project();
        let root = tmp.path();
        assert_eq!(
            discover_spec_files(root, &Exclude::default()),
            vec![root.join("a.spec"), root.join("q/c.spec")]
        );
    }

    #[ignore = "fixed by T2"]
    #[specforge_test(
        behavior = "resolve_use_imports",
        verify = "a .spec file inside a nested project below the spec root is no source"
    )]
    fn a_path_in_a_nested_project_is_no_source() {
        let _tmp = tree_with_a_nested_project();
        let exclude = Exclude::default();
        assert!(!is_discovered("p/b.spec", &exclude));
        assert!(!is_discovered("p/deep/d.spec", &exclude));
        assert!(is_discovered("q/c.spec", &exclude));
        assert!(is_discovered("p.spec", &exclude));
        assert!(is_discovered("gone/x.spec", &exclude));
    }
}
