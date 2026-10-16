use super::*;

fn scene() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(root.join("spec")).unwrap();
    std::fs::create_dir_all(dir.path().join("outside/deep")).unwrap();
    // The temp directory itself may sit behind a symlink (macOS `/var`): work from its real path.
    let real = std::fs::canonicalize(dir.path()).unwrap();
    (dir, real)
}

#[test]
fn a_name_is_resolved_against_the_root_and_cannot_leave_it() {
    let (_dir, base) = scene();
    let root = base.join("proj");
    let containment = Containment::within(&root);

    for inside in ["spec", "./spec/../spec", "not/yet/made", "."] {
        assert!(containment.resolve(Path::new(inside)).is_ok(), "{inside}");
    }
    let outside = base.join("outside");
    for leaving in [
        "..",
        "../outside",
        "spec/../../outside",
        "not/yet/../../../outside",
        outside.to_str().unwrap(),
    ] {
        assert!(
            containment.resolve(Path::new(leaving)).is_err(),
            "{leaving}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_symlink_that_leaves_the_root_is_followed_before_the_check() {
    let (_dir, base) = scene();
    let root = base.join("proj");
    std::os::unix::fs::symlink(base.join("outside"), root.join("link")).unwrap();
    std::os::unix::fs::symlink(&root, base.join("outside/back")).unwrap();
    let containment = Containment::within(&root);

    assert!(containment.resolve(Path::new("link")).is_err());
    assert!(containment.resolve(Path::new("link/deep")).is_err());
    // `..` after a symlink is the symlink target's parent, not the root.
    assert!(containment.resolve(Path::new("link/deep/..")).is_err());
    // A symlink that comes back inside is inside.
    assert!(containment.resolve(Path::new("link/back")).is_ok());
}

#[cfg(unix)]
#[test]
fn an_unbounded_containment_refuses_nothing_and_still_lands_each_name() {
    let (_dir, base) = scene();
    std::os::unix::fs::symlink(base.join("outside/deep"), base.join("proj/a")).unwrap();
    let containment = Containment::unbounded();

    // Relative and absolute names are handed back as given.
    let held = containment.resolve(Path::new("x/y")).unwrap();
    assert_eq!(held.spelled, Path::new("x/y"));
    let leaving = base.join("proj/spec/../../outside");
    let held = containment.resolve(&leaving).unwrap();
    assert_eq!(
        (held.spelled.as_path(), held.real),
        (leaving.as_path(), base.join("outside"))
    );
    // A symlink then `..` lands where the OS would: beside the link's target, not the link.
    let held = containment.resolve(&base.join("proj/a/../x.wasm")).unwrap();
    assert_eq!(held.real, base.join("outside/x.wasm"));
}

#[cfg(unix)]
#[test]
fn a_missing_name_behind_a_symlink_that_leaves_is_outside() {
    let (_dir, base) = scene();
    let root = base.join("proj");
    std::os::unix::fs::symlink(base.join("outside"), root.join("src")).unwrap();
    let outside = Containment::within(&root)
        .resolve(Path::new("src/missing.rs"))
        .unwrap_err();
    assert_eq!(outside.landed(), base.join("outside/missing.rs"));
    assert_eq!(outside.root(), root);
}

#[cfg(unix)]
#[test]
fn a_dangling_symlink_lands_where_it_points() {
    let (_dir, base) = scene();
    let root = base.join("proj");
    std::os::unix::fs::symlink(base.join("outside/new.json"), root.join("link")).unwrap();
    let containment = Containment::within(&root);

    let outside = containment.resolve(Path::new("link")).unwrap_err();
    assert_eq!(outside.landed(), base.join("outside/new.json"));
    // One that points inside is inside, by its target text read against its own directory.
    std::os::unix::fs::symlink("spec/new.json", root.join("inner")).unwrap();
    let held = containment.resolve(Path::new("inner")).unwrap();
    assert_eq!(held.real, root.join("spec/new.json"));
}

#[cfg(unix)]
#[test]
fn a_symlink_loop_lands_at_itself() {
    let (_dir, base) = scene();
    let root = base.join("proj");
    std::os::unix::fs::symlink(root.join("b"), root.join("a")).unwrap();
    std::os::unix::fs::symlink(root.join("a"), root.join("b")).unwrap();
    let held = Containment::within(&root).resolve(Path::new("a")).unwrap();
    assert!(held.real.starts_with(&root));
}

#[test]
fn narrowed_bounds_a_bounded_containment_to_the_directory_and_leaves_an_unbounded_one() {
    let (_dir, base) = scene();
    let narrowed = Containment::within(&base).narrowed(&base.join("proj"));
    assert_eq!(narrowed.root(), Some(base.join("proj").as_path()));
    assert!(Containment::unbounded().narrowed(&base).root().is_none());
}

fn held_in(dir: &Path, name: &str) -> Held {
    Containment::within(dir).resolve(Path::new(name)).unwrap()
}

#[test]
fn replace_writes_the_file_and_leaves_no_temporary_file() {
    let (_dir, base) = scene();
    let root = base.join("proj");
    let held = held_in(&root, "x.json");
    held.replace("one").unwrap();
    held.replace("two").unwrap();
    assert_eq!(std::fs::read_to_string(root.join("x.json")).unwrap(), "two");
    assert!(!root.join("x.json.tmp").exists());
}

#[cfg(unix)]
#[test]
fn replace_removes_a_link_at_its_temporary_name_and_writes_nothing_through_it() {
    let (_dir, base) = scene();
    let root = base.join("proj");
    let victim = base.join("outside/victim");
    std::fs::write(&victim, "ORIGINAL").unwrap();
    std::os::unix::fs::symlink(&victim, root.join("x.json.tmp")).unwrap();
    held_in(&root, "x.json").replace("new").unwrap();
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "ORIGINAL");
    let meta = std::fs::symlink_metadata(root.join("x.json")).unwrap();
    assert!(meta.is_file());
    assert_eq!(std::fs::read_to_string(root.join("x.json")).unwrap(), "new");
    assert!(std::fs::symlink_metadata(root.join("x.json.tmp")).is_err());
}

#[cfg(unix)]
#[test]
fn replace_rewrites_the_target_of_a_link_that_stays_inside_and_keeps_the_link() {
    let (_dir, base) = scene();
    let root = base.join("proj");
    std::fs::write(root.join("real.json"), "old").unwrap();
    std::os::unix::fs::symlink(root.join("real.json"), root.join("x.json")).unwrap();
    held_in(&root, "x.json").replace("new").unwrap();
    assert!(
        std::fs::symlink_metadata(root.join("x.json"))
            .unwrap()
            .is_symlink()
    );
    assert_eq!(
        std::fs::read_to_string(root.join("real.json")).unwrap(),
        "new"
    );
}

#[test]
fn a_failed_replace_leaves_the_file_and_no_temporary_file() {
    let (_dir, base) = scene();
    let root = base.join("proj");
    std::fs::create_dir(root.join("x.json")).unwrap();
    assert!(held_in(&root, "x.json").replace("new").is_err());
    assert!(root.join("x.json").is_dir());
    assert!(!root.join("x.json.tmp").exists());
}

#[cfg(unix)]
#[test]
fn create_new_refuses_an_existing_file_and_a_link() {
    let (_dir, base) = scene();
    let root = base.join("proj");
    std::fs::write(root.join("a"), "a").unwrap();
    let error = held_in(&root, "a").create_new("b").unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    // A link that exists is followed by the resolve, so the open meets its target.
    std::os::unix::fs::symlink(root.join("a"), root.join("l")).unwrap();
    let error = held_in(&root, "l").create_new("b").unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read_to_string(root.join("a")).unwrap(), "a");
}

#[cfg(unix)]
#[test]
fn remove_unlinks_a_link_and_not_its_target() {
    let (_dir, base) = scene();
    let root = base.join("proj");
    std::fs::write(root.join("a"), "a").unwrap();
    std::os::unix::fs::symlink(root.join("a"), root.join("l")).unwrap();
    // `spelled` is the link; the unlink never follows the last component.
    let held = Held {
        spelled: root.join("l"),
        real: root.join("a"),
    };
    held.remove().unwrap();
    assert!(std::fs::symlink_metadata(root.join("l")).is_err());
    assert!(root.join("a").exists());
    assert_eq!(
        held.remove().unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
}

#[test]
fn read_hands_back_the_held_name_with_its_bytes_or_why_not() {
    let (_dir, base) = scene();
    let root = base.join("proj");
    std::fs::write(root.join("a"), "bytes").unwrap();
    std::fs::write(base.join("outside/o"), "o").unwrap();
    let within = Containment::within(&root);
    let (held, bytes) = within.read(Path::new("a")).unwrap();
    assert_eq!((held.spelled, bytes), (root.join("a"), b"bytes".to_vec()));
    assert!(matches!(
        within.read(&base.join("outside/o")),
        Err(ReadFailure::Outside(_))
    ));
    match within.read(Path::new("missing")) {
        Err(ReadFailure::Io { held, error }) => {
            assert_eq!(held.spelled, root.join("missing"));
            assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn declared_takes_a_relative_name_without_parent_steps() {
    assert!(declared(Path::new("out/r.json")).is_some());
    assert!(declared(Path::new("./r.json")).is_some());
    assert!(declared(Path::new("../r.json")).is_none());
    assert!(declared(Path::new("/etc/r.json")).is_none());
    assert!(declared(Path::new("a/../b")).is_none());
}
