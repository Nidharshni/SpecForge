//! The disk the checks read through in a project (ADR 0107): the file system,
//! and the one way a directory is listed.

use std::io;
use std::path::Path;

use specforge_registry::Disk;

use super::{CheckInput, CheckRun};

/// The checks' disk in a project session: each read goes through the check
/// run, which stamps the path first and records it as a check input.
pub(super) struct RunDisk<'r, 'l>(pub(super) &'r CheckRun<'l>);

impl Disk for RunDisk<'_, '_> {
    fn exists(&self, path: &Path) -> bool {
        self.record_link_targets(path);
        self.0.read(CheckInput::File, path, Path::exists)
    }

    fn list(&self, dir: &Path) -> Option<Vec<String>> {
        self.0.read(CheckInput::Listing, dir, list_names)
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.record_link_targets(path);
        self.0
            .read(CheckInput::File, path, |path| std::fs::read(path))
    }
}

/// How many links one path is followed through (as the OS does, `ELOOP`).
const MAX_LINK_HOPS: usize = 40;

impl RunDisk<'_, '_> {
    /// A read of `path` follows it when it is a symlink: the target is read
    /// too, so a change to it (it appears, goes, or changes) is a change to
    /// the check. Each hop is recorded, stamped first, and a dangling link
    /// still names its target (ADR 0107).
    fn record_link_targets(&self, path: &Path) {
        let mut current = path.to_path_buf();
        for _ in 0..MAX_LINK_HOPS {
            let is_link = std::fs::symlink_metadata(&current)
                .map(|meta| meta.file_type().is_symlink())
                .unwrap_or(false);
            let Some(target) = is_link.then(|| std::fs::read_link(&current).ok()).flatten() else {
                return;
            };
            current = match current.parent() {
                Some(dir) => dir.join(target),
                None => target,
            };
            self.0.read(CheckInput::File, &current, |_| ());
        }
    }
}

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
