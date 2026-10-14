//! The repository's own `.spec` files, parsed for the tests that hold code
//! to what the spec declares (events, types).

use std::path::PathBuf;

use specforge_parser::SpecFile;

/// Every `spec/<dir>/*.spec` (`dir` is `"events"`, `"types"`, …), sorted by
/// path, parsed. Panics when the directory or a file cannot be read.
pub fn spec_files(dir: &str) -> Vec<(PathBuf, SpecFile)> {
    let dir = specforge_repo_root::repo_root().join("spec").join(dir);
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "spec"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let file = specforge_parser::parse(&source, &path.display().to_string());
            (path, file)
        })
        .collect()
}
