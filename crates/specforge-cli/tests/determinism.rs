//! Cross-process determinism gate (hardening-plan D1, S1; R-6).
//!
//! HashMap iteration order is seeded per process, so in-process repetition
//! cannot prove run-to-run determinism. This test spawns the real binary
//! 20 times over a fixture whose diagnostic set was proven order-sensitive
//! before the fix (feature dependency cycle with a feeder node) and asserts
//! byte-identical stdout, stderr, and exit code on every run.
//!
//! It also pins the D1 semantics: only nodes ON the cycle are flagged — a
//! feeder node leading into the cycle is not.

use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_specforge")
}

/// feature cycle_b ⇄ cycle_c (a real 2-cycle), with cycle_a feeding into it.
/// Before hardening D1, the flagged set flip-flopped between `{cycle_b,
/// cycle_c}` and `{cycle_a, cycle_b, cycle_c}` depending on HashMap seeding.
fn setup_cycle_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let config = serde_json::json!({
        "name": "det-fixture",
        "version": "0.1.0",
        "extensions": ["@specforge/product"]
    });
    fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();

    let spec = "\
feature cycle_a \"Feeder\" {
    depends_on [cycle_b]
    problem \"a\"
    solution \"a\"
    flow [step]
}

feature cycle_b \"Cycle B\" {
    depends_on [cycle_c]
    problem \"b\"
    solution \"b\"
    flow [step]
}

feature cycle_c \"Cycle C\" {
    depends_on [cycle_b]
    problem \"c\"
    solution \"c\"
    flow [step]
}
";
    fs::write(dir.path().join("cycle.spec"), spec).unwrap();
    dir
}

fn run_check(dir: &TempDir) -> (Vec<u8>, Vec<u8>, i32) {
    let out = Command::new(binary())
        .arg("check")
        .arg("--path")
        .arg(dir.path())
        .output()
        .expect("specforge binary runs");
    (out.stdout, out.stderr, out.status.code().unwrap_or(-1))
}

#[test]
fn check_output_identical_across_20_processes() {
    let dir = setup_cycle_project();

    let mut first: Option<(Vec<u8>, Vec<u8>, i32)> = None;
    for run in 0..20 {
        let result = run_check(&dir);
        match &first {
            None => first = Some(result),
            Some(expected) => {
                assert_eq!(
                    &result, expected,
                    "run {run}: check output differs from run 0 (nondeterminism)"
                );
            }
        }
    }
}

#[test]
fn cycle_members_flagged_feeder_not_flagged() {
    let dir = setup_cycle_project();
    let (_stdout, stderr, _code) = run_check(&dir);
    // Diagnostics render to stderr; stdout carries nothing for `check`.
    let text = String::from_utf8_lossy(&stderr);

    let w045: Vec<&str> = text
        .lines()
        .filter(|l| l.contains("W045"))
        .filter(|l| l.contains("cycle detected"))
        .collect();

    assert!(
        text.contains("cycle_b") && text.contains("cycle_c"),
        "cycle members must be flagged; output:\n{text}"
    );
    // The feeder leads into the cycle but is not part of it (D1 semantics).
    let feeder_flagged = w045
        .iter()
        .any(|l| l.contains("cycle_a") || text.contains("involving 'cycle_a'"));
    assert!(
        !feeder_flagged,
        "feeder node cycle_a must not be flagged as a cycle member; output:\n{text}"
    );
}
