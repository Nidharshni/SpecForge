//! The project a path is in (`ProjectRoot`, behavior `locate_project_root`,
//! ADR 0053): the one rule every surface opens a project by.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use specforge_common::{NoSuchPath, ProjectRoot, read_project_config};
use specforge_test_macros::test as specforge_test;
use tempfile::TempDir;

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap()
}

/// Whether any ancestor of `dir` (itself included) holds `specforge.json`:
/// a machine whose temporary directory is inside a project can't show a
/// loose root.
fn project_above(dir: &Path) -> bool {
    canonical(dir).ancestors().any(ProjectRoot::is_root)
}

#[specforge_test(
    behavior = "locate_project_root",
    verify = "specforge.json found in the path's own directory"
)]
fn found_in_its_own_directory() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("specforge.json"), "{}").unwrap();

    let root = ProjectRoot::of(dir.path()).unwrap();
    assert_eq!(root.canonical(), canonical(dir.path()));
    assert_eq!(root.dir(), dir.path());
    assert!(root.is_configured());
    assert_eq!(root.config_file(), dir.path().join("specforge.json"));
}

#[specforge_test(
    behavior = "locate_project_root",
    verify = "specforge.json found in an ancestor directory"
)]
fn found_in_an_ancestor() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("specforge.json"), "{}").unwrap();
    let child = dir.path().join("sub").join("deep");
    fs::create_dir_all(&child).unwrap();

    let root = ProjectRoot::of(&child).unwrap();
    assert_eq!(root.canonical(), canonical(dir.path()));
    assert_eq!(root.dir(), dir.path());
    assert_eq!(ProjectRoot::enclosing(&child), Some(root));
}

#[specforge_test(
    behavior = "locate_project_root",
    verify = "closest directory wins over ancestor directory"
)]
fn the_closest_wins() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("specforge.json"), "{}").unwrap();
    let child = dir.path().join("child");
    fs::create_dir_all(child.join("spec")).unwrap();
    fs::write(child.join("specforge.json"), "{}").unwrap();

    let root = ProjectRoot::of(&child.join("spec")).unwrap();
    assert_eq!(root.canonical(), canonical(&child));
}

#[specforge_test(
    behavior = "locate_project_root",
    verify = "a specforge.spec file marks no project"
)]
fn a_specforge_spec_file_marks_no_project() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("specforge.json"), "{}").unwrap();
    let spec = dir.path().join("spec");
    fs::create_dir_all(&spec).unwrap();
    fs::write(spec.join("specforge.spec"), "spec \"p\" {}\n").unwrap();

    assert!(!ProjectRoot::is_root(&spec));
    let root = ProjectRoot::of(&spec).unwrap();
    assert_eq!(root.canonical(), canonical(dir.path()));
    assert!(root.is_configured());
}

#[specforge_test(
    behavior = "locate_project_root",
    verify = "a path with no specforge.json at or above it is a loose root, its own directory"
)]
fn a_path_in_no_project_is_its_own_loose_root() {
    let dir = TempDir::new().unwrap();
    let loose = dir.path().join("loose");
    fs::create_dir_all(&loose).unwrap();
    fs::write(loose.join("specforge.spec"), "spec \"l\" {}\n").unwrap();
    if project_above(&loose) {
        return;
    }

    let root = ProjectRoot::of(&loose).unwrap();
    assert_eq!(root.dir(), loose);
    assert_eq!(root.canonical(), canonical(&loose));
    assert!(!root.is_configured());
    assert_eq!(ProjectRoot::enclosing(&loose), None);
}

#[specforge_test(
    behavior = "locate_project_root",
    verify = "a file path is in the project of its directory"
)]
fn a_file_path_is_in_its_directorys_project() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("specforge.json"), "{}").unwrap();
    fs::create_dir_all(dir.path().join("spec")).unwrap();
    let file = dir.path().join("spec/a.spec");
    fs::write(&file, "behavior a \"A\" {}\n").unwrap();

    let root = ProjectRoot::of(&file).unwrap();
    assert_eq!(root.dir(), dir.path());
    assert!(root.is_configured());

    // A file in no project: its directory is the loose root.
    let loose = TempDir::new().unwrap();
    if !project_above(loose.path()) {
        let file = loose.path().join("a.spec");
        fs::write(&file, "").unwrap();
        assert_eq!(ProjectRoot::of(&file).unwrap().dir(), loose.path());
    }
}

#[specforge_test(
    behavior = "locate_project_root",
    verify = "a path that does not exist is refused as file_not_found"
)]
fn a_missing_path_is_no_such_path() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("specforge.json"), "{}").unwrap();
    let missing = dir.path().join("nowhere");

    let refused = ProjectRoot::of(&missing).unwrap_err();
    assert_eq!(
        refused,
        NoSuchPath {
            path: missing.clone(),
            kind: ErrorKind::NotFound
        }
    );
    assert_eq!(
        refused.to_string(),
        format!("path not found: {}", missing.display())
    );
    assert_eq!(ProjectRoot::enclosing(&missing), None);
}

#[cfg(unix)]
#[specforge_test(
    behavior = "locate_project_root",
    verify = "symlinks are resolved before path comparison"
)]
fn resolves_symlinks() {
    let dir = TempDir::new().unwrap();
    let real = dir.path().join("real");
    fs::create_dir_all(&real).unwrap();
    fs::write(real.join("specforge.json"), "{}").unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let through_link = ProjectRoot::of(&link).unwrap();
    assert_eq!(through_link.canonical(), canonical(&real));
    assert_eq!(through_link, ProjectRoot::of(&real).unwrap());
}

#[cfg(unix)]
#[specforge_test(
    behavior = "locate_project_root",
    verify = "the root is spelled as the path spells it"
)]
fn the_root_keeps_the_paths_spelling() {
    let dir = TempDir::new().unwrap();
    let real = canonical(dir.path()).join("real");
    fs::create_dir_all(real.join("project/spec")).unwrap();
    fs::write(real.join("project/specforge.json"), "{}").unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    // A link's spelling is kept where it reaches the root...
    let spelled = ProjectRoot::of(&link.join("project/spec")).unwrap();
    assert_eq!(spelled.dir(), link.join("project"));
    assert_eq!(spelled.canonical(), real.join("project"));
    // ...a `..` spelling is not followed: the root is canonical...
    let dotted = ProjectRoot::of(&link.join("project/spec/..")).unwrap();
    assert_eq!(dotted.dir(), real.join("project"));
    // ...and equality is the directory's, however spelled.
    assert_eq!(spelled, dotted);
    assert_eq!(spelled.clone().canonicalized().dir(), real.join("project"));
    // A symlink that leaves the tree: the root is canonical.
    let outside = dir.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(real.join("project/spec"), outside.join("spec")).unwrap();
    let left = ProjectRoot::of(&outside.join("spec")).unwrap();
    assert_eq!(left.dir(), real.join("project"));
}

#[cfg(unix)]
#[specforge_test(
    behavior = "locate_project_root",
    verify = "circular symlink chain does not cause infinite loop"
)]
fn handles_circular_symlinks() {
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::create_dir_all(&a).unwrap();
    // a/link -> b -> a: resolving a/link/link/... never ends, and the
    // resolution refuses the chain instead of walking it.
    std::os::unix::fs::symlink(&a, &b).unwrap();
    std::os::unix::fs::symlink(&b, a.join("link")).unwrap();
    let c = dir.path().join("c");
    let d = dir.path().join("d");
    std::os::unix::fs::symlink(&d, &c).unwrap();
    std::os::unix::fs::symlink(&c, &d).unwrap();

    // Terminates: a resolvable chain is a root, a circular one refused.
    assert!(ProjectRoot::of(&a.join("link")).is_ok());
    assert!(ProjectRoot::of(&c).is_err());
}

#[specforge_test(
    behavior = "locate_project_root",
    verify = "directory traversal completes in under 100ms for 20-level deep hierarchy"
)]
fn performance() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("specforge.json"), "{}").unwrap();
    let mut deep = dir.path().to_path_buf();
    for i in 0..20 {
        deep = deep.join(format!("level_{i}"));
    }
    fs::create_dir_all(&deep).unwrap();

    let start = std::time::Instant::now();
    let root = ProjectRoot::of(&deep).unwrap();
    let elapsed = start.elapsed();

    assert_eq!(root.dir(), dir.path(), "found 20 levels up");
    assert!(
        elapsed.as_millis() < 100,
        "traversal took {}ms, expected < 100ms",
        elapsed.as_millis()
    );
}

/// Every clause of the `locate_project_root` contract, on one directory tree.
#[specforge_test(
    behavior = "locate_project_root",
    verify = "Locate the Project a Path Is In: project root location holds — filesystem_available, closest_wins_enforced, one_marker, loose_root, missing_path_refused, symlinks_resolved, every_surface_agrees, one_option"
)]
fn contract() {
    // filesystem_available: a real directory tree to walk.
    let dir = TempDir::new().unwrap();
    let top = canonical(dir.path());
    fs::write(top.join("specforge.json"), r#"{"name":"outer"}"#).unwrap();
    let inner = top.join("inner");
    let deep = inner.join("a").join("b");
    fs::create_dir_all(&deep).unwrap();

    // one_marker: `inner/specforge.spec` alone does not stop the walk.
    fs::write(inner.join("specforge.spec"), "spec \"inner\" {}\n").unwrap();
    assert_eq!(ProjectRoot::of(&deep).unwrap().canonical(), top);

    // closest_wins_enforced: once `inner` holds specforge.json it wins below
    // it, and its configuration is that file's; outside it the ancestor wins.
    fs::write(inner.join("specforge.json"), r#"{"name":"inner-json"}"#).unwrap();
    let root = ProjectRoot::of(&deep).unwrap();
    assert_eq!(root.canonical(), inner);
    assert_eq!(
        read_project_config(root.dir()).config.name.as_deref(),
        Some("inner-json")
    );
    let sibling = top.join("sibling");
    fs::create_dir_all(&sibling).unwrap();
    assert_eq!(ProjectRoot::of(&sibling).unwrap().canonical(), top);

    // symlinks_resolved: a link to `deep` placed directly under `top`
    // resolves to `inner`; walking the link's own path would reach `top`.
    #[cfg(unix)]
    {
        let link = top.join("link");
        std::os::unix::fs::symlink(&deep, &link).unwrap();
        let root = ProjectRoot::of(&link).unwrap();
        assert_eq!(root.canonical(), inner);
        assert_eq!(root.dir(), inner);
    }

    // missing_path_refused: never an empty project.
    let missing = top.join("nowhere");
    assert_eq!(
        ProjectRoot::of(&missing).unwrap_err().kind,
        ErrorKind::NotFound
    );

    // loose_root: a tree with no specforge.json up to the filesystem root.
    let empty = TempDir::new().unwrap();
    let nothing = canonical(empty.path()).join("nothing");
    fs::create_dir_all(&nothing).unwrap();
    if !project_above(&nothing) {
        let loose = ProjectRoot::of(&nothing).unwrap();
        assert_eq!(loose.dir(), nothing);
        assert!(!loose.is_configured());
    }

    // every_surface_agrees: one value, compared by directory, whichever
    // path inside the project named it.
    assert_eq!(
        ProjectRoot::of(&inner.join("a")).unwrap(),
        ProjectRoot::of(&deep).unwrap()
    );

    // one_option: a clause of the command line, held by the CLI tests
    // (`project_from_path`, `main.rs`'s command-tree test), not by this crate.
}
