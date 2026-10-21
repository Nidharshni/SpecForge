use super::*;

#[test]
fn check_mode_reports_every_change_and_writes_nothing() {
    let dir = project();

    let outcome = run(&request(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Mode::Check,
    ));

    assert_eq!(outcome.checked, 2);
    assert_eq!(outcome.changes.len(), 2);
    assert!(outcome.changes.iter().all(|c| !c.written));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("spec/a.spec")).unwrap(),
        MESSY
    );
}

#[test]
fn write_mode_rewrites_and_a_second_run_is_clean() {
    let dir = project();

    let first = run(&request(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Mode::Write,
    ));
    let second = run(&request(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Mode::Write,
    ));

    assert!(first.changes.iter().all(|c| c.written));
    assert_eq!(first.written().count(), 2);
    assert_eq!(second.checked, 2);
    assert!(second.changes.is_empty());
    assert!(second.clean());
}

#[test]
fn ok_fails_only_a_check_on_a_change() {
    let dir = project();
    let ok = |mode| {
        run(&request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            mode,
        ))
        .ok()
    };

    assert!(!ok(Mode::Check), "a check that finds a change fails");
    assert!(ok(Mode::Preview), "a preview of a change passes");
    assert!(ok(Mode::Write), "a write passes, whatever it changed");
    // Everything is canonical after the write: every mode passes.
    assert!(ok(Mode::Check) && ok(Mode::Preview));

    // An unformatted region fails every mode.
    let region = project_with(
        "{}",
        &[("spec/a.spec", "behavior a \"A\" {\n  @@@ ]]\n}\n")],
    );
    for mode in [Mode::Write, Mode::Check, Mode::Preview] {
        assert!(
            !run(&request(
                &specforge_common::ProjectRoot::of(region.path()).unwrap(),
                mode
            ))
            .ok(),
            "{mode:?}"
        );
    }
}

#[test]
fn clean_needs_every_file_read_and_canonical() {
    let canonical = "behavior messy \"Messy\" {\n  contract \"The system MUST work\"\n}\n";
    // Canonical: clean.
    let dir = project_with("{}", &[("spec/a.spec", canonical)]);
    let outcome = run(&request(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Mode::Check,
    ));
    assert!(outcome.clean() && outcome.complete() && outcome.succeeded());

    // A change: not clean, though complete and succeeded.
    let dir = project_with("{}", &[("spec/a.spec", MESSY)]);
    let outcome = run(&request(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Mode::Check,
    ));
    assert!(!outcome.clean() && outcome.complete() && outcome.succeeded());

    // A region left unformatted: not complete, so not clean.
    let broken = format!("{canonical}\n}}}}}}\n");
    let dir = project_with("{}", &[("spec/a.spec", &broken)]);
    let outcome = run(&request(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Mode::Check,
    ));
    assert!(outcome.changes.is_empty(), "{:?}", outcome.changes);
    assert!(!outcome.complete() && !outcome.clean() && outcome.succeeded());

    // A file that cannot be read: a failure, so not clean.
    let dir = project_with("{}", &[("spec/a.spec", canonical)]);
    let missing = [dir.path().join("spec/missing.spec")];
    let outcome = run(&Request {
        root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        paths: &missing,
        mode: Mode::Check,
        caller: crate::testing::caller(),
        forms: crate::authoring::format::Forms::none(),
    });
    assert!(!outcome.succeeded() && !outcome.clean());
    assert!(!outcome.found_nothing());
    assert!(matches!(&outcome.failures[0], Failure::Read { path, .. } if path == &missing[0]));
    // A missing file is not found, and the run says so.
    assert_eq!(outcome.failures[0].kind(), OpErrorKind::FileNotFound);
    assert_eq!(outcome.failure_kind(), Some(OpErrorKind::FileNotFound));
    let error = outcome.failures[0].to_op_error();
    assert_eq!(
        (error.kind, &*error.code),
        (OpErrorKind::FileNotFound, crate::FILE_UNREADABLE)
    );
    assert!(error.message.starts_with("failed to read "), "{error:?}");
}

#[test]
fn a_failed_run_is_a_target_the_run_could_not_read() {
    let dir = project_with("{}", &[("spec/a.spec", MESSY)]);
    let missing = [dir.path().join("spec/missing.spec")];
    let outcome = run(&Request {
        root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        paths: &missing,
        mode: Mode::Check,
        caller: crate::testing::caller(),
        forms: crate::authoring::format::Forms::none(),
    });

    let error = outcome.failed(false).expect("a failed run");

    assert_eq!(error.kind, OpErrorKind::FileNotFound);
    assert_eq!(error.code, "format_file_failed");
    assert_eq!(error.message, outcome.document(false).message.unwrap());
    assert_eq!(
        *error.data.expect("the document"),
        serde_json::to_value(outcome.document(false)).unwrap()
    );
    assert_eq!(error.writes, outcome.writes());
    assert!(error.suggestion.is_none());
    assert!(!outcome.ok());
}

#[test]
fn a_finding_is_no_failed_run() {
    let canonical = "behavior messy \"Messy\" {\n  contract \"The system MUST work\"\n}\n";
    let broken = format!("{canonical}\n}}}}}}\n");
    let findings = [
        // An E069 the run goes around.
        (r#"{"exclude": ["["]}"#, canonical, Mode::Write),
        // A region left unformatted (W142).
        ("{}", broken.as_str(), Mode::Write),
        // A change under --check.
        ("{}", MESSY, Mode::Check),
    ];
    for (config, text, mode) in findings {
        let dir = project_with(config, &[("spec/a.spec", text)]);
        let outcome = run(&request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            mode,
        ));
        assert!(!outcome.ok(), "{config} {mode:?}");
        assert!(outcome.failed(true).is_none(), "{config} {mode:?}");
    }
}

#[cfg(unix)]
#[test]
fn written_lists_only_files_on_disk() {
    use std::os::unix::fs::PermissionsExt;
    let dir = project();
    let locked = dir.path().join("spec/a.spec");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o444)).unwrap();

    let outcome = run(&request(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Mode::Write,
    ));
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();

    let change = outcome.changes.iter().find(|c| c.path == locked).unwrap();
    assert!(!change.written);
    assert_eq!(outcome.failures.len(), 1, "{:?}", outcome.failures);
    assert!(matches!(&outcome.failures[0], Failure::Write { path, .. } if path == &locked));
    // A file the OS refused to write is permission denied.
    assert_eq!(outcome.failures[0].kind(), OpErrorKind::PermissionDenied);
    assert_eq!(outcome.failure_kind(), Some(OpErrorKind::PermissionDenied));
    assert_eq!(
        &*outcome.failures[0].to_op_error().code,
        crate::FILE_UNWRITABLE
    );
    let written: Vec<&Path> = outcome.written().collect();
    assert_eq!(written, [dir.path().join("spec/b.spec").as_path()]);
}

#[cfg(unix)]
#[test]
fn a_locked_or_missing_file_fails_with_the_kind_the_os_gave() {
    use std::os::unix::fs::PermissionsExt;
    let dir = project();
    let locked = dir.path().join("spec/a.spec");
    let missing = dir.path().join("spec/missing.spec");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let named = [locked.clone(), missing.clone()];
    let outcome = run(&Request {
        root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        paths: &named,
        mode: Mode::Check,
        caller: crate::testing::caller(),
        forms: crate::authoring::format::Forms::none(),
    });
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();

    let kinds: Vec<(&Path, OpErrorKind)> = outcome
        .failures
        .iter()
        .map(|f| (f.path(), f.kind()))
        .collect();
    assert_eq!(
        kinds,
        [
            (locked.as_path(), OpErrorKind::PermissionDenied),
            (missing.as_path(), OpErrorKind::FileNotFound),
        ]
    );
    // Failures of different kinds are an internal failure of the run;
    // one kind is that kind.
    assert_eq!(outcome.failure_kind(), Some(OpErrorKind::Internal));
    let only_locked = Outcome {
        failures: outcome.failures[..1].to_vec(),
        ..Outcome::default()
    };
    assert_eq!(
        only_locked.failure_kind(),
        Some(OpErrorKind::PermissionDenied)
    );
    assert_eq!(Outcome::default().failure_kind(), None);
}

#[test]
fn mode_reads_the_flags_once() {
    use Mode::{Check, Preview, Write};
    let flags = [
        // (check, diff, write) → mode
        ((false, false, None), Write),
        ((true, false, None), Check),
        ((false, true, None), Preview),
        ((true, true, None), Check),
        ((false, false, Some(false)), Preview),
        ((true, false, Some(true)), Write),
        ((false, true, Some(true)), Write),
        ((false, false, Some(true)), Write),
    ];
    for ((check, diff, write), mode) in flags {
        assert_eq!(
            Mode::of_flags(check, diff, write),
            mode,
            "{check} {diff} {write:?}"
        );
    }
}

#[specforge_test(
    behavior = "format_spec_files",
    verify = "the format document names each changed file, each failed file with its kind, and the run's verdict"
)]
fn the_document_is_the_run_both_surfaces_print() {
    let dir = project();
    let mut outcome = run(&request(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Mode::Check,
    ));

    let plain = serde_json::to_value(outcome.document(false)).unwrap();
    assert_eq!(plain["total_checked"], 2);
    assert_eq!(plain["changed_files"].as_array().unwrap().len(), 2);
    assert_eq!(plain["ok"], false, "a check that finds a change fails");
    assert_eq!(plain["all_clean"], false);
    assert_eq!(plain["dry_run"], true);
    assert_eq!(plain["diagnostics"], serde_json::json!([]));
    // No null: what the run did not do is absent.
    for absent in ["diffs", "message", "failed_files", "failures"] {
        assert!(plain.get(absent).is_none(), "{absent}");
    }

    let diffs = serde_json::to_value(outcome.document(true)).unwrap();
    let first = &diffs["diffs"][0];
    assert_eq!(first["before"], MESSY);
    assert_eq!(first["file_path"], plain["changed_files"][0]);
    assert!(first["insertions"].as_u64().unwrap() > 0);

    let locked = dir.path().join("spec/locked.spec");
    outcome.failures.push(Failure::read(
        locked.clone(),
        &std::io::Error::from(std::io::ErrorKind::PermissionDenied),
    ));
    let failed = serde_json::to_value(outcome.document(false)).unwrap();
    assert_eq!(
        failed["failed_files"],
        serde_json::json!(["spec/locked.spec"])
    );
    assert_eq!(failed["failures"][0]["file"], "spec/locked.spec");
    assert_eq!(
        failed["message"],
        "failed to read spec/locked.spec: permission denied"
    );
    assert_eq!(failed["failures"][0]["operation"], "read");
    assert_eq!(failed["failures"][0]["code"], "permission_denied");
    assert_eq!(failed["message"], failed["failures"][0]["message"]);
}

/// [`MESSY`] in canonical form.
const CANONICAL: &str = "behavior messy \"Messy\" {\n  contract \"The system MUST work\"\n}\n";

#[specforge_test(
    behavior = "discover_format_targets",
    verify = "a specforge.json that does not say which files are the project's sources refuses the run, and nothing is written"
)]
fn a_config_that_hides_the_sources_refuses_the_run() {
    for config in crate::config::testing::HIDES_SOURCES {
        for mode in [Mode::Write, Mode::Check, Mode::Preview] {
            let dir = project_with(config, &[("spec/a.spec", MESSY), ("vendor/v.spec", MESSY)]);
            let before = crate::config::testing::files_under(dir.path());

            let refused = super::super::run(&Request {
                root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                paths: &[],
                mode,
                caller: crate::testing::caller(),
                forms: crate::authoring::format::Forms::none(),
            })
            .unwrap_err();

            let read = specforge_common::read_project_config(dir.path());
            let first = read.problems.iter().find(|p| p.blocks_rewrites()).unwrap();
            assert_eq!(refused.error.code, "config_invalid", "{config}");
            assert_eq!(refused.error.message, first.to_string(), "{config}");
            assert_eq!(
                crate::config::testing::files_under(dir.path()),
                before,
                "{config} {mode:?}"
            );
        }
    }
}

#[specforge_test(
    behavior = "discover_format_targets",
    verify = "a specforge.json that does not say which files are the project's sources refuses the run, and nothing is written"
)]
fn a_nested_projects_config_refuses_the_walk_that_reaches_it() {
    let dir = project_with(
        "{}",
        &[
            ("spec/a.spec", MESSY),
            ("ws/b.spec", MESSY),
            ("ws/p/specforge.json", r#"{"spec_root": "#),
            ("ws/p/spec/p.spec", MESSY),
        ],
    );
    let before = crate::config::testing::files_under(dir.path());

    let refused = super::super::run(&Request {
        root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        paths: &[dir.path().join("ws")],
        mode: Mode::Write,
        caller: crate::testing::caller(),
        forms: crate::authoring::format::Forms::none(),
    })
    .unwrap_err();

    assert_eq!(refused.error.code, "config_invalid");
    assert!(
        refused.error.message.contains("ws/p/specforge.json"),
        "{}",
        refused.error.message
    );
    assert_eq!(crate::config::testing::files_under(dir.path()), before);
}

#[specforge_test(
    behavior = "check_formatting",
    verify = "a problem in specforge.json the run goes around is an E069 that fails the check"
)]
fn a_problem_the_run_goes_around_is_e069_and_fails_ok() {
    let config = r#"{"spec_root":"spec","exclude":["[", 5],"name":7}"#;
    let dir = project_with(
        config,
        &[("spec/a.spec", CANONICAL), ("vendor/v.spec", MESSY)],
    );
    let root = specforge_common::ProjectRoot::of(dir.path()).unwrap();

    let check = run(&Request {
        root: &root,
        paths: &[],
        mode: Mode::Check,
        caller: crate::testing::caller(),
        forms: crate::authoring::format::Forms::none(),
    });

    let e069: Vec<_> = check
        .diagnostics
        .iter()
        .take_while(|d| d.is(codes::E069))
        .collect();
    assert_eq!(e069.len(), 3, "{:?}", check.diagnostics);
    assert!(
        e069.iter().any(|d| d.message.contains("\"name\"")),
        "{e069:?}"
    );
    assert!(!check.ok());
    assert!(check.clean(), "formatting itself is clean");

    let write = run(&Request {
        root: &root,
        paths: &[],
        mode: Mode::Write,
        caller: crate::testing::caller(),
        forms: crate::authoring::format::Forms::none(),
    });
    assert!(!write.ok());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("vendor/v.spec")).unwrap(),
        MESSY,
        "outside spec_root"
    );
}

#[specforge_test(
    behavior = "discover_format_targets",
    verify = "a named file is formatted whatever specforge.json says"
)]
fn a_named_file_reads_no_config() {
    let dir = project_with(r#"{"spec_root": "#, &[("spec/a.spec", MESSY)]);
    let file = dir.path().join("spec/a.spec");

    let outcome = super::super::run(&Request {
        root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        paths: std::slice::from_ref(&file),
        mode: Mode::Write,
        caller: crate::testing::caller(),
        forms: crate::authoring::format::Forms::none(),
    })
    .unwrap();

    assert!(outcome.configured(), "{:?}", outcome.diagnostics);
    assert_eq!(outcome.changes.len(), 1);
    assert_ne!(std::fs::read_to_string(file).unwrap(), MESSY);
}

/// A source that is a symlink leaving the bound is neither read nor
/// written, and the others are formatted (ADR 0092 D8).
#[cfg(unix)]
#[test]
fn a_symlink_that_leaves_the_bound_is_a_failure_and_nothing_is_written() {
    let dir = project_with("{}", &[("spec/a.spec", MESSY)]);
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("x.spec"), MESSY).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("x.spec"),
        dir.path().join("spec/link.spec"),
    )
    .unwrap();
    let root = specforge_common::ProjectRoot::of(dir.path()).unwrap();
    let held = crate::Containment::within(dir.path());
    let bounded = Request {
        root: &root,
        paths: &[],
        mode: Mode::Write,
        caller: crate::testing::bounded(&held),
        forms: crate::authoring::format::Forms::none(),
    };

    let bound = super::super::run(&bounded).unwrap();
    assert_eq!(bound.failures.len(), 1, "{:?}", bound.failures);
    assert_eq!(bound.failures[0].kind(), OpErrorKind::InvalidInput);
    assert!(bound.failures[0].path().ends_with("spec/link.spec"));
    assert_eq!(
        std::fs::read_to_string(outside.path().join("x.spec")).unwrap(),
        MESSY
    );
    assert_ne!(
        std::fs::read_to_string(dir.path().join("spec/a.spec")).unwrap(),
        MESSY
    );

    // Unbounded (the CLI): the user's own symlink is followed.
    let unbounded = super::super::run(&Request {
        caller: crate::testing::caller(),
        ..bounded
    })
    .unwrap();
    assert!(unbounded.failures.is_empty());
    assert_ne!(
        std::fs::read_to_string(outside.path().join("x.spec")).unwrap(),
        MESSY
    );
}

/// A path the request names that lands outside its containment refuses the run before anything is
/// read or written, about `paths`.
#[cfg(unix)]
#[test]
fn a_named_path_that_leaves_the_containment_is_refused_on_paths() {
    let dir = project_with("{}", &[("spec/a.spec", MESSY)]);
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("x.spec"), MESSY).unwrap();
    let root = specforge_common::ProjectRoot::of(dir.path()).unwrap();
    let held = crate::Containment::within(dir.path());
    let before = crate::config::testing::files_under(dir.path());

    let refused = super::super::run(&Request {
        root: &root,
        paths: &[
            std::path::PathBuf::from("spec"),
            outside.path().join("x.spec"),
        ],
        mode: Mode::Write,
        caller: crate::testing::bounded(&held),
        forms: crate::authoring::format::Forms::none(),
    })
    .unwrap_err();

    assert_eq!(refused.subject, Some(super::super::FormatField::Paths));
    assert_eq!(refused.error.code, crate::OUTSIDE_ROOT);
    assert_eq!(crate::config::testing::files_under(dir.path()), before);
    assert_eq!(
        std::fs::read_to_string(outside.path().join("x.spec")).unwrap(),
        MESSY
    );
}

/// A project (`a.spec`, `b.spec`) with `spec/c.spec` hard-linked to a messy file outside it.
#[cfg(unix)]
fn with_a_hard_linked_target() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
    let dir = project();
    let outside = tempfile::TempDir::new().unwrap();
    let victim = outside.path().join("victim.spec");
    std::fs::write(&victim, MESSY).unwrap();
    std::fs::hard_link(&victim, dir.path().join("spec/c.spec")).unwrap();
    (dir, outside, victim)
}

#[cfg(unix)]
#[specforge_test(
    behavior = "format_spec_files",
    verify = "an unbounded run formats a file with more than one name; a bounded one fails it and formats the others"
)]
fn a_bounded_run_fails_a_hard_linked_target_and_formats_the_others() {
    let (dir, _outside, victim) = with_a_hard_linked_target();
    let held = crate::Containment::within(dir.path());

    let outcome = super::super::run(&Request {
        root: &ProjectRoot::of(dir.path()).unwrap(),
        paths: &[],
        mode: Mode::Write,
        caller: crate::testing::bounded(&held),
        forms: crate::authoring::format::Forms::none(),
    })
    .unwrap();

    assert_eq!(outcome.failures.len(), 1, "{:?}", outcome.failures);
    assert!(outcome.failures[0].path().ends_with("spec/c.spec"));
    assert_eq!(outcome.failures[0].kind(), OpErrorKind::InvalidInput);
    assert!(
        outcome.failures[0].error().contains("2 names"),
        "{:?}",
        outcome.failures
    );
    assert!(outcome.failed(false).is_some());
    for (file, messy) in [
        ("spec/a.spec", MESSY.to_string()),
        ("spec/b.spec", MESSY.replace("messy", "b")),
    ] {
        assert_ne!(
            std::fs::read_to_string(dir.path().join(file)).unwrap(),
            messy
        );
    }
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), MESSY);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("spec/c.spec")).unwrap(),
        MESSY
    );
}

#[cfg(unix)]
#[specforge_test(
    behavior = "format_spec_files",
    verify = "an unbounded run formats a file with more than one name; a bounded one fails it and formats the others"
)]
fn an_unbounded_run_formats_a_hard_linked_target() {
    let (dir, _outside, victim) = with_a_hard_linked_target();

    let outcome = super::super::run(&Request {
        root: &ProjectRoot::of(dir.path()).unwrap(),
        paths: &[],
        mode: Mode::Write,
        caller: crate::testing::caller(),
        forms: crate::authoring::format::Forms::none(),
    })
    .unwrap();

    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
    assert_ne!(
        std::fs::read_to_string(dir.path().join("spec/c.spec")).unwrap(),
        MESSY
    );
    // Written by name (ADR 0144): the outside file keeps its text.
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), MESSY);
}
