//! The contained disk: a read lands inside the root or is refused naming the landing, never the content
//! (ADR 0145). A hard link has no path that differs from a file in the project, so it reads.
use super::*;
use specforge_registry::Disk;

fn scene() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("proj")).unwrap();
    std::fs::create_dir_all(dir.path().join("outside")).unwrap();
    let real = std::fs::canonicalize(dir.path()).unwrap();
    (dir, real)
}

#[cfg(unix)]
#[test]
fn a_contained_disk_reads_a_chain_that_stays_inside() {
    let (_dir, base) = scene();
    let root = base.join("proj");
    std::fs::write(root.join("real"), "INSIDE").unwrap();
    std::os::unix::fs::symlink(root.join("real"), root.join("l1")).unwrap();
    std::os::unix::fs::symlink(root.join("l1"), root.join("l2")).unwrap();
    let disk = Containment::within(&root).disk();
    assert_eq!(disk.read(&root.join("l2")).unwrap(), b"INSIDE");
}

#[cfg(unix)]
#[test]
fn a_contained_disk_refuses_a_final_target_outside_with_invalid_input_naming_the_landing_not_the_content()
 {
    let (_dir, base) = scene();
    let root = base.join("proj");
    std::fs::write(base.join("outside/secret"), "SECRETWORD").unwrap();
    std::os::unix::fs::symlink(base.join("outside/secret"), root.join("leak")).unwrap();
    let disk = Containment::within(&root).disk();
    let error = disk.read(&root.join("leak")).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    let text = error.to_string();
    assert!(
        text.contains(&base.join("outside/secret").display().to_string()),
        "{text}"
    );
    assert!(!text.contains("SECRETWORD"), "{text}");
}

#[cfg(unix)]
#[test]
fn a_contained_disk_refuses_a_dangling_link_outside() {
    let (_dir, base) = scene();
    let root = base.join("proj");
    std::os::unix::fs::symlink(base.join("outside/missing"), root.join("gone")).unwrap();
    let disk = Containment::within(&root).disk();
    assert_eq!(
        disk.read(&root.join("gone")).unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );
    assert!(!disk.exists(&root.join("gone")));
}

#[cfg(unix)]
#[test]
fn a_contained_disk_reads_a_hard_link_like_any_file() {
    let (_dir, base) = scene();
    let root = base.join("proj");
    std::fs::write(base.join("outside/secret"), "SECRETWORD").unwrap();
    std::fs::hard_link(base.join("outside/secret"), root.join("hard")).unwrap();
    let disk = Containment::within(&root).disk();
    assert_eq!(disk.read(&root.join("hard")).unwrap(), b"SECRETWORD");
}
