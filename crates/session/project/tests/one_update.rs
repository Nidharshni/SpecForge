//! One batch of changes is one update (behavior `bring_session_up_to_date`, ADR 0159): a source
//! that reads as the session holds it does not drop a check input changed with it, and the checks
//! run once.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use specforge_extension_sdk::prelude::*;
use specforge_project::testing::TestProject;
use specforge_project::{Buffer, ProjectSession, SharedRuntime, UpdateKind};
use specforge_test::prelude::*;
use specforge_wasm::testing::InProcessRuntime;
#[cfg(unix)]
use tempfile::TempDir;

use crate::buffers::check_runs;

const EXT: &str = "@test/owed";
const NOTE: &str = "note n \"N\" {\n  doc \"guide.md\"\n}\n";

/// A `note` kind whose `doc` a `file_exists` rule F001 reads, and a check pass that counts runs.
fn owed() -> Arc<InProcessRuntime> {
    Arc::new(InProcessRuntime::new().with(|| {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new(EXT, "1.0.0"));
        c.kind("note", |k| {
            k.keyword("note");
            k.field("doc", |f| {
                f.field_type(FieldType::String);
            });
        });
        c.rule("F001", |r| {
            r.check(CheckKind::FileExists)
                .severity(ValidationSeverity::Warning)
                .target_kind("note")
                .field("doc")
                .message_template("note '{id}': missing '{value}'");
        });
        c.pass("count", |p| {
            p.phase("check").run(|_: &PassInput| {
                vec![PassDiagnostic::new(
                    "I990",
                    PassSeverity::Info,
                    "the checks ran",
                )]
            });
        });
        c
    }))
}

fn opened(rt: &Arc<InProcessRuntime>) -> (TestProject, ProjectSession) {
    let dir = TestProject::new()
        .config(|c| {
            c["name"] = "p".into();
            c["spec_root"] = "spec".into();
        })
        .enabling(&[EXT])
        .installing(&[EXT])
        .file("spec/a.spec", NOTE);
    let session = dir.session_in(Arc::clone(rt) as SharedRuntime);
    (dir, session)
}

fn f001(session: &ProjectSession) -> usize {
    session
        .project()
        .diagnostics()
        .iter()
        .filter(|d| d.code == "F001")
        .count()
}

/// The file's own bytes again, with a modification time that differs.
fn touch(path: &Path) {
    let text = fs::read(path).unwrap();
    fs::write(path, text).unwrap();
    let file = fs::OpenOptions::new().write(true).open(path).unwrap();
    let modified = fs::metadata(path).unwrap().modified().unwrap();
    file.set_modified(modified + Duration::from_secs(3))
        .unwrap();
}

fn created_guide(dir: &TestProject) -> PathBuf {
    let guide = dir.root().join("spec/guide.md");
    fs::write(&guide, "# Guide\n").unwrap();
    guide
}

#[specforge_test(
    behavior = "bring_session_up_to_date",
    verify = "a source that reads as the session holds it does not drop a check input changed with it"
)]
fn a_current_source_and_a_changed_check_input_run_the_checks() {
    let rt = owed();
    let (dir, mut session) = opened(&rt);
    assert_eq!(f001(&session), 1);
    let a = dir.root().join("spec/a.spec");
    touch(&a);
    let guide = created_guide(&dir);
    let changes = session.changes([a.as_path(), guide.as_path()]);
    assert_eq!(changes.sources(), ["a.spec"]);
    assert!(!changes.check_inputs().is_empty());
    let before = check_runs(&rt);

    let update = session.apply(&changes).expect("a check input changed");

    assert_eq!(update.kind, UpdateKind::Checks);
    assert!(update.rebuilt_files.is_empty());
    assert_eq!(check_runs(&rt) - before, 1);
    assert_eq!(f001(&session), 0);
    assert!(session.stale().is_empty());
}

#[test]
fn ensure_fresh_applies_a_current_source_and_a_changed_check_input_at_once() {
    let rt = owed();
    let (dir, mut session) = opened(&rt);
    touch(&dir.root().join("spec/a.spec"));
    created_guide(&dir);

    session.ensure_fresh().expect("a check input changed");

    assert_eq!(f001(&session), 0);
    assert!(session.ensure_fresh().is_none());
}

#[cfg(unix)]
#[test]
fn a_relinked_source_and_a_changed_check_input_run_the_checks() {
    let rt = owed();
    let (dir, mut session) = opened(&rt);
    let a = dir.root().join("spec/a.spec");
    let outside = TempDir::new().unwrap();
    let real = fs::canonicalize(outside.path()).unwrap().join("real.spec");
    fs::write(&real, NOTE).unwrap();
    fs::remove_file(&a).unwrap();
    std::os::unix::fs::symlink(&real, &a).unwrap();
    let guide = created_guide(&dir);

    let update = session
        .apply(&session.changes([a.as_path(), guide.as_path()]))
        .expect("the link is an input");

    assert!(update.inputs_changed);
    assert!(update.rebuilt_files.is_empty());
    assert_eq!(f001(&session), 0, "{:?}", update.kind);
    assert!(session.stale().is_empty());
}

#[specforge_test(
    behavior = "bring_session_up_to_date",
    verify = "a changed source and a changed check input run the checks once"
)]
fn a_changed_source_and_a_changed_check_input_run_the_checks_once() {
    let rt = owed();
    let (dir, mut session) = opened(&rt);
    let a = dir.root().join("spec/a.spec");
    fs::write(&a, format!("{NOTE}// edited\n")).unwrap();
    let guide = created_guide(&dir);
    let before = check_runs(&rt);

    let update = session
        .apply(&session.changes([a.as_path(), guide.as_path()]))
        .expect("a source changed");

    assert_eq!(update.kind, UpdateKind::Sources);
    assert_eq!(update.rebuilt_files, ["a.spec"]);
    assert_eq!(check_runs(&rt) - before, 1);
    assert_eq!(f001(&session), 0);
    assert!(session.stale().is_empty());
}

#[specforge_test(
    behavior = "bring_session_up_to_date",
    verify = "an environment reload covers a check input changed with it"
)]
fn an_environment_change_covers_a_changed_check_input() {
    let rt = owed();
    let (dir, mut session) = opened(&rt);
    touch(&dir.root().join("specforge.json"));
    let guide = created_guide(&dir);
    let changes = session.changes([dir.root().join("specforge.json").as_path(), guide.as_path()]);
    assert!(!changes.environment().is_empty());
    assert!(!changes.check_inputs().is_empty());

    let update = session.apply(&changes).expect("the environment changed");

    assert_eq!(update.kind, UpdateKind::Environment);
    assert_eq!(f001(&session), 0);
    assert!(session.stale().is_empty());
}

#[test]
fn a_held_source_and_a_changed_check_input_run_the_checks() {
    let rt = owed();
    let (dir, mut session) = opened(&rt);
    let a = dir.root().join("spec/a.spec");
    session.hold(&[Buffer::new(&a, NOTE)]);
    touch(&a);
    let guide = created_guide(&dir);

    let update = session
        .apply(&session.changes([a.as_path(), guide.as_path()]))
        .expect("a check input changed");

    assert_eq!(update.kind, UpdateKind::Checks);
    assert_eq!(f001(&session), 0);
}
