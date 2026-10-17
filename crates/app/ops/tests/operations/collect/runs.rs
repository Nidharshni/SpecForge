//! collect's flow past consent, through a scripted machine: approve, run, read, dispatch, merge.

use super::{Fixture, Runner, paths};
use specforge_ops::analysis::collect::{Mode, RunnerOutput};
use specforge_ops::asking::Asking;
use specforge_ops::asking::testing::{Asked, Scripted};
use specforge_ops::testing::{ScriptedMachine, ScriptedRun};
use specforge_test_macros::test as specforge_test;

/// A run that writes the runner's report file and exits 0.
fn reports() -> ScriptedRun {
    ScriptedRun {
        files: vec![("".into(), "{}".to_string())],
        exit_code: Some(0),
        ..ScriptedRun::default()
    }
}

fn acme() -> Fixture {
    Fixture::new(vec![Runner::new("acme").proving("a", "proves_a")])
}

#[test]
fn an_approved_command_runs_once_and_its_report_is_merged() {
    let fx = acme();
    let machine = ScriptedMachine::new()
        .approving(fx.approval("acme"))
        .answering(reports());

    let outcome = fx.run(Asking::Nobody, &machine).unwrap();

    let runs = machine.runs();
    assert_eq!(runs.len(), 1, "{runs:?}");
    let report = fx.report_of("acme");
    assert_eq!(
        runs[0].argv,
        ["acme".to_string(), format!("--out={}", report.display())]
    );
    assert_eq!(runs[0].dir, fx.root());
    assert_eq!(runs[0].report, report);
    assert_eq!(runs[0].output, RunnerOutput::Discard);
    let runner = &outcome.runners[0];
    assert!(runner.ran);
    assert_eq!(runner.exit_code, Some(0));
    assert_eq!(runner.files, 1);
    let written = fx.report().unwrap();
    assert!(written["results"]["a"].is_object(), "{written}");
}

#[specforge_test(
    behavior = "run_collector_command",
    verify = "command line expands the report placeholder"
)]
fn the_command_line_expands_the_report_placeholder() {
    let fx = acme();
    let machine = ScriptedMachine::new()
        .approving_every_command()
        .answering(reports());

    fx.run(Asking::Nobody, &machine).unwrap();

    let argv = &machine.runs()[0].argv;
    assert_eq!(
        argv[1],
        format!("--out={}", fx.root().join("out/acme.json").display())
    );
    assert!(!argv.iter().any(|a| a.contains("{report}")), "{argv:?}");
}

#[specforge_test(
    behavior = "auto_detect_collector",
    verify = "file pattern match selects collector"
)]
fn a_collector_whose_detection_file_is_present_is_selected() {
    let fx = Fixture::new(vec![
        Runner::new("first").proving("a", "t"),
        Runner::new("second").proving("b", "t"),
    ]);
    fx.write("first.cfg", "");
    let machine = ScriptedMachine::new()
        .approving_every_command()
        .answering(reports());

    fx.run(Asking::Nobody, &machine).unwrap();

    let runs = machine.runs();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].argv[0], "first");
}

#[specforge_test(
    behavior = "auto_detect_collector",
    verify = "a single enabled collector is used without detection"
)]
fn a_single_enabled_runner_needs_no_detection() {
    let fx = acme();
    assert!(!fx.exists("acme.cfg"));
    let machine = ScriptedMachine::new()
        .approving_every_command()
        .answering(reports());

    fx.run(Asking::Nobody, &machine).unwrap();

    assert_eq!(machine.runs().len(), 1);
}

#[test]
fn yes_runs_the_command_and_remembers_nothing() {
    let fx = acme();
    let machine = ScriptedMachine::new().answering(reports());

    fx.run(Asking::Yes, &machine).unwrap();

    assert_eq!(machine.runs().len(), 1);
    assert!(machine.approvals().is_empty());
}

#[test]
fn an_asker_is_asked_once_and_its_yes_is_remembered() {
    let fx = acme();
    let asker = Scripted::answering(true);
    let machine = ScriptedMachine::new()
        .answering(reports())
        .answering(reports());

    fx.run(Asking::Ask(&asker), &machine).unwrap();

    let asked = asker.asked();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert!(matches!(&asked[0], Asked::RunCommand { extension, .. } if extension == "@acme/acme"));
    assert_eq!(machine.approvals(), [fx.approval("acme")]);
    // The second collect finds the approval and asks nobody.
    fx.run(Asking::Ask(&asker), &machine).unwrap();
    assert_eq!(asker.asked().len(), 1);
    assert_eq!(machine.runs().len(), 2);
}

#[test]
fn an_approval_that_cannot_be_remembered_is_an_unsaved_approval() {
    let fx = acme();
    let asker = Scripted::answering(true);
    let machine = ScriptedMachine::new()
        .unable_to_remember("disk full")
        .answering(reports());

    let outcome = fx.run(Asking::Ask(&asker), &machine).unwrap();

    assert_eq!(machine.runs().len(), 1, "the command still runs");
    assert_eq!(outcome.unsaved_approvals, ["disk full"]);
}

#[specforge_test(
    behavior = "run_collector_command",
    verify = "run ignores stale reports and sets the report env"
)]
fn a_report_file_left_by_an_earlier_run_is_never_read() {
    // A report file is removed before the command starts: a run that writes none has no report.
    let fx = acme();
    fx.write("out/acme.json", "stale");
    let machine = ScriptedMachine::new()
        .approving_every_command()
        .answering(ScriptedRun {
            exit_code: Some(101),
            ..ScriptedRun::default()
        });
    let refusal = fx.run(Asking::Nobody, &machine).unwrap_err();
    assert_eq!(refusal.error.code, "E045");
    assert!(
        refusal.error.message.contains("produced no report"),
        "{refusal:?}"
    );
    assert!(!fx.exists("out/acme.json"), "the old report is gone");

    // A report directory keeps its other files, and only those written since the start are read.
    let fx = Fixture::new(vec![Runner::new("acme").report("out").proving("a", "t")]);
    fx.write("out/old.json", "{}");
    let earlier = std::fs::File::options()
        .write(true)
        .open(fx.root().join("out/old.json"))
        .unwrap();
    earlier
        .set_modified(ScriptedMachine::started() - std::time::Duration::from_secs(1))
        .unwrap();
    let machine = ScriptedMachine::new()
        .approving_every_command()
        .answering(ScriptedRun {
            files: vec![("new.json".into(), "{}".to_string())],
            exit_code: Some(0),
            ..ScriptedRun::default()
        });
    fx.run(Asking::Nobody, &machine).unwrap();
    assert!(fx.exists("out/old.json"));
    assert_eq!(paths(&fx.dispatched()[0]), ["out/new.json"]);
}

#[specforge_test(
    behavior = "dispatch_collector",
    verify = "reads a file or every json file in a directory"
)]
fn reads_a_file_or_every_json_file_in_a_directory() {
    let fx = Fixture::new(vec![Runner::new("acme").report("out").proving("a", "t")]);
    fx.write("out/b.json", "{}");
    fx.write("out/a.json", "[]");
    fx.write("out/notes.txt", "x");
    let machine = ScriptedMachine::new();

    fx.no_run(None, &machine).unwrap();

    assert_eq!(paths(&fx.dispatched()[0]), ["out/a.json", "out/b.json"]);
}

#[test]
fn a_failing_run_with_a_report_is_recorded() {
    let fx = acme();
    let machine = ScriptedMachine::new()
        .approving_every_command()
        .answering(ScriptedRun {
            exit_code: Some(101),
            ..reports()
        });

    let outcome = fx.run(Asking::Nobody, &machine).unwrap();

    assert_eq!(outcome.runners[0].exit_code, Some(101));
    assert!(fx.report().is_some());
}

#[test]
fn a_captured_stdout_reaches_the_export_and_is_kept() {
    let fx = Fixture::new(vec![
        Runner::new("acme")
            .report("out/r.json")
            .capturing_stdout()
            .proving("a", "t"),
    ]);
    let machine = ScriptedMachine::new()
        .approving_every_command()
        .answering(ScriptedRun {
            stdout: Some("t ... ok\n".to_string()),
            exit_code: Some(0),
            ..ScriptedRun::default()
        });

    fx.run(Asking::Nobody, &machine).unwrap();

    assert_eq!(fx.dispatched()[0].input["stdout"], "t ... ok\n");
    assert_eq!(fx.read("out/r.json.stdout.txt"), "t ... ok\n");
    // `--no-run` reads the captured output back.
    fx.no_run(None, &machine).unwrap();
    assert_eq!(fx.dispatched()[1].input["stdout"], "t ... ok\n");
}

#[test]
fn no_run_reads_the_existing_report_and_runs_nothing() {
    let fx = acme();
    fx.write("out/acme.json", "{}");
    let machine = ScriptedMachine::new();

    let outcome = fx.no_run(None, &machine).unwrap();

    assert!(machine.runs().is_empty());
    assert!(!outcome.runners[0].ran);
    assert_eq!(outcome.runners[0].exit_code, None);
    assert!(fx.report().is_some());
}

#[test]
fn reports_mode_reads_the_named_files_and_runs_nothing() {
    let fx = acme();
    fx.write("elsewhere/mine.json", "{}");
    let files = [fx.root().join("elsewhere/mine.json")];
    let machine = ScriptedMachine::new();

    let outcome = fx
        .collect_with(
            Some("acme"),
            Mode::Reports(&files),
            &specforge_ops::Containment::unbounded(),
            Asking::Nobody,
            &machine,
        )
        .unwrap();

    assert!(machine.runs().is_empty());
    assert!(!outcome.runners[0].ran);
    assert_eq!(paths(&fx.dispatched()[0]), ["elsewhere/mine.json"]);
}

#[specforge_test(
    behavior = "run_collector_command",
    verify = "Run Collector Command: collector command execution holds — command_approved, report_inside_project, stale_report_ignored, report_path_exported, failing_tests_recorded, refused_before_running"
)]
fn the_run_and_approval_contracts_hold() {
    // command_approved: an unapproved command with nobody to ask runs nothing (E059).
    let fx = acme();
    let machine = ScriptedMachine::new();
    let refusal = fx.run(Asking::Nobody, &machine).unwrap_err();
    assert_eq!(refusal.error.code, "E059");
    assert!(machine.runs().is_empty());

    // report_inside_project: a report declared outside is E058 and runs nothing.
    let outside = Fixture::new(vec![Runner::new("acme").report("../x.json")]);
    let machine = ScriptedMachine::new().approving_every_command();
    let refusal = outside.run(Asking::Nobody, &machine).unwrap_err();
    assert_eq!(refusal.error.code, "E058");
    assert!(machine.runs().is_empty());

    // stale_report_ignored, report_path_exported, failing_tests_recorded.
    let fx = acme();
    fx.write("out/acme.json", "stale");
    let machine = ScriptedMachine::new()
        .approving_every_command()
        .answering(ScriptedRun {
            exit_code: Some(101),
            ..reports()
        });
    let outcome = fx.run(Asking::Nobody, &machine).unwrap();
    assert_eq!(machine.runs()[0].report, fx.report_of("acme"));
    assert_eq!(fx.dispatched()[0].input["reports"][0]["content"], "{}");
    assert_eq!(outcome.runners[0].exit_code, Some(101));

    // refused_before_running: with two collectors, the second's refusal runs nothing (order.rs).
    let two = Fixture::new(vec![Runner::new("first"), Runner::new("second")]);
    two.write("first.cfg", "");
    two.write("second.cfg", "");
    let machine = ScriptedMachine::new().approving(two.approval("first"));
    let refusal = two.run(Asking::Nobody, &machine).unwrap_err();
    assert_eq!(refusal.error.code, "E059");
    assert!(machine.runs().is_empty());
}

/// A hard link at the capture file's name is not written through (ADR 0144): the run creates a new file.
#[cfg(unix)]
#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a mutation writes no file through a hard link to a file outside the project"
)]
fn collect_capture_is_not_written_through_a_hard_link() {
    let fx = Fixture::new(vec![
        Runner::new("acme")
            .report("out/r.json")
            .capturing_stdout()
            .proving("a", "t"),
    ]);
    let outside = tempfile::TempDir::new().unwrap();
    let victim = outside.path().join("victim.txt");
    std::fs::write(&victim, "ORIGINAL").unwrap();
    std::fs::create_dir_all(fx.root().join("out")).unwrap();
    std::fs::hard_link(&victim, fx.root().join("out/r.json.stdout.txt")).unwrap();
    let machine = ScriptedMachine::new()
        .approving_every_command()
        .answering(ScriptedRun {
            stdout: Some("t ... ok\n".to_string()),
            exit_code: Some(0),
            ..ScriptedRun::default()
        });

    fx.run(Asking::Nobody, &machine).unwrap();

    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "ORIGINAL");
    assert_eq!(fx.read("out/r.json.stdout.txt"), "t ... ok\n");
}
