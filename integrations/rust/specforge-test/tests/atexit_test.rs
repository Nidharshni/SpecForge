/// Verifies the report write pipeline that atexit invokes.
#[test]
fn atexit_writes_report_on_process_exit() {
    let dir = tempfile::tempdir().unwrap();
    let report_dir = dir.path().join("specforge");
    use specforge_test::registry::{TestOutcome, TestRecordEntry};
    use specforge_test::report;

    let entries = vec![TestRecordEntry {
        entity_kind: "behavior".to_string(),
        entity_id: "test_entity".to_string(),
        test_name: "test_fn".to_string(),
        module_path: None,
        file: "test.rs".to_string(),
        verify: None,
        duration_ms: 0,
        outcome: TestOutcome::Pass,
    }];

    report::write_report(&report_dir, "test_binary", &entries).unwrap();

    let path = report_dir.join("test_binary.json");
    assert!(path.exists());

    let content: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(content["schema_version"], "1.0");
    assert_eq!(content["binary_name"], "test_binary");
    assert_eq!(content["entries"][0]["entity_id"], "test_entity");
}

#[test]
fn build_hash_is_stripped_from_report_names() {
    use specforge_test::atexit::strip_build_hash;
    assert_eq!(strip_build_hash("tests-87edb3de886b35bf"), "tests");
    assert_eq!(
        strip_build_hash("specforge_emitter-9ca163c6142f8316"),
        "specforge_emitter"
    );
    assert_eq!(strip_build_hash("my-tool"), "my-tool");
    assert_eq!(strip_build_hash("plain"), "plain");
}

#[specforge_test_macros::test(
    behavior = "record_test_via_drop_guard",
    verify = "under nextest each test writes its own report and reports of other runs are pruned"
)]
fn nextest_reports_are_per_test_and_prune_other_runs() {
    use specforge_test::atexit::{prune_superseded, report_name};
    assert_eq!(report_name("pkg--tests", None), "pkg--tests");
    assert_eq!(
        report_name(
            "pkg--tests",
            Some(("0123456789abcdef", "collect::runs_with <x>"))
        ),
        "pkg--tests--01234567--collect.runs_with__x_"
    );

    let dir = tempfile::tempdir().unwrap();
    let touch = |name: &str| std::fs::write(dir.path().join(name), "{}").unwrap();
    let names = || {
        let mut names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    };
    for name in [
        "pkg--tests.json",
        "pkg--tests--aaaaaaaa--old.json",
        "pkg--tests--bbbbbbbb--kept.json",
        "pkg--tests_more.json",
        "other--tests.json",
    ] {
        touch(name);
    }

    // A nextest process of run bbbbbbbb: the cargo test report and run
    // aaaaaaaa's reports of this target go; other targets stay.
    prune_superseded(dir.path(), "pkg--tests", Some("bbbbbbbb-rest-of-uuid"));
    assert_eq!(
        names(),
        vec![
            "other--tests.json",
            "pkg--tests--bbbbbbbb--kept.json",
            "pkg--tests_more.json"
        ]
    );

    // A cargo test run of the target replaces every nextest report of it.
    prune_superseded(dir.path(), "pkg--tests", None);
    assert_eq!(names(), vec!["other--tests.json", "pkg--tests_more.json"]);
}

/// Where a report goes without `SPECFORGE_REPORT`: the binary's cargo target
/// directory, found by the markers cargo writes at its root.
#[test]
fn the_target_directory_is_the_nearest_marked_ancestor() {
    use specforge_test::atexit::target_dir_of;

    let tmp = tempfile::tempdir().unwrap();
    let build = tmp.path().join("build");
    std::fs::create_dir_all(&build).unwrap();
    std::fs::write(
        build.join("CACHEDIR.TAG"),
        "Signature: 8a477f597d28d172789f06886806bc55\n",
    )
    .unwrap();
    assert_eq!(target_dir_of(&build.join("debug/deps/x")), Some(build));

    let other = tmp.path().join("other");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join(".rustc_info.json"), "{}").unwrap();
    assert_eq!(
        target_dir_of(&other.join("aarch64-apple-darwin/debug/deps/x")),
        Some(other)
    );

    assert_eq!(target_dir_of(&tmp.path().join("loose/x")), None);
}
