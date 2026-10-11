//! Shared `.spec` discovery policy (C14-16): one walker, one skip-list, one
//! symlink rule for every surface that scans a workspace (resolver, LSP,
//! formatter, CLI). Divergent per-crate walkers made the same job run at
//! three speeds with three results.

use std::path::{Path, PathBuf};

/// Directories never traversed during `.spec` discovery: build artifacts,
/// dependency trees, and VCS internals.
pub const SKIP_DIRS: &[&str] = &["target", "node_modules", ".git", ".hg", "dist", "build"];

/// Discover `.spec` files under `root` with the shared policy: skip
/// [`SKIP_DIRS`], never follow symlinks, and drop paths matching any
/// `exclude` pattern. Patterns are matched as substrings against the
/// workspace-relative path (project `specforge.json` `exclude` entries,
/// C4-04). Returned paths are sorted for deterministic downstream behavior.
pub fn discover_spec_files(root: &Path, exclude: &[String]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false) // symlink cycles and out-of-tree traversal are never wanted here
        .into_iter()
        .filter_entry(|e| {
            if e.file_type().is_dir()
                && let Some(name) = e.file_name().to_str()
            {
                return !SKIP_DIRS.contains(&name);
            }
            true
        })
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "spec") {
            let relative = path.strip_prefix(root).unwrap_or(path);
            if is_excluded(&relative.to_string_lossy(), exclude) {
                continue;
            }
            files.push(path.to_path_buf());
        }
    }
    files.sort();
    files
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

    /// Pins today's defect; plan 12 T3 flips it to
    /// `a_walked_root_is_walked_whatever_its_name`.
    #[test]
    fn a_walked_root_named_like_a_skipped_directory_is_skipped_today() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("build")).unwrap();
        fs::write(root.join("build/a.spec"), "").unwrap();

        assert!(discover_spec_files(&root.join("build"), &[]).is_empty());
        // The `build` directory as a child is skipped, which stays right.
        assert!(discover_spec_files(root, &[]).is_empty());
    }

    /// Pins today's defect; plan 12 T3 flips it to `only_files_are_sources`.
    #[test]
    fn a_directory_named_like_a_spec_file_is_a_source_today() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::write(root.join("a.spec"), "").unwrap();
        fs::create_dir_all(root.join("archive.spec")).unwrap();

        assert_eq!(
            discover_spec_files(root, &[]),
            vec![root.join("a.spec"), root.join("archive.spec")]
        );
    }
}
