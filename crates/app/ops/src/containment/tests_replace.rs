//! A hard link at a held name or its temporary name: replace unlinks, never writes through (ADR 0144).
use super::*;

fn scene() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(dir.path().join("outside")).unwrap();
    let real = std::fs::canonicalize(dir.path()).unwrap();
    (dir, real)
}

fn held_in(dir: &Path, name: &str) -> Held {
    Containment::within(dir).resolve(Path::new(name)).unwrap()
}

#[cfg(unix)]
#[test]
fn replace_unlinks_the_temp_name_a_hard_link_sits_at() {
    let (_dir, base) = scene();
    let root = base.join("proj");
    let victim = base.join("outside/victim");
    std::fs::write(&victim, "ORIGINAL").unwrap();
    std::fs::hard_link(&victim, root.join("x.json.tmp")).unwrap();
    held_in(&root, "x.json").replace("new").unwrap();
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "ORIGINAL");
    assert_eq!(std::fs::read_to_string(root.join("x.json")).unwrap(), "new");
}

#[cfg(unix)]
#[test]
fn replace_keeps_the_other_name_of_a_hard_link() {
    let (_dir, base) = scene();
    let root = base.join("proj");
    let victim = base.join("outside/victim");
    std::fs::write(&victim, "ORIGINAL").unwrap();
    std::fs::hard_link(&victim, root.join("x.json")).unwrap();
    held_in(&root, "x.json").replace("new").unwrap();
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "ORIGINAL");
    assert_eq!(std::fs::read_to_string(root.join("x.json")).unwrap(), "new");
}

#[cfg(unix)]
#[test]
fn create_new_refuses_an_existing_hard_link() {
    let (_dir, base) = scene();
    let root = base.join("proj");
    let victim = base.join("outside/victim");
    std::fs::write(&victim, "ORIGINAL").unwrap();
    std::fs::hard_link(&victim, root.join("x.json")).unwrap();
    let error = held_in(&root, "x.json").create_new("new").unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "ORIGINAL");
}
