use assert_cmd::Command;
use predicates::prelude::*;
use specforge_test_macros::test as specforge_test;
use std::fs;
use tempfile::TempDir;

fn setup_project(dir: &std::path::Path) {
    fs::write(
        dir.join("specforge.json"),
        r#"{"name":"test","version":"0.1.0"}"#,
    )
    .unwrap();
    let spec_dir = dir.join("spec");
    fs::create_dir_all(&spec_dir).unwrap();
}

fn write_spec(dir: &std::path::Path, name: &str, content: &str) {
    let spec_dir = dir.join("spec");
    fs::create_dir_all(&spec_dir).unwrap();
    fs::write(spec_dir.join(name), content).unwrap();
}

// --- Slice 8: format_spec_files ---

#[specforge_test(
    behavior = "format_spec_files",
    verify = "formatting all files in spec/ directory succeeds"
)]
fn format_command_formats_files() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    write_spec(
        root,
        "test.spec",
        "behavior foo \"Foo\" {\n      contract \"does stuff\"\n}\n",
    );

    Command::cargo_bin("specforge")
        .unwrap()
        .args(["format", "--path", root.to_str().unwrap()])
        .assert()
        .success();

    let formatted = fs::read_to_string(root.join("spec/test.spec")).unwrap();
    assert_eq!(
        formatted, "behavior foo \"Foo\" {\n  contract \"does stuff\"\n}\n",
        "the file is rewritten in canonical form"
    );
}

/// The canonical form of the `behavior foo` inputs used below.
const CANONICAL_FOO: &str = "behavior foo \"Foo\" {\n  contract \"stuff\"\n}\n";

/// Run `specforge format --stdin` on `input` from `cwd`; returns stdout.
fn format_stdin_in(cwd: &std::path::Path, input: &str) -> String {
    let output = Command::cargo_bin("specforge")
        .unwrap()
        .current_dir(cwd)
        .args(["format", "--stdin"])
        .write_stdin(input)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// Every file under `dir` with its content, sorted by path.
fn snapshot_tree(dir: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                files.push((path.clone(), fs::read(&path).unwrap()));
            }
        }
    }
    files.sort();
    files
}

/// A project whose spec/ holds an unformatted file and a malformed one,
/// so any read or write of them would show in the output or on disk.
fn project_with_spec_files(root: &std::path::Path) {
    setup_project(root);
    write_spec(
        root,
        "unformatted.spec",
        "behavior other \"Other\" {\n      contract \"other\"\n}\n",
    );
    write_spec(root, "broken.spec", "behavior {{{ not valid\n");
}

#[specforge_test(
    behavior = "format_spec_files",
    verify = "files matching the canonical format are not rewritten"
)]
fn format_does_not_rewrite_unchanged_files() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    // Already formatted content
    let content = "behavior foo \"Foo\" {\n  contract \"does stuff\"\n}\n";
    write_spec(root, "test.spec", content);

    Command::cargo_bin("specforge")
        .unwrap()
        .args(["format", "--path", root.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("0 changed"));
}

#[specforge_test(
    behavior = "format_spec_files",
    verify = "changed files are printed to stdout"
)]
fn format_prints_changed_file_names() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    write_spec(
        root,
        "bad.spec",
        "behavior foo \"Foo\" {\n      contract \"stuff\"\n}\n",
    );

    Command::cargo_bin("specforge")
        .unwrap()
        .args(["format", "--path", root.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("bad.spec"));
}

// --- Slice 9: check_formatting ---

#[specforge_test(
    behavior = "check_formatting",
    verify = "already formatted files exit with code 0"
)]
fn check_already_formatted_exits_zero() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    write_spec(
        root,
        "test.spec",
        "behavior foo \"Foo\" {\n  contract \"does stuff\"\n}\n",
    );

    Command::cargo_bin("specforge")
        .unwrap()
        .args(["format", "--check", "--path", root.to_str().unwrap()])
        .assert()
        .success();
}

#[specforge_test(
    behavior = "check_formatting",
    verify = "unformatted files exit with code 1"
)]
fn check_unformatted_exits_one() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    write_spec(
        root,
        "test.spec",
        "behavior foo \"Foo\" {\n      contract \"stuff\"\n}\n",
    );

    Command::cargo_bin("specforge")
        .unwrap()
        .args(["format", "--check", "--path", root.to_str().unwrap()])
        .assert()
        .code(1);
}

#[specforge_test(
    behavior = "check_formatting",
    verify = "check mode writes no files to disk"
)]
fn check_mode_writes_no_files() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    let original = "behavior foo \"Foo\" {\n      contract \"stuff\"\n}\n";
    write_spec(root, "test.spec", original);

    Command::cargo_bin("specforge")
        .unwrap()
        .args(["format", "--check", "--path", root.to_str().unwrap()])
        .assert()
        .code(1);

    let after = fs::read_to_string(root.join("spec/test.spec")).unwrap();
    assert_eq!(after, original, "check mode should not modify files");
}

// --- Slice 10: show_formatting_diff ---

#[specforge_test(
    behavior = "show_formatting_diff",
    verify = "diff output uses unified format"
)]
fn diff_shows_unified_format() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    write_spec(
        root,
        "test.spec",
        "behavior foo \"Foo\" {\n      contract \"stuff\"\n}\n",
    );

    Command::cargo_bin("specforge")
        .unwrap()
        .args(["format", "--diff", "--path", root.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("---"))
        .stdout(predicate::str::contains("+++"))
        .stdout(predicate::str::contains("@@"));
}

#[specforge_test(
    behavior = "show_formatting_diff",
    verify = "diff mode writes no files to disk"
)]
fn diff_mode_writes_no_files() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    let original = "behavior foo \"Foo\" {\n      contract \"stuff\"\n}\n";
    write_spec(root, "test.spec", original);

    Command::cargo_bin("specforge")
        .unwrap()
        .args(["format", "--diff", "--path", root.to_str().unwrap()])
        .assert()
        .success();

    let after = fs::read_to_string(root.join("spec/test.spec")).unwrap();
    assert_eq!(after, original, "diff mode should not modify files");
}

#[specforge_test(
    behavior = "show_formatting_diff",
    verify = "unchanged files produce no diff output"
)]
fn diff_unchanged_produces_no_output() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    write_spec(
        root,
        "test.spec",
        "behavior foo \"Foo\" {\n  contract \"stuff\"\n}\n",
    );

    Command::cargo_bin("specforge")
        .unwrap()
        .args(["format", "--diff", "--path", root.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::is_empty().or(predicate::str::contains("---").not()));
}

// --- Slice 11: format_from_stdin ---

#[specforge_test(
    behavior = "format_from_stdin",
    verify = "stdin content is formatted and written to stdout"
)]
fn stdin_formats_and_writes_to_stdout() {
    Command::cargo_bin("specforge")
        .unwrap()
        .args(["format", "--stdin"])
        .write_stdin("behavior foo \"Foo\" {\n      contract \"stuff\"\n}\n")
        .assert()
        .success()
        .stdout(predicate::eq(CANONICAL_FOO));
}

#[specforge_test(
    behavior = "format_from_stdin",
    verify = "stdin mode does not read or write files"
)]
fn stdin_mode_does_not_read_files() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    project_with_spec_files(root);
    let before = snapshot_tree(root);

    let output = Command::cargo_bin("specforge")
        .unwrap()
        .current_dir(root)
        .args(["format", "--stdin"])
        .write_stdin("behavior foo \"Foo\" {\n      contract \"stuff\"\n}\n")
        .output()
        .unwrap();

    assert!(output.status.success());
    // Only stdin shows up: nothing from the project's spec files.
    assert_eq!(String::from_utf8(output.stdout).unwrap(), CANONICAL_FOO);
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "",
        "no diagnostics from the malformed spec file"
    );
    assert_eq!(snapshot_tree(root), before, "no file was written");
}

// --- Integration: formatting all files in spec/ directory ---

#[specforge_test(
    behavior = "format_spec_files",
    verify = "formatting all files in spec/ directory succeeds"
)]
fn format_integration_all_spec_files_in_directory() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);

    // Create multiple spec files in subdirectories
    let behaviors_dir = root.join("spec").join("behaviors");
    let types_dir = root.join("spec").join("types");
    fs::create_dir_all(&behaviors_dir).unwrap();
    fs::create_dir_all(&types_dir).unwrap();

    fs::write(
        behaviors_dir.join("auth.spec"),
        "behavior login \"Login\" {\n      contract \"authenticates user\"\n}\n",
    )
    .unwrap();
    fs::write(
        types_dir.join("core.spec"),
        "type user \"User\" {\n      name \"string\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("spec").join("main.spec"),
        "use \"behaviors/auth\"\nuse \"types/core\"\n",
    )
    .unwrap();

    Command::cargo_bin("specforge")
        .unwrap()
        .args(["format", "--path", root.to_str().unwrap()])
        .assert()
        .success();

    // Verify all files were formatted
    let auth = fs::read_to_string(behaviors_dir.join("auth.spec")).unwrap();
    assert_eq!(
        auth, "behavior login \"Login\" {\n  contract \"authenticates user\"\n}\n",
        "auth.spec should be formatted"
    );
    let core = fs::read_to_string(types_dir.join("core.spec")).unwrap();
    assert_eq!(
        core, "type user \"User\" {\n  name \"string\"\n}\n",
        "types/core.spec should be formatted"
    );
}

// --- Property: stdin formatting is idempotent ---

#[specforge_test(
    behavior = "format_from_stdin",
    verify = "stdin formatting is idempotent"
)]
fn stdin_formatting_is_idempotent() {
    let input = "behavior foo \"Foo\" {\n      contract   \"stuff\"\n    types [a, b]\n}\n";
    let tmp = TempDir::new().unwrap();

    let first_output = format_stdin_in(tmp.path(), input);
    assert_eq!(
        first_output, "behavior foo \"Foo\" {\n  contract \"stuff\"\n  types    [a, b]\n}\n",
        "the first pass formats"
    );

    let second_output = format_stdin_in(tmp.path(), &first_output);
    assert_eq!(
        first_output, second_output,
        "stdin formatting should be idempotent"
    );
}

// --- Property: stdin formatting converges to canonical form ---

#[specforge_test(
    behavior = "format_from_stdin",
    verify = "stdin formatting converges to canonical form"
)]
fn stdin_formatting_converges_to_canonical_form() {
    let variants = [
        "behavior foo \"Foo\" {\n      contract   \"stuff\"\n}\n",
        "behavior foo \"Foo\" {\n\tcontract \"stuff\"\n}\n",
        "behavior foo \"Foo\" {\n    contract     \"stuff\"\n}\n",
    ];

    let tmp = TempDir::new().unwrap();
    for variant in &variants {
        assert_eq!(
            format_stdin_in(tmp.path(), variant),
            CANONICAL_FOO,
            "{variant:?} should converge to the canonical form"
        );
    }
}

// --- Contract: format_spec_files ---

#[specforge_test(
    behavior = "format_spec_files",
    verify = "Format Spec Files: spec file formatting holds — spec_files_available, format_config_loaded, formatted_output_written, unchanged_files_preserved, format_complete_emitted, summary_printed"
)]
fn format_spec_files_contract_requires_ensures() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    // Requires: spec files available, format config loaded
    write_spec(
        root,
        "test.spec",
        "behavior foo \"Foo\" {\n      contract \"stuff\"\n}\n",
    );

    Command::cargo_bin("specforge")
        .unwrap()
        .args(["format", "--path", root.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("test.spec")); // ensures: summary_printed

    // ensures: formatted_output_written
    let content = fs::read_to_string(root.join("spec/test.spec")).unwrap();
    assert!(
        content.contains("  contract \"stuff\""),
        "formatted output should be written"
    );

    // ensures: unchanged_files_preserved (re-run should show 0 changed)
    Command::cargo_bin("specforge")
        .unwrap()
        .args(["format", "--path", root.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("0 changed"));
}

// --- Contract: check_formatting ---

#[specforge_test(
    behavior = "check_formatting",
    verify = "Check Formatting Without Modifying Files: formatting check holds — spec_files_available, format_config_loaded, no_files_written, exit_code_correct, unformatted_paths_printed"
)]
fn check_formatting_contract_requires_ensures() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);

    // Unformatted file
    write_spec(
        root,
        "bad.spec",
        "behavior foo \"Foo\" {\n      contract \"stuff\"\n}\n",
    );

    // ensures: no_files_written, exit_code_correct, unformatted_paths_printed
    Command::cargo_bin("specforge")
        .unwrap()
        .args(["format", "--check", "--path", root.to_str().unwrap()])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("bad.spec"));

    // Verify file was NOT modified
    let content = fs::read_to_string(root.join("spec/bad.spec")).unwrap();
    assert!(
        content.contains("      contract"),
        "check mode should not modify files"
    );
}

// --- Contract: show_formatting_diff ---

#[specforge_test(
    behavior = "show_formatting_diff",
    verify = "Show Formatting Diff: formatting diff holds — spec_files_available, format_config_loaded, no_files_written, unified_diff_produced"
)]
fn show_formatting_diff_contract_requires_ensures() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    let original = "behavior foo \"Foo\" {\n      contract \"stuff\"\n}\n";
    write_spec(root, "test.spec", original);

    // ensures: no_files_written, unified_diff_produced
    Command::cargo_bin("specforge")
        .unwrap()
        .args(["format", "--diff", "--path", root.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("---"))
        .stdout(predicate::str::contains("+++"));

    let after = fs::read_to_string(root.join("spec/test.spec")).unwrap();
    assert_eq!(after, original, "diff mode should not modify files");
}

// --- Contract: format_from_stdin ---

#[specforge_test(
    behavior = "format_from_stdin",
    verify = "Format from Standard Input: stdin formatting holds — stdin_available, format_config_loaded, stdout_produced, no_files_touched, format_complete_emitted"
)]
fn format_from_stdin_contract_requires_ensures() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    project_with_spec_files(root);
    let input = "behavior foo \"Foo\" {\n      contract \"stuff\"\n}\n";

    // stdin_available, stdout_produced, no_files_touched,
    // format_complete_emitted (a successful run ends with exit 0)
    let before = snapshot_tree(root);
    assert_eq!(format_stdin_in(root, input), CANONICAL_FOO);
    assert_eq!(snapshot_tree(root), before, "no file was written");

    // format_config_loaded: the project's .specforgefmt.toml applies.
    fs::write(root.join(".specforgefmt.toml"), "indent_width = 4\n").unwrap();
    assert_eq!(
        format_stdin_in(root, input),
        "behavior foo \"Foo\" {\n    contract \"stuff\"\n}\n",
        "stdin formatting uses the resolved FormatConfig"
    );
}

#[specforge_test(
    behavior = "load_format_config",
    verify = "a file's configuration does not depend on where format runs"
)]
fn check_from_a_subdirectory_agrees_with_the_root() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    let sub = root.join("spec/sub");
    fs::create_dir_all(&sub).unwrap();
    fs::write(sub.join(".specforgefmt.toml"), "indent_width = 4\n").unwrap();
    fs::write(
        sub.join("a.spec"),
        "behavior login \"Login\" {\n    contract \"The system MUST log in\"\n}\n",
    )
    .unwrap();

    // From the project root, and from spec/sub: the file's nearest
    // configuration decides both times.
    let (code, stderr) = check_in(root);
    assert_eq!(code, Some(0), "{stderr}");
    let output = Command::cargo_bin("specforge")
        .unwrap()
        .current_dir(&sub)
        .args(["format", "--check"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// --- Unreadable files and regions left unformatted ---

/// `specforge format --check --path <root>`: exit code and stderr.
fn check_in(root: &std::path::Path) -> (Option<i32>, String) {
    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args(["format", "--check", "--path", root.to_str().unwrap()])
        .output()
        .unwrap();
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[cfg(unix)]
#[specforge_test(
    behavior = "check_formatting",
    verify = "a file that cannot be read makes the check fail"
)]
fn check_exits_one_on_an_unreadable_file() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    write_spec(root, "ok.spec", CANONICAL_FOO);
    write_spec(root, "locked.spec", CANONICAL_FOO);
    let locked = root.join("spec/locked.spec");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    let (code, stderr) = check_in(root);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o644)).unwrap();

    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("error[file_unreadable]: failed to read"),
        "{stderr}"
    );
    assert!(stderr.contains("locked.spec"), "{stderr}");
}

/// A project whose only file is canonical, then a stray `}}}`.
fn project_with_a_kept_region(root: &std::path::Path) -> std::path::PathBuf {
    setup_project(root);
    write_spec(root, "broken.spec", &format!("{CANONICAL_FOO}\n}}}}}}\n"));
    root.join("spec/broken.spec")
}

#[specforge_test(
    behavior = "check_formatting",
    verify = "a region left unformatted makes the check fail"
)]
fn check_exits_one_on_a_region_left_unformatted() {
    let tmp = TempDir::new().unwrap();
    project_with_a_kept_region(tmp.path());

    let (code, stderr) = check_in(tmp.path());

    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("broken.spec: Parse error at lines 5-5, error region preserved verbatim"),
        "{stderr}"
    );
}

#[specforge_test(
    behavior = "check_formatting",
    verify = "a region left unformatted makes the check fail"
)]
fn diff_exits_one_on_a_region_left_unformatted() {
    let tmp = TempDir::new().unwrap();
    project_with_a_kept_region(tmp.path());

    Command::cargo_bin("specforge")
        .unwrap()
        .args(["format", "--diff", "--path", tmp.path().to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("error region preserved verbatim"));
}

#[specforge_test(
    behavior = "format_spec_files",
    verify = "a region left unformatted makes the run exit 1, the rest of the file still written"
)]
fn write_exits_one_on_a_region_left_unformatted() {
    let tmp = TempDir::new().unwrap();
    let file = project_with_a_kept_region(tmp.path());
    let misindented = CANONICAL_FOO.replace("  contract", "      contract");
    fs::write(&file, format!("{misindented}\n}}}}}}\n")).unwrap();

    Command::cargo_bin("specforge")
        .unwrap()
        .args(["format", "--path", tmp.path().to_str().unwrap()])
        .assert()
        .code(1);

    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        format!("{CANONICAL_FOO}\n}}}}}}\n"),
        "the well-formed block is rewritten, the region kept"
    );
}

#[specforge_test(
    behavior = "format_from_stdin",
    verify = "stdin with a region left unformatted prints the formatted text and exits 1"
)]
fn stdin_exits_one_on_a_region_left_unformatted() {
    let tmp = TempDir::new().unwrap();
    setup_project(tmp.path());
    let misindented = CANONICAL_FOO.replace("  contract", "      contract");

    let output = Command::cargo_bin("specforge")
        .unwrap()
        .current_dir(tmp.path())
        .args(["format", "--stdin"])
        .write_stdin(format!("{misindented}\n}}}}}}\n"))
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("{CANONICAL_FOO}\n}}}}}}\n")
    );
}

// --- --format json: the format document (ADR 0052) ---

/// `specforge format --format json --path <root>` with `flags`: exit code,
/// stdout parsed as one JSON document, and stderr.
fn format_json(root: &std::path::Path, flags: &[&str]) -> (Option<i32>, serde_json::Value, String) {
    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args([
            "format",
            "--format",
            "json",
            "--path",
            root.to_str().unwrap(),
        ])
        .args(flags)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let document = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is one JSON document ({e}): {stdout}"));
    (
        output.status.code(),
        document,
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// A project with one canonical file and one that formatting changes.
fn project_with_one_change(root: &std::path::Path) {
    setup_project(root);
    write_spec(root, "good.spec", CANONICAL_FOO);
    write_spec(
        root,
        "bad.spec",
        "behavior bar \"Bar\" {\n      contract \"stuff\"\n}\n",
    );
}

#[specforge_test(
    behavior = "format_spec_files",
    verify = "under --format json the run prints the format document on stdout and nothing on stderr"
)]
fn json_prints_the_format_document_and_the_files_written() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    project_with_one_change(root);

    let (code, document, stderr) = format_json(root, &[]);

    assert_eq!(code, Some(0), "{document}");
    assert_eq!(stderr, "", "JSON output writes nothing on stderr");
    // Shown under the project root `--path` names, as it spells it.
    let changed = root.join("spec/bad.spec").display().to_string();
    assert_eq!(document["changed_files"], serde_json::json!([changed]));
    assert_eq!(document["total_checked"], 2);
    assert_eq!(document["ok"], true);
    assert_eq!(document["all_clean"], false);
    assert_eq!(document["check_only"], false);
    assert_eq!(document["diagnostics"], serde_json::json!([]));
    assert_eq!(
        document["files_written"],
        serde_json::json!(["spec/bad.spec"])
    );
    assert!(document.get("diffs").is_none(), "{document}");
}

#[specforge_test(
    behavior = "format_spec_files",
    verify = "under --format json the run prints the format document on stdout and nothing on stderr"
)]
fn json_carries_what_formatting_reported_instead_of_stderr() {
    let tmp = TempDir::new().unwrap();
    project_with_a_kept_region(tmp.path());

    let (code, document, stderr) = format_json(tmp.path(), &[]);

    assert_eq!(code, Some(1), "a region left unformatted fails the run");
    assert_eq!(stderr, "");
    assert_eq!(document["ok"], false);
    assert_eq!(document["diagnostics"][0]["code"], "W142", "{document}");
}

#[specforge_test(
    behavior = "format_spec_files",
    verify = "under --format json the run prints the format document on stdout and nothing on stderr"
)]
fn json_with_nothing_to_format_is_an_empty_run() {
    let tmp = TempDir::new().unwrap();
    setup_project(tmp.path());

    let (code, document, stderr) = format_json(tmp.path(), &[]);

    assert_eq!(code, Some(0));
    assert_eq!(stderr, "");
    assert_eq!(document["total_checked"], 0);
    assert_eq!(document["all_clean"], true);
}

#[specforge_test(
    behavior = "check_formatting",
    verify = "under --format json a check that finds a change prints the document and exits 1"
)]
fn json_check_that_finds_a_change_exits_one() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    project_with_one_change(root);

    let (code, document, stderr) = format_json(root, &["--check"]);

    assert_eq!(code, Some(1), "{document}");
    assert_eq!(stderr, "");
    assert_eq!(document["ok"], false);
    assert_eq!(document["check_only"], true);
    assert_eq!(document["changed_files"].as_array().unwrap().len(), 1);
    assert!(
        document.get("files_written").is_none(),
        "a check writes nothing: {document}"
    );
}

#[cfg(unix)]
#[specforge_test(
    behavior = "check_formatting",
    verify = "under --format json a check that finds a change prints the document and exits 1"
)]
fn json_names_a_file_that_cannot_be_read_in_the_document() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    write_spec(root, "locked.spec", CANONICAL_FOO);
    let locked = root.join("spec/locked.spec");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    let (code, document, stderr) = format_json(root, &["--check"]);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o644)).unwrap();

    assert_eq!(code, Some(1), "{document}");
    assert_eq!(stderr, "");
    let shown = locked.display().to_string();
    assert_eq!(document["failed_files"], serde_json::json!([shown]));
    assert_eq!(document["failures"][0]["operation"], "read");
    assert_eq!(document["failures"][0]["code"], "permission_denied");
}

#[specforge_test(
    behavior = "show_formatting_diff",
    verify = "under --format json --diff the document carries each changed file's before and after"
)]
fn json_diff_carries_each_change_before_and_after() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    project_with_one_change(root);
    let before = fs::read_to_string(root.join("spec/bad.spec")).unwrap();

    let (code, document, stderr) = format_json(root, &["--diff"]);

    assert_eq!(code, Some(0), "{document}");
    assert_eq!(stderr, "");
    let diff = &document["diffs"][0];
    assert_eq!(diff["file_path"], document["changed_files"][0]);
    assert_eq!(diff["before"], before);
    assert_eq!(
        diff["after"],
        "behavior bar \"Bar\" {\n  contract \"stuff\"\n}\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("spec/bad.spec")).unwrap(),
        before,
        "a diff writes nothing"
    );
}

#[specforge_test(
    behavior = "format_from_stdin",
    verify = "--stdin refuses --format json with the error document"
)]
fn stdin_refuses_json_with_the_error_document() {
    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args(["format", "--stdin", "--format", "json"])
        .write_stdin(CANONICAL_FOO)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    let document: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(document["code"], "invalid_input", "{document}");
}
