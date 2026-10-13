//! `specforge explain <CODE>`: the CLI adapter over the diagnostic catalog.
//! These pin what a user sees, so the catalog can move or grow without the
//! command's output or exit codes changing by accident.

use crate::support::{Cli, TestProject};

/// A directory no project is in: `explain` reads none.
fn nowhere() -> TestProject {
    TestProject::new().without_config()
}

#[specforge_test_macros::test(
    behavior = "explain_diagnostic_code",
    verify = "specforge explain prints the catalogued entry with its docs link, and a retired code with the entry that replaced it"
)]
fn explain_prints_a_catalogued_code_case_insensitively() {
    let nowhere = nowhere();
    let out = Cli::on(&nowhere)
        .bare_command(&["explain", "e001"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "a catalogued code exits 0");
    let stdout = String::from_utf8(out.stdout).unwrap();
    // The code is printed bold; strip the escapes before matching.
    let plain = stdout.replace("\x1b[1m", "").replace("\x1b[0m", "");
    assert!(plain.contains("E001: Parse error"), "stdout: {plain}");
    assert!(plain.contains("Owner: core"), "stdout: {plain}");
    assert!(plain.contains("Level: error"), "stdout: {plain}");
    assert_eq!(
        plain.lines().last(),
        Some("Docs: https://github.com/leaderiop/SpecForge/blob/main/docs/diagnostics.md#e001"),
        "stdout: {plain}"
    );
}

#[specforge_test_macros::test(
    behavior = "explain_diagnostic_code",
    verify = "specforge explain prints the catalogued entry with its docs link, and a retired code with the entry that replaced it"
)]
fn explain_points_a_retired_code_at_its_replacement() {
    let nowhere = nowhere();
    let out = Cli::on(&nowhere)
        .bare_command(&["explain", "E047"])
        .output()
        .unwrap();
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
#[specforge_test_macros::test(
    behavior = "print_diagnostics_structured",
    verify = "specforge explain bolds the code only where its output is coloured"
)]
fn explain_is_plain_into_a_pipe_and_bold_when_forced() {
    let nowhere = nowhere();
    let out = Cli::on(&nowhere)
        .bare_command(&[])
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

    let out = Cli::on(&nowhere)
        .bare_command(&[])
        .env_remove("NO_COLOR")
        .env("CLICOLOR_FORCE", "1")
        .args(["explain", "E001"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.starts_with("\x1b[1mE001"), "stdout: {stdout:?}");
}

#[specforge_test_macros::test(
    behavior = "explain_diagnostic_code",
    verify = "an uncatalogued code is refused with unknown_code on both surfaces"
)]
fn explain_rejects_an_unknown_code_with_format_hints() {
    let nowhere = nowhere();
    let out = Cli::on(&nowhere)
        .bare_command(&["explain", "Z999"])
        .output()
        .unwrap();
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

    // Under --format json the refusal is the error document on stdout, nothing on stderr.
    let out = Cli::on(&nowhere)
        .bare_command(&["explain", "Z999", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stderr.is_empty(), "{:?}", out.stderr);
    let document: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(document["code"], "unknown_code");
    assert_eq!(document["error"], "unknown diagnostic code: Z999");
    assert!(
        document["suggestion"]
            .as_str()
            .unwrap()
            .starts_with("codes are E###"),
        "{document}"
    );
}

#[test]
fn explain_reads_its_code_as_specforge_explain_does() {
    let nowhere = nowhere();
    let out = Cli::on(&nowhere)
        .bare_command(&["explain", " e001"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Parse error"), "stdout: {stdout}");
}

#[specforge_test_macros::test(
    behavior = "explain_diagnostic_code",
    verify = "specforge explain --format json prints the document specforge.explain answers"
)]
fn explain_prints_its_document_under_format_json() {
    let nowhere = nowhere();
    let out = Cli::on(&nowhere)
        .bare_command(&["explain", "e001", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let document: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(document["code"], "E001");
    assert_eq!(document["retired"], false);
    assert_eq!(
        document["docs"],
        "https://github.com/leaderiop/SpecForge/blob/main/docs/diagnostics.md#e001"
    );
}
