//! `specforge explain <CODE>`: the CLI adapter over the diagnostic catalog.
//! These pin what a user sees, so the catalog can move or grow without the
//! command's output or exit codes changing by accident.

use assert_cmd::Command;

fn specforge() -> Command {
    assert_cmd::cargo_bin_cmd!("specforge")
}

#[test]
fn explain_prints_a_catalogued_code_case_insensitively() {
    let out = specforge().args(["explain", "e001"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "a catalogued code exits 0");
    let stdout = String::from_utf8(out.stdout).unwrap();
    // The code is printed bold; strip the escapes before matching.
    let plain = stdout.replace("\x1b[1m", "").replace("\x1b[0m", "");
    assert!(plain.contains("E001: Parse error"), "stdout: {plain}");
    assert!(plain.contains("Owner: core"), "stdout: {plain}");
    assert!(plain.contains("Level: error"), "stdout: {plain}");
}

#[test]
fn explain_points_a_retired_code_at_its_replacement() {
    let out = specforge().args(["explain", "E047"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "a retired code exits 0");
    let stdout = String::from_utf8(out.stdout).unwrap();
    let plain = stdout.replace("\x1b[1m", "").replace("\x1b[0m", "");
    assert!(
        plain.contains("E047 is retired; it was renumbered to W139."),
        "stdout: {plain}"
    );
    assert!(
        plain.contains("W139: Formal claim not entailed by declared bounds"),
        "stdout: {plain}"
    );
    assert!(plain.contains("Level: warning"), "stdout: {plain}");
}

/// Colour follows the one rule: a pipe and `NO_COLOR` are plain, and
/// `CLICOLOR_FORCE` bolds the code even into a pipe.
#[test]
fn explain_is_plain_into_a_pipe_and_bold_when_forced() {
    let out = specforge()
        .env("NO_COLOR", "1")
        .env_remove("CLICOLOR_FORCE")
        .args(["explain", "E001"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.starts_with("E001: Parse error"),
        "stdout: {stdout:?}"
    );
    assert!(!stdout.contains('\x1b'), "stdout: {stdout:?}");

    let out = specforge()
        .env_remove("NO_COLOR")
        .env("CLICOLOR_FORCE", "1")
        .args(["explain", "E001"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.starts_with("\x1b[1mE001"), "stdout: {stdout:?}");
}

#[test]
fn explain_rejects_an_unknown_code_with_format_hints() {
    let out = specforge().args(["explain", "Z999"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "an unknown code exits 1");
    assert!(
        out.stdout.is_empty(),
        "nothing on stdout for an unknown code"
    );
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.starts_with(
            "error[unknown_code]: unknown diagnostic code: Z999\n  hint: codes are E###"
        ),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("E### (error), W### (warning), I### (info)"),
        "the format hint is printed: {stderr}"
    );
    assert!(
        stderr.contains("E900-E998, W900-W998 and I900-I998 are third-party extensions'"),
        "the third-party range hint is printed: {stderr}"
    );
}

#[test]
fn explain_reads_its_argument_as_given() {
    let out = specforge().args(["explain", " e001"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.starts_with("error[unknown_code]: unknown diagnostic code:  e001"),
        "stderr: {stderr}"
    );
}

#[test]
fn explain_takes_no_format_flag() {
    let out = specforge()
        .args(["explain", "e001", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}
