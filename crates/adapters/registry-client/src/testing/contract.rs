use specforge_package::PackageName;
use specforge_package::Version;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry_wire::{
    ErrorCode, PackageSignature, PublishReceipt, SearchQuery, SignedContent, address, sha256_hex,
};

use crate::registry_client::{RegistryClient, RegistryError};
use crate::registry_config::{RegistryConfig, RegistryCredential};
use crate::{SigningKey, TrustCheck};

/// The declaration the contract publishes: `@contract/tool` at `version`, described "Contract tool".
fn contract_declaration(version: &str) -> ExtensionDeclaration {
    serde_json::from_value(serde_json::json!({
        "handshake": {
            "protocol_version": "1.0.0",
            "name": "@contract/tool",
            "version": version,
            "description": "Contract tool",
            "keywords": ["contract"],
            "peer_dependencies": [],
            "sandbox_policy": null
        }
    }))
    .expect("the contract declaration is a declaration")
}

/// The client contract (ADR 0044): what every `RegistryClient` adapter does against a registry that holds
/// nothing yet. `credential` must be one `registry` accepts for publishing. Panics naming the clause that
/// failed.
pub fn assert_client_contract(
    client: &dyn RegistryClient,
    registry: &RegistryConfig,
    credential: &RegistryCredential,
) {
    let tool = PackageName::parse("@contract/tool").unwrap();
    let key = SigningKey::generate();

    // K1
    let error = client.versions(&tool, registry, None).unwrap_err();
    assert!(
        matches!(error, RegistryError::NotFound { .. }),
        "K1: versions before any publish is NotFound, not {error:?}"
    );

    // K2
    let mut declarations = Vec::new();
    for version in ["1.0.0", "1.1.0"] {
        let declaration = contract_declaration(version);
        let wasm = format!("\0asm contract tool {version}").into_bytes();
        let url = crate::publish_to_registry(
            &wasm,
            &declaration,
            registry,
            Some(credential),
            client,
            &key,
        )
        .unwrap_or_else(|d| panic!("K2: publishing {version} fails: {d:?}"));
        let expected = registry.url.address(&address::Call::Version(
            &tool,
            &Version::parse(version).unwrap(),
        ));
        assert_eq!(url, expected, "K2: publish answers the version's URL");
        declarations.push((declaration, wasm));
    }

    // K3
    let mut versions = client
        .versions(&tool, registry, None)
        .expect("K3: versions");
    versions.sort();
    assert_eq!(versions, ["1.0.0", "1.1.0"], "K3: the published versions");

    // K4
    let (declaration, wasm) = &declarations[1];
    let metadata = client
        .metadata(&tool, &Version::new(1, 1, 0), registry, None)
        .expect("K4: metadata of 1.1.0");
    assert_eq!(metadata.name, "@contract/tool", "K4: name");
    assert_eq!(metadata.version, "1.1.0", "K4: version");
    assert_eq!(metadata.sha256, sha256_hex(wasm), "K4: sha256");
    assert_eq!(metadata.size_bytes, wasm.len() as u64, "K4: size_bytes");
    assert_eq!(
        metadata.manifest,
        serde_json::to_string(declaration).unwrap(),
        "K4: the manifest is the exact JSON uploaded"
    );
    assert_eq!(metadata.description, "Contract tool", "K4: description");
    assert_eq!(metadata.keywords, ["contract"], "K4: keywords");
    assert_eq!(
        metadata.signature.as_ref().map(PackageSignature::key_id),
        Some(key.key_id()),
        "K4: the signature is the publisher key's"
    );
    assert!(!metadata.wasm_url.is_empty(), "K4: wasm_url");
    assert!(
        reqwest::Url::parse(&metadata.wasm_url).is_ok(),
        "K4: wasm_url is absolute"
    );

    // K5
    let bytes = client
        .download(&metadata.wasm_url, registry, None)
        .expect("K5: download of the metadata's wasm_url");
    assert_eq!(&bytes, wasm, "K5: the published bytes");
    assert_eq!(
        crate::verify_package_signature(&metadata, &bytes).expect("K5: the signature verifies"),
        TrustCheck::Verified {
            key_id: key.key_id()
        },
        "K5: verified by the publisher key"
    );

    // K6
    let error = client
        .metadata(&tool, &Version::new(9, 9, 9), registry, None)
        .unwrap_err();
    assert!(
        matches!(error, RegistryError::NotFound { .. }),
        "K6: metadata of an unpublished version is NotFound, not {error:?}"
    );

    // K7
    let manifest = serde_json::to_string(declaration).unwrap();
    let signature = key.sign(&SignedContent::of(
        "@contract/tool",
        "1.1.0",
        wasm,
        &manifest,
    ));
    let error = client
        .publish(
            wasm,
            declaration,
            &manifest,
            &signature,
            registry,
            Some(credential),
        )
        .unwrap_err();
    assert!(
        matches!(error, RegistryError::DuplicateVersion { .. }),
        "K7: publishing 1.1.0 again is DuplicateVersion, not {error:?}"
    );

    // K8
    let hits = client
        .search(&SearchQuery::new("contract"), registry, None)
        .expect("K8: search");
    assert_eq!(hits.len(), 1, "K8: one hit for \"contract\": {hits:?}");
    assert_eq!(hits[0].name, "@contract/tool", "K8: the hit's name");
    assert_eq!(hits[0].version, "1.1.0", "K8: the latest version");
    assert_eq!(hits[0].description, "Contract tool", "K8: the description");
    // A query is text: case is ignored, `_` and `%` are no wildcards.
    let found = |q: &str| {
        client
            .search(&SearchQuery::new(q), registry, None)
            .expect("K8: search")
            .len()
    };
    assert_eq!(found("CONTRACT"), 1, "K8: case is ignored");
    assert_eq!(found("t_ol"), 0, "K8: _ is text");
    assert_eq!(found("%"), 0, "K8: % is text");
    assert_eq!(
        client
            .search(&SearchQuery::new("a&b=c#d"), registry, None)
            .expect("K8: odd query"),
        vec![],
        "K8: a query cannot change the parameters"
    );

    // K9
    client
        .authenticate(registry, credential)
        .expect("K9: the credential authenticates");
    let wrong = RegistryCredential::new(credential.alias.clone(), "not-a-token");
    let error = client.authenticate(registry, &wrong).unwrap_err();
    assert!(
        matches!(error, RegistryError::Unauthorized { .. }),
        "K9: a token the registry does not know is Unauthorized, not {error:?}"
    );

    // K10
    let next = contract_declaration("2.0.0");
    let manifest = serde_json::to_string(&next).unwrap();
    let signature = key.sign(&SignedContent::of(
        "@contract/tool",
        "2.0.0",
        b"\0asm next",
        &manifest,
    ));
    let error = client
        .publish(b"\0asm next", &next, &manifest, &signature, registry, None)
        .unwrap_err();
    assert!(
        matches!(error, RegistryError::Unauthorized { .. }),
        "K10: publishing with no credential is Unauthorized, not {error:?}"
    );

    // K11
    let next_wasm = b"\0asm next";
    let mismatched = key.sign(&SignedContent::of(
        "@contract/tool",
        "2.0.0",
        b"\0asm other",
        &manifest,
    ));
    let error = client
        .publish(
            next_wasm,
            &next,
            &manifest,
            &mismatched,
            registry,
            Some(credential),
        )
        .unwrap_err();
    assert!(
        matches!(
            error,
            RegistryError::Refused {
                code: ErrorCode::InvalidSignature,
                ..
            }
        ),
        "K11: a signature over other bytes is Refused(InvalidSignature), not {error:?}"
    );
    let error = client
        .metadata(&tool, &Version::new(2, 0, 0), registry, None)
        .unwrap_err();
    assert!(
        matches!(error, RegistryError::NotFound { .. }),
        "K11: a refused publish stores nothing, not {error:?}"
    );
    let signature = key.sign(&SignedContent::of(
        "@contract/tool",
        "2.0.0",
        next_wasm,
        &manifest,
    ));
    let receipt = client
        .publish(
            next_wasm,
            &next,
            &manifest,
            &signature,
            registry,
            Some(credential),
        )
        .expect("K11: the same version, correctly signed, publishes");
    assert_eq!(
        receipt,
        PublishReceipt {
            name: "@contract/tool".into(),
            version: "2.0.0".into(),
            sha256: sha256_hex(next_wasm),
            size_bytes: next_wasm.len() as u64,
            key_id: key.key_id(),
        },
        "K11: the receipt names what was stored"
    );

    // K12: an upload is the package its manifest declares: a manifest naming another version than the upload,
    // or one that is no declaration, is refused and stores nothing.
    let four = contract_declaration("4.0.0");
    let wasm = b"\0asm four";
    let three = serde_json::to_string(&contract_declaration("3.0.0")).unwrap();
    let signature = key.sign(&SignedContent::of("@contract/tool", "4.0.0", wasm, &three));
    let error = client
        .publish(wasm, &four, &three, &signature, registry, Some(credential))
        .unwrap_err();
    assert!(
        matches!(
            error,
            RegistryError::Refused {
                code: ErrorCode::NameMismatch,
                ..
            }
        ),
        "K12: a manifest naming another version is Refused(NameMismatch), not {error:?}"
    );
    let junk = "not a declaration";
    let signature = key.sign(&SignedContent::of("@contract/tool", "4.0.0", wasm, junk));
    let error = client
        .publish(wasm, &four, junk, &signature, registry, Some(credential))
        .unwrap_err();
    assert!(
        matches!(
            error,
            RegistryError::Refused {
                code: ErrorCode::InvalidManifest,
                ..
            }
        ),
        "K12: a manifest that is no declaration is Refused(InvalidManifest), not {error:?}"
    );
    for version in [Version::new(3, 0, 0), Version::new(4, 0, 0)] {
        let error = client
            .metadata(&tool, &version, registry, None)
            .unwrap_err();
        assert!(
            matches!(error, RegistryError::NotFound { .. }),
            "K12: a refused publish stores nothing, not {error:?}"
        );
    }
}

/// The private client contract (plan 16): what every `RegistryClient` adapter does against a registry that
/// requires a token to read and holds nothing yet. `credential` must be one `registry` accepts.
pub fn assert_private_client_contract(
    client: &dyn RegistryClient,
    registry: &RegistryConfig,
    credential: &RegistryCredential,
) {
    let tool = PackageName::parse("@contract/tool").unwrap();
    let unauthorized = |what: &str, result: Result<(), RegistryError>| {
        assert!(
            matches!(result, Err(RegistryError::Unauthorized { .. })),
            "K-P1: {what} without a credential is Unauthorized, not {result:?}"
        );
    };

    // K-P1
    unauthorized(
        "versions",
        client.versions(&tool, registry, None).map(|_| ()),
    );
    unauthorized(
        "metadata",
        client
            .metadata(&tool, &Version::new(1, 0, 0), registry, None)
            .map(|_| ()),
    );
    unauthorized(
        "download",
        client
            .download(
                &registry
                    .url
                    .address(&address::Call::Download(&tool, &Version::new(1, 0, 0))),
                registry,
                None,
            )
            .map(|_| ()),
    );
    unauthorized(
        "search",
        client
            .search(&SearchQuery::new("x"), registry, None)
            .map(|_| ()),
    );

    // K-P2
    let error = client
        .versions(&tool, registry, Some(credential))
        .unwrap_err();
    assert!(
        matches!(error, RegistryError::NotFound { .. }),
        "K-P2: versions with the credential is NotFound, not {error:?}"
    );
    assert_eq!(
        client
            .search(&SearchQuery::new("x"), registry, Some(credential))
            .expect("K-P2: search with the credential"),
        vec![],
        "K-P2: nothing is published yet"
    );

    // K-P3
    let declaration = contract_declaration("1.0.0");
    let wasm = b"\0asm contract tool private".to_vec();
    let key = SigningKey::generate();
    crate::publish_to_registry(
        &wasm,
        &declaration,
        registry,
        Some(credential),
        client,
        &key,
    )
    .expect("K-P3: publishing with the credential");
    let metadata = client
        .metadata(&tool, &Version::new(1, 0, 0), registry, Some(credential))
        .expect("K-P3: metadata with the credential");
    let bytes = client
        .download(&metadata.wasm_url, registry, Some(credential))
        .expect("K-P3: download with the credential");
    assert_eq!(bytes, wasm, "K-P3: the published bytes");
    unauthorized(
        "a published version's metadata",
        client
            .metadata(&tool, &Version::new(1, 0, 0), registry, None)
            .map(|_| ()),
    );
    unauthorized(
        "a published version's download",
        client
            .download(&metadata.wasm_url, registry, None)
            .map(|_| ()),
    );

    // K-P4
    client
        .authenticate(registry, credential)
        .expect("K-P4: the credential authenticates");
    let wrong = RegistryCredential::new(credential.alias.clone(), "not-a-token");
    assert!(
        matches!(
            client.authenticate(registry, &wrong),
            Err(RegistryError::Unauthorized { .. })
        ),
        "K-P4: a token the registry does not know is Unauthorized"
    );
}
