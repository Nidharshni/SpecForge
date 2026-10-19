//! A directory link on the way to a source's hop is part of its chain (behaviors
//! `classify_project_changes` and `invalidate_changed_files`; ADR 0129, ADR 0173): pointing it
//! elsewhere is that source's change. Both targets get one old modification time, so the clock
//! never decides.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::json;
use specforge_project::testing::TestProject;
use specforge_project::{ProjectSession, WatchRoot, Watched};
use specforge_test::prelude::*;
use tempfile::TempDir;

const ONE: &str = "term one_a \"A\" {}\n";
const TWO: &str = "term two_a \"A\" {}\n";

struct Aliased {
    dir: TestProject,
    _out: TempDir,
    out: PathBuf,
    alias: PathBuf,
    session: ProjectSession,
}

/// `a.spec` is a link to `alias/b.spec`, `alias` a link to `out/v1` (inside the project when
/// `inside`, else beside it in `out`); `out/v2/b.spec` has the length and time of `out/v1/b.spec`.
fn aliased(inside: bool) -> Aliased {
    let outside = TempDir::new().unwrap();
    let out = fs::canonicalize(outside.path()).unwrap();
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    for (version, text) in [("v1", ONE), ("v2", TWO)] {
        fs::create_dir(out.join(version)).unwrap();
        let file = out.join(version).join("b.spec");
        fs::write(&file, text).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(old)
            .unwrap();
    }
    let dir = TestProject::new()
        .raw_config(json!({"name": "c", "version": "0.1.0"}).to_string())
        .file("keep.spec", "term keep_a \"K\" {}\n");
    let root = fs::canonicalize(dir.root()).unwrap();
    let alias = if inside {
        root.join("alias")
    } else {
        out.join("alias")
    };
    symlink(out.join("v1"), &alias).unwrap();
    let target = if inside {
        PathBuf::from("alias/b.spec")
    } else {
        alias.join("b.spec")
    };
    symlink(target, root.join("a.spec")).unwrap();
    let session = dir.session_bare();
    Aliased {
        dir,
        _out: outside,
        out,
        alias,
        session,
    }
}

fn retarget(link: &Path, to: &Path) {
    fs::remove_file(link).unwrap();
    symlink(to, link).unwrap();
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a directory link on the way to a source's hop pointed elsewhere is that source's change"
)]
fn a_directory_link_on_a_sources_chain_pointed_elsewhere_changes_that_source() {
    let mut a = aliased(true);
    assert_eq!(
        a.session.project().source_text("a.spec").as_deref(),
        Some(ONE)
    );
    retarget(&a.alias, &a.out.join("v2"));

    let changes = a.session.changes([a.alias.as_path()]);
    assert_eq!(changes.sources(), ["a.spec"]);
    assert!(a.session.apply(&changes).is_some());
    assert_eq!(
        a.session.project().source_text("a.spec").as_deref(),
        Some(TWO)
    );
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a directory link on the way to an input outside the root is watched by its own directory"
)]
fn a_directory_link_outside_the_project_is_watched_by_its_own_directory() {
    let a = aliased(false);
    let roots = a.session.inputs().watch_roots();
    assert!(
        roots.contains(&WatchRoot {
            dir: a.out.clone(),
            recursive: false
        }),
        "{roots:?}"
    );
    assert!(
        a.session
            .inputs()
            .watched()
            .contains(&Watched::File(a.alias.clone()))
    );

    retarget(&a.alias, &a.out.join("v2"));
    assert_eq!(a.session.changes([a.alias.as_path()]).sources(), ["a.spec"]);
}

#[specforge_test(
    behavior = "invalidate_changed_files",
    verify = "a source whose chain goes through a directory link pointed at an equal-stamp file still moves"
)]
fn stale_sees_a_directory_link_on_a_chain_pointed_at_an_equal_stamp_file() {
    let mut a = aliased(true);
    retarget(&a.alias, &a.out.join("v2"));

    assert_eq!(a.session.stale().sources(), ["a.spec"]);
    let stale = a.session.stale();
    assert!(a.session.apply(&stale).is_some(), "the chain moved");
    assert_eq!(
        a.session.project().source_text("a.spec").as_deref(),
        Some(TWO)
    );
}

#[test]
fn a_hop_climbing_out_of_a_directory_link_reaches_the_file_the_os_reads() {
    let mut a = aliased(true);
    let b = a.out.join("v1/b.spec");
    fs::remove_file(&b).unwrap();
    symlink("../c.spec", &b).unwrap();
    let c = a.out.join("c.spec");
    fs::write(&c, ONE).unwrap();
    a.session = a.dir.session_bare();
    assert_eq!(
        a.session.project().source_text("a.spec").as_deref(),
        Some(ONE)
    );

    fs::write(&c, "term c_one \"C\" {}\nterm c_two \"C\" {}\n").unwrap();
    assert_eq!(a.session.changes([c.as_path()]).sources(), ["a.spec"]);
}
