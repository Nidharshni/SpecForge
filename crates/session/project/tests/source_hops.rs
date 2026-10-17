//! A symlinked source's read follows link hops (behavior
//! `classify_project_changes`, ADR 0129): pointing any hop of the chain
//! elsewhere is the source's change, as it is for an environment or check input.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use specforge_project::testing::{TestProject, compile_at};
use specforge_project::{ProjectSession, WatchRoot, Watched};
use specforge_test::prelude::*;
use tempfile::TempDir;

/// The files of a chain `proj/a.spec -> hop -> outside/real/real.spec`.
#[cfg(unix)]
struct Chain {
    root: PathBuf,
    hop: PathBuf,
    real: PathBuf,
    other: PathBuf,
}

/// The session agrees with a fresh compile of what is on disk now.
fn assert_matches_a_fresh_compile(session: &ProjectSession, root: &Path) {
    let fresh = compile_at(root, None);
    assert_eq!(
        session.project().diagnostics(),
        fresh.diagnostics(),
        "a session reports a fresh compile's diagnostics"
    );
}

/// A project whose `a.spec` is a link to a hop (inside the project when
/// `hop_inside`, else outside it) that is a link to `real/real.spec`.
#[cfg(unix)]
fn chained(hop_inside: bool) -> (TestProject, ProjectSession, Chain) {
    let outside = TempDir::new().unwrap();
    let outside_dir = fs::canonicalize(outside.path()).unwrap();
    // Kept alive for the test: the directory is leaked into the process.
    std::mem::forget(outside);
    fs::create_dir_all(outside_dir.join("real")).unwrap();
    fs::create_dir_all(outside_dir.join("hops")).unwrap();
    let real = outside_dir.join("real/real.spec");
    let other = outside_dir.join("real/other.spec");
    fs::write(&real, "term real_one \"R\" {}\n").unwrap();
    fs::write(&other, "term other_one \"O\" {}\nterm other_two \"O\" {}\n").unwrap();
    let dir = TestProject::new()
        .raw_config(json!({"name": "c", "version": "0.1.0"}).to_string())
        .file("links/keep", "");
    let root = fs::canonicalize(dir.root()).unwrap();
    let (hop, first) = if hop_inside {
        (root.join("links/l1"), PathBuf::from("links/l1"))
    } else {
        (outside_dir.join("hops/l1"), outside_dir.join("hops/l1"))
    };
    std::os::unix::fs::symlink(&real, &hop).unwrap();
    std::os::unix::fs::symlink(&first, root.join("a.spec")).unwrap();
    let session = dir.session_bare();
    (
        dir,
        session,
        Chain {
            root,
            hop,
            real,
            other,
        },
    )
}

#[cfg(unix)]
fn retarget(link: &Path, to: &Path) {
    fs::remove_file(link).unwrap();
    std::os::unix::fs::symlink(to, link).unwrap();
}

#[cfg(unix)]
#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a symlinked source is reached by each hop its read followed"
)]
fn a_sources_middle_hop_pointed_elsewhere_changes_that_source() {
    let (_dir, mut session, chain) = chained(true);
    retarget(&chain.hop, &chain.other);

    let changes = session.changes([chain.hop.as_path()]);
    assert_eq!(changes.sources, ["a.spec"]);
    assert!(session.apply(&changes).is_some());
    assert_matches_a_fresh_compile(&session, &chain.root);
}

#[cfg(unix)]
#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a symlinked source is reached by each hop its read followed"
)]
fn a_hop_that_is_itself_a_source_changes_both() {
    let (dir, _, chain) = chained(true);
    // `b.spec` is a link to the real file and `a.spec` a link to `b.spec`.
    let b = chain.root.join("b.spec");
    std::os::unix::fs::symlink(&chain.real, &b).unwrap();
    retarget(&chain.root.join("a.spec"), Path::new("b.spec"));
    let mut session = dir.session_bare();
    retarget(&b, &chain.other);

    let changes = session.changes([b.as_path()]);
    assert_eq!(changes.sources, ["a.spec", "b.spec"]);
    assert!(session.apply(&changes).is_some());
    assert_matches_a_fresh_compile(&session, &chain.root);
}

#[cfg(unix)]
#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a session's watch roots and the LSP's watchers cover every symlinked source's target"
)]
fn each_hop_of_a_symlinked_source_is_watched() {
    let (_dir, session, chain) = chained(false);
    let hops = chain.hop.parent().unwrap().to_path_buf();

    assert!(
        session.inputs().watch_roots().contains(&WatchRoot {
            dir: hops,
            recursive: true
        }),
        "{:?}",
        session.inputs().watch_roots()
    );
    let watched = session.inputs().watched();
    assert!(
        watched.contains(&Watched::File(chain.hop.clone())),
        "{watched:?}"
    );
    assert!(
        watched.contains(&Watched::File(chain.real.clone())),
        "{watched:?}"
    );
}

#[cfg(unix)]
#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a symlinked source is keyed by its link and changes when its target does"
)]
fn stale_sees_a_sources_middle_hop_pointed_elsewhere() {
    let (_dir, session, chain) = chained(true);
    retarget(&chain.hop, &chain.other);

    assert_eq!(session.stale().sources, ["a.spec"]);
}

#[cfg(unix)]
#[ignore = "flipped by T6"]
#[test]
fn a_hop_into_a_missing_directory_is_seen_when_it_is_created() {
    let (_dir, mut session, chain) = chained(false);
    let far = chain.real.parent().unwrap().parent().unwrap().join("far");
    fs::create_dir_all(&far).unwrap();
    retarget(&chain.hop, &far.join("newdir/final.spec"));

    let changes = session.changes([chain.hop.as_path()]);
    assert!(session.apply(&changes).is_some());
    assert!(
        session.inputs().watch_roots().contains(&WatchRoot {
            dir: far.clone(),
            recursive: false
        }),
        "{:?}",
        session.inputs().watch_roots()
    );

    fs::create_dir(far.join("newdir")).unwrap();
    fs::write(far.join("newdir/final.spec"), "term final_one \"F\" {}\n").unwrap();
    let changes = session.changes([far.join("newdir").as_path()]);
    assert_eq!(changes.sources, ["a.spec"]);
    assert!(session.apply(&changes).is_some());
    assert_matches_a_fresh_compile(&session, &chain.root);
    assert!(
        session.inputs().watch_roots().contains(&WatchRoot {
            dir: far.join("newdir"),
            recursive: true
        }),
        "{:?}",
        session.inputs().watch_roots()
    );
}
