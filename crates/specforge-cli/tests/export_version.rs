//! C6-02 acceptance: `--schema-version` negotiates a real same-major range.

use crate::support::{Cli, TestProject};

/// A project whose sources are under `src/`.
fn seeded() -> TestProject {
    TestProject::new()
        .raw_config(r#"{"name":"v","spec_root":"src","extensions":["@specforge/product"]}"#)
        .file(
            "src/a.spec",
            "type Widget {\n  id string @unique\n  verify unit \"widget valid\"\n}\n",
        )
}

#[test]
fn schema_version_within_same_major_is_accepted() {
    let tmp = seeded();
    // Same major as the produced schema (1.x): accepted by the negotiated range.
    let out = Cli::on(&tmp)
        .bare_command(&[
            "export",
            "--path",
            tmp.root().join("src").to_str().unwrap(),
            "--format",
            "graph",
            "--schema-version",
            "1.0.0",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "same-major request should pass: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"schema_version\":\"1.0.0\""),
        "the project's own version (compact)"
    );
}

#[test]
fn schema_version_other_major_is_rejected_with_range() {
    let tmp = seeded();
    let out = Cli::on(&tmp)
        .bare_command(&[
            "export",
            "--path",
            tmp.root().join("src").to_str().unwrap(),
            "--format",
            "graph",
            "--schema-version",
            "2.0.0",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("2.0.0"),
        "names the rejected version: {stderr}"
    );
    assert!(
        stderr.starts_with("error[incompatible_schema_version]"),
        "{stderr}"
    );
    assert!(
        stderr.contains("another major version"),
        "explains the failure: {stderr}"
    );
}

#[specforge_test_macros::test(
    behavior = "negotiate_schema_version",
    verify = "a requested version of another major, or a newer one, is refused as incompatible_schema_version naming the readable range"
)]
fn a_refused_schema_version_is_not_a_peer_error() {
    let tmp = seeded();
    let root = tmp.root().join("src");
    let root = root.to_str().unwrap();

    let graph = Cli::on(&tmp)
        .bare_command(&[
            "export",
            "--path",
            root,
            "--format",
            "graph",
            "--schema-version",
            "2.0.0",
        ])
        .output()
        .unwrap();
    assert_eq!(graph.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&graph.stderr);
    assert!(
        stderr.starts_with("error[incompatible_schema_version]: requested schema version 2.0.0 has another major version"),
        "{stderr}"
    );

    // The request is checked whatever the format.
    let brief = Cli::on(&tmp)
        .bare_command(&[
            "export",
            "--path",
            root,
            "--format",
            "brief",
            "--schema-version",
            "9.0.0",
        ])
        .output()
        .unwrap();
    assert_eq!(brief.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&brief.stderr);
    assert!(stderr.contains("incompatible_schema_version"), "{stderr}");
}
