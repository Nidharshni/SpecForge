//! The user's registry files are private and replaced whole (ADR 0193): every file is created 0600 in a 0700
//! directory, never written through a link, and a signing key file is named, never quoted.
#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use specforge_registry_client::credentials::CredentialStore;
use specforge_registry_client::{KnownKeys, load_or_create_signing_key_at, save_known_keys_at};
use specforge_registry_wire::{RegistryRoot, sha256_hex};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn mode(path: &Path) -> u32 {
    std::fs::symlink_metadata(path)
        .unwrap()
        .permissions()
        .mode()
        & 0o777
}

fn pins() -> KnownKeys {
    let mut known = KnownKeys::default();
    known.pin("@acme/x", "k1");
    known
}

#[specforge_test_macros::test(
    invariant = "credential_secrecy",
    verify = "every file in the user's registry directory is created 0600 in a 0700 directory"
)]
#[ignore = "flipped by T3"]
fn a_first_signing_key_is_0600_in_a_0700_directory() {
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join(".specforge");
    load_or_create_signing_key_at(&dir.join("signing-key.json")).unwrap();
    assert_eq!(mode(&dir), 0o700, "the user's directory");
    assert_eq!(mode(&dir.join("signing-key.json")), 0o600);
}

#[specforge_test_macros::test(
    invariant = "credential_secrecy",
    verify = "every file in the user's registry directory is created 0600 in a 0700 directory"
)]
#[ignore = "flipped by T4"]
fn a_first_pin_save_is_0600_in_a_0700_directory() {
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join(".specforge");
    save_known_keys_at(&dir.join("known-keys.json"), &pins()).unwrap();
    assert_eq!(mode(&dir), 0o700, "the user's directory");
    assert_eq!(mode(&dir.join("known-keys.json")), 0o600);
}

#[specforge_test_macros::test(
    invariant = "credential_secrecy",
    verify = "a user's registry file is never written through a link at its temporary name"
)]
fn a_kept_secret_is_never_written_through_a_link_at_its_temporary_name() {
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join(".specforge");
    let root = RegistryRoot::parse("https://r.invalid").unwrap();
    let id = &sha256_hex(root.as_str().as_bytes())[..16];
    let secrets = dir.join("secrets");
    std::fs::create_dir_all(&secrets).unwrap();
    let outside = home.path().join("outside.txt");
    std::fs::write(&outside, "ORIGINAL").unwrap();
    // Today's temporary name and the private writer's.
    for name in [format!("{id}.tmp"), format!("{id}.json.tmp")] {
        std::os::unix::fs::symlink(&outside, secrets.join(name)).unwrap();
    }
    CredentialStore::empty(&dir)
        .keep_secret(root, "sfr_pin_secret".into(), None)
        .unwrap();
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "ORIGINAL");
    let secret = secrets.join(format!("{id}.json"));
    assert!(
        !std::fs::symlink_metadata(&secret)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(mode(&secret), 0o600);
}

#[specforge_test_macros::test(
    invariant = "credential_secrecy",
    verify = "a user's registry file is never written through a link at its temporary name"
)]
#[ignore = "flipped by T4"]
fn a_pin_save_is_never_written_through_a_link_at_its_temporary_name() {
    let home = tempfile::tempdir().unwrap();
    let outside = home.path().join("outside.txt");
    std::fs::write(&outside, "ORIGINAL").unwrap();
    std::os::unix::fs::symlink(&outside, home.path().join("known-keys.json.tmp")).unwrap();
    save_known_keys_at(&home.path().join("known-keys.json"), &pins()).unwrap();
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "ORIGINAL");
    assert_eq!(mode(&home.path().join("known-keys.json")), 0o600);
}

#[specforge_test_macros::test(
    invariant = "credential_secrecy",
    verify = "a corrupt signing key file is named, never quoted"
)]
#[ignore = "flipped by T3"]
fn a_corrupt_signing_key_file_is_named_never_quoted() {
    let home = tempfile::tempdir().unwrap();
    let key = home.path().join("signing-key.json");
    // A wrong-shaped value: serde_json's error would echo it.
    std::fs::write(&key, "\"ae9833f43338f507c0ffee\"").unwrap();
    let why = load_or_create_signing_key_at(&key).unwrap_err();
    assert!(why.contains("signing-key.json"), "{why}");
    assert!(!why.contains("ae9833f43338f507c0ffee"), "{why}");
    assert_eq!(
        std::fs::read_to_string(&key).unwrap(),
        "\"ae9833f43338f507c0ffee\""
    );
}
