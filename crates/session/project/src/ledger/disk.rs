//! The disk the checks read through in a project (ADR 0107): the file system,
//! and the one way a directory is listed.

use std::io;
use std::path::Path;

use specforge_registry::Disk;

/// The checks' disk of a one-shot compile: the file system, nothing recorded.
pub(crate) struct PlainDisk;

impl Disk for PlainDisk {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn list(&self, dir: &Path) -> Option<Vec<String>> {
        list_names(dir)
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }
}

/// The names of the entries directly in `dir`, sorted, those that are not
/// UTF-8 left out; `None` when `dir` is not a directory that can be read.
pub(super) fn list_names(dir: &Path) -> Option<Vec<String>> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();
    Some(names)
}
