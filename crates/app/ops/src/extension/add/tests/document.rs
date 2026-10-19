use super::*;

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "the add document lists what the add did for each specifier by action, with source always and key_id only for a signed package"
)]
fn add_document_names_what_the_add_did() {
    use specforge_installed::LockSource;

    let installed_from = |publisher, source| AddOutcome::Installed {
        name: "@acme/x".into(),
        version: "1.0.0".into(),
        sha256: "ab".into(),
        publisher,
        origin: Origin::Installed { source },
        peers_enabled: Vec::new(),
    };
    let signed = Publisher::Signed {
        key_id: "k1".into(),
        first_use: true,
    };
    let cases = [
        (
            AddOutcome::Builtin {
                name: "@specforge/product",
                changed: true,
                peers_enabled: vec!["@specforge/software"],
            },
            serde_json::json!({"specifier": "s", "action": "enabled", "extension": "@specforge/product",
                "source": "builtin", "changed": true, "peers_enabled": ["@specforge/software"]}),
        ),
        (
            installed_from(Some(Publisher::Unsigned), LockSource::Registry),
            serde_json::json!({"specifier": "s", "action": "installed", "extension": "@acme/x", "version": "1.0.0",
                "sha256": "ab", "source": "registry", "publisher": "unsigned", "peers_enabled": []}),
        ),
        (
            installed_from(Some(signed), LockSource::Registry),
            serde_json::json!({"specifier": "s", "action": "installed", "extension": "@acme/x", "version": "1.0.0",
                "sha256": "ab", "source": "registry", "key_id": "k1", "publisher": "pinned_now", "peers_enabled": []}),
        ),
        (
            installed_from(None, LockSource::Local("x.wasm".into())),
            serde_json::json!({"specifier": "s", "action": "installed", "extension": "@acme/x", "version": "1.0.0",
                "sha256": "ab", "source": "local:x.wasm", "peers_enabled": []}),
        ),
        (
            AddOutcome::AlreadyPresent {
                name: "@acme/x".into(),
                version: "1.0.0".into(),
            },
            serde_json::json!({"specifier": "s", "action": "already_present", "extension": "@acme/x", "version": "1.0.0"}),
        ),
        (
            AddOutcome::Planned {
                name: "@specforge/product".into(),
                version: None,
                origin: Origin::Builtin,
                peers_enabled: vec!["@specforge/software"],
            },
            serde_json::json!({"specifier": "s", "action": "planned", "extension": "@specforge/product", "source": "builtin", "peers_enabled": ["@specforge/software"]}),
        ),
    ];
    let cases = cases.into_iter().chain([(
        AddOutcome::Duplicate {
            name: "@acme/x".into(),
            first: "@acme/x@1".into(),
        },
        serde_json::json!({"specifier": "s", "action": "duplicate", "extension": "@acme/x",
            "duplicate_of": "@acme/x@1"}),
    )]);
    let (outcomes, documents): (Vec<_>, Vec<_>) = cases.unzip();
    let added = Added {
        added: outcomes
            .into_iter()
            .map(|outcome| AddedExtension {
                specifier: "s".into(),
                outcome,
            })
            .collect(),
        writes: Writes::none(),
        extensions_enabled: 1,
        dry_run: false,
    };
    assert_eq!(
        serde_json::to_value(added.document()).unwrap(),
        serde_json::json!({ "dry_run": false, "added": documents })
    );
    assert!(
        AddOutcome::AlreadyPresent {
            name: "a".into(),
            version: "1".into()
        }
        .is_duplicate()
    );
    assert!(!installed_from(None, LockSource::Registry).is_duplicate());
    assert!(
        AddOutcome::Duplicate {
            name: "a".into(),
            first: "a".into()
        }
        .is_duplicate()
    );
}
