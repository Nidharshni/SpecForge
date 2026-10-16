//! The disk the checks read through in a project (ADR 0107): the file system
//! through the recording run.

use std::io;
use std::path::Path;

use specforge_registry::{Disk, PlainDisk};

use super::{Reading, Run};

/// The checks' disk in a project session: each read goes through the check
/// run, which stamps the path first and records it as a check input.
pub(super) struct RunDisk<'r, 'l>(pub(super) &'r Run<'l>);

impl Disk for RunDisk<'_, '_> {
    fn exists(&self, path: &Path) -> bool {
        self.record_link_targets(path);
        self.0
            .read(Reading::File, path, |path| PlainDisk.exists(path))
    }

    fn list(&self, dir: &Path) -> Option<Vec<String>> {
        self.0
            .read(Reading::Listing, dir, |dir| PlainDisk.list(dir))
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.record_link_targets(path);
        self.0
            .read(Reading::File, path, |path| PlainDisk.read(path))
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
            self.0.read(Reading::File, &current, |_| ());
        }
    }
}
