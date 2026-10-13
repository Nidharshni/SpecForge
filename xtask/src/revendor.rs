//! One command that leaves the tree consistent after a change to what the
//! builtin extensions compile from (their sources, the SDK and its macros,
//! protocol-types, coverage, any `inputs.json` input). See docs/agents/revendor.md.
//!
//!   cargo run -p xtask --bin revendor
//!
//! In order: refuse a place it cannot work from; refresh the guest lockfiles;
//! rebuild and vendor the builtin blobs (`extensions/*/wasm`, `inputs.json`);
//! rebuild the SDK greet fixture blob; rewrite the declaration snapshots;
//! check the lot and print the paths to stage.

use specforge_component::builtins::BUILTINS;
use std::path::{Path, PathBuf};
use std::process::Command;

const GREET: &str = "fixtures/greet-extension";

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
        .expect("repo root resolves");
    let env_target = std::env::var_os("CARGO_TARGET_DIR").is_some();
    if let Err(message) = preflight(&root, env_target) {
        eprintln!("revendor: {message}");
        std::process::exit(1);
    }
    if let Err(message) = run(&root) {
        eprintln!("revendor: {message}");
        std::process::exit(1);
    }
}

/// Where a re-vendor cannot work: with `CARGO_TARGET_DIR` set (a staged
/// build and the guest crates would share it), or in a checkout nested in
/// another Cargo workspace (cargo then reads the outer one).
fn preflight(root: &Path, target_dir_set: bool) -> Result<(), String> {
    if target_dir_set {
        return Err(
            "CARGO_TARGET_DIR is set: unset it (`env -u CARGO_TARGET_DIR cargo run -p xtask --bin revendor`); \
             the builtin blobs build in target/builtin-stage/"
                .to_string(),
        );
    }
    for ancestor in root.ancestors().skip(1) {
        let manifest = ancestor.join("Cargo.toml");
        if std::fs::read_to_string(&manifest).is_ok_and(|text| text.contains("[workspace]")) {
            return Err(format!(
                "{} sits inside the Cargo workspace at {}: run it from the main checkout, or a worktree outside the repository",
                root.display(),
                ancestor.display()
            ));
        }
    }
    Ok(())
}

fn run(root: &Path) -> Result<(), String> {
    println!("== lockfiles");
    let mut changed = Vec::new();
    let mut guests: Vec<String> = BUILTINS
        .iter()
        .map(|b| format!("extensions/{}", b.dir))
        .collect();
    guests.push(GREET.to_string());
    for guest in &guests {
        if refresh_lockfile(root, guest)? {
            changed.push(format!("{guest}/Cargo.lock"));
        }
    }
    println!("== builtin blobs");
    xtask(root, "build-builtins", &["--install"])?;
    println!("== greet fixture");
    build_greet(root)?;
    println!("== declaration snapshots");
    xtask(root, "snapshot-builtins", &[])?;
    println!("== checks");
    xtask(root, "build-builtins", &["--check"])?;
    xtask(root, "snapshot-builtins", &["--check"])?;

    println!("\nre-vendored. Review `git diff --stat`, then stage:");
    println!(
        "  git add extensions/*/wasm extensions/*/Cargo.lock {GREET} crates/specforge-component/tests/declarations"
    );
    if !changed.is_empty() {
        println!("lockfiles refreshed: {}", changed.join(", "));
    }
    Ok(())
}

/// `cargo metadata --locked` for `guest`; when the lockfile is stale, once
/// without `--locked` (a stale guest lockfile follows a workspace dependency
/// change, never a decision). True when the lockfile changed.
fn refresh_lockfile(root: &Path, guest: &str) -> Result<bool, String> {
    let manifest = root.join(guest).join("Cargo.toml");
    let lock = root.join(guest).join("Cargo.lock");
    let before = std::fs::read(&lock).ok();
    let metadata = |locked: bool| {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "metadata",
            "--format-version",
            "1",
            "--quiet",
            "--manifest-path",
        ])
        .arg(&manifest)
        .stdout(std::process::Stdio::null());
        if locked {
            cmd.arg("--locked");
        }
        cmd.status().is_ok_and(|s| s.success())
    };
    if !metadata(true) && !metadata(false) {
        return Err(format!("cargo metadata failed for {guest}"));
    }
    let changed = std::fs::read(&lock).ok() != before;
    println!("[{}] {guest}", if changed { "lock" } else { "ok  " });
    Ok(changed)
}

/// Rebuild the greet fixture blob and vendor it beside its sources.
fn build_greet(root: &Path) -> Result<(), String> {
    let dir = root.join(GREET);
    let status = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--target",
            "wasm32-wasip2",
            "--manifest-path",
        ])
        .arg(dir.join("Cargo.toml"))
        .status()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if !status.success() {
        return Err(format!(
            "building {GREET} failed (rustup target add wasm32-wasip2?)"
        ));
    }
    let built = dir.join("target/wasm32-wasip2/release/greet.wasm");
    std::fs::copy(&built, dir.join("greet.wasm"))
        .map_err(|e| format!("cannot copy {}: {e}", built.display()))?;
    let _ = std::fs::remove_dir_all(dir.join("target"));
    println!("[ok]     {GREET}/greet.wasm");
    Ok(())
}

/// Run a sibling xtask bin from the repository root.
fn xtask(root: &Path, bin: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new("cargo")
        .args(["run", "-q", "-p", "xtask", "--bin", bin, "--"])
        .args(args)
        .current_dir(root)
        .status()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{bin} {} failed", args.join(" ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_committed_wasm_is_listed() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let out = Command::new("git")
            .args(["ls-files", "-z", "*.wasm"])
            .current_dir(&root)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git ls-files failed");
        let tracked: std::collections::BTreeSet<String> = String::from_utf8(out.stdout)
            .unwrap()
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(str::to_string)
            .collect();
        let listed: std::collections::BTreeSet<String> = specforge_component::committed::COMMITTED
            .iter()
            .map(|blob| blob.repo_path())
            .collect();
        assert_eq!(
            tracked, listed,
            "git tracks these .wasm files; list each in specforge_component::committed"
        );
    }

    #[test]
    fn refuses_when_cargo_target_dir_is_set() {
        let err = preflight(Path::new("/nowhere/repo"), true).unwrap_err();
        assert!(err.contains("CARGO_TARGET_DIR"), "{err}");
    }

    #[test]
    fn refuses_a_checkout_nested_in_another_workspace() {
        let outer = std::env::temp_dir().join(format!("revendor-nested-{}", std::process::id()));
        let inner = outer.join("worktrees/repo");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(outer.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        let err = preflight(&inner, false).unwrap_err();
        std::fs::remove_dir_all(&outer).unwrap();
        assert!(err.contains("inside the Cargo workspace"), "{err}");
    }

    #[test]
    fn accepts_a_standalone_checkout() {
        let dir = std::env::temp_dir().join(format!("revendor-alone-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ok = preflight(&dir, false);
        std::fs::remove_dir_all(&dir).unwrap();
        // /tmp has no enclosing workspace manifest
        assert!(ok.is_ok(), "{ok:?}");
    }
}
