//! The suite does not depend on the developer's HOME.
//!
//! `doctor` reports the user's registry credentials and exits 1 when one has expired (ADR 0029 D3a). A test that
//! runs `specforge` under the caller's own `HOME` therefore fails for a developer whose login expired. These tests
//! prove the suite is independent of that: they re-run this test binary under a `HOME` that holds an expired login.

use std::path::PathBuf;

use tempfile::TempDir;

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
#[ignore = "fixed by T4"]
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
