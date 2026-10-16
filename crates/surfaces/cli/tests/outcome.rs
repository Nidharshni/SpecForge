//! How a core command ends (plan 02, ADR 0029): what it prints when its
//! operation refuses, and the exit code of its verdict.
//!
//! `refusals_after` is the table of every refusal as the binary writes it
//! (it was taken before the renderer as `refusals_today`, bugs included);
//! the ticket that changes a row re-blesses the snapshot, so its diff shows
//! each user-visible change. `verdicts` pins the exit codes of the
//! commands that judge.

use std::path::Path;

use crate::support::pairing::normalized_text;
use crate::support::{Cli, TestProject};

/// An empty directory.
fn scratch() -> TestProject {
    TestProject::new().without_config()
}

/// A scratch copy of `fixtures/read_views/rv1`, optionally with a report.
fn rv1(report: Option<&str>) -> TestProject {
    let project = TestProject::copy_of(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/read_views/rv1"),
    );
    match report {
        Some(report) => project.file("specforge-report.json", report),
        None => project,
    }
}

/// What one command printed.
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run_in(project: &TestProject, args: &[&str], stdin: Option<&str>) -> Run {
    let dir = project.root();
    let cli = match stdin {
        Some(text) => Cli::on(project).stdin(text),
        None => Cli::on(project),
    };
    let out = cli.bare(args);
    let clean = |text: &str| normalized_text(text, dir);
    Run {
        code: out.code,
        stdout: clean(&out.stdout),
        stderr: clean(&out.stderr),
    }
}

/// One row of the table: `label | command`, then exit, stdout, stderr.
fn row(label: &str, args: &[&str], run: &Run) -> String {
    format!(
        "{label} | specforge {}\nexit {}\n-- stdout:\n{}-- stderr:\n{}\n",
        args.join(" "),
        run.code,
        run.stdout,
        run.stderr
    )
}

#[specforge_test_macros::test(
    behavior = "report_command_outcome",
    verify = "Report a Command's Outcome: command outcome holds — operation_ran, one_refusal_shape, one_exit_table, surfaces_agree"
)]
fn refusals_after() {
    let mut table = String::new();
    let mut add = |label: &str, dir: &TestProject, args: &[&str]| {
        let run = run_in(dir, args, None);
        table.push_str(&row(label, args, &run));
    };

    let broken = rv1(Some("{"));
    add("C1", &broken, &["stats"]);
    add("C2", &broken, &["stats", "--format", "json"]);
    add("C3", &broken, &["analyze", "--format", "json"]);
    let project = rv1(None);
    add("C4", &project, &["analyze", "nope"]);
    add("C5", &project, &["trace", "nope", "--format", "json"]);
    add("C6", &project, &["migrate", "--target-version", "9.9"]);
    add(
        "C7",
        &project,
        &["migrate", "--target-version", "9.9", "--format", "json"],
    );
    add("C8", &project, &["schema", "--kind", "behavor"]);
    add("C9", &project, &["export", "--scope", "nope"]);
    let empty = scratch();
    add("C10", &empty, &["init", "--name", "a", "--format", "json"]);
    let loose = scratch();
    add(
        "C11",
        &loose,
        &["collect", "--path", ".", "--format", "json"],
    );
    add(
        "C12",
        &project,
        &["add", "@specforge/nope", "--format", "json"],
    );

    insta::assert_snapshot!("refusals_after", table);
}

const MESSY: &str =
    "behavior messy \"Messy\" {\ncategory \"core\"\ncontract \"The system MUST work\"\n}\n";
/// A region the formatter cannot parse and keeps as written (W142).
const REGION: &str = "behavior broken \"Broken\" {\n  @@@ ]]\n}\n";

#[test]
fn verdicts() {
    let exit = |dir: &TestProject, args: &[&str]| run_in(dir, args, None).code;

    let messy = rv1(None);
    messy.write("spec/messy.spec", MESSY);
    let check = exit(&messy, &["format", "--check"]);
    let diff = exit(&messy, &["format", "--diff"]);
    let write = exit(&messy, &["format"]);

    let region = rv1(None);
    region.write("spec/region.spec", REGION);
    let region_write = exit(&region, &["format"]);
    let region_stdin = run_in(&region, &["format", "--stdin"], Some(REGION)).code;

    let bad_header = rv1(None);
    bad_header.write(
        "spec/bad.spec",
        "// specforge-format: 99.0\nbehavior bad \"Bad\" {\n}\n",
    );
    let migrate_fails = exit(&bad_header, &["migrate"]);

    let old = rv1(None);
    old.write("spec/old.spec",
        "// specforge-format: 0.1\nbehavior old_one \"Old\" {\n  category \"core\"\n  contract \"The system MUST work\"\n}\n",
    );
    let migrate_dry_run = exit(&old, &["migrate", "--dry-run"]);

    let dangling = rv1(None);
    dangling.write("spec/dangling.spec",
        "behavior dangling \"D\" {\n  category \"core\"\n  contract \"The system MUST work\"\n  invariants [missing]\n}\n",
    );
    let check_fails = exit(&dangling, &["check"]);

    let doctor_fails = rv1(None);
    doctor_fails.write(
        "specforge.json",
        r#"{"name":"d","version":"0.1.0","extensions":["@acme/missing"]}"#,
    );
    let doctor_exit = exit(&doctor_fails, &["doctor"]);
    let explain_unknown = exit(&old, &["explain", "ZZZ9"]);
    let none = scratch();
    let wasm = none.root().join("none.wasm");
    std::fs::write(&wasm, b"nope").unwrap();
    let validate_none = exit(
        &none,
        &["extension", "validate", "--dir", wasm.to_str().unwrap()],
    );

    for (what, got, expected) in [
        ("doctor, an E028", doctor_exit, 1),
        ("explain ZZZ9", explain_unknown, 1),
        (
            "extension validate, a binary that is none",
            validate_none,
            1,
        ),
        ("format --check, a change", check, 1),
        ("format --diff, a change", diff, 0),
        ("format, a change", write, 0),
        ("format, a W142 region", region_write, 1),
        ("format --stdin, a W142 region", region_stdin, 1),
        ("migrate, a 99.0 header", migrate_fails, 1),
        ("migrate --dry-run, a 0.1 header", migrate_dry_run, 0),
        ("check, an E003", check_fails, 1),
    ] {
        assert_eq!(got, expected, "{what}");
    }
}

/// The refusals that print the error document under JSON output, with the
/// code each carries: stdout holds one document, stderr nothing.
#[specforge_test_macros::test(
    behavior = "report_command_outcome",
    verify = "stats, trace, analyze, migrate and init refuse with the error document under --format json"
)]
fn json_refusals_are_the_error_document() {
    let broken = rv1(Some("{"));
    let project = rv1(None);
    let empty = scratch();
    let rows: [(&TestProject, &[&str], &str); 5] = [
        (&broken, &["stats", "--format", "json"], "E045"),
        (&project, &["trace", "nope", "--format", "json"], "E003"),
        (&broken, &["analyze", "--format", "json"], "E045"),
        (
            &project,
            &["migrate", "--target-version", "9.9", "--format", "json"],
            "E019",
        ),
        (
            &empty,
            &["init", "--name", "a", "--format", "json"],
            "invalid_name",
        ),
    ];
    for (dir, args, code) in rows {
        let run = run_in(dir, args, None);
        let document: serde_json::Value = serde_json::from_str(&run.stdout)
            .unwrap_or_else(|e| panic!("{args:?}: stdout is no document ({e}): {}", run.stdout));
        assert_eq!(document["code"], code, "{args:?}: {document}");
        assert!(document["error"].is_string(), "{args:?}: {document}");
        assert_eq!(run.stderr, "", "{args:?}: nothing on stderr");
    }
}

/// `stats` and `analyze` measure: a refusal means they could not judge the
/// project, exit 2; every other command's refusal is exit 1.
#[specforge_test_macros::test(
    behavior = "report_command_outcome",
    verify = "a passed run exits 0, a failed verdict or a refusal 1, a refusal of a measuring command 2"
)]
fn measuring_commands_exit_two_on_refusal() {
    let broken = rv1(Some("{"));
    let project = rv1(None);

    for args in [&["stats"][..], &["analyze"], &["analyze", "nope"]] {
        let dir = if args == ["analyze", "nope"] {
            &project
        } else {
            &broken
        };
        assert_eq!(run_in(dir, args, None).code, 2, "{args:?}");
    }
    for args in [&["trace", "nope"][..], &["schema", "--kind", "behavor"]] {
        assert_eq!(run_in(&project, args, None).code, 1, "{args:?}");
    }
    assert_eq!(run_in(&project, &["stats"], None).code, 0);
}

/// `review` is a read view: an unreadable report exits 1, as `query --include-coverage` does.
#[specforge_test_macros::test(
    behavior = "provide_review_cli",
    verify = "a recorded report that cannot be read is E045, exit 1"
)]
fn review_refuses_as_query_does_when_the_report_cannot_be_read() {
    let broken = rv1(Some("{"));

    let review = run_in(&broken, &["review"], None);
    assert_eq!(review.code, 1, "{}", review.stderr);
    assert!(
        review.stderr.starts_with("error[E045]"),
        "{}",
        review.stderr
    );

    let query = run_in(&broken, &["query", "alpha", "--include-coverage"], None);
    assert_eq!(query.code, 1, "{}", query.stderr);
}

#[specforge_test_macros::test(
    behavior = "report_command_outcome",
    verify = "a command run outside any project refuses with no_project"
)]
fn collect_outside_a_project_is_no_project() {
    let loose = scratch();

    let run = run_in(
        &loose,
        &["collect", "--path", ".", "--format", "json"],
        None,
    );

    assert_eq!(run.code, 1);
    let document: serde_json::Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(document["code"], "no_project", "{document}");
    let human = run_in(&loose, &["collect", "--path", "."], None);
    assert!(
        human.stderr.starts_with("error[no_project]:"),
        "{}",
        human.stderr
    );
}

#[specforge_test_macros::test(
    behavior = "report_command_outcome",
    verify = "an operation's refusal is error[CODE]: message, its hint and the files it left written, on stderr"
)]
fn an_unknown_explain_code_is_a_refusal() {
    let dir = scratch();

    let run = run_in(&dir, &["explain", "ZZZ9"], None);

    assert_eq!(run.code, 1);
    assert!(
        run.stderr.starts_with(
            "error[unknown_code]: unknown diagnostic code: ZZZ9\n  hint: codes are E###"
        ),
        "{}",
        run.stderr
    );
}

#[cfg(unix)]
#[specforge_test_macros::test(
    behavior = "report_command_outcome",
    verify = "an operation's refusal is error[CODE]: message, its hint and the files it left written, on stderr"
)]
fn a_build_cache_that_cannot_be_written_is_a_refusal() {
    use std::os::unix::fs::PermissionsExt;
    let project = rv1(None);
    let mode = |m| std::fs::Permissions::from_mode(m);
    std::fs::set_permissions(project.root(), mode(0o555)).unwrap();
    if std::fs::write(project.root().join("probe"), b"").is_ok() {
        // Permissions do not bind this user (root): nothing to test.
        std::fs::set_permissions(project.root(), mode(0o755)).unwrap();
        return;
    }

    let human = run_in(&project, &["check", "--cache"], None);
    let json = run_in(&project, &["check", "--cache", "--format", "json"], None);
    std::fs::set_permissions(project.root(), mode(0o755)).unwrap();

    assert_eq!(human.code, 1, "{}", human.stderr);
    assert!(
        human
            .stderr
            .contains("error[cache_write_failed]: cannot write"),
        "{}",
        human.stderr
    );
    assert_eq!(json.code, 1, "{}", json.stderr);
    assert!(
        json.stdout.contains("\"code\": \"cache_write_failed\""),
        "{}",
        json.stdout
    );
}

#[specforge_test_macros::test(
    behavior = "report_command_outcome",
    verify = "a command run outside any project refuses with no_project"
)]
fn collect_from_a_sub_path_finds_its_project() {
    // A project without a collector: `collect` finds it from a sub-path and
    // fails for want of a collector (E058), not for want of a project.
    let project = rv1(None);
    let sub = project.root().join("spec");

    let run = run_in(
        &project,
        &["collect", "--no-run", "--path", sub.to_str().unwrap()],
        None,
    );

    assert_eq!(run.code, 1, "{}", run.stderr);
    assert!(run.stderr.contains("E058"), "{}", run.stderr);
    assert!(!run.stderr.contains("no_project"), "{}", run.stderr);
}
