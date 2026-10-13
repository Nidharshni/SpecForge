//! The exit handler writes the binary report and nothing else: it reads no
//! graph, prints nothing and leaves no side file (ADR 0068). Runs the
//! `ignore_fixture` binary, as `ignore_test.rs` does.
mod support;
use specforge_test_macros::test as specforge_test;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The shape `specforge-graph`'s build script wrote to `target/specforge/graph.json`,
/// which the exit handler read for its coverage table.
const BUILD_SCRIPT_GRAPH: &str = r#"{"entities":[{"id":"record_test_via_drop_guard","kind":"behavior","verify":[{"kind":"unit","description":"an obligation","slug":"an_obligation"}],"testable":true}]}"#;

#[specforge_test(
    behavior = "record_test_via_drop_guard",
    verify = "the exit handler prints no coverage, even beside a graph export"
)]
fn the_exit_handler_prints_no_coverage() {
    let reports = tempfile::tempdir().unwrap();
    std::fs::write(reports.path().join("graph.json"), BUILD_SCRIPT_GRAPH).unwrap();
    let out = support::run_fixture(&["--include-ignored"], reports.path());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert_eq!(stderr, "", "the exit handler printed: {stderr}");
}

#[specforge_test(
    behavior = "record_test_via_drop_guard",
    verify = "the exit handler writes the report and nothing else"
)]
#[ignore = "fixed by T4"]
fn the_exit_handler_writes_the_report_and_nothing_else() {
    let reports = tempfile::tempdir().unwrap();
    support::run_fixture(&["--include-ignored"], reports.path());
    let names: Vec<String> = std::fs::read_dir(reports.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
    assert!(names[0].ends_with(".json"), "{names:?}");
}

/// Copy the fixture to `dir`, so it runs from a target directory the test
/// lays out, and run it from `cwd` without `SPECFORGE_REPORT`.
fn run_copied_fixture(dir: &Path, cwd: &Path) -> std::process::Output {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::create_dir_all(cwd).unwrap();
    let fixture = support::fixture();
    let exe = dir.join(fixture.file_name().unwrap());
    std::fs::copy(&fixture, &exe).unwrap();
    let out = Command::new(&exe)
        .arg("--include-ignored")
        .env_remove("SPECFORGE_REPORT")
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    out
}

/// Every file under `dir`, recursively; empty when `dir` does not exist.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return files;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(files_under(&path));
        } else {
            files.push(path);
        }
    }
    files
}

#[specforge_test(
    behavior = "record_test_via_drop_guard",
    verify = "without SPECFORGE_REPORT the report goes to the binary's cargo target directory, never the working directory"
)]
#[ignore = "fixed by T9"]
fn the_report_goes_to_the_binarys_target_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("build"); // a target directory with no `target` component
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(
        target.join("CACHEDIR.TAG"),
        "Signature: 8a477f597d28d172789f06886806bc55\n",
    )
    .unwrap();
    let project = tmp.path().join("project");
    run_copied_fixture(&target.join("debug/deps"), &project);
    assert_eq!(
        files_under(&project),
        Vec::<PathBuf>::new(),
        "wrote into the working directory"
    );
    let reports = files_under(&target.join("specforge"));
    assert_eq!(reports.len(), 1, "{reports:?}");
    assert!(
        reports[0].extension().is_some_and(|e| e == "json"),
        "{reports:?}"
    );
}

#[specforge_test(
    behavior = "record_test_via_drop_guard",
    verify = "a binary outside any cargo target directory writes no report and says so"
)]
#[ignore = "fixed by T9"]
fn a_binary_outside_a_target_directory_writes_no_report() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let loose = tmp.path().join("loose");
    let out = run_copied_fixture(&loose, &project);
    assert_eq!(
        files_under(&project),
        Vec::<PathBuf>::new(),
        "wrote into the working directory"
    );
    assert_eq!(files_under(&loose.join("specforge")), Vec::<PathBuf>::new());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("SPECFORGE_REPORT"), "{stderr}");
}
