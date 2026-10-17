//! A containment as the `Disk` port (ADR 0139): what a crate below app reads a project file
//! through when the surface bounds it. Every read is held first, so the file opened is the
//! file checked (ADR 0104, ADR 0112 D1).

use std::io;
use std::path::Path;

use specforge_registry::{Disk, PlainDisk};

use super::{Containment, ReadFailure};

/// The disk inside one containment: a name that lands outside reads as an error of kind
/// `InvalidInput` (the only way the port's `io::Result` can carry `Outside`), exists not and
/// lists nothing; anything inside is the file system's answer.
pub(crate) struct ContainedDisk(pub(crate) Containment);

impl Disk for ContainedDisk {
    fn exists(&self, path: &Path) -> bool {
        self.0
            .resolve(path)
            .is_ok_and(|held| PlainDisk.exists(&held.real))
    }

    fn list(&self, dir: &Path) -> Option<Vec<String>> {
        PlainDisk.list(&self.0.resolve(dir).ok()?.real)
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        match self.0.read(path) {
            Ok((_, bytes)) => Ok(bytes),
            Err(ReadFailure::Io { error, .. }) => Err(error),
            Err(ReadFailure::Outside(outside)) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "it resolves to {}, outside {}, which bounds the request",
                    outside.landed().display(),
                    outside.root().display()
                ),
            )),
        }
    }
}
