//! What a call left on disk: snapshots of a directory and their difference.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Every file under a directory with its bytes.
pub type Snapshot = BTreeMap<PathBuf, Vec<u8>>;

/// Every file under `root` with its bytes; empty when `root` does not
/// exist (a directory `init` is about to create).
pub fn files_under(root: &Path) -> Snapshot {
    let mut files = Snapshot::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                let bytes =
                    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
                files.insert(path, bytes);
            }
        }
    }
    files
}

/// The files whose content differs between two [`files_under`] snapshots,
/// added and removed ones included, relative to `root`, sorted.
pub fn changed_files(root: &Path, before: &Snapshot, after: &Snapshot) -> Vec<PathBuf> {
    let mut changed: Vec<PathBuf> = before
        .keys()
        .chain(after.keys())
        .filter(|path| before.get(*path) != after.get(*path))
        .map(|path| {
            path.strip_prefix(root)
                .unwrap_or_else(|_| panic!("{} is not under {}", path.display(), root.display()))
                .to_path_buf()
        })
        .collect();
    changed.sort();
    changed.dedup();
    changed
}
