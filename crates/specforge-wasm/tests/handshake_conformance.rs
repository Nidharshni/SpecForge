//! Fixture conformance for the extension handshake/describe protocol
//! (audit C7-07): every builtin's pinned wire answers (`handshake.json` and
//! `describe_*.json` under `crates/specforge-component/tests/declarations/`,
//! the exact bytes each vendored blob answers) must deserialize against the
//! shared Rust protocol types, and the handshake's critical fields must be
//! present — a truncated handshake now FAILS deserialization instead of
//! silently yielding no peer dependencies / no
//! sandbox limits.

use std::fs;
use std::path::{Path, PathBuf};

use specforge_protocol_types::{DescribeResponse, HandshakeResponse};

/// Handshake fields the host requires on the wire. Drift between this list
/// and `HandshakeResponse`'s required fields fails the conformance test.
const REQUIRED_HANDSHAKE_KEYS: &[&str] = &[
    "protocol_version",
    "name",
    "version",
    "peer_dependencies",
    "sandbox_policy",
];

fn declarations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../specforge-component/tests/declarations")
}

/// Every pinned declaration (one directory per builtin, and greet).
fn extension_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(declarations_dir())
        .expect("the pinned declarations exist")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("handshake.json").is_file())
        .collect();
    dirs.sort();
    assert!(
        dirs.len() >= 10,
        "expected every builtin's pinned declaration"
    );
    dirs
}

#[test]
fn every_builtin_handshake_fixture_parses_against_protocol_types() {
    for dir in extension_dirs() {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        let path = dir.join("handshake.json");
        let raw = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!("declarations/{name}/handshake.json missing or unreadable: {e}")
        });

        let value: serde_json::Value = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("declarations/{name}/handshake.json is not JSON: {e}"));

        let missing: Vec<&str> = REQUIRED_HANDSHAKE_KEYS
            .iter()
            .copied()
            .filter(|key| !value.get(key).is_some())
            .collect();
        assert!(
            missing.is_empty(),
            "declarations/{name}/handshake.json drifted: missing required handshake field(s) \
             {missing:?} — the host now REJECTS handshakes without them (C7-07). \
             Fix the fixture (it is the source of truth the guest blob embeds)."
        );

        let response: HandshakeResponse = serde_json::from_str(&raw).unwrap_or_else(|e| {
            panic!(
                "declarations/{name}/handshake.json no longer deserializes as \
                     HandshakeResponse: {e}"
            )
        });
        assert!(
            !response.protocol_version.is_empty() && !response.name.is_empty(),
            "declarations/{name}/handshake.json has empty protocol_version/name"
        );
    }
}

#[test]
fn every_builtin_describe_fixture_parses_against_protocol_types() {
    for dir in extension_dirs() {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        let mut describe_fixtures: Vec<PathBuf> = fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("declarations/{name} unreadable: {e}"))
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .is_some_and(|f| f.to_string_lossy().starts_with("describe_"))
            })
            .collect();
        describe_fixtures.sort();
        assert!(
            !describe_fixtures.is_empty(),
            "declarations/{name} has no describe_*.json fixtures"
        );

        for path in describe_fixtures {
            let file = path.file_name().unwrap().to_string_lossy().to_string();
            let raw = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("declarations/{name}/{file} unreadable: {e}"));
            let response: DescribeResponse = serde_json::from_str(&raw).unwrap_or_else(|e| {
                panic!("declarations/{name}/{file} no longer deserializes as DescribeResponse: {e}")
            });
            // `items` is a raw JSON array of typed descriptors.
            assert!(
                response.items.is_array(),
                "declarations/{name}/{file} items must be a JSON array"
            );
        }
    }
}

#[test]
fn handshake_with_all_fields_round_trips() {
    let raw = r#"{
        "protocol_version": "1.0.0",
        "name": "@acme/test",
        "version": "0.1.0",
        "providers": true,
        "peer_dependencies": [],
        "sandbox_policy": { "max_execution_ms": 5000 }
    }"#;
    let response: HandshakeResponse = serde_json::from_str(raw).expect("full handshake parses");
    assert_eq!(response.name, "@acme/test");
    assert!(response.providers);
    assert_eq!(
        response.sandbox_policy.and_then(|p| p.max_execution_ms),
        Some(5000)
    );
}

/// A guest whose handshake answers `protocol_version`, whatever the SDK it
/// is served with would say.
fn guest_answering(protocol_version: &str) -> specforge_wasm::testing::InProcessRuntime {
    use specforge_extension_sdk::prelude::*;
    let handshake = serde_json::to_vec(&HandshakeResponse {
        protocol_version: protocol_version.to_string(),
        name: "@test/older".to_string(),
        version: "0.1.0".to_string(),
        ..Default::default()
    })
    .unwrap();
    specforge_wasm::testing::InProcessRuntime::new()
        .with(|| {
            let mut c = ContributionsBuilder::new(ExtensionMeta::new("@test/older", "0.1.0"));
            c.kind("thing", |k| {
                k.testable(true);
            });
            c
        })
        .answer_raw(
            "@test/older",
            "__handshake",
            specforge_wasm::WasmCallResult::Ok(handshake),
        )
}

#[specforge_test_macros::test(
    behavior = "validate_extension_manifest",
    verify = "a handshake whose protocol major differs from the host's produces E028"
)]
fn an_older_minor_protocol_still_loads() {
    use specforge_protocol_types::{PROTOCOL_VERSION, ProtocolError};
    use specforge_wasm::protocol::load_declaration;

    assert_eq!(PROTOCOL_VERSION, "1.2.0");
    // A guest built with the 1.0 SDK loads under a 1.2 host: only the
    // major is checked.
    let loaded = load_declaration(&guest_answering("1.0.0"), "@test/older")
        .expect("a 1.0.0 guest loads under a 1.2.0 host");
    assert_eq!(loaded.declaration.name(), "@test/older");
    // Another major still does not.
    match load_declaration(&guest_answering("2.0.0"), "@test/older").unwrap_err() {
        ProtocolError::IncompatibleVersion {
            host_version,
            extension_version,
        } => {
            assert_eq!(host_version, "1.2.0");
            assert_eq!(extension_version, "2.0.0");
        }
        other => panic!("expected IncompatibleVersion, got {other:?}"),
    }
}
