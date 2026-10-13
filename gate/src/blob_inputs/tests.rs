use super::*;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn fed(paths: &[&str]) -> Vec<String> {
    fed_by(&root(), &ChangeSet::of(paths.iter().copied())).unwrap()
}

const NINE: [&str; 9] = [
    "cargo-test",
    "formal",
    "governance",
    "product",
    "rust",
    "software",
    "testing",
    "typescript",
    "vitest",
];

#[test]
fn a_record_round_trips_byte_for_byte() {
    let scratch = tempfile::tempdir().unwrap();
    for builtin in builtins(&root()).unwrap() {
        let record = BlobRecord::read(&root(), &builtin).unwrap();
        record.write(scratch.path(), &builtin).unwrap();
        let original = std::fs::read(root().join(shown(&builtin))).unwrap();
        let written = std::fs::read(scratch.path().join(shown(&builtin))).unwrap();
        assert_eq!(original, written, "{builtin}");
    }
}

#[test]
fn a_change_to_coverage_feeds_the_blobs_that_record_it() {
    assert_eq!(
        fed(&["crates/specforge-coverage/src/lib.rs"]),
        ["formal", "product", "testing"]
    );
}

#[test]
fn protocol_types_and_the_root_manifest_feed_all_nine() {
    assert_eq!(fed(&["crates/specforge-protocol-types/src/lib.rs"]), NINE);
    assert_eq!(fed(&["Cargo.toml"]), NINE);
    assert_eq!(fed(&["crates/specforge-extension-sdk/src/lib.rs"]), NINE);
}

#[test]
fn a_test_only_sdk_file_feeds_no_blob() {
    assert!(fed(&["crates/specforge-extension-sdk/src/testing.rs"]).is_empty());
    assert!(fed(&["crates/specforge-lsp/src/lib.rs"]).is_empty());
}

#[test]
fn builtins_lists_the_nine() {
    assert_eq!(builtins(&root()).unwrap(), NINE);
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
