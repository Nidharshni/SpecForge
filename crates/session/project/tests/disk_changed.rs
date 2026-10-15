//! `testing::disk_changed`: a test's writes taken as a watcher's batch (`changes`, then `apply`).

use specforge_project::ProjectSession;
use specforge_project::testing::{TestProject, disk_changed};

const A: &str = "term a \"A\" {\n}\n";

#[test]
fn a_written_source_is_one_update() {
    let dir = TestProject::new().files(&[("a.spec", A)]);
    let mut session = dir.session();
    dir.write("a.spec", "term a \"B\" {\n}\n");
    let update = disk_changed(&mut session, &["a.spec"]).expect("a written source is an update");
    assert_eq!(update.rebuilt_files, ["a.spec"]);
}

#[test]
fn an_untouched_source_is_no_update() {
    let dir = TestProject::new().files(&[("a.spec", A)]);
    let mut session = dir.session();
    assert!(disk_changed(&mut session, &["a.spec"]).is_none());
}

#[test]
fn keys_are_relative_to_the_spec_root() {
    let dir = TestProject::new()
        .raw_config(r#"{"name":"t","version":"0.1.0","extensions":[],"spec_root":"spec"}"#)
        .files(&[("spec/a.spec", A)]);
    let mut session = dir.session_bare();
    dir.write("spec/a.spec", "term a \"B\" {\n}\n");
    assert!(disk_changed(&mut session, &["a.spec"]).is_some());
}

#[test]
fn an_excluded_source_is_no_update() {
    let dir = TestProject::new()
        .raw_config(r#"{"name":"t","version":"0.1.0","extensions":[],"exclude":["drafts/"]}"#)
        .files(&[("a.spec", A)]);
    let mut session = dir.session_bare();
    dir.write("drafts/d.spec", "term d \"D\" {\n}\n");
    assert!(disk_changed(&mut session, &["drafts/d.spec"]).is_none());
}

#[test]
fn a_detached_session_takes_nothing() {
    let mut session = ProjectSession::detached();
    assert!(disk_changed(&mut session, &["a.spec"]).is_none());
}
