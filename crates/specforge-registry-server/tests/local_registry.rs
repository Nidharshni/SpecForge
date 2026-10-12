//! The real server in process (`specforge_registry_server::testing`), as the HTTP client sees it.

use sha2::{Digest, Sha256};
use specforge_registry_server::testing::LocalRegistry;
use specforge_registry_wire::{PackageMetadata, VersionList};

fn get(url: &str) -> reqwest::blocking::Response {
    reqwest::blocking::get(url).expect("the local registry answers")
}

#[test]
fn a_stored_package_is_served_as_published() {
    let registry = LocalRegistry::start();
    let wasm: &[u8] = b"\0asm unsigned bytes";
    // The download is checked against the stored digest, so store the real one.
    let sha256 = hex::encode(Sha256::digest(wasm));
    registry.store(
        &PackageMetadata {
            name: "@acme/tool".into(),
            version: "1.0.0".into(),
            sha256: sha256.clone(),
            wasm_url: "ignored".into(),
            ..Default::default()
        },
        wasm,
    );

    let list: VersionList = get(&format!("{}/packages/@acme%2Ftool", registry.url()))
        .json()
        .unwrap();
    assert_eq!(list.versions, ["1.0.0"]);

    let metadata: PackageMetadata = get(&format!("{}/packages/@acme%2Ftool/1.0.0", registry.url()))
        .json()
        .unwrap();
    assert_eq!(metadata.sha256, sha256);
    assert_eq!(metadata.wasm_url, "/packages/@acme%2Ftool/1.0.0/download");

    let download = get(&format!(
        "{}/packages/@acme%2Ftool/1.0.0/download",
        registry.url()
    ))
    .bytes()
    .unwrap();
    assert_eq!(download.as_ref(), wasm);

    assert_eq!(
        registry.requests(),
        [
            "GET /v1/packages/@acme%2Ftool",
            "GET /v1/packages/@acme%2Ftool/1.0.0",
            "GET /v1/packages/@acme%2Ftool/1.0.0/download",
        ]
    );
}

#[test]
fn versions_list_in_store_order() {
    let registry = LocalRegistry::start();
    for version in ["2.0.0", "1.0.0", "1.5.0"] {
        registry.store(
            &PackageMetadata {
                name: "@acme/tool".into(),
                version: version.into(),
                ..Default::default()
            },
            b"\0asm",
        );
    }
    let list: VersionList = get(&format!("{}/packages/@acme%2Ftool", registry.url()))
        .json()
        .unwrap();
    assert_eq!(list.versions, ["2.0.0", "1.0.0", "1.5.0"]);
}

#[test]
fn a_stored_signature_reads_back_typed() {
    use specforge_registry_wire::SignedContent;
    let registry = LocalRegistry::start();
    let wasm: &[u8] = b"\0asm signed bytes";
    let signature = specforge_registry_client::SigningKey::from_bytes(&[7; 32])
        .sign(&SignedContent::of("@acme/tool", "1.0.0", wasm, "{}"));
    registry.store(
        &PackageMetadata {
            name: "@acme/tool".into(),
            version: "1.0.0".into(),
            sha256: hex::encode(Sha256::digest(wasm)),
            signature: Some(signature.clone()),
            manifest: "{}".into(),
            ..Default::default()
        },
        wasm,
    );
    let metadata: PackageMetadata = get(&format!("{}/packages/@acme%2Ftool/1.0.0", registry.url()))
        .json()
        .unwrap();
    assert_eq!(metadata.signature, Some(signature));
}
