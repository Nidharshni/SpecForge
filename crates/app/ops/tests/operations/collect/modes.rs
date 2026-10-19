//! What each collect mode reads, and so what it refuses (ADR 0125 D7): a run and a no-run read the
//! collector's declared report and capture, so a bad one is refused; `--report` reads only the files it names.

use super::{Fixture, Runner, paths};
use specforge_ops::analysis::collect::{CollectField, Mode, Outcome};
use specforge_ops::asking::Asking;
use specforge_ops::testing::ScriptedMachine;
use specforge_ops::{Containment, OpErrorKind, Refusal};
use specforge_test_macros::test as specforge_test;

/// Collect `mine.json` as `--report` would, for runner `acme`, reaching `containment`.
fn given(fx: &Fixture, containment: &Containment) -> Result<Outcome, Refusal<CollectField>> {
    fx.write("mine.json", "{}");
    let files = [fx.root().join("mine.json")];
    fx.collect_with(
        Some("acme"),
        Mode::Reports(&files),
        containment,
        Asking::Nobody,
        &ScriptedMachine::new(),
    )
}

/// The named file alone was dispatched, and the report was written.
fn read_only_the_named_file(fx: &Fixture) {
    assert_eq!(paths(&fx.dispatched()[0]), ["mine.json"]);
    assert!(fx.report().is_some());
}

#[cfg(unix)]
fn link_out(fx: &Fixture) -> tempfile::TempDir {
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), fx.root().join("out2")).unwrap();
    outside
}

#[specforge_test(
    behavior = "dispatch_collector",
    verify = "--report reads only the named files, whatever the collector declares"
)]
fn reports_mode_reads_past_a_report_location_declared_outside() {
    let fx = Fixture::new(vec![
        Runner::new("acme").report("../x.json").proving("a", "t"),
    ]);

    given(&fx, &Containment::unbounded()).unwrap();

    read_only_the_named_file(&fx);
}

#[test]
fn reports_mode_reads_past_an_unknown_capture() {
    let fx = Fixture::new(vec![
        Runner::new("acme").capturing("stderr").proving("a", "t"),
    ]);

    given(&fx, &Containment::unbounded()).unwrap();

    read_only_the_named_file(&fx);
    assert!(
        fx.dispatched()[0]
            .input
            .get("stdout")
            .is_none_or(|s| s.is_null())
    );
}

#[cfg(unix)]
#[test]
fn reports_mode_reads_past_a_report_location_that_lands_outside() {
    let fx = Fixture::new(vec![
        Runner::new("acme").report("out2/a.json").proving("a", "t"),
    ]);
    let _outside = link_out(&fx);

    given(&fx, &Containment::within(fx.root())).unwrap();

    read_only_the_named_file(&fx);
}

#[test]
fn no_run_refuses_a_report_location_declared_outside() {
    let fx = Fixture::new(vec![Runner::new("acme").report("../x.json")]);
    let machine = ScriptedMachine::new();

    let refusal = fx.no_run(Some("acme"), &machine).unwrap_err();

    assert_eq!(refusal.error.kind, OpErrorKind::PreconditionFailed);
    assert_eq!(refusal.error.code, "E058");
    assert!(
        refusal.error.message.contains("outside the project"),
        "{refusal:?}"
    );
    assert!(machine.runs().is_empty());
    assert!(fx.report().is_none());
}

#[test]
fn no_run_refuses_an_unknown_capture() {
    let fx = Fixture::new(vec![Runner::new("acme").capturing("stderr")]);
    let machine = ScriptedMachine::new();

    let refusal = fx.no_run(Some("acme"), &machine).unwrap_err();

    assert_eq!(refusal.error.kind, OpErrorKind::PreconditionFailed);
    assert_eq!(refusal.error.code, "E058");
    assert!(
        refusal.error.message.contains("unknown capture 'stderr'"),
        "{refusal:?}"
    );
    assert!(machine.runs().is_empty());
}

#[cfg(unix)]
#[test]
fn no_run_refuses_a_report_location_that_lands_outside() {
    let fx = Fixture::new(vec![Runner::new("acme").report("out2/a.json")]);
    let _outside = link_out(&fx);
    let machine = ScriptedMachine::new();

    let refusal = fx
        .collect_with(
            Some("acme"),
            Mode::NoRun,
            &Containment::within(fx.root()),
            Asking::Nobody,
            &machine,
        )
        .unwrap_err();

    assert_eq!(refusal.error.kind, OpErrorKind::InvalidInput);
    assert_eq!(refusal.error.code, specforge_ops::OUTSIDE_ROOT);
}
