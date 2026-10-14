//! One command that leaves the tree consistent after a change to what the
//! committed blobs compile from (their sources, the SDK and its macros,
//! protocol-types, coverage, any `inputs.json` input). See docs/agents/revendor.md.
//!
//!   cargo run -p xtask --bin revendor
//!
//! In order: refuse a place it cannot work from; refresh the lockfile of every
//! committed blob's crate (`specforge_component::committed::COMMITTED`); rebuild
//! and vendor every blob with its `inputs.json` (`build-blobs --install`);
//! rewrite the declaration snapshots of every SDK blob, in process; check what it
//! did not write (`build-blobs --check` and the declarations test) and print
//! the paths to stage.
#![expect(
    clippy::print_stdout,
    reason = "a developer tool: its report is its stdout"
)]

use specforge_component::ComponentRuntime;
use specforge_component::committed::{COMMITTED, snapshots, wire_answers};
use specforge_wasm::runtime::WasmRuntime;
use std::path::Path;
use std::process::Command;

fn main() {
    let root = specforge_repo_root::repo_root();
    let env_target = std::env::var_os("CARGO_TARGET_DIR").is_some();
    if let Err(message) = preflight(root, env_target) {
        eprintln!("revendor: {message}");
        std::process::exit(1);
    }
    if let Err(message) = run(root) {
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
             the builtin blobs build in target/blob-stage/"
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
    for blob in COMMITTED {
        if refresh_lockfile(root, blob.crate_dir)? {
            changed.push(format!("{}/Cargo.lock", blob.crate_dir));
        }
    }
    println!("== blobs");
    xtask(root, "build-blobs", &["--install"])?;
    println!("== declaration snapshots");
    let written = write_snapshots()?;
    println!("[ok]     {written} snapshot files written");
    println!("== checks");
    xtask(root, "build-blobs", &["--check"])?;
    declarations_test(root)?;

    println!("\nre-vendored. Review `git diff --stat`, then stage:");
    let paths: Vec<String> = COMMITTED
        .iter()
        .flat_map(|blob| {
            [
                format!("{}/wasm", blob.crate_dir),
                format!("{}/Cargo.lock", blob.crate_dir),
            ]
        })
        .collect();
    println!(
        "  git add {} crates/runtime/component/tests/declarations",
        paths.join(" ")
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

/// Rewrite the pinned wire answers of every SDK blob from the blobs just vendored, and remove a
/// snapshot directory no SDK blob names any more. Returns the number of files written.
fn write_snapshots() -> Result<usize, String> {
    let root = snapshots();
    let runtime = ComponentRuntime::new();
    let mut written = 0;
    let mut kept = Vec::new();
    for blob in COMMITTED.iter().filter(|blob| blob.is_sdk()) {
        runtime
            .load(blob.name, &blob.read())
            .map_err(|e| format!("{}: {e}", blob.name))?;
        let dir = root.join(blob.snapshot_dir());
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for answer in wire_answers(&runtime, blob)? {
            let path = dir.join(&answer.file);
            std::fs::write(&path, &answer.bytes).map_err(|e| format!("{}: {e}", path.display()))?;
            written += 1;
        }
        kept.push(blob.snapshot_dir());
    }
    for entry in std::fs::read_dir(&root).map_err(|e| format!("{}: {e}", root.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.path().is_dir() && !kept.contains(&name.as_str()) {
            std::fs::remove_dir_all(entry.path())
                .map_err(|e| format!("{}: {e}", entry.path().display()))?;
        }
    }
    Ok(written)
}

/// The declarations test: the snapshot set, the typed round trip and the protocol freshness test
/// can fail on a fresh write. (The comparison with the files just written cannot.)
fn declarations_test(root: &Path) -> Result<(), String> {
    let status = Command::new("cargo")
        .args([
            "test",
            "-q",
            "-p",
            "specforge-component",
            "--test",
            "declarations",
            "--",
            "--include-ignored",
        ])
        .current_dir(root)
        .status()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("the declarations test failed".to_string())
    }
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
        let root = specforge_repo_root::repo_root();
        let out = Command::new("git")
            .args(["ls-files", "-z", "*.wasm"])
            .current_dir(root)
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
