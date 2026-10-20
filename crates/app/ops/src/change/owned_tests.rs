//! A name the operation owns (ADR 0171): written new, never read, unlinked when the change is put back.

#![cfg(unix)]

use super::*;
use crate::Containment;
use std::os::unix::fs::PermissionsExt as _;

fn scene() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    (dir, root)
}

fn held(root: &Path, name: &str) -> Held {
    Containment::within(root).resolve(Path::new(name)).unwrap()
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn lock(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

fn permissions(mode: u32) -> std::fs::Permissions {
    std::fs::Permissions::from_mode(mode)
}

#[test]
fn an_owned_name_is_written_new_and_never_read() {
    let (_dir, root) = scene();
    std::fs::write(root.join("owned"), "stale").unwrap();
    lock(&root.join("owned"), 0o000);
    let mut change = ProjectChange::new(&root, "cannot test");
    change.stage_owned(held(&root, "owned"), b"fresh".to_vec(), permissions(0o644));

    change.commit().unwrap();

    assert_eq!(
        std::fs::read_to_string(root.join("owned")).unwrap(),
        "fresh"
    );
}

#[test]
fn an_owned_name_replaces_a_link_and_leaves_its_target() {
    let (_dir, root) = scene();
    std::fs::write(root.join("target"), "TARGET").unwrap();
    std::os::unix::fs::symlink(root.join("target"), root.join("soft")).unwrap();
    std::fs::hard_link(root.join("target"), root.join("hard")).unwrap();
    let mut change = ProjectChange::new(&root, "cannot test");
    for name in ["soft", "hard"] {
        // A symlink's `real` is its target: only `spelled` is the owned name.
        let held = Held {
            spelled: root.join(name),
            real: root.join(name),
        };
        change.stage_owned(held, b"new".to_vec(), permissions(0o644));
    }

    change.commit().unwrap();

    assert_eq!(
        std::fs::read_to_string(root.join("target")).unwrap(),
        "TARGET"
    );
    for name in ["soft", "hard"] {
        let meta = std::fs::symlink_metadata(root.join(name)).unwrap();
        assert!(meta.is_file(), "{name} is a regular file now");
        assert_eq!(std::fs::read_to_string(root.join(name)).unwrap(), "new");
    }
}

#[test]
fn an_owned_name_is_removed_when_the_change_is_put_back() {
    let (_dir, root) = scene();
    if std::fs::OpenOptions::new()
        .write(true)
        .open({
            std::fs::write(root.join("probe"), "x").unwrap();
            lock(&root.join("probe"), 0o444);
            root.join("probe")
        })
        .is_ok()
    {
        return;
    }
    std::fs::write(root.join("a.txt"), "old").unwrap();
    std::fs::write(root.join("b.txt"), "old").unwrap();
    lock(&root.join("b.txt"), 0o444);
    let mut change = ProjectChange::new(&root, "cannot test");
    change.stage_owned(
        held(&root, "a.txt.bak"),
        b"old".to_vec(),
        permissions(0o644),
    );
    change.stage(held(&root, "a.txt"), Some(b"old".to_vec()), b"new".to_vec());
    change.stage(held(&root, "b.txt"), Some(b"old".to_vec()), b"new".to_vec());

    let error = change.commit().unwrap_err();

    assert_eq!(error.code, FILE_UNWRITABLE);
    assert!(error.writes.is_empty());
    assert!(!root.join("a.txt.bak").exists());
    assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "old");
}

#[test]
fn an_owned_name_gets_the_permissions_it_is_staged_with() {
    let (_dir, root) = scene();
    let mut change = ProjectChange::new(&root, "cannot test");
    change.stage_owned(held(&root, "owned"), b"secret".to_vec(), permissions(0o600));

    change.commit().unwrap();

    assert_eq!(mode(&root.join("owned")), 0o600);
}
