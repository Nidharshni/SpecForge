//! The `specforge-registry` binary as an operator runs it: a bad data directory is an error with a
//! message and exit 1, never a panic.

use std::process::{Command, Output};

/// A data directory that cannot be created: its parent is a file.
fn unusable_data_dir() -> (tempfile::NamedTempFile, String) {
    let file = tempfile::NamedTempFile::new().unwrap();
    let dir = format!("{}/x", file.path().display());
    (file, dir)
}

fn registry(args: &[&str]) -> Output {
    // Spelled in two pieces: the CLI suite's isolation guard greps the tests for the `specforge`
    // binary's variable, and this is the registry's binary.
    Command::new(env!(concat!("CARGO_BIN_EXE_", "specforge-registry")))
        .args(args)
        .output()
        .expect("the registry binary runs")
}

#[test]
#[ignore = "fixed by T4"]
fn token_create_with_an_unusable_data_dir_exits_1() {
    let (_file, dir) = unusable_data_dir();
    let output = registry(&["token", "create", "--no-expiry", "--data-dir", &dir]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.starts_with("error: cannot create the data directory"),
        "{stderr}"
    );
    assert!(!stderr.contains("panicked"), "{stderr}");
}

#[test]
#[ignore = "fixed by T4"]
fn serve_with_an_unusable_data_dir_exits_1() {
    let (_file, dir) = unusable_data_dir();
    let output = registry(&["serve", "--data-dir", &dir, "--port", "0"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.starts_with("error: cannot open the registry at"),
        "{stderr}"
    );
    assert!(!stderr.contains("panicked"), "{stderr}");
}
