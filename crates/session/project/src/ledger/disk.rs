//! The disk a project session reads through: the environment load, the sources
//! and the checks (ADR 0107, ADR 0114, ADR 0129): the file system through the
//! recording run.

use std::io;
use std::path::Path;
use std::sync::Arc;

use specforge_registry::Disk;

use super::{Reading, Run, chain};

/// The disk a surface hands a session to read its sources through (ADR 0145): MCP hands a
/// contained one, the CLI, watch and the LSP hand none and the session reads the plain file system.
pub type SourceDisk = Arc<dyn Disk + Send + Sync>;

/// The recording disk in a project session: each read goes through the recording
/// run, which stamps the path first and records it as an input, then reads with
/// the disk it wraps (the plain file system, or the one a surface handed for sources).
pub(super) struct RunDisk<'r, 'l>(pub(super) &'r Run<'l>, pub(super) &'r dyn Disk);

impl Disk for RunDisk<'_, '_> {
    fn exists(&self, path: &Path) -> bool {
        self.record_link_targets(path);
        self.0
            .read(Reading::File, path, self.1, |path| self.1.exists(path))
    }

    fn list(&self, dir: &Path) -> Option<Vec<String>> {
        self.0
            .read(Reading::Listing, dir, self.1, |dir| self.1.list(dir))
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.record_link_targets(path);
        self.0
            .read(Reading::File, path, self.1, |path| self.1.read(path))
    }
}

impl RunDisk<'_, '_> {
    /// A read of `path` follows it when it is a symlink: the target is read
    /// too, so a change to it (it appears, goes, or changes) is a change to
    /// what read it. Each hop is recorded, stamped first, and a dangling link
    /// still names its target (ADR 0107).
    fn record_link_targets(&self, path: &Path) {
        for hop in chain::hops(path) {
            self.0.read(Reading::File, &hop, self.1, |_| ());
        }
    }
}
