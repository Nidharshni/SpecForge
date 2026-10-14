//! The suite does not depend on the developer's HOME.
//!
//! `doctor` reports the user's registry credentials and exits 1 when one has expired (ADR 0029 D3a). A test that
//! runs `specforge` under the caller's own `HOME` therefore fails for a developer whose login expired. These tests
//! prove the suite is independent of that: they re-run this test binary under a `HOME` that holds an expired login.

use std::path::PathBuf;

use tempfile::TempDir;

use crate::support::{Cli, TestProject};

/// A `HOME` whose only content is a registry login that expired in 2020.
fn home_with_an_expired_login() -> TempDir {
    let home = TempDir::new().unwrap();
    let dir = home.path().join(".specforge");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("credentials.json"),
        r#"{"registries":{"old":{"token":"x","expires_at":"2020-01-01T00:00:00Z"}}}"#,
    )
    .unwrap();
    home
}

/// The compile cache this process uses, resolved before any `HOME` override, so a child under another `HOME` shares
/// it (a fresh cache recompiles every builtin).
fn compile_cache() -> PathBuf {
    std::env::var_os("SPECFORGE_WASMTIME_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").expect("HOME is set"))
                .join(".cache")
                .join("specforge")
                .join("wasmtime")
        })
}

#[test]
fn the_doctor_tests_pass_under_a_home_holding_an_expired_login() {
    let home = home_with_an_expired_login();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["doctor", "--skip", "isolation::", "--test-threads", "4"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("SPECFORGE_WASMTIME_CACHE", compile_cache())
        .env_remove("SPECFORGE_REPORT")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "the doctor tests fail under an expired login:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// What the driver reports, run by [`a_driver_run_does_not_see_the_callers_home`] under a `HOME` holding an expired
/// login.
#[test]
#[ignore = "run by a_driver_run_does_not_see_the_callers_home"]
fn probe_the_driver_reports_no_credentials() {
    let project = TestProject::new();
    let doctor = Cli::on(&project).document(&["doctor"]);
    assert_eq!(doctor.code, 0, "{}\n{}", doctor.json, doctor.stderr);
    let credentials = &doctor.json["credentials"];
    assert!(
        credentials.is_null() || credentials.as_array().is_some_and(|c| c.is_empty()),
        "{credentials}"
    );
}

#[test]
fn a_driver_run_does_not_see_the_callers_home() {
    let home = home_with_an_expired_login();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "isolation::probe_the_driver_reports_no_credentials",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("SPECFORGE_WASMTIME_CACHE", compile_cache())
        .env_remove("SPECFORGE_REPORT")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "the driver's run saw the caller's HOME:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn document_appends_the_json_format() {
    let project = TestProject::new();
    let stats = Cli::on(&project).document(&["stats"]);
    assert_eq!(stats.code, 0, "{}", stats.stderr);
    assert!(stats.json["entity_count"].is_number(), "{}", stats.json);
}

#[test]
fn bare_spells_no_path() {
    let project = TestProject::new();
    Cli::on(&project).bare(&["explain", "E003"]).exits(0);
}

// --- the guard: nothing but the support module runs the binary or picks a HOME ---

/// Every `.rs` file under `crates/*/tests/`, with its path relative to the repository.
fn test_sources() -> Vec<(String, String)> {
    fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for krate in std::fs::read_dir(repo.join("crates"))
        .unwrap()
        .filter_map(Result::ok)
    {
        let tests = krate.path().join("tests");
        if tests.is_dir() {
            walk(&tests, &mut files);
        }
    }
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).unwrap();
            let rel = path
                .strip_prefix(&repo)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            (rel, text)
        })
        .collect()
}

/// `file:line` of each line of the suite's test sources that holds one of `tokens`, outside `allowed` (a path
/// prefix, or this file).
fn uses(tokens: &[&str], allowed: &[&str]) -> Vec<String> {
    const HERE: &str = "crates/specforge-cli/tests/isolation.rs";
    let mut found = Vec::new();
    for (file, text) in test_sources() {
        if file == HERE || allowed.iter().any(|prefix| file.starts_with(prefix)) {
            continue;
        }
        for (number, line) in text.lines().enumerate() {
            if tokens.iter().any(|token| line.contains(token)) {
                found.push(format!("{file}:{}: {}", number + 1, line.trim()));
            }
        }
    }
    found
}

#[test]
fn the_support_module_alone_runs_the_binary() {
    const SUPPORT: &str = "crates/specforge-cli/tests/support/";
    let spawners = uses(
        &[
            "cargo_bin(",
            "cargo_bin!(",
            "cargo_bin_cmd!(",
            "CARGO_BIN_EXE_specforge",
        ],
        &[SUPPORT],
    );
    let homes = uses(&[".env(\"HOME\"", "\"USERPROFILE\""], &[SUPPORT]);
    let helpers = uses(&["fn specforge_cmd", "fn specforge(", "fn copy_tree"], &[]);
    let found: Vec<String> = spawners.into_iter().chain(homes).chain(helpers).collect();
    assert!(
        found.is_empty(),
        "use `support::Cli` (crates/specforge-cli/tests/support/cli.rs) to run the binary, and `TestProject` for the \
         project and its HOME:\n{}",
        found.join("\n")
    );
}
