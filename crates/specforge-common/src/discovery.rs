//! Shared `.spec` discovery policy (C14-16): one walker, one skip-list, one
//! symlink rule for every surface that scans a workspace (resolver, LSP,
//! formatter, CLI). Divergent per-crate walkers made the same job run at
//! three speeds with three results.

use std::path::{Path, PathBuf};

/// Directories never entered below a walked root: build artifacts,
/// dependency trees, and VCS internals. A walked root is entered whatever
/// its own name.
const SKIP_DIRS: &[&str] = &["target", "node_modules", ".git", ".hg", "dist", "build"];

/// What one walk of a directory found.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Walked {
    /// The `.spec` files reached, sorted. Only files: a directory named like
    /// one is not a source.
    pub files: Vec<PathBuf>,
    /// The directories below the root that the walk reached and did not
    /// enter because `stop` held for them, sorted.
    pub stopped: Vec<PathBuf>,
}

/// The one `.spec` walk, under the shared policy:
/// - `root` is entered whatever its name;
/// - no [`SKIP_DIRS`] directory below it is entered;
/// - symlinks are not followed;
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
            !entry.file_type().is_dir() && entry.path().extension().is_some_and(|ext| ext == "spec")
        })
        .map(walkdir::DirEntry::into_path)
        .collect();
    files.sort();
    stopped.sort();
    Walked { files, stopped }
}

/// A project's sources under `root` (its spec root): the `.spec` files
/// [`walk_spec_files`] reaches that no `exclude` pattern matches, as
/// substrings of the path relative to `root` (project `specforge.json`
/// `exclude` entries, C4-04). Sorted for deterministic downstream behavior.
pub fn discover_spec_files(root: &Path, exclude: &[String]) -> Vec<PathBuf> {
    walk_spec_files(root, |_| false)
        .files
        .into_iter()
        .filter(|path| {
            let relative = path.strip_prefix(root).unwrap_or(path);
            !is_excluded(&relative.to_string_lossy(), exclude)
        })
        .collect()
}

/// Whether [`discover_spec_files`] finds `relative` (a path relative to
/// the walked root): a `.spec` file inside the root, under no
/// [`SKIP_DIRS`] directory, that no `exclude` pattern matches. A surface
/// told about one changed file applies the policy the walk would.
pub fn is_discovered(relative: &str, exclude: &[String]) -> bool {
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
        && !is_excluded(relative, exclude)
}

/// Whether `relative` (a path relative to the walked root) matches an
/// `exclude` pattern: patterns are plain substrings, not globs.
pub fn is_excluded(relative: &str, exclude: &[String]) -> bool {
    exclude
        .iter()
        .any(|pattern| relative.contains(pattern.as_str()))
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

        let found = discover_spec_files(root, &[]);
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

        let found = discover_spec_files(root, &["legacy".to_string()]);
        let names: Vec<String> = found
            .iter()
            .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["specs/current.spec".to_string()]);
    }

    #[test]
    fn is_discovered_applies_the_walk_policy_to_one_path() {
        let exclude = vec!["drafts/".to_string()];
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
        let exclude: Vec<String> = exclude.iter().map(|e| e.to_string()).collect();
        is_discovered(relative, &exclude)
    }

    #[specforge_test(
        behavior = "resolve_use_imports",
        verify = "an exclude entry without a slash matches a name at any depth"
    )]
    #[ignore = "fixed by T3"]
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
    #[ignore = "fixed by T3"]
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
    #[ignore = "fixed by T3"]
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
            discover_spec_files(&root.join("build"), &[]),
            vec![root.join("build/a.spec")]
        );
        // The `build` directory as a child is still skipped.
        assert!(discover_spec_files(root, &[]).is_empty());
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

        assert_eq!(discover_spec_files(root, &[]), vec![root.join("a.spec")]);
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
}
