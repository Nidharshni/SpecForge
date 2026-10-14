use specforge_registry_client::credentials::{
    CredentialEntry, CredentialStore, read_credentials, write_credentials,
};
use specforge_registry_client::{RegistryCredential, RegistryError};
use tempfile::TempDir;

#[test]
fn roundtrip_credentials() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("credentials.json");

    let mut store = CredentialStore::default();
    store.registries.insert(
        "default".to_string(),
        CredentialEntry::Token {
            token: "sfr_test_token_123".to_string(),
            expires_at: None,
            in_keyring: false,
        },
    );
    store.registries.insert(
        "private".to_string(),
        CredentialEntry::Token {
            token: "priv_token_456".to_string(),
            expires_at: None,
            in_keyring: false,
        },
    );

    write_credentials(&path, &store).unwrap();
    let loaded = read_credentials(&path).unwrap();

    assert_eq!(loaded.registries.len(), 2);
    assert!(loaded.registries.contains_key("default"));
    assert!(loaded.registries.contains_key("private"));
}

#[test]
fn read_nonexistent_returns_empty() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nonexistent.json");
    let store = read_credentials(&path).unwrap();
    assert!(store.registries.is_empty());
}

#[test]
fn get_credential_returns_bearer() {
    let mut store = CredentialStore::default();
    store.registries.insert(
        "myregistry".to_string(),
        CredentialEntry::Token {
            token: "my_token".to_string(),
            expires_at: None,
            in_keyring: false,
        },
    );

    let cred = store.credential("myregistry").unwrap().unwrap();
    assert_eq!(cred.alias, "myregistry");
    assert_eq!(cred.token(), "my_token");
}

#[specforge_test_macros::test(
    behavior = "authenticate_registry_request",
    verify = "raw tokens never logged or stored in config"
)]
fn a_credentials_debug_never_shows_its_token() {
    let credential = RegistryCredential::new("a", "secret-token-value");
    let shown = format!("{credential:?}");
    assert!(shown.contains("****"), "{shown}");
    assert!(!shown.contains("secret"), "{shown}");
}

#[specforge_test_macros::test(
    behavior = "authenticate_registry_request",
    verify = "a 401 is R001 naming how to log in again"
)]
fn a_401_is_r001_naming_how_to_log_in_again() {
    let diagnostic = RegistryError::Unauthorized {
        guidance: "token expired or invalid".to_string(),
    }
    .to_diagnostic();
    assert_eq!(diagnostic.code, "R001");
    assert!(
        diagnostic
            .suggestion
            .unwrap()
            .contains("specforge login --registry"),
    );
}

#[test]
fn remove_credential() {
    let mut store = CredentialStore::default();
    store.registries.insert(
        "temp".to_string(),
        CredentialEntry::Token {
            token: "token".to_string(),
            expires_at: None,
            in_keyring: false,
        },
    );
    assert!(store.remove("temp"));
    assert!(!store.remove("temp"));
    assert!(store.registries.is_empty());
}
