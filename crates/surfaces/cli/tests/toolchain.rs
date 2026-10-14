//! The SMT solver a `specforge` child process finds on PATH, chosen by the test
//! (ADR 0079 D5): doctor's toolchain finding and `analyze --prove` never
//! depend on the machine. Unix only: the fakes are shell scripts.

use std::path::PathBuf;

/// Write `z3` (a shell script) into `<target tmp>/toolchain/<name>/` once and
/// return that directory. Parallel tests race safely: the script is written to
/// `z3.tmp`, made executable, then renamed into place.
#[cfg(unix)]
fn fixture(name: &str, script: Option<&str>) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("toolchain")
        .join(name);
    std::fs::create_dir_all(&dir).unwrap();
    if let Some(script) = script {
        let z3 = dir.join("z3");
        if !z3.exists() {
            let tmp = dir.join(format!("z3.tmp.{}", std::process::id()));
            std::fs::write(&tmp, script).unwrap();
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).unwrap();
            std::fs::rename(&tmp, &z3).unwrap();
        }
    }
    dir
}

/// A PATH whose only `z3` answers `--version`: z3 runs.
#[cfg(unix)]
pub fn z3_ready() -> PathBuf {
    fixture(
        "ready",
        Some("#!/bin/sh\necho \"Z3 version 4.13.0 - test\"\n"),
    )
}

/// A PATH with no `z3`.
#[cfg(unix)]
pub fn z3_absent() -> PathBuf {
    fixture("absent", None)
}

/// A PATH whose `z3` exits 1 on everything.
#[cfg(unix)]
pub fn z3_broken() -> PathBuf {
    fixture(
        "broken",
        Some("#!/bin/sh\necho \"z3: cannot load libz3\" >&2\nexit 1\n"),
    )
}

/// A PATH whose `z3 --version` answers after 30 s.
#[cfg(unix)]
pub fn z3_hung() -> PathBuf {
    fixture(
        "hung",
        Some("#!/bin/sh\n/bin/sleep 30\necho \"Z3 version 4.13.0 - hung\"\n"),
    )
}

/// A PATH whose `z3` answers `--version` and takes 30 s on a script, through a
/// child process.
#[cfg(unix)]
pub fn z3_slow() -> PathBuf {
    fixture(
        "slow",
        Some(
            "#!/bin/sh\n[ \"$1\" = \"--version\" ] && { echo \"Z3 version 4.13.0 - slow\"; exit 0; }\n/bin/sleep 30\ntrue\n",
        ),
    )
}

/// Panics, saying how to install it, unless the real z3 runs: for tests of what
/// z3 answers.
pub fn require_z3() {
    assert!(
        std::process::Command::new("z3")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success()),
        "z3 is required for this test: install it (brew install z3 / apt-get install z3); CI installs it"
    );
}
