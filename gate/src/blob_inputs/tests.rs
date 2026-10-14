use super::*;
use specforge_repo_root::repo_root;

/// The builtins a change feeds; the fixtures that record their inputs are left out.
fn fed(paths: &[&str]) -> Vec<String> {
    let mut fed = fed_by(repo_root(), &ChangeSet::of(paths.iter().copied())).unwrap();
    fed.retain(|crate_dir| crate_dir.starts_with("extensions/"));
    fed
}

/// The blobs that record their inputs today: the nine builtins. The fixtures join at the round's re-vendor.
const NINE: [&str; 9] = [
    "extensions/cargo-test",
    "extensions/formal",
    "extensions/governance",
    "extensions/product",
    "extensions/rust",
    "extensions/software",
    "extensions/testing",
    "extensions/typescript",
    "extensions/vitest",
];

#[test]
fn a_record_round_trips_byte_for_byte() {
    let scratch = tempfile::tempdir().unwrap();
    for builtin in blobs(repo_root()).unwrap() {
        let record = BlobRecord::read(repo_root(), &builtin).unwrap();
        record.write(scratch.path(), &builtin).unwrap();
        let original = std::fs::read(repo_root().join(shown(&builtin))).unwrap();
        let written = std::fs::read(scratch.path().join(shown(&builtin))).unwrap();
        assert_eq!(original, written, "{builtin}");
    }
}

#[test]
fn a_change_to_coverage_feeds_the_blobs_that_record_it() {
    assert_eq!(
        fed(&["crates/leaf/coverage/src/lib.rs"]),
        [
            "extensions/formal",
            "extensions/product",
            "extensions/testing"
        ]
    );
}

#[test]
fn protocol_types_and_the_root_manifest_feed_all_nine() {
    assert_eq!(fed(&["crates/leaf/protocol-types/src/lib.rs"]), NINE);
    assert_eq!(fed(&["Cargo.toml"]), NINE);
    assert_eq!(fed(&["crates/runtime/extension-sdk/src/lib.rs"]), NINE);
}

#[test]
fn a_test_only_sdk_file_feeds_no_blob() {
    assert!(fed(&["crates/runtime/extension-sdk/src/testing.rs"]).is_empty());
    assert!(fed(&["crates/surfaces/lsp/src/lib.rs"]).is_empty());
}

#[test]
fn blobs_lists_the_recorded_blobs() {
    let listed = blobs(repo_root()).unwrap();
    assert!(NINE.iter().all(|nine| listed.iter().any(|l| l == nine)));
    assert!(
        listed
            .iter()
            .all(|l| l.starts_with("extensions/") || l.starts_with("fixtures/"))
    );
}

#[test]
fn inherited_tables_ignore_members_and_profiles() {
    let a = "[workspace]\nmembers = [\"x\"]\n[workspace.package]\nedition = \"2024\"\n\
             [workspace.dependencies]\nserde = \"1\"\n[profile.release]\nlto = true\n";
    let b = "[workspace]\nmembers = [\"x\", \"y\"]\n[workspace.package]\nedition = \"2024\"\n\
             [workspace.dependencies]\nserde = \"1\"\n[profile.release]\nlto = false\n";
    assert_eq!(inherited_tables(a), inherited_tables(b));

    let c = a.replace(
        "serde = \"1\"",
        "serde = { version = \"1\", features = [\"rc\"] }",
    );
    assert_ne!(inherited_tables(a), inherited_tables(&c));
}

#[test]
fn a_key_names_the_repo_path_it_hashes() {
    assert_eq!(path_of(WORKSPACE_INHERITED), "Cargo.toml");
    assert_eq!(path_of("crates/x/src/lib.rs"), "crates/x/src/lib.rs");
}
