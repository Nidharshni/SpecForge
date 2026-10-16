use super::*;
use crate::{Containment, OpErrorKind};
use std::path::Path;

fn held(dir: &Path, name: &str) -> Held {
    Containment::within(dir).resolve(Path::new(name)).unwrap()
}

fn bytes(text: &str) -> Vec<u8> {
    text.as_bytes().to_vec()
}

fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).unwrap()
}

/// A project directory, canonical (the temp directory may sit behind a link).
fn scene() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    (dir, root)
}

/// Whether a 0444 file or a 0555 directory binds this user (not root).
#[cfg(unix)]
fn permissions_bind(root: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let probe = root.join("probe");
    std::fs::write(&probe, "x").unwrap();
    std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o444)).unwrap();
    let binds = std::fs::OpenOptions::new()
        .write(true)
        .open(&probe)
        .is_err();
    std::fs::remove_file(&probe).unwrap();
    binds
}

#[cfg(unix)]
fn lock(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

fn no_temporaries(root: &Path) {
    fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "tmp") {
                found.push(path.clone());
            }
            if path.is_dir() {
                walk(&path, found);
            }
        }
    }
    let mut found = Vec::new();
    walk(root, &mut found);
    assert!(found.is_empty(), "temporary files left: {found:?}");
}

#[test]
fn a_change_writes_each_staged_file_and_names_it() {
    let (_dir, root) = scene();
    std::fs::write(root.join("a.txt"), "old").unwrap();
    std::fs::write(root.join("same.txt"), "same").unwrap();
    let old = std::fs::File::options()
        .write(true)
        .open(root.join("same.txt"))
        .unwrap();
    let stamp = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
    old.set_modified(stamp).unwrap();
    drop(old);

    let mut change = ProjectChange::new();
    change.stage(held(&root, "a.txt"), Some(bytes("old")), bytes("new"));
    change.stage(held(&root, "deep/er/b.txt"), None, bytes("fresh"));
    change.stage(held(&root, "same.txt"), Some(bytes("same")), bytes("same"));
    let writes = change.commit().unwrap();

    assert_eq!(
        writes.paths().collect::<Vec<_>>(),
        [root.join("a.txt"), root.join("deep/er/b.txt")]
    );
    assert_eq!(read(&root, "a.txt"), "new");
    assert_eq!(read(&root, "deep/er/b.txt"), "fresh");
    let modified = std::fs::metadata(root.join("same.txt"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(modified, stamp, "an unchanged file is not touched");
    no_temporaries(&root);
}

#[test]
fn a_file_that_changed_since_it_was_staged_refuses_the_change_writing_nothing() {
    let (_dir, root) = scene();
    std::fs::write(root.join("first.txt"), "first").unwrap();
    for case in ["edited", "appeared", "gone"] {
        let mut change = ProjectChange::new();
        change.stage(
            held(&root, "first.txt"),
            Some(bytes("first")),
            bytes("FIRST"),
        );
        match case {
            "edited" => {
                std::fs::write(root.join("second.txt"), "second").unwrap();
                change.stage(held(&root, "second.txt"), Some(bytes("second")), bytes("2"));
                std::fs::write(root.join("second.txt"), "edited since").unwrap();
            }
            "appeared" => {
                change.stage(held(&root, "third.txt"), None, bytes("3"));
                std::fs::write(root.join("third.txt"), "appeared since").unwrap();
            }
            _ => {
                std::fs::write(root.join("fourth.txt"), "fourth").unwrap();
                change.stage(held(&root, "fourth.txt"), Some(bytes("fourth")), bytes("4"));
                std::fs::remove_file(root.join("fourth.txt")).unwrap();
            }
        }

        let failed = change.commit().unwrap_err();

        assert!(
            matches!(&failed.cause, Cause::Changed { file } if file.starts_with(&root)),
            "{case}: {failed:?}"
        );
        assert!(failed.left.is_empty(), "{case}");
        assert_eq!(read(&root, "first.txt"), "first", "{case}: nothing written");
    }
    assert_eq!(read(&root, "second.txt"), "edited since");
    assert_eq!(read(&root, "third.txt"), "appeared since");
    assert!(!root.join("fourth.txt").exists());
    no_temporaries(&root);
}

#[cfg(unix)]
#[test]
fn a_change_that_fails_at_file_k_puts_back_every_file_before_it() {
    let (_dir, root) = scene();
    if !permissions_bind(&root) {
        return; // permissions don't bind this user (root)
    }
    for k in 0..3 {
        let names = ["f0.txt", "f1.txt", "f2.txt"];
        let mut change = ProjectChange::new();
        for name in names {
            std::fs::write(root.join(name), format!("old {name}")).unwrap();
        }
        lock(&root.join(names[k]), 0o444);
        for name in names {
            change.stage(
                held(&root, name),
                Some(bytes(&format!("old {name}"))),
                bytes(&format!("new {name}")),
            );
        }

        let failed = change.commit().unwrap_err();

        assert!(
            matches!(&failed.cause, Cause::Unwritable { file, .. } if *file == root.join(names[k])),
            "k = {k}: {failed:?}"
        );
        assert!(failed.left.is_empty(), "k = {k}");
        for name in names {
            assert_eq!(read(&root, name), format!("old {name}"), "k = {k}");
        }
        no_temporaries(&root);
        lock(&root.join(names[k]), 0o644);
    }
}

#[cfg(unix)]
#[test]
fn a_change_that_cannot_create_file_k_removes_the_files_it_created() {
    let (_dir, root) = scene();
    if !permissions_bind(&root) {
        return;
    }
    std::fs::write(root.join("kept.txt"), "old").unwrap();
    std::fs::create_dir(root.join("locked")).unwrap();
    lock(&root.join("locked"), 0o555);
    std::fs::create_dir(root.join("existing")).unwrap();

    let mut change = ProjectChange::new();
    change.stage(held(&root, "made/deeper/new0.txt"), None, bytes("0"));
    change.stage(held(&root, "existing/new1.txt"), None, bytes("1"));
    change.stage(held(&root, "kept.txt"), Some(bytes("old")), bytes("new"));
    change.stage(held(&root, "locked/new3.txt"), None, bytes("3"));
    let failed = change.commit().unwrap_err();

    assert!(
        matches!(&failed.cause, Cause::Unwritable { file, .. } if *file == root.join("locked/new3.txt")),
        "{failed:?}"
    );
    assert!(failed.left.is_empty());
    assert!(
        !root.join("made").exists(),
        "the directories it made are gone"
    );
    assert!(!root.join("existing/new1.txt").exists());
    assert!(
        root.join("existing").is_dir(),
        "a directory it did not make stays"
    );
    assert_eq!(read(&root, "kept.txt"), "old");
    no_temporaries(&root);
    lock(&root.join("locked"), 0o755);
}

#[test]
fn a_last_step_that_fails_puts_back_every_file() {
    let (_dir, root) = scene();
    std::fs::write(root.join("a.txt"), "old").unwrap();
    std::fs::create_dir(root.join("existing")).unwrap();
    let mut change = ProjectChange::new();
    change.stage(held(&root, "a.txt"), Some(bytes("old")), bytes("new"));
    change.stage(held(&root, "made/b.txt"), None, bytes("b"));
    change.stage(held(&root, "existing/c.txt"), None, bytes("c"));
    let seen = root.clone();

    let failed = change
        .commit_then(|| {
            assert_eq!(
                read(&seen, "a.txt"),
                "new",
                "the last step runs after the writes"
            );
            Err("pin refused")
        })
        .unwrap_err();

    assert!(
        matches!(failed.cause, Cause::Then("pin refused")),
        "{failed:?}"
    );
    assert!(failed.left.is_empty());
    assert_eq!(read(&root, "a.txt"), "old");
    assert!(!root.join("made").exists());
    assert!(!root.join("existing/c.txt").exists());
    assert!(root.join("existing").is_dir());
    no_temporaries(&root);
}

#[cfg(unix)]
#[test]
fn a_file_the_change_cannot_put_back_is_left_and_named() {
    let (_dir, root) = scene();
    if !permissions_bind(&root) {
        return;
    }
    for name in ["f0.txt", "f1.txt"] {
        std::fs::write(root.join(name), format!("old {name}")).unwrap();
    }
    let mut change = ProjectChange::new();
    for name in ["f0.txt", "f1.txt"] {
        change.stage(
            held(&root, name),
            Some(bytes(&format!("old {name}"))),
            bytes(&format!("new {name}")),
        );
    }
    let path = root.join("f0.txt");

    let failed = change
        .commit_then(|| {
            lock(&path, 0o444);
            Err(())
        })
        .unwrap_err();

    assert_eq!(
        failed.left.paths().collect::<Vec<_>>(),
        [root.join("f0.txt")]
    );
    assert_eq!(
        read(&root, "f0.txt"),
        "new f0.txt",
        "left whole, as the change wrote it"
    );
    assert_eq!(read(&root, "f1.txt"), "old f1.txt");
    no_temporaries(&root);
    lock(&path, 0o644);
}

#[test]
fn a_failed_change_reports_what_it_left_as_the_errors_writes() {
    let failed: Failed<()> = Failed {
        cause: Cause::Then(()),
        left: Writes::of([PathBuf::from("/p/a.spec")]),
    };

    let error = failed.error(|cause| {
        assert!(matches!(cause, Cause::Then(())));
        OpError::new(
            OpErrorKind::Internal,
            "last_step_failed",
            "the last step failed",
        )
    });

    assert_eq!(
        error.writes.paths().collect::<Vec<_>>(),
        [Path::new("/p/a.spec")]
    );
}

#[cfg(unix)]
#[test]
fn a_file_staged_through_an_inside_link_keeps_the_link() {
    let (_dir, root) = scene();
    std::fs::write(root.join("real.txt"), "old").unwrap();
    std::os::unix::fs::symlink(root.join("real.txt"), root.join("link.txt")).unwrap();
    let mut change = ProjectChange::new();
    change.stage(held(&root, "link.txt"), Some(bytes("old")), bytes("new"));

    let writes = change.commit().unwrap();

    assert_eq!(writes.paths().collect::<Vec<_>>(), [root.join("link.txt")]);
    assert!(
        std::fs::symlink_metadata(root.join("link.txt"))
            .unwrap()
            .is_symlink()
    );
    assert_eq!(read(&root, "real.txt"), "new");

    // A file created through a dangling link: a failure removes what was created, not the link.
    std::os::unix::fs::symlink(root.join("target.txt"), root.join("dangling.txt")).unwrap();
    let mut change = ProjectChange::new();
    change.stage(held(&root, "dangling.txt"), None, bytes("created"));
    let failed = change.commit_then(|| Err(())).unwrap_err();
    assert!(failed.left.is_empty());
    assert!(
        !root.join("target.txt").exists(),
        "the created target is removed"
    );
    assert!(
        std::fs::symlink_metadata(root.join("dangling.txt"))
            .unwrap()
            .is_symlink(),
        "the link stays"
    );
}
