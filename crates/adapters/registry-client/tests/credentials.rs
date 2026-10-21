use specforge_registry_client::credentials::{CredentialEntry, CredentialStore};
use specforge_registry_client::{RegistryCredential, RegistryError};
use specforge_registry_wire::RegistryRoot;
use tempfile::TempDir;

fn root(name: &str) -> RegistryRoot {
    RegistryRoot::parse(&format!("https://{name}.invalid")).unwrap()
}

fn token(token: &str) -> CredentialEntry {
    CredentialEntry::Token {
        token: token.to_string(),
        expires_at: None,
    }
}

#[test]
fn roundtrip_credentials() {
    let dir = TempDir::new().unwrap();

    let mut store = CredentialStore::empty(dir.path());
    store
        .registries
        .insert(root("default"), token("sfr_test_token_123"));
    store
        .registries
        .insert(root("private"), token("priv_token_456"));

    store.write().unwrap();
    let loaded = CredentialStore::read(dir.path()).unwrap();

    assert_eq!(loaded.registries.len(), 2);
    assert!(loaded.registries.contains_key(&root("default")));
    assert!(loaded.registries.contains_key(&root("private")));
}

#[test]
fn read_nonexistent_returns_empty() {
    let dir = TempDir::new().unwrap();
    let store = CredentialStore::read(dir.path()).unwrap();
    assert!(store.registries.is_empty());
}

#[test]
fn get_credential_returns_bearer() {
    let mut store = CredentialStore::empty("unused");
    store
        .registries
        .insert(root("myregistry"), token("my_token"));

    let cred = store.credential(&root("myregistry")).unwrap().unwrap();
    assert_eq!(cred.root, root("myregistry"));
    assert_eq!(cred.token(), "my_token");
}

#[test]
fn a_store_keyed_by_aliases_is_unreadable() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("credentials.json"),
        r#"{"registries":{"main":{"token_env":"X"}}}"#,
    )
    .unwrap();

    let error = CredentialStore::read(dir.path()).unwrap_err();

    assert_eq!(error.code, "R012");
    assert!(error.message.contains("main"), "{}", error.message);
}

#[specforge_test_macros::test(
    behavior = "authenticate_registry_request",
    verify = "raw tokens never logged or stored in config"
)]
fn a_credentials_debug_never_shows_its_token() {
    let credential = RegistryCredential::new(root("a"), "secret-token-value");
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
    let dir = TempDir::new().unwrap();
    let mut store = CredentialStore::empty(dir.path());
    store.registries.insert(root("temp"), token("token"));
    assert!(store.forget(&root("temp")));
    assert!(!store.forget(&root("temp")));
    assert!(store.registries.is_empty());
}
