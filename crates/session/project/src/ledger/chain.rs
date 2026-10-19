//! A name read through its links (ADR 0129, ADR 0173): the hops a read follows at the name, where
//! each lands (what a stamp records), and the directory links on the way to it (what reaches a
//! recorded input). The file system is asked directly (std::fs), as the read itself is: the Disk
//! port has no link or metadata method (ADR 0114, ADR 0146).

use std::path::{Component, Path, PathBuf};

use super::key::in_canonical_dir;

/// How many links one name is followed through (as the OS does, `ELOOP`).
const MAX_LINK_HOPS: usize = 40;

/// Each link hop a read of `path` follows, in order: empty for a plain file or a missing path. A
/// link's target is joined onto the link's directory as spelled; the file system applies its `..`
/// (ADR 0129).
pub(super) fn hops(path: &Path) -> Vec<PathBuf> {
    let mut hops = Vec::new();
    let mut current = path.to_path_buf();
    for _ in 0..MAX_LINK_HOPS {
        let is_link = std::fs::symlink_metadata(&current)
            .map(|meta| meta.file_type().is_symlink())
            .unwrap_or(false);
        let Some(target) = is_link.then(|| std::fs::read_link(&current).ok()).flatten() else {
            break;
        };
        current = match current.parent() {
            Some(dir) => dir.join(target),
            None => target,
        };
        hops.push(current.clone());
    }
    hops
}

/// What `path` means now: the metadata of the file a read reaches, and where each hop of its chain
/// lands (its own name in its canonical directory, as a recorded input is keyed, so a directory link
/// on the way to a hop moves it). A plain file costs one `lstat`.
pub(super) fn followed(path: &Path) -> Option<(std::fs::Metadata, Vec<PathBuf>)> {
    let own = std::fs::symlink_metadata(path).ok()?;
    if !own.file_type().is_symlink() {
        return Some((own, Vec::new()));
    }
    let landings = hops(path).iter().map(|hop| in_canonical_dir(hop)).collect();
    Some((std::fs::metadata(path).ok()?, landings))
}

/// The directory links a read of `path` goes through before its own name, each by its own name in
/// its canonical directory, in order. Not one in `root`'s own spelling (`root` or a directory above
/// it: where the session was opened), not one directly under the file-system root (the machine's
/// own, as macOS `/tmp` and `/var`), and not `path`'s own name (a link there is a hop).
// shortcut: no per-directory memo, one lstat per component below the root; upgrade to a memo keyed
// by parent directory if a check-run renewal shows it in a profile.
pub(super) fn way(path: &Path, root: &Path) -> Vec<PathBuf> {
    let mut way = Vec::new();
    let mut at = PathBuf::new();
    let mut components = path.components().peekable();
    while let Some(component) = components.next() {
        at.push(component);
        if components.peek().is_none() {
            break;
        }
        let Component::Normal(_) = component else {
            continue;
        };
        let machines = at.parent().is_some_and(|dir| dir.parent().is_none());
        if root.starts_with(&at) || machines {
            continue;
        }
        if std::fs::symlink_metadata(&at).is_ok_and(|meta| meta.file_type().is_symlink()) {
            way.push(in_canonical_dir(&at));
        }
    }
    way
}
