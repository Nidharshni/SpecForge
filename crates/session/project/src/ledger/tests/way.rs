//! A directory link on the way to an environment or check input (ADR 0173).
#![cfg(unix)]

use std::fs;

use specforge_common::ProjectConfig;
use specforge_registry::{Disk, PlainDisk};
use specforge_test_macros::test as specforge_test;

use crate::ledger::chain::way;
use crate::ledger::disk::RunDisk;
use crate::ledger::key::in_canonical_dir;
use crate::ledger::recorded::Recording;
use crate::ledger::{Ledger, SessionInputs};

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a directory link on the way to an environment or check input pointed elsewhere is that input's change"
)]
fn a_directory_link_on_the_way_to_an_environment_or_check_input_pointed_elsewhere_is_its_change() {
    for environment in [true, false] {
        let t = tempfile::TempDir::new().unwrap();
        let base = fs::canonicalize(t.path()).unwrap();
        let (root, out) = (base.join("root"), base.join("out"));
        for dir in [&root, &out.join("v1"), &out.join("v2")] {
            fs::create_dir_all(dir).unwrap();
        }
        fs::write(out.join("v1/x.json"), "first").unwrap();
        fs::write(out.join("v2/x.json"), "another, longer").unwrap();
        std::os::unix::fs::symlink(out.join("v1"), root.join("vendor")).unwrap();
        let input = root.join("vendor/x.json");
        let config = ProjectConfig::default();
        let mut ledger = Ledger::over(SessionInputs::opened(&root, &config, Vec::new()));
        if environment {
            let run = ledger.environment_run();
            let _ = RunDisk(&run, &PlainDisk).read(&input);
            run.finish(Recording::Environment {
                root: &root,
                config: &config,
            });
        } else {
            let run = ledger.checks();
            RunDisk(&run, &PlainDisk).exists(&input);
            run.finish(Recording::Checks);
        }

        fs::remove_file(root.join("vendor")).unwrap();
        std::os::unix::fs::symlink(out.join("v2"), root.join("vendor")).unwrap();
        let changes = ledger.changes([root.join("vendor").as_path()]);
        let moved = if environment {
            &changes.environment
        } else {
            &changes.check_inputs
        };
        assert!(!moved.is_empty(), "environment: {environment}, {changes:?}");
    }
}

#[test]
fn the_roots_own_spelling_is_no_way() {
    let t = tempfile::TempDir::new().unwrap();
    let base = fs::canonicalize(t.path()).unwrap();
    fs::create_dir_all(base.join("real/proj/a")).unwrap();
    std::os::unix::fs::symlink(base.join("real"), base.join("link")).unwrap();
    let root = base.join("link/proj");
    assert!(way(&root.join("a/x"), &root).is_empty());

    fs::remove_dir(base.join("real/proj/a")).unwrap();
    fs::create_dir(base.join("real/proj/d")).unwrap();
    std::os::unix::fs::symlink(base.join("real/proj/d"), base.join("real/proj/a")).unwrap();
    assert_eq!(
        way(&root.join("a/x"), &root),
        [in_canonical_dir(&root.join("a"))]
    );
}

#[test]
fn a_link_directly_under_the_file_system_root_is_no_way() {
    // On macOS `/tmp` is a link, on Linux a directory: empty either way.
    assert!(
        way(
            std::path::Path::new("/tmp/x/y"),
            std::path::Path::new("/nowhere")
        )
        .is_empty()
    );
}

#[test]
fn a_links_own_name_is_a_hop_not_its_way() {
    let t = tempfile::TempDir::new().unwrap();
    let root = fs::canonicalize(t.path()).unwrap();
    fs::create_dir(root.join("d")).unwrap();
    std::os::unix::fs::symlink(root.join("d"), root.join("l")).unwrap();
    assert!(way(&root.join("l"), &root).is_empty());
}
