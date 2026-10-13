//! Shared by the tests that run the `ignore_fixture` test binary, which
//! `cargo test` builds next to them.

use std::path::{Path, PathBuf};

/// The newest `ignore_fixture-<hash>` binary in this binary's directory.
pub fn fixture() -> PathBuf {
    let dir = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.starts_with("ignore_fixture-")
                && matches!(p.extension().and_then(|e| e.to_str()), None | Some("exe"))
        })
        .max_by_key(|p| p.metadata().and_then(|m| m.modified()).ok())
        .unwrap_or_else(|| {
            panic!(
                "no ignore_fixture binary in {}: run the whole suite (cargo test), not one --test",
                dir.display()
            )
        })
}

/// Run the fixture with `args`, its reports going to `reports`.
pub fn run_fixture(args: &[&str], reports: &Path) -> std::process::Output {
    let out = std::process::Command::new(fixture())
        .args(args)
        .env("SPECFORGE_REPORT", reports)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    out
}
