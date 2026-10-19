//! Writing a project file by replacing its name (ADR 0144). A path rule cannot see a hard link, so the
//! rule is on the write: the inode that held a name is never opened for writing. A file is replaced by
//! creating a new one beside it and renaming it over the name; a stream that cannot be staged is
//! created new after its name is unlinked. A link at the name or at the temporary name is removed,
//! never followed.

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Result, Write as _};
use std::path::{Path, PathBuf};

/// Replace `path` atomically: write `<path>.tmp` (the file name with `.tmp` appended, beside `path`),
/// synced, then rename it onto `path`. Whatever is at the temp name first (a file left by a crash, or a
/// link) is removed, a link and never its target, and the temp is created new (`O_EXCL`), so it is never
/// written through a link. On failure the temp is removed and `path` is as it was.
///
/// A file that is there keeps its permissions, and one the process may not write is refused with the
/// error of opening it for writing (`PermissionDenied`), as an in-place write would have: its owner's
/// read-only marker is not lifted by a rewrite. The temp gets them before a byte is written. Ownership and extended attributes are not carried over.
pub fn replace_file(path: &Path, contents: &[u8]) -> Result<()> {
    let permissions = match OpenOptions::new().write(true).open(path) {
        Ok(existing) => Some(existing.metadata()?.permissions()),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let temp = temporary(path);
    unlink(&temp)?;
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .and_then(|mut file| {
            if let Some(permissions) = permissions {
                file.set_permissions(permissions)?;
            }
            file.write_all(contents)?;
            file.sync_all()
        })
        .and_then(|()| std::fs::rename(&temp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

/// A new file at `path`, to stream into: the name is unlinked first (a link, never its target; a
/// missing name passes), then the file is created new, so the old inode is never opened. A name
/// recreated between the two steps fails with `AlreadyExists` instead of escaping.
pub fn create_fresh(path: &Path) -> Result<File> {
    unlink(path)?;
    OpenOptions::new().write(true).create_new(true).open(path)
}

fn unlink(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// `<path>.tmp`: the file name with `.tmp` appended.
fn temporary(path: &Path) -> PathBuf {
    let mut name: OsString = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn replace_file_leaves_a_hard_links_other_name_alone() {
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("other");
        std::fs::write(&other, "ORIGINAL").unwrap();
        std::fs::hard_link(&other, dir.path().join("x")).unwrap();
        replace_file(&dir.path().join("x"), b"new").unwrap();
        assert_eq!(std::fs::read_to_string(&other).unwrap(), "ORIGINAL");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("x")).unwrap(),
            "new"
        );
    }

    #[cfg(unix)]
    #[test]
    fn replace_file_unlinks_a_temp_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("other");
        std::fs::write(&other, "ORIGINAL").unwrap();
        std::os::unix::fs::symlink(&other, dir.path().join("x.tmp")).unwrap();
        replace_file(&dir.path().join("x"), b"new").unwrap();
        assert_eq!(std::fs::read_to_string(&other).unwrap(), "ORIGINAL");
        assert!(!dir.path().join("x.tmp").exists());
    }

    #[cfg(unix)]
    #[test]
    fn replace_file_refuses_a_read_only_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x");
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
        // A process that may write anyway (root) cannot show the refusal.
        if OpenOptions::new().write(true).open(&path).is_ok() {
            return;
        }
        let error = replace_file(&path, b"new").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::PermissionDenied);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old");
    }

    #[cfg(unix)]
    #[test]
    fn create_fresh_never_opens_the_old_inode() {
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("other");
        std::fs::write(&other, "ORIGINAL").unwrap();
        std::fs::hard_link(&other, dir.path().join("x")).unwrap();
        let mut file = create_fresh(&dir.path().join("x")).unwrap();
        file.write_all(b"new").unwrap();
        assert_eq!(std::fs::read_to_string(&other).unwrap(), "ORIGINAL");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("x")).unwrap(),
            "new"
        );
    }
}
