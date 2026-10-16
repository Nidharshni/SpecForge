//! How operations fail and judge (plan 02, ADR 0029): an unusable test
//! report is one `OpError` on every view, a format run's verdict inputs,
//! the project a migration compiles. The unlinked tests were pinned before
//! ops decided any of it; the ticket that changed a pinned fact flipped it.
//! The plan check reads no report (plan.rs `the_plan_check_reads_no_recorded_report`).

use crate::view_support::{Assembled, Viewed, registries};
use specforge_ops::analysis::analyze::{AnalyzeError, AnalyzeOptions, ReportSource, analyze};
use specforge_ops::authoring::format::{Mode, Request, run};
use specforge_ops::{OpError, OpErrorKind};
use specforge_test::prelude::*;
use std::path::Path;

const SOURCE: &str = "behavior b \"B\" {\n  verify unit \"it works\"\n}\n";

fn project() -> Assembled {
    Assembled::over(registries(&["behavior"], &[])).source(SOURCE)
}

fn write_report(project: &Assembled, text: &str) {
    std::fs::write(project.root().join("specforge-report.json"), text).unwrap();
}

/// Make the report a directory: reading it fails with something that is
/// neither "not found" nor "permission denied".
fn report_is_a_directory(project: &Assembled) {
    std::fs::create_dir(project.root().join("specforge-report.json")).unwrap();
}

/// Make the report unreadable (`chmod 000`); `false` when the OS still lets
/// this process read it (running as root).
#[cfg(unix)]
fn lock_report(project: &Assembled) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let path = project.root().join("specforge-report.json");
    std::fs::write(&path, r#"{"results":{}}"#).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    std::fs::read(&path).is_err()
}

/// What every view that reads the recorded report answers for it.
fn every_view(project: &Assembled) -> Vec<(&'static str, OpError)> {
    let view = project.view();
    vec![
        ("test_report", view.test_report().unwrap_err()),
        (
            "stats",
            specforge_ops::read::stats::stats(&view).unwrap_err(),
        ),
        (
            "coverage",
            specforge_ops::read::coverage::coverage(&view, &Default::default()).unwrap_err(),
        ),
        (
            "row",
            specforge_ops::read::coverage::row(&view, "b").unwrap_err(),
        ),
        (
            "inspect",
            specforge_ops::read::inspect::coverage(&view, "b").unwrap_err(),
        ),
        (
            "analyze",
            match analyze(&view, &AnalyzeOptions::default()).unwrap_err() {
                AnalyzeError::UnusableReport(error) => error,
                other => panic!("expected the report's failure: {other:?}"),
            },
        ),
    ]
}

#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "an unusable report is the same failure, of the kind the operation decides, on every view"
)]
fn an_unusable_report_is_one_failure_on_every_view() {
    let malformed = project();
    write_report(&malformed, "{");
    let directory = project();
    report_is_a_directory(&directory);
    let mut cases = vec![
        (malformed, OpErrorKind::SchemaMismatch),
        (directory, OpErrorKind::Internal),
    ];
    #[cfg(unix)]
    {
        let locked = project();
        if lock_report(&locked) {
            cases.push((locked, OpErrorKind::PermissionDenied));
        }
    }

    for (project, kind) in cases {
        let views = every_view(&project);
        let (_, first) = &views[0];
        assert_eq!((first.kind, first.code.as_ref()), (kind, "E045"));
        for (view, error) in &views {
            assert_eq!(error, first, "{view} fails as every other view does");
        }
    }
}

#[test]
fn an_unparsable_report_fails_every_view() {
    let project = project();
    write_report(&project, "{");

    for (view, error) in every_view(&project) {
        assert_eq!(error.kind, OpErrorKind::SchemaMismatch, "{view}");
        assert_eq!(error.code, "E045", "{view}");
        assert!(
            error.message.starts_with("invalid test results"),
            "{view}: {}",
            error.message
        );
    }
}

#[test]
fn an_unreadable_report_is_internal() {
    let project = project();
    report_is_a_directory(&project);

    let error = project.view().test_report().unwrap_err();

    assert_eq!(
        (error.kind, error.code.as_ref()),
        (OpErrorKind::Internal, "E045")
    );
    assert!(error.message.starts_with("cannot read test results"));
}

#[cfg(unix)]
#[test]
fn a_locked_report_is_permission_denied() {
    let project = project();
    if !lock_report(&project) {
        return;
    }

    let error = project.view().test_report().unwrap_err();

    assert_eq!(
        (error.kind, error.code.as_ref()),
        (OpErrorKind::PermissionDenied, "E045")
    );
}

#[test]
fn analyze_calls_a_missing_named_report_file_not_found() {
    let project = project();
    let options = AnalyzeOptions {
        report: ReportSource::Named {
            path: project.root().join("none.json"),
            containment: specforge_ops::Containment::unbounded(),
        },
        ..Default::default()
    };

    let error = analyze(&project.view(), &options).unwrap_err();

    let AnalyzeError::UnusableTestResults(error) = error else {
        panic!("expected an unusable report: {error:?}");
    };
    assert_eq!(error.kind, OpErrorKind::FileNotFound);
    assert_eq!(error.code, "E045");
}

#[specforge_test(
    behavior = "invoke_extension_migration_hooks",
    verify = "hooks run for the project a sub-path is in"
)]
fn hooks_run_for_the_project_a_sub_path_is_in() {
    use specforge_extension_sdk::{ContributionsBuilder, ExtensionMeta};
    use specforge_ops::authoring::migrate::{Request, run};
    use specforge_parser::CURRENT_FORMAT_VERSION;
    use specforge_wasm::testing::InProcessRuntime;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("specforge.json"),
        r#"{"name": "p", "version": "0.1.0", "spec_root": "spec", "extensions": ["@t/x"]}"#,
    )
    .unwrap();
    specforge_installed::testing::install(root, &["@t/x"]);
    std::fs::create_dir(root.join("spec")).unwrap();
    std::fs::write(
        root.join("spec/old.spec"),
        "// specforge-format: 0.1\nbehavior old_one \"Old\" {\n}\n",
    )
    .unwrap();
    let runtime: specforge_project::SharedRuntime =
        std::sync::Arc::new(InProcessRuntime::new().with(|| {
            let mut c = ContributionsBuilder::new(ExtensionMeta::new("@t/x", "1.0.0"));
            c.migration_hook_handler("migrate_x", |_| Ok(()));
            c
        }));
    let declared = |at: &Path| {
        // Start from a fresh copy of the old file each time.
        std::fs::write(
            root.join("spec/old.spec"),
            "// specforge-format: 0.1\nbehavior old_one \"Old\" {\n}\n",
        )
        .unwrap();
        run(
            &Request {
                root: &specforge_common::ProjectRoot::of(at).unwrap(),
                target: CURRENT_FORMAT_VERSION,
                dry_run: false,
                no_backup: true,
                containment: &specforge_ops::Containment::unbounded(),
            },
            Some(runtime.clone()),
        )
        .unwrap()
        .hooks_invoked
    };

    let from_the_sub_path = declared(&root.join("spec"));
    let from_the_project = declared(root);

    assert_eq!(from_the_sub_path, ["@t/x:migrate_x"]);
    assert_eq!(from_the_project, ["@t/x:migrate_x"]);
}

#[test]
fn format_verdict_inputs() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("specforge.json"), "{}").unwrap();
    std::fs::write(
        dir.path().join("a.spec"),
        "behavior a \"A\" {\ncontract \"The system MUST work\"\n}\n",
    )
    .unwrap();

    let checked = run(&Request {
        root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        paths: &[],
        mode: Mode::Check,
        containment: &specforge_ops::Containment::unbounded(),
    })
    .unwrap();
    assert!(checked.succeeded() && checked.complete() && !checked.clean());
    assert_eq!(checked.changes.len(), 1);
    assert!(!checked.ok(), "a check that finds a change fails the run");

    let written = run(&Request {
        root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        paths: &[],
        mode: Mode::Write,
        containment: &specforge_ops::Containment::unbounded(),
    })
    .unwrap();
    assert!(written.succeeded() && written.complete() && !written.clean());
    assert!(written.changes.iter().all(|c| c.written));
    assert!(written.ok(), "a write that changed a file passes the run");
}
