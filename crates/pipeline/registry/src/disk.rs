//! What the checks and the environment load see of the file system (ADR 0107,
//! ADR 0114): the one way a check, a rule, a check pass or the extension load
//! reaches it. They never call `std::fs`; they ask the [`Disk`] they are
//! given (the checks' in their [`crate::RuleInput`]).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

/// The file system as the checks and the environment load read it. Each call
/// is one read; an adapter may record it (a project session's ledger stamps
/// the path and records it as an input).
pub trait Disk {
    /// Whether `path` names an existing file or directory, following
    /// symlinks (`Path::exists`): `false` when it does not exist or cannot
    /// be examined.
    fn exists(&self, path: &Path) -> bool;
    /// The names of the entries directly in directory `dir`, sorted, those
    /// that are not UTF-8 left out; `None` when `dir` is not a directory that
    /// can be read.
    fn list(&self, dir: &Path) -> Option<Vec<String>>;
    /// The bytes of the file at `path`; an error of kind `NotFound` when it
    /// does not exist.
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
}

/// The file system, nothing recorded: what a one-shot compile and every
/// operation outside a session read through.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlainDisk;

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
fn list_names(dir: &Path) -> Option<Vec<String>> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();
    Some(names)
}

/// One read a [`MemoryDisk`] was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    Exists(PathBuf),
    List(PathBuf),
    Read(PathBuf),
}

/// A disk held in memory (tests): the files given, the directories they
/// imply, and a log of every read asked, in order.
#[derive(Debug, Default)]
pub struct MemoryDisk {
    files: BTreeMap<PathBuf, Vec<u8>>,
    asked: RefCell<Vec<Asked>>,
}

impl MemoryDisk {
    /// A disk with nothing on it.
    pub fn new() -> Self {
        Self::default()
    }

    /// With a file at `path` holding `bytes` (its parent directories exist
    /// with it).
    #[must_use]
    pub fn with_file(mut self, path: impl Into<PathBuf>, bytes: &[u8]) -> Self {
        self.files.insert(path.into(), bytes.to_vec());
        self
    }

    /// Every read asked so far, in order.
    pub fn asked(&self) -> Vec<Asked> {
        self.asked.borrow().clone()
    }

    fn ask(&self, asked: Asked) {
        self.asked.borrow_mut().push(asked);
    }

    /// The first component under `dir` of every file below it, when `dir` is
    /// a proper ancestor of some file.
    fn entries(&self, dir: &Path) -> Option<BTreeSet<String>> {
        let mut names = BTreeSet::new();
        let mut implied = false;
        for file in self.files.keys() {
            let Ok(rest) = file.strip_prefix(dir) else {
                continue;
            };
            if let Some(first) = rest.components().next() {
                implied = true;
                if let Some(name) = first.as_os_str().to_str() {
                    names.insert(name.to_string());
                }
            }
        }
        implied.then_some(names)
    }
}

impl Disk for MemoryDisk {
    fn exists(&self, path: &Path) -> bool {
        self.ask(Asked::Exists(path.to_path_buf()));
        self.files.contains_key(path) || self.entries(path).is_some()
    }

    fn list(&self, dir: &Path) -> Option<Vec<String>> {
        self.ask(Asked::List(dir.to_path_buf()));
        self.entries(dir).map(|names| names.into_iter().collect())
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.ask(Asked::Read(path.to_path_buf()));
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disk() -> MemoryDisk {
        MemoryDisk::new()
            .with_file("/spec/b.md", b"b")
            .with_file("/spec/a.md", b"a")
            .with_file("/spec/sub/deep/c.md", b"c")
    }

    #[test]
    fn a_memory_disk_lists_the_entries_directly_in_a_directory() {
        let disk = disk();
        assert_eq!(
            disk.list(Path::new("/spec")),
            Some(vec!["a.md".into(), "b.md".into(), "sub".into()])
        );
        assert_eq!(disk.list(Path::new("/spec/a.md")), None);
        assert_eq!(disk.list(Path::new("/nowhere")), None);
    }

    #[test]
    fn a_memory_disk_holds_the_directories_its_files_imply() {
        let disk = disk();
        assert!(disk.exists(Path::new("/spec/sub/deep")));
        assert!(disk.exists(Path::new("/spec/a.md")));
        assert!(!disk.exists(Path::new("/spec/sub/other")));
        assert!(!disk.exists(Path::new("/spec/a")));
    }

    #[test]
    fn a_memory_disk_logs_every_read_in_order() {
        let disk = disk();
        disk.exists(Path::new("/x"));
        disk.list(Path::new("/spec"));
        let _ = disk.read(Path::new("/spec/a.md"));
        assert_eq!(
            disk.asked(),
            [
                Asked::Exists("/x".into()),
                Asked::List("/spec".into()),
                Asked::Read("/spec/a.md".into()),
            ]
        );
    }

    #[test]
    fn a_missing_file_reads_as_not_found() {
        let disk = disk();
        assert_eq!(disk.read(Path::new("/spec/a.md")).unwrap(), b"a".to_vec());
        let error = disk.read(Path::new("/spec/none.md")).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }
}
