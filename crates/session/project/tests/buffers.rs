//! A project session and the editor buffers it is given (behavior
//! `hold_editor_buffers`, ADR 0046).

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use specforge_extension_sdk::prelude::*;
use specforge_project::testing::TestProject;
use specforge_project::{
    Buffer, CheckMode, ProjectSession, RuntimeSource, SharedRuntime, UpdateKind,
};
use specforge_test::prelude::*;
use specforge_wasm::testing::InProcessRuntime;
use tempfile::TempDir;

const EXT: &str = "@test/count";
const ALPHA: &str = "type alpha \"A\" {}\n";
const OMEGA: &str = "type omega \"O\" {}\n";
const ZETA: &str = "type zeta \"Z\" {}\n";

/// An extension whose one check pass reports I990: each run of the checks is
/// one call, and the session reports I990 exactly when the checks ran last.
pub(super) fn counting() -> Arc<InProcessRuntime> {
    Arc::new(InProcessRuntime::new().with(|| {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new(EXT, "1.0.0"));
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

pub(super) fn check_runs(runtime: &InProcessRuntime) -> usize {
    runtime
        .calls()
        .iter()
        .filter(|c| c.export == "__pass_count")
        .count()
}

/// A project holding `a.spec` (alpha) that enables the counting extension,
/// opened over `runtime`.
pub(super) fn opened(runtime: &Arc<InProcessRuntime>) -> (TestProject, ProjectSession) {
    let dir = TestProject::new()
        .config(|c| c["name"] = "p".into())
        .enabling(&[EXT])
        .installing(&[EXT])
        .file("a.spec", ALPHA);
    let session = dir.session_in(Arc::clone(runtime) as SharedRuntime);
    (dir, session)
}

pub(super) fn has(session: &ProjectSession, id: &str) -> bool {
    session.project().graph().node(id).is_some()
}

fn hold(session: &mut ProjectSession, dir: &TestProject, text: &str) {
    session.hold(&[Buffer::new(dir.root().join("a.spec"), text)]);
}

#[specforge_test(
    behavior = "hold_editor_buffers",
    verify = "a held buffer is the truth for its file until it is released"
)]
fn a_held_buffer_is_the_truth_for_its_file_until_released() {
    let rt = counting();
    let (dir, mut session) = opened(&rt);
    let a = dir.root().join("a.spec");
    hold(&mut session, &dir, OMEGA);
    fs::write(&a, ZETA).unwrap();
    assert!(session.changes(std::iter::once(a.as_path())).is_empty());
    assert!(session.stale().is_empty());
    assert!(session.ensure_fresh().is_none());
    assert!(has(&session, "omega"));

    assert!(session.release(&[a]).is_some());
    assert!(has(&session, "zeta") && !has(&session, "omega"));
    assert!(session.stale().is_empty());
}

#[specforge_test(
    behavior = "hold_editor_buffers",
    verify = "a held buffer's file changed or deleted on disk is not stale"
)]
fn a_held_buffers_deleted_file_is_not_stale() {
    let rt = counting();
    let (dir, mut session) = opened(&rt);
    let a = dir.root().join("a.spec");
    fs::remove_file(&a).unwrap();
    // Taken before the hold, applied after it: the buffer is still the truth.
    let named = session.changes(std::iter::once(a.as_path()));
    assert_eq!(named.sources(), ["a.spec"]);
    hold(&mut session, &dir, OMEGA);
    assert!(session.stale().is_empty());
    assert!(session.apply(&named).is_none());
    assert!(has(&session, "omega"));

    assert!(session.release(&[a]).is_some());
    assert!(!has(&session, "omega"));
}

#[specforge_test(
    behavior = "hold_editor_buffers",
    verify = "holding a buffer whose text is its file's runs no check"
)]
fn holding_a_buffer_whose_text_is_its_files_runs_no_check() {
    let rt = counting();
    let (dir, mut session) = opened(&rt);
    let before = check_runs(&rt);
    let update = session.hold(&[Buffer::new(dir.root().join("a.spec"), ALPHA).at_version(Some(3))]);
    assert_eq!(check_runs(&rt) - before, 0);
    assert!(update.rebuilt_files.is_empty());
    assert_eq!(session.buffer("a.spec").unwrap().version, Some(3));
}

#[specforge_test(
    behavior = "hold_editor_buffers",
    verify = "releasing a buffer reads its file through the one read and leaves nothing stale"
)]
fn releasing_a_saved_buffer_reads_it_once_and_leaves_nothing_stale() {
    let rt = counting();
    let (dir, mut session) = opened(&rt);
    let a = dir.root().join("a.spec");
    hold(&mut session, &dir, OMEGA);
    fs::write(&a, OMEGA).unwrap(); // saved
    let before = check_runs(&rt);
    assert!(session.release(std::slice::from_ref(&a)).is_none());
    assert_eq!(check_runs(&rt) - before, 0);
    assert!(session.stale().is_empty());
    assert!(session.buffer("a.spec").is_none());

    // The file is the disk's again.
    fs::write(&a, ZETA).unwrap();
    assert_eq!(session.stale().sources(), ["a.spec"]);
}

#[specforge_test(
    behavior = "hold_editor_buffers",
    verify = "releasing a buffer outside the project drops its file"
)]
fn releasing_a_buffer_outside_the_project_drops_it() {
    // No project: the buffer was the file's only text.
    let mut detached = ProjectSession::detached();
    detached.hold(&[Buffer::new("/p/a.spec", ALPHA)]);
    assert!(has(&detached, "alpha"));
    assert!(detached.release(&[PathBuf::from("/p/a.spec")]).is_some());
    assert!(!has(&detached, "alpha"));

    // In a project, a file outside the spec root is held and builds nothing.
    let rt = counting();
    let (_dir, mut session) = opened(&rt);
    let elsewhere = TempDir::new().unwrap();
    session.hold(&[Buffer::new(elsewhere.path().join("x.spec"), ZETA)]);
    assert!(!has(&session, "zeta"));
    let before = check_runs(&rt);
    assert!(
        session
            .release(&[elsewhere.path().join("x.spec")])
            .is_none()
    );
    assert_eq!(check_runs(&rt) - before, 0);
}

#[specforge_test(
    behavior = "hold_editor_buffers",
    verify = "releasing a buffer that did not parse runs the skipped checks"
)]
fn releasing_a_buffer_that_did_not_parse_runs_the_skipped_checks() {
    let rt = counting();
    let (dir, mut session) = opened(&rt);
    let a = dir.root().join("a.spec");
    let broken = "type alpha \"A\" {\n  contract \"\n";
    session.hold_with(
        &[Buffer::new(&a, broken)],
        CheckMode::SyntaxOnlyIfParseErrors,
    );
    let reported = |s: &ProjectSession| s.project().diagnostics().iter().any(|d| d.code == "I990");
    assert!(!reported(&session), "the typing fast path skips the checks");

    fs::write(&a, broken).unwrap(); // saved
    let update = session.release(&[a]).expect("the skipped checks run");
    assert_eq!(update.kind, UpdateKind::Checks);
    assert!(reported(&session));
    let fresh = dir.compile_in(rt.clone());
    let codes = |d: Vec<specforge_common::Diagnostic>| -> Vec<String> {
        d.into_iter().map(|d| d.code).collect()
    };
    assert_eq!(
        codes(session.project().diagnostics()),
        codes(fresh.diagnostics())
    );
}

#[specforge_test(
    behavior = "hold_editor_buffers",
    verify = "an environment reload keeps the held buffers and runs the checks once"
)]
fn a_reload_keeps_the_held_buffers_and_runs_the_checks_once() {
    let rt = counting();
    let (dir, mut session) = opened(&rt);
    hold(&mut session, &dir, OMEGA);
    let before = check_runs(&rt);
    session.reload_environment();
    assert_eq!(check_runs(&rt) - before, 1);
    assert!(has(&session, "omega") && !has(&session, "alpha"));
    assert!(session.buffer("a.spec").is_some());
    assert!(session.stale().is_empty());
}

#[specforge_test(
    behavior = "hold_editor_buffers",
    verify = "opening a project in place of a session keeps its buffers and runs the checks once"
)]
fn opening_a_project_in_place_of_a_session_keeps_its_buffers() {
    let rt = counting();
    let (dir, session) = opened(&rt);
    drop(session);
    let mut session = ProjectSession::detached();
    session.hold(&[Buffer::new(dir.root().join("a.spec"), OMEGA).at_version(Some(4))]);
    let before = check_runs(&rt);
    session.reopen(
        &dir.project_root(),
        RuntimeSource::Fixed(Some(Arc::clone(&rt) as SharedRuntime)),
        |_| {},
    );
    assert_eq!(check_runs(&rt) - before, 1);
    assert!(has(&session, "omega") && !has(&session, "alpha"));
    // Re-keyed relative to the spec root.
    assert_eq!(session.buffer("a.spec").unwrap().version, Some(4));
}

#[specforge_test(
    behavior = "hold_editor_buffers",
    verify = "a held buffer that a reload brings into the project is built from its text"
)]
fn a_held_buffer_a_reload_brings_into_the_project_is_built_from_its_text() {
    let dir = TestProject::new()
        .config(|c| {
            c["name"] = "p".into();
            c["spec_root"] = "spec".into();
        })
        .file("spec2/b.spec", ZETA);
    fs::create_dir_all(dir.root().join("spec")).unwrap();
    let mut session = dir.session_bare();

    session.hold(&[Buffer::new(dir.root().join("spec2/b.spec"), OMEGA)]);
    assert!(!has(&session, "omega"), "outside the spec root");

    dir.write(
        "specforge.json",
        serde_json::json!({"name": "p", "version": "0.1.0", "extensions": [], "spec_root": "spec2"}).to_string(),
    );
    session.reload_environment();
    assert!(has(&session, "omega") && !has(&session, "zeta"));
}

#[specforge_test(
    behavior = "hold_editor_buffers",
    verify = "a held buffer's file changed or deleted on disk is not stale"
)]
fn the_session_classifies_a_held_file_as_nothing() {
    let rt = counting();
    let (dir, mut session) = opened(&rt);
    hold(&mut session, &dir, OMEGA);
    // A file as the session read it changes nothing: write the config again.
    let root = std::fs::canonicalize(dir.root()).unwrap();
    let config = std::fs::read_to_string(root.join("specforge.json")).unwrap();
    std::fs::write(root.join("specforge.json"), format!("{config}\n")).unwrap();
    std::fs::write(root.join("a.spec"), "type other \"O\" {}\n").unwrap();
    let paths = [root.join("a.spec"), root.join("specforge.json")];
    let changes = session.changes(paths.iter().map(PathBuf::as_path));
    assert!(changes.sources().is_empty() && changes.check_inputs().is_empty());
    assert_eq!(changes.environment(), [root.join("specforge.json")]);
}
