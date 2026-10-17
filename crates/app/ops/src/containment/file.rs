//! Opening a held file (ADR 0112). A [`Held`] is a name a containment resolved: what is opened is
//! `real`, where the name landed, so the file checked is the file opened; what messages, documents
//! and `Writes` name is `spelled`. Every project file an operation reads or writes is opened here.

use std::io::Write as _;
use std::path::{Component, Path, PathBuf};

use super::{Containment, Held, Outside};

impl Held {
    /// The bytes at `real`. A missing file is the caller's `NotFound` (E054, E045, `None`...).
    pub fn read(&self) -> std::io::Result<Vec<u8>> {
        std::fs::read(&self.real)
    }

    /// The bytes at `real`, or `None` for a file that is not there (`NotFound`); any other error passes.
    pub fn read_if_there(&self) -> std::io::Result<Option<Vec<u8>>> {
        match std::fs::read(&self.real) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The text at `real`.
    pub fn read_to_string(&self) -> std::io::Result<String> {
        std::fs::read_to_string(&self.real)
    }

    /// Create `real`, refusing one that exists (`AlreadyExists`); never through a link at the name.
    pub fn create_new(&self, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.real)?;
        file.write_all(contents.as_ref())
    }

    /// A new file at `real`, to stream into (collect's captured output): the name is unlinked first,
    /// so a hard link at it is never written through (ADR 0144).
    pub fn create_fresh(&self) -> std::io::Result<std::fs::File> {
        specforge_common::fresh::create_fresh(&self.real)
    }

    /// Replace `real` atomically, by its name: [`specforge_common::fresh::replace_file`], so the inode
    /// that held `real` is never opened for writing (a hard link keeps its other name, ADR 0144). A file
    /// that is there keeps its permissions, and one the process may not write is refused
    /// (`PermissionDenied`): its owner's read-only marker is not lifted by a rewrite.
    pub fn replace(&self, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
        specforge_common::fresh::replace_file(&self.real, contents.as_ref())
    }

    /// Unlink the name (`spelled`): a link is removed, never its target. `NotFound` passes through.
    pub fn remove(&self) -> std::io::Result<()> {
        std::fs::remove_file(&self.spelled)
    }

    /// Create `real`'s missing parent directories (they land inside: `real` did).
    pub fn create_parent(&self) -> std::io::Result<()> {
        match self.real.parent() {
            Some(parent) => std::fs::create_dir_all(parent),
            None => Ok(()),
        }
    }

    /// Create `real`'s missing parent directories one by one, shallowest first, pushing each onto
    /// `created` as it is made: a failure part-way still tells the caller what to remove.
    pub(crate) fn create_parents(&self, created: &mut Vec<PathBuf>) -> std::io::Result<()> {
        let mut missing: Vec<&Path> = Vec::new();
        let mut at = self.real.parent();
        while let Some(dir) = at {
            if dir.as_os_str().is_empty() || dir.exists() {
                break;
            }
            missing.push(dir);
            at = dir.parent();
        }
        for dir in missing.into_iter().rev() {
            std::fs::create_dir(dir)?;
            created.push(dir.to_path_buf());
        }
        Ok(())
    }

    /// Unlink `real` itself: a file a change created there. [`Self::remove`] unlinks `spelled`,
    /// which for a file created through a link would delete the link and keep the file.
    pub(crate) fn remove_created(&self) -> std::io::Result<()> {
        std::fs::remove_file(&self.real)
    }

    /// Remove each of `dirs` that is empty, in the order given; a directory holding anything stays.
    /// Never recursive.
    pub(crate) fn remove_empty(dirs: &[PathBuf]) {
        for dir in dirs {
            let _ = std::fs::remove_dir(dir);
        }
    }
}

impl Containment {
    /// Resolve `named` and read it, in one call: the `Held` with its bytes, or why not. For a
    /// reader that names what it read (inference sources, analyze's named report).
    pub fn read(&self, named: &Path) -> Result<(Held, Vec<u8>), ReadFailure> {
        let held = self.resolve(named).map_err(ReadFailure::Outside)?;
        match held.read() {
            Ok(bytes) => Ok((held, bytes)),
            Err(error) => Err(ReadFailure::Io { held, error }),
        }
    }
}

/// Why [`Containment::read`] read nothing.
#[derive(Debug)]
pub enum ReadFailure {
    /// The name lands outside the containment; nothing was opened.
    Outside(Outside),
    /// It was held, and the open or the read failed (`NotFound` for a missing file).
    Io { held: Held, error: std::io::Error },
}

/// A name a declaration may give (a collector's report location), judged as text: relative, no
/// root, no prefix, no `..` (ADR 0112 D5). `None` when it is not one. Where it *lands* is still
/// the request's containment: `resolve(&root.join(name))` after this.
pub fn declared(name: &Path) -> Option<&Path> {
    name.components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
        .then_some(name)
}
