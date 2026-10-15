use super::collector::{CAPTURE_STDOUT, wildcard};
use super::consent::load_consent;
use super::runner::REPORT_ENV;
use super::*;
use specforge_common::Severity;
use specforge_protocol_types::{CollectEntityResult, CollectOutput, TestResult, TestStatus};
use specforge_test_macros::test as specforge_test;

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

#[specforge_test(
    behavior = "auto_detect_collector",
    verify = "wildcards match within the last path segment"
)]
fn wildcard_matches_segments() {
    assert!(wildcard("vitest.config.*", "vitest.config.ts"));
    assert!(wildcard("*.json", "a.json"));
    assert!(wildcard("a*b*c", "a-b-c"));
    assert!(!wildcard("vitest.config.*", "vite.config.ts"));
    assert!(!wildcard("a*a", "a"));
}

#[specforge_test(
    behavior = "auto_detect_collector",
    verify = "file pattern match selects collector"
)]
fn detects_by_root_files() {
    let dir = tempfile::tempdir().unwrap();
    let all = vec![collector("r.json")];
    assert!(detect(&all, dir.path()).is_empty());
    std::fs::write(dir.path().join("acme.config.js"), "").unwrap();
    assert_eq!(detect(&all, dir.path()).len(), 1);
}

#[specforge_test(
    behavior = "run_collector_command",
    verify = "report path must stay inside the project"
)]
fn report_path_must_stay_inside_the_project() {
    let root = Path::new("/p");
    assert_eq!(
        report_path(&collector("out/r.json"), root).unwrap(),
        Path::new("/p/out/r.json")
    );
    assert!(report_path(&collector("../r.json"), root).is_err());
    assert!(report_path(&collector("/etc/r.json"), root).is_err());
}

#[specforge_test(
    behavior = "run_collector_command",
    verify = "command line expands the report placeholder"
)]
fn command_line_expands_the_report_placeholder() {
    let argv = command_line(&collector("r.json"), Path::new("/p/r.json"));
    assert_eq!(argv, vec!["acme", "--out=/p/r.json"]);
}

#[specforge_test(
    behavior = "approve_collector_command",
    verify = "consent is keyed by project, extension and command"
)]
fn consent_is_keyed_by_project_extension_and_command() {
    let dir = tempfile::tempdir().unwrap();
    let store = dir.path().join("consent.json");
    let root = dir.path();
    let mut c = collector("r.json");
    assert!(!is_approved(&store, &c, root));
    approve(&store, &c, root).unwrap();
    assert!(is_approved(&store, &c, root));
    assert!(!is_approved(&store, &c, &root.join("other")));
    c.run.push("--extra".into());
    assert!(
        !is_approved(&store, &c, root),
        "a changed command re-prompts"
    );
    approve(&store, &c, root).unwrap();
    assert_eq!(load_consent(&store).approved.len(), 1, "approval replaced");
}

#[test]
fn consent_yes_approved_and_prompt_decide_as_documented() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = dir.path().join("consent.json");
    let c = collector("r.json");
    let argv = c.run.clone();
    let mut unsaved = Vec::new();

    // --yes runs it and remembers nothing.
    assert!(Consent::Yes.allows(&store, &c, &argv, dir.path(), &mut unsaved));
    assert!(!is_approved(&store, &c, dir.path()));
    // Nothing approved yet: refused without asking.
    assert!(!Consent::Approved.allows(&store, &c, &argv, dir.path(), &mut unsaved));
    // A prompt that says no is not remembered...
    let mut no = |_: &Collector, _: &[String]| false;
    assert!(!Consent::Prompt(&mut no).allows(&store, &c, &argv, dir.path(), &mut unsaved));
    assert!(!is_approved(&store, &c, dir.path()));
    // ...a yes is, and is not asked again.
    let mut asked = 0;
    let mut yes = |_: &Collector, _: &[String]| {
        asked += 1;
        true
    };
    {
        let mut prompt = Consent::Prompt(&mut yes);
        assert!(prompt.allows(&store, &c, &argv, dir.path(), &mut unsaved));
        assert!(prompt.allows(&store, &c, &argv, dir.path(), &mut unsaved));
    }
    assert_eq!(asked, 1);
    assert!(Consent::Approved.allows(&store, &c, &argv, dir.path(), &mut unsaved));
    assert!(unsaved.is_empty(), "{unsaved:?}");
}

#[specforge_test(
    behavior = "dispatch_collector",
    verify = "reads a file or every json file in a directory"
)]
fn reads_a_file_or_every_json_file_in_a_directory() {
    let dir = tempfile::tempdir().unwrap();
    let reports = dir.path().join("out");
    std::fs::create_dir(&reports).unwrap();
    std::fs::write(reports.join("b.json"), "{}").unwrap();
    std::fs::write(reports.join("a.json"), "[]").unwrap();
    std::fs::write(reports.join("notes.txt"), "x").unwrap();
    let files = read_report(&reports, dir.path(), None).unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["out/a.json", "out/b.json"]);
    assert!(
        read_report(&dir.path().join("none.json"), dir.path(), None)
            .unwrap()
            .is_empty()
    );
}

#[specforge_test(
    behavior = "run_collector_command",
    verify = "run ignores stale reports and sets the report env"
)]
fn run_ignores_stale_reports_and_sets_the_report_env() {
    let dir = tempfile::tempdir().unwrap();
    let report = dir.path().join("r.json");
    std::fs::write(&report, "stale").unwrap();
    let mut c = collector("r.json");
    c.run = vec![
        "sh".into(),
        "-c".into(),
        format!("test ! -e {{report}} && printf fresh > \"${REPORT_ENV}\""),
    ];
    let ran = run(&c, dir.path(), &report, RunnerOutput::Discard).unwrap();
    assert_eq!(ran.exit_code, Some(0));
    assert_eq!(std::fs::read_to_string(&report).unwrap(), "fresh");

    // A report directory keeps its other files; only files the run
    // wrote are read.
    let out = dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    // Backdate the old files: a sleep is shorter than some file systems'
    // timestamp granularity.
    let long_ago = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
    for name in ["old.json", "graph.json"] {
        let file = std::fs::File::create(out.join(name)).unwrap();
        std::io::Write::write_all(&mut &file, b"{}").unwrap();
        file.set_modified(long_ago).unwrap();
    }
    c.run = vec![
        "sh".into(),
        "-c".into(),
        format!("printf '{{}}' > \"${REPORT_ENV}/new.json\""),
    ];
    let ran = run(&c, dir.path(), &out, RunnerOutput::Discard).unwrap();
    assert!(out.join("old.json").exists() && out.join("graph.json").exists());
    let read = read_report(&out, dir.path(), Some(ran.started)).unwrap();
    let paths: Vec<&str> = read.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["out/new.json"]);
}

#[specforge_test(
    behavior = "run_collector_command",
    verify = "a captured stdout is kept in every output mode"
)]
fn a_captured_stdout_is_kept_in_every_output_mode() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    let mut c = collector("out");
    c.run = vec!["sh".into(), "-c".into(), "echo 'test a ... ok'".into()];
    c.capture = Some(CAPTURE_STDOUT.into());
    for mode in [
        RunnerOutput::Discard,
        RunnerOutput::Stderr,
        RunnerOutput::Inherit,
    ] {
        let ran = run(&c, dir.path(), &out, mode).unwrap();
        let kept = ran.stdout.expect("captured");
        assert_eq!(kept, out.join("acme.stdout.txt"));
        assert_eq!(read_capture(&kept).as_deref(), Some("test a ... ok\n"));
    }
    // Not captured without the declaration.
    c.capture = None;
    std::fs::remove_file(out.join("acme.stdout.txt")).unwrap();
    let ran = run(&c, dir.path(), &out, RunnerOutput::Discard).unwrap();
    assert!(ran.stdout.is_none());
    assert!(!out.join("acme.stdout.txt").exists());
}

#[specforge_test(
    behavior = "run_collector_command",
    verify = "an unknown capture is refused"
)]
fn an_unknown_capture_is_refused() {
    let mut c = collector("r.json");
    assert_eq!(captures_stdout(&c), Ok(false));
    c.capture = Some("stderr".into());
    let err = captures_stdout(&c).unwrap_err();
    assert!(err.contains("unknown capture 'stderr'"), "{err}");
    // A file report keeps its capture next to it.
    assert_eq!(
        capture_path(&c, Path::new("/p/out/r.json")),
        Path::new("/p/out/r.json.stdout.txt")
    );
}

fn collected(entries: &[(&str, &str, TestStatus)]) -> CollectOutput {
    let mut by_entity: BTreeMap<&str, Vec<TestResult>> = BTreeMap::new();
    for (id, name, status) in entries {
        by_entity.entry(id).or_default().push(TestResult {
            name: name.to_string(),
            status: *status,
            verify: None,
            duration_ms: None,
        });
    }
    CollectOutput {
        entity_results: by_entity
            .into_iter()
            .map(|(id, tests)| CollectEntityResult {
                entity_id: id.to_string(),
                test_results: tests,
            })
            .collect(),
        unlinked: Vec::new(),
    }
}

#[specforge_test(
    behavior = "ingest_collector_report",
    verify = "the entities results may name are the entity snapshot's, with their obligation texts"
)]
fn known_entities_are_the_snapshots_records() {
    use specforge_common::{SourceSpan, Sym};
    use specforge_parser::{EntityId, EntityKind, FieldMap, VerifyStatement};

    let mut fixture = crate::view::testing::Fixture::new();
    let fields = FieldMap::new();
    fixture.graph.add_node(specforge_graph::Node {
        id: EntityId {
            raw: Sym::new("widget"),
        },
        kind: EntityKind {
            raw: Sym::new("behavior"),
        },
        title: None,
        fields,
        source_span: SourceSpan {
            file: Sym::new("t.spec"),
            start_line: 1,
            start_col: 1,
            end_line: 1,
            end_col: 1,
        },
        methods: Vec::new(),
        obligations: vec![
            VerifyStatement {
                kind: "unit".into(),
                description: "first".into(),
            },
            VerifyStatement {
                kind: "integration".into(),
                description: "second".into(),
            },
        ],
    });
    let view = fixture.view();

    let known = KnownEntities::of(view.entities());

    assert!(known.contains("widget"));
    assert!(!known.contains("gadget"));
    assert_eq!(known.obligations("widget"), ["first", "second"]);
}

#[specforge_test(
    behavior = "ingest_collector_report",
    verify = "merge replaces only the same runner"
)]
fn merge_replaces_only_the_same_runner() {
    let known = KnownEntities::from_iter(["a", "b"].map(|s| (s.to_string(), Vec::new())));
    let mut report = TestReport {
        runner: None,
        results: BTreeMap::new(),
    };
    merge(
        &mut report,
        "vitest",
        &collected(&[("a", "t1", TestStatus::Passed)]),
        &known,
    );
    let (stats, diags) = merge(
        &mut report,
        "cargo-test",
        &collected(&[
            ("a", "t2", TestStatus::Failed),
            ("b", "t3", TestStatus::Skipped),
            ("zz", "t4", TestStatus::Passed),
        ]),
        &known,
    );
    assert_eq!(
        stats,
        MergeStats {
            entities: 1,
            passed: 0,
            failed: 1,
            skipped: 1
        }
    );
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, "W115");
    assert_eq!(report.results["a"].tests.len(), 2);
    assert!(!report.results.contains_key("b"));
    assert_eq!(report.runner.as_deref(), Some("cargo-test, vitest"));

    // Re-collecting cargo-test drops its old results, keeps vitest's.
    merge(&mut report, "cargo-test", &collected(&[]), &known);
    assert_eq!(report.results["a"].tests.len(), 1);
    assert_eq!(
        report.results["a"].tests[0].result.status,
        TestStatus::Passed
    );
    assert_eq!(report.runner.as_deref(), Some("vitest"));
}

fn empty_report() -> TestReport {
    TestReport {
        runner: None,
        results: BTreeMap::new(),
    }
}

#[specforge_test(
    behavior = "ingest_collector_report",
    verify = "the recorded test report speaks the collect vocabulary, and a status outside it is a report that cannot be used"
)]
fn the_report_collect_writes() {
    let known = KnownEntities::from_iter([("a".to_string(), Vec::new())]);
    let mut report = empty_report();
    merge(
        &mut report,
        "vitest",
        &collected(&[("a", "t1", TestStatus::Passed)]),
        &known,
    );
    assert_eq!(
        serde_json::to_value(&report).unwrap()["results"]["a"],
        serde_json::json!({"tests": [
            {"name": "t1", "status": "passed", "runner": "vitest"}
        ]})
    );
}

#[specforge_test(
    invariant = "collector_output_conformance",
    verify = "unknown entity ID in collector entry produces W115"
)]
fn unknown_entities_are_dropped_with_w115() {
    let known = KnownEntities::from_iter([("a".to_string(), Vec::new())]);
    let mut report = empty_report();
    let (_, diags) = merge(
        &mut report,
        "r",
        &collected(&[("zz", "t", TestStatus::Passed)]),
        &known,
    );
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, "W115");
    assert_eq!(diags[0].severity, Severity::Warning);
    assert!(report.results.is_empty());
}

#[specforge_test(
    invariant = "collector_output_conformance",
    verify = "skipped tests are not recorded as proof"
)]
fn skipped_tests_are_not_recorded() {
    let known = KnownEntities::from_iter([("a".to_string(), Vec::new())]);
    let mut report = empty_report();
    let (stats, _) = merge(
        &mut report,
        "r",
        &collected(&[("a", "t", TestStatus::Skipped)]),
        &known,
    );
    assert_eq!(stats.skipped, 1);
    assert!(!report.results.contains_key("a"));
}

#[test]
fn collect_without_a_root_is_no_project() {
    let fixture = crate::view::testing::Fixture::new();

    let error = collect(
        &fixture.rootless_view(),
        Request {
            runner: None,
            mode: Mode::NoRun,
            consent: Consent::Approved,
            announce: &mut |_, _| {},
        },
    )
    .unwrap_err();

    assert_eq!(error.error.code, "no_project");
    assert!(
        !fixture.dir.path().join("specforge-report.json").exists(),
        "nothing is written"
    );
}

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
)]
fn an_unknown_runner_is_invalid_input_about_it() {
    let dir = tempfile::tempdir().unwrap();
    let available = vec![collector("r.json")];

    let refusal = select(&available, Some("nope"), dir.path()).unwrap_err();
    assert_eq!(refusal.error.kind, OpErrorKind::InvalidInput);
    assert_eq!(refusal.error.code, "E058");
    assert_eq!(refusal.subject, Some(CollectField::Runner));
    // The collector's own name and its extension's both answer.
    assert!(select(&available, Some("acme"), dir.path()).is_ok());
    assert!(select(&available, Some("@acme/runner"), dir.path()).is_ok());

    // No collector at all, or none detected: a refusal of the project.
    let refusal = select(&[], Some("nope"), dir.path()).unwrap_err();
    assert_eq!(refusal.error.kind, OpErrorKind::PreconditionFailed);
    assert_eq!(refusal.subject, None);
    let several = [collector("a.json"), collector("b.json")];
    let refusal = select(&several, None, dir.path());
    assert_eq!(refusal.unwrap_err().subject, None);
}
