//! Every refusal collect makes, in process (ADR 0125): its kind, its code, what it is about, and that
//! neither a command ran nor a report was written.

use super::{Fixture, Runner};
use specforge_ops::analysis::collect::{CollectField, Mode, Outcome};
use specforge_ops::asking::Asking;
use specforge_ops::asking::testing::Scripted;
use specforge_ops::testing::{ScriptedFailure, ScriptedMachine, ScriptedRun};
use specforge_ops::{Containment, OpErrorKind, Refusal};
use specforge_test_macros::test as specforge_test;

type Refused = Refusal<CollectField>;

fn refused(result: Result<Outcome, Refused>) -> Refused {
    match result {
        Ok(outcome) => panic!("expected a refusal, collected {outcome:?}"),
        Err(refusal) => refusal,
    }
}

/// Nothing ran and nothing was recorded.
fn untouched(fx: &Fixture, machine: &ScriptedMachine) {
    assert!(machine.runs().is_empty(), "{:?}", machine.runs());
    assert!(fx.report().is_none(), "specforge-report.json was written");
}

fn acme() -> Fixture {
    Fixture::new(vec![Runner::new("acme").proving("a", "t")])
}

#[test]
fn no_enabled_collector_is_e058_naming_how_to_add_one() {
    let fx = Fixture::new(Vec::new());
    let machine = ScriptedMachine::new();

    let refusal = refused(fx.run(Asking::Yes, &machine));

    assert_eq!(refusal.error.kind, OpErrorKind::PreconditionFailed);
    assert_eq!(refusal.error.code, "E058");
    assert!(
        refusal
            .error
            .message
            .contains("specforge add @specforge/cargo-test"),
        "{refusal:?}"
    );
    untouched(&fx, &machine);
}

#[specforge_test(
    behavior = "auto_detect_collector",
    verify = "no match emits E058 with available collectors"
)]
fn no_match_among_several_collectors_is_e058_listing_them() {
    let fx = Fixture::new(vec![Runner::new("first"), Runner::new("second")]);
    let machine = ScriptedMachine::new();

    let refusal = refused(fx.run(Asking::Yes, &machine));

    assert_eq!(refusal.error.kind, OpErrorKind::PreconditionFailed);
    assert_eq!(refusal.error.code, "E058");
    assert!(
        refusal.error.message.contains("no test runner detected"),
        "{refusal:?}"
    );
    assert!(
        refusal.error.message.contains("available: first, second"),
        "{refusal:?}"
    );
    untouched(&fx, &machine);
}

#[test]
fn an_unknown_runner_is_e058_about_the_runner() {
    let fx = acme();
    let machine = ScriptedMachine::new();

    let refusal = refused(fx.no_run(Some("nope"), &machine));

    assert_eq!(refusal.error.kind, OpErrorKind::InvalidInput);
    assert_eq!(refusal.error.code, "E058");
    assert_eq!(refusal.subject, Some(CollectField::Runner));
    untouched(&fx, &machine);
}

#[test]
fn reports_mode_with_several_collectors_is_e058() {
    let fx = Fixture::new(vec![Runner::new("first"), Runner::new("second")]);
    fx.write("first.cfg", "");
    fx.write("second.cfg", "");
    let files = [fx.root().join("x.json")];
    let machine = ScriptedMachine::new();

    let refusal = refused(fx.collect_with(
        None,
        Mode::Reports(&files),
        &Containment::unbounded(),
        Asking::Nobody,
        &machine,
    ));

    assert_eq!(refusal.error.kind, OpErrorKind::PreconditionFailed);
    assert_eq!(refusal.error.code, "E058");
    assert!(
        refusal
            .error
            .message
            .contains("--report needs one collector, but first, second apply here"),
        "{refusal:?}"
    );
    untouched(&fx, &machine);
}

#[test]
fn a_report_location_declared_outside_is_e058() {
    let fx = Fixture::new(vec![Runner::new("acme").report("../x.json")]);
    let machine = ScriptedMachine::new().approving_every_command();

    let refusal = refused(fx.run(Asking::Nobody, &machine));

    assert_eq!(refusal.error.kind, OpErrorKind::PreconditionFailed);
    assert_eq!(refusal.error.code, "E058");
    assert!(
        refusal.error.message.contains("outside the project"),
        "{refusal:?}"
    );
    untouched(&fx, &machine);
}

#[specforge_test(
    behavior = "run_collector_command",
    verify = "an unknown capture is refused"
)]
fn an_unknown_capture_is_refused() {
    let fx = Fixture::new(vec![Runner::new("acme").capturing("stderr")]);
    let machine = ScriptedMachine::new().approving_every_command();

    let refusal = refused(fx.run(Asking::Nobody, &machine));

    assert_eq!(refusal.error.kind, OpErrorKind::PreconditionFailed);
    assert_eq!(refusal.error.code, "E058");
    assert!(
        refusal.error.message.contains("unknown capture 'stderr'"),
        "{refusal:?}"
    );
    untouched(&fx, &machine);
}

#[cfg(unix)]
#[test]
fn a_report_location_that_lands_outside_is_outside_root() {
    let fx = Fixture::new(vec![Runner::new("acme").report("out2/a.json")]);
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), fx.root().join("out2")).unwrap();
    let machine = ScriptedMachine::new().approving_every_command();

    let refusal = refused(fx.collect_with(
        None,
        Mode::Run(specforge_ops::analysis::collect::RunnerOutput::Discard),
        &Containment::within(fx.root()),
        Asking::Nobody,
        &machine,
    ));

    assert_eq!(refusal.error.kind, OpErrorKind::InvalidInput);
    assert_eq!(refusal.error.code, specforge_ops::OUTSIDE_ROOT);
    untouched(&fx, &machine);
}

#[test]
fn an_unapproved_command_with_nobody_to_ask_is_e059() {
    let fx = acme();
    let machine = ScriptedMachine::new();

    let refusal = refused(fx.run(Asking::Nobody, &machine));

    assert_eq!(refusal.error.kind, OpErrorKind::PermissionDenied);
    assert_eq!(refusal.error.code, "E059");
    assert!(
        refusal.error.message.starts_with("running `"),
        "{refusal:?}"
    );
    let suggestion = refusal.error.suggestion.as_deref().unwrap();
    assert!(
        suggestion.contains("`specforge collect` in a terminal"),
        "{suggestion}"
    );
    untouched(&fx, &machine);
}

#[test]
fn an_asker_that_says_no_is_e059_and_remembers_nothing() {
    let fx = acme();
    let asker = Scripted::answering(false);
    let machine = ScriptedMachine::new();

    let refusal = refused(fx.run(Asking::Ask(&asker), &machine));

    assert_eq!(refusal.error.code, "E059");
    assert_eq!(asker.asked().len(), 1);
    assert!(machine.approvals().is_empty());
    untouched(&fx, &machine);
}

#[test]
fn a_collector_that_declares_no_command_is_e045() {
    let fx = Fixture::new(vec![Runner::new("acme").run(&[])]);
    let machine = ScriptedMachine::new().approving_every_command();

    let refusal = refused(fx.run(Asking::Nobody, &machine));

    assert_eq!(refusal.error.kind, OpErrorKind::SchemaMismatch);
    assert_eq!(refusal.error.code, "E045");
    assert!(
        refusal.error.message.contains("declares no command"),
        "{refusal:?}"
    );
    untouched(&fx, &machine);
}

#[test]
fn a_command_that_cannot_start_is_e045() {
    let fx = acme();
    let machine = ScriptedMachine::new()
        .approving_every_command()
        .answering(ScriptedRun {
            fails: Some(ScriptedFailure::NotRun),
            ..ScriptedRun::default()
        });

    let refusal = refused(fx.run(Asking::Nobody, &machine));

    assert_eq!(refusal.error.kind, OpErrorKind::SchemaMismatch);
    assert_eq!(refusal.error.code, "E045");
    assert!(
        refusal.error.message.starts_with("failed to run `acme "),
        "{refusal:?}"
    );
    assert!(fx.report().is_none());
}

#[test]
fn a_capture_that_cannot_be_written_is_e045() {
    let fx = Fixture::new(vec![Runner::new("acme").capturing_stdout()]);
    let machine = ScriptedMachine::new()
        .approving_every_command()
        .answering(ScriptedRun {
            fails: Some(ScriptedFailure::Capture),
            ..ScriptedRun::default()
        });

    let refusal = refused(fx.run(Asking::Nobody, &machine));

    assert_eq!(refusal.error.code, "E045");
    assert!(
        refusal.error.message.contains("acme.json.stdout.txt"),
        "{refusal:?}"
    );
    assert!(fx.report().is_none());
}

#[test]
fn a_run_that_writes_no_report_is_e045() {
    let fx = acme();
    let machine = ScriptedMachine::new()
        .approving_every_command()
        .answering(ScriptedRun {
            exit_code: Some(101),
            ..ScriptedRun::default()
        });

    let refusal = refused(fx.run(Asking::Nobody, &machine));

    assert_eq!(refusal.error.kind, OpErrorKind::SchemaMismatch);
    assert_eq!(refusal.error.code, "E045");
    assert!(
        refusal.error.message.contains("produced no report"),
        "{refusal:?}"
    );
    assert!(fx.report().is_none());
}

#[test]
fn no_run_without_a_report_says_how_to_get_one() {
    let fx = acme();
    let machine = ScriptedMachine::new();

    let refusal = refused(fx.no_run(None, &machine));

    assert_eq!(refusal.error.code, "E045");
    assert!(
        refusal.error.message.contains("run without --no-run"),
        "{refusal:?}"
    );
    untouched(&fx, &machine);
}

#[test]
fn reports_mode_files_that_do_not_exist_are_e045() {
    let fx = acme();
    let files = [fx.root().join("missing.json")];
    let machine = ScriptedMachine::new();

    let refusal = refused(fx.collect_with(
        Some("acme"),
        Mode::Reports(&files),
        &Containment::unbounded(),
        Asking::Nobody,
        &machine,
    ));

    assert_eq!(refusal.error.code, "E045");
    assert_eq!(refusal.error.message, "the --report files don't exist");
    untouched(&fx, &machine);
}

#[test]
fn an_export_that_traps_is_an_internal_refusal_with_e028() {
    let fx = Fixture::new(vec![Runner::new("acme").trapping()]);
    fx.write("out/acme.json", "{}");
    let machine = ScriptedMachine::new();

    let refusal = refused(fx.no_run(None, &machine));

    assert_eq!(refusal.error.kind, OpErrorKind::Internal);
    assert_eq!(refusal.error.code, "E028");
    assert!(
        refusal.error.message.contains("collect__acme"),
        "{refusal:?}"
    );
    untouched(&fx, &machine);
}

#[test]
fn a_report_that_cannot_be_written_is_e056() {
    let fx = acme();
    fx.write("out/acme.json", "{}");
    std::fs::create_dir(fx.root().join("specforge-report.json")).unwrap();
    let machine = ScriptedMachine::new();

    let refusal = refused(fx.no_run(None, &machine));

    assert_eq!(refusal.error.code, "E056");
    assert!(machine.runs().is_empty());
}
