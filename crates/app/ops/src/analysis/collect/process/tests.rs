use super::*;
use crate::analysis::collect::Collector;
use specforge_test_macros::test as specforge_test;
use std::path::Path;

fn collector(report: &str) -> Collector {
    Collector {
        extension: "@acme/runner".into(),
        name: "acme".into(),
        export: "collect__acme".into(),
        detect: vec!["acme.config.*".into()],
        run: vec!["acme".into(), "--out={report}".into()],
        report: report.into(),
        capture: None,
    }
}

/// Run `argv` in `dir` with `report` as its report location.
fn run(
    argv: &[String],
    dir: &Path,
    report: &Path,
    capture: Option<&mut dyn Write>,
    output: RunnerOutput,
) -> Result<Ran, RunFailure> {
    ProcessMachine::with_store(dir.join("consent.json")).run(
        &CollectorCommand {
            argv,
            dir,
            report,
            output,
        },
        capture,
    )
}

#[specforge_test(
    port = "CollectorMachine",
    verify = "CollectorMachine contract is satisfied"
)]
fn the_process_machine_keeps_the_port_contract() {
    let dir = tempfile::tempdir().unwrap();
    // `$CARGO` is set by cargo for every test run; a binary run by hand has none.
    if let Some(cargo) = std::env::var_os("CARGO") {
        let argv = vec![cargo.to_string_lossy().into_owned(), "--version".into()];
        let mut capture = Vec::new();
        let ran = run(
            &argv,
            dir.path(),
            &dir.path().join("r.json"),
            Some(&mut capture),
            RunnerOutput::Discard,
        )
        .unwrap();
        assert_eq!(ran.exit_code, Some(0));
        assert!(String::from_utf8(capture).unwrap().starts_with("cargo "));
        assert!(ran.started <= SystemTime::now());
    }
    let missing = vec!["specforge-no-such-program-here".to_string()];
    let failure = run(
        &missing,
        dir.path(),
        &dir.path().join("r.json"),
        None,
        RunnerOutput::Discard,
    )
    .unwrap_err();
    assert!(matches!(failure, RunFailure::NotRun(_)));

    let machine = ProcessMachine::with_store(dir.path().join("consent.json"));
    let mut c = collector("r.json");
    let approval = CommandApproval::of(&c, dir.path());
    assert!(!machine.approved(&approval));
    machine.approve(&approval).unwrap();
    assert!(machine.approved(&approval));
    assert!(!machine.approved(&CommandApproval::of(&c, &dir.path().join("other"))));
    c.run.push("--extra".into());
    let changed = CommandApproval::of(&c, dir.path());
    assert!(!machine.approved(&changed), "a changed command re-prompts");
    machine.approve(&changed).unwrap();
    assert_eq!(machine.load().approved.len(), 1, "approval replaced");

    // A store whose parent is a file cannot be written.
    let file = dir.path().join("a-file");
    std::fs::write(&file, "").unwrap();
    let stuck = ProcessMachine::with_store(file.join("consent.json"));
    assert!(stuck.approve(&approval).is_err());
}

#[cfg(unix)]
#[test]
fn run_sets_the_report_env_in_the_directory_and_dates_its_files_after_the_start() {
    let dir = tempfile::tempdir().unwrap();
    let report = dir.path().join("r.json");
    let argv = vec![
        "sh".to_string(),
        "-c".into(),
        format!(
            "test \"$(pwd -P)\" = \"$(cd {} && pwd -P)\" && printf fresh > \"${REPORT_ENV}\"",
            dir.path().display()
        ),
    ];
    let ran = run(&argv, dir.path(), &report, None, RunnerOutput::Discard).unwrap();
    assert_eq!(ran.exit_code, Some(0));
    assert_eq!(std::fs::read_to_string(&report).unwrap(), "fresh");
    let written = std::fs::metadata(&report).unwrap().modified().unwrap();
    assert!(written >= ran.started, "{written:?} < {:?}", ran.started);
}

#[cfg(unix)]
#[specforge_test(
    behavior = "run_collector_command",
    verify = "a captured stdout is kept in every output mode"
)]
fn a_captured_stdout_is_kept_in_every_output_mode() {
    let dir = tempfile::tempdir().unwrap();
    let argv = vec!["sh".to_string(), "-c".into(), "echo 'test a ... ok'".into()];
    for mode in [
        RunnerOutput::Discard,
        RunnerOutput::Stderr,
        RunnerOutput::Inherit,
    ] {
        let mut capture = Vec::new();
        let ran = run(
            &argv,
            dir.path(),
            &dir.path().join("out"),
            Some(&mut capture),
            mode,
        )
        .unwrap();
        assert_eq!(ran.exit_code, Some(0));
        assert_eq!(String::from_utf8(capture).unwrap(), "test a ... ok\n");
    }
}

#[specforge_test(
    behavior = "approve_collector_command",
    verify = "consent is keyed by project, extension and command"
)]
fn consent_is_keyed_by_project_extension_and_command() {
    let dir = tempfile::tempdir().unwrap();
    let machine = ProcessMachine::with_store(dir.path().join("consent.json"));
    let root = dir.path();
    let mut c = collector("r.json");
    let approval = CommandApproval::of(&c, root);
    assert!(!machine.approved(&approval));
    machine.approve(&approval).unwrap();
    assert!(machine.approved(&approval));
    assert!(!machine.approved(&CommandApproval::of(&c, &root.join("other"))));
    c.run.push("--extra".into());
    let changed = CommandApproval::of(&c, root);
    assert!(!machine.approved(&changed), "a changed command re-prompts");
    machine.approve(&changed).unwrap();
    assert_eq!(machine.load().approved.len(), 1, "approval replaced");
}
