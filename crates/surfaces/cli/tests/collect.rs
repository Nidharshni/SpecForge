use crate::support::{Cli, TestProject};
use assert_cmd::Command;
use predicates::prelude::*;
use specforge_test_macros::test as specforge_test;
use std::path::Path;
use tempfile::TempDir;

/// An empty directory: no `specforge.json`.
fn scratch() -> TestProject {
    TestProject::new().without_config()
}

/// A Rust project collected by `@specforge/cargo-test`, a fake `cargo` on
/// `PATH` that writes a report the way `specforge-test` does (and leaves a
/// marker so tests can tell whether it ran). The consent store is the project's own HOME.
/// The flow itself is tested in process (`specforge-ops`, `collect::*`); these tests are the CLI's wiring.
struct Fixture {
    project: TestProject,
    /// The directory holding the fake `cargo`, beside the project.
    tools: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let project = TestProject::new()
            .raw_config(
                r#"{"name":"demo","version":"0.1.0","extensions":["@specforge/software","@specforge/testing","@specforge/cargo-test"]}"#,
            )
            .file(
                "app.spec",
                "behavior login \"Login\" {\n  verify unit \"accepts valid credentials\"\n}\n",
            )
            .file("Cargo.toml", "");

        let tools = TempDir::new().unwrap();
        let bin = tools.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let cargo = bin.join("cargo");
        std::fs::write(
            &cargo,
            r#"#!/bin/sh
touch "$(dirname "$0")/ran"
mkdir -p "$SPECFORGE_REPORT"
cat > "$SPECFORGE_REPORT/demo.json" <<'EOF'
{"entries":[{"entity_id":"login","test_name":"accepts","verify":"accepts valid credentials","status":"passed"}]}
EOF
exit 0
"#,
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        Fixture { project, tools }
    }

    fn root(&self) -> &Path {
        self.project.root()
    }

    fn ran(&self) -> bool {
        self.tools.path().join("bin/ran").exists()
    }

    /// `specforge <args>` in the fixture's environment, with stdin closed.
    fn cmd(&self, args: &[&str]) -> Command {
        let path = format!(
            "{}:{}",
            self.tools.path().join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut cmd = Cli::on(&self.project).env("PATH", path).command(args);
        cmd.write_stdin("");
        cmd
    }

    fn report(&self) -> serde_json::Value {
        let raw = std::fs::read_to_string(self.root().join("specforge-report.json")).unwrap();
        serde_json::from_str(&raw).unwrap()
    }
}

fn write_report(root: &Path, entries: &str) {
    let dir = root.join("target/specforge");
    std::fs::create_dir_all(&dir).unwrap();
    let file = std::fs::File::create(dir.join("old.json")).unwrap();
    std::io::Write::write_all(&mut &file, format!(r#"{{"entries":{entries}}}"#).as_bytes())
        .unwrap();
    // Backdate it: collect keeps a report whose time is within 100 ms of the
    // run's start (the kernel's coarse file clock), so a report written just
    // before the run would read as the run's own.
    let long_ago = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
    file.set_modified(long_ago).unwrap();
}

#[test]
fn test_collect_help_exits_0() {
    Cli::on(&scratch())
        .bare_command(&["collect", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--no-run"))
        .stdout(predicate::str::contains("--yes"));
}

#[test]
fn test_collect_in_empty_dir_exits_1() {
    let dir = scratch();
    Cli::on(&dir)
        .command(&["collect"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no specforge project at"));
}

#[test]
fn test_collect_json_format_in_empty_dir() {
    let dir = scratch();
    Cli::on(&dir)
        .command(&["collect", "--format", "json"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("\"error\""));
}

#[specforge_test(
    behavior = "approve_collector_command",
    verify = "unapproved command without a terminal fails with E059 and runs nothing"
)]
fn unapproved_command_without_a_terminal_is_refused() {
    let fx = Fixture::new();
    fx.cmd(&["collect"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error[E059]"))
        .stderr(predicate::str::contains(
            "cargo test --workspace --no-fail-fast",
        ));
    assert!(!fx.ran(), "the command must not run without approval");
    assert!(!fx.root().join("specforge-report.json").exists());
}

#[cfg(unix)]
#[specforge_test(
    behavior = "approve_collector_command",
    verify = "--yes runs the declared command"
)]
fn yes_runs_the_declared_command() {
    let fx = Fixture::new();
    // A stale report from an earlier run must not survive the new one.
    write_report(
        fx.root(),
        r#"[{"entity_id":"login","test_name":"old","status":"failed"}]"#,
    );
    fx.cmd(&["collect", "--yes"])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "running cargo-test (@specforge/cargo-test): cargo test --workspace --no-fail-fast",
        ))
        .stdout(predicate::str::contains(
            "cargo-test: 1 entities, 1 passed, 0 failed, 0 skipped",
        ));
    assert!(fx.ran());
    let tests = &fx.report()["results"]["login"]["tests"];
    assert_eq!(tests.as_array().unwrap().len(), 1);
    assert_eq!(tests[0]["name"], "accepts");
    assert_eq!(tests[0]["verify"], "accepts valid credentials");
    assert_eq!(tests[0]["runner"], "cargo-test");
}

#[specforge_test(
    behavior = "ingest_collector_report",
    verify = "collect then analyze scores the recorded tests"
)]
fn collect_then_analyze_scores_the_recorded_tests() {
    let fx = Fixture::new();
    // No process: collect reads the report it is given, and analyze reads what collect wrote.
    let report = fx.tools.path().join("report.json");
    std::fs::write(
        &report,
        r#"{"entries":[{"entity_id":"login","test_name":"accepts","verify":"accepts valid credentials","status":"passed"}]}"#,
    )
    .unwrap();
    fx.cmd(&[
        "collect",
        "--runner",
        "cargo-test",
        &format!("--report={}", report.display()),
    ])
    .assert()
    .success();
    let out = Cli::on(&fx.project)
        .command(&["analyze", "coverage", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.contains(r#""entities_proven": 1"#) || text.contains(r#""entities_proven":1"#),
        "analyze should read specforge-report.json by default: {text}"
    );
}

#[specforge_test(
    behavior = "resolve_test_conventions",
    verify = "a plain cargo test proves obligations by naming convention"
)]
fn a_plain_cargo_test_proves_obligations_by_naming_convention() {
    let project = TestProject::new()
        .raw_config(
            r#"{"name":"shop","version":"0.1.0","extensions":["@specforge/software","@specforge/testing","@specforge/cargo-test"]}"#,
        )
        .file(
            "cart.spec",
            "behavior add_item \"Add an item\" {\n  verify unit \"rejects an empty name\"\n  verify unit \"rejects a duplicate item\"\n}\n",
        )
        // No specforge-test dependency: only the names link the tests.
        .file(
            "Cargo.toml",
            "[package]\nname = \"shop\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
        )
        .file(
            "src/lib.rs",
            r#"#[cfg(test)]
mod tests {
    #[test]
    #[allow(non_snake_case)]
    fn add_item__rejects_an_empty_name() {}

    mod add_item {
        #[test]
        fn rejects_a_duplicate_item() {}
    }

    #[test]
    fn unrelated() {}
}
"#,
        );
    let root = project.root();

    Cli::on(&project)
        .with_toolchain()
        .env("CARGO_TARGET_DIR", root.join("target"))
        .command(&["collect", "--yes"])
        .write_stdin("")
        .assert()
        .success()
        .stdout(predicate::str::contains("2 linked by naming convention"));

    let out = Cli::on(&project)
        .command(&["analyze", "coverage", "--format", "json"])
        .assert()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(
        !text.contains("A015"),
        "both obligations are proven: {text}"
    );
    assert!(
        text.contains(r#""obligations_proven": 2"#) || text.contains(r#""obligations_proven":2"#),
        "{text}"
    );
}
