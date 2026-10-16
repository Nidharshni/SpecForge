//! The order collect decides in: a refusal that needs no run comes before any collector's command runs
//! (ADR 0125 D5; `spec/behaviors/mcp-operations.spec`: "invalid_input before any command runs").

use super::{Fixture, Runner};
use specforge_ops::analysis::collect::Mode;
use specforge_ops::analysis::collect::RunnerOutput;
use specforge_ops::asking::Asking;
use specforge_ops::asking::testing::{Asked, Scripted};
use specforge_ops::testing::{ScriptedMachine, ScriptedRun};
use specforge_ops::{Containment, OpErrorKind};

/// `first` then `second`, both detected.
fn two(second: Runner) -> Fixture {
    let fx = Fixture::new(vec![Runner::new("first").proving("a", "t"), second]);
    fx.write("first.cfg", "");
    fx.write("second.cfg", "");
    fx
}

/// A machine that would run `first` and write its report, if it were asked to.
fn first_runs() -> ScriptedMachine {
    ScriptedMachine::new().answering(ScriptedRun {
        files: vec![("".into(), "{}".to_string())],
        exit_code: Some(0),
        ..ScriptedRun::default()
    })
}

#[test]
fn no_command_runs_when_a_later_collector_is_not_approved() {
    let fx = two(Runner::new("second").proving("b", "t"));
    let approval = fx.approval("first");

    // Nobody to ask: `second` is refused, and `first` never ran.
    let machine = first_runs().approving(approval.clone());
    let refusal = fx.run(Asking::Nobody, &machine).unwrap_err();
    assert_eq!(refusal.error.code, "E059");
    assert!(refusal.error.message.contains("second"), "{refusal:?}");
    assert!(machine.runs().is_empty(), "{:?}", machine.runs());

    // An asker that says no: it is asked about `second` only, and nothing ran.
    let asker = Scripted::answering(false);
    let machine = first_runs().approving(approval);
    let refusal = fx.run(Asking::Ask(&asker), &machine).unwrap_err();
    assert_eq!(refusal.error.code, "E059");
    let asked = asker.asked();
    assert!(
        matches!(asked.as_slice(), [Asked::RunCommand { extension, .. }] if extension == "@acme/second"),
        "{asked:?}"
    );
    assert!(machine.runs().is_empty(), "{:?}", machine.runs());
}

#[test]
fn no_command_runs_when_a_later_capture_is_unknown() {
    let fx = two(Runner::new("second").capturing("stderr"));
    let machine = first_runs().approving_every_command();

    let refusal = fx.run(Asking::Nobody, &machine).unwrap_err();

    assert_eq!(refusal.error.kind, OpErrorKind::PreconditionFailed);
    assert_eq!(refusal.error.code, "E058");
    assert!(
        refusal.error.message.contains("unknown capture"),
        "{refusal:?}"
    );
    assert!(machine.runs().is_empty(), "{:?}", machine.runs());
}

#[cfg(unix)]
#[test]
fn no_command_runs_when_a_later_report_location_leaves_the_project() {
    let fx = two(Runner::new("second").report("out2/second.json"));
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), fx.root().join("out2")).unwrap();
    let machine = first_runs().approving_every_command();

    let refusal = fx
        .collect_with(
            None,
            Mode::Run(RunnerOutput::Discard),
            &Containment::within(fx.root()),
            Asking::Nobody,
            &machine,
        )
        .unwrap_err();

    assert_eq!(refusal.error.code, specforge_ops::OUTSIDE_ROOT);
    assert!(machine.runs().is_empty(), "{:?}", machine.runs());
}
