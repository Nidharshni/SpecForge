//! The transport under `load_declaration`: the handshake, its protocol
//! version check and its execution budget, then every describe category.
//! Extensions are declared with the SDK and served in process; the answers
//! no SDK guest gives (another protocol version, a trap, bytes that do not
//! parse) are given raw.

use serde_json::json;
use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::{
    DeclaredCategory, HandshakeResponse, PROTOCOL_VERSION, ProtocolError,
};
use specforge_wasm::protocol::{Loaded, load_declaration};
use specforge_wasm::testing::InProcessRuntime;
use specforge_wasm::{ExtensionCalls, Limits, WasmCallResult, WasmTrapInfo};

/// `name`, declaring a testable `behavior` kind, an `Implements` edge and
/// a `W001` rule.
fn declaring(name: &'static str) -> impl Fn() -> ContributionsBuilder + Send + Sync + 'static {
    move || {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new(name, "1.0.0"));
        c.kind("behavior", |k| {
            k.testable(true);
        });
        c.edge("Implements", |_| {});
        c.rule("W001", |r| {
            r.check(CheckKind::NoIncomingEdges).message_template("test");
        });
        c
    }
}

fn raw(bytes: &[u8]) -> WasmCallResult {
    WasmCallResult::Ok(bytes.to_vec())
}

fn trap(kind: &str, message: &str, export: &str) -> WasmCallResult {
    WasmCallResult::Trap(WasmTrapInfo {
        kind: kind.to_string(),
        message: message.to_string(),
        export_name: export.to_string(),
    })
}

/// A handshake of `@test/ext` speaking `protocol_version`.
fn handshake_with(protocol_version: &str) -> Vec<u8> {
    serde_json::to_vec(&HandshakeResponse {
        protocol_version: protocol_version.to_string(),
        name: "@test/ext".to_string(),
        version: "1.0.0".to_string(),
        ..Default::default()
    })
    .unwrap()
}

#[test]
fn handshake_returns_parsed_response() {
    let runtime = InProcessRuntime::new().with(declaring("@specforge/software"));
    let loaded = load_declaration(&runtime, "@specforge/software").unwrap();
    let resp = &loaded.declaration.handshake;
    assert_eq!(resp.name, "@specforge/software");
    assert_eq!(resp.version, "1.0.0");
    assert_eq!(resp.protocol_version, PROTOCOL_VERSION);
    assert!(loaded.declaration.declares(DeclaredCategory::Entities));
    assert!(
        loaded
            .declaration
            .declares(DeclaredCategory::ValidationRules)
    );
    assert!(!resp.providers);
}

// ── Handshake applies the extension's sandbox: its limits (C7-10, ADR 0037) ──

#[specforge_test_macros::test(
    behavior = "configure_sandbox_policy",
    verify = "a declared limit below the ceiling is applied as declared"
)]
#[specforge_test_macros::test(
    behavior = "load_extension_declaration",
    verify = "the loader applies the execution budget its handshake declares, and a handshake call alone applies none"
)]
fn the_loader_applies_the_declared_execution_budget() {
    let runtime = InProcessRuntime::new().with(|| {
        let mut meta = ExtensionMeta::new("@specforge/software", "1.0.0");
        meta.sandbox_policy = Some(SandboxPolicy {
            max_execution_ms: Some(5000),
            ..Default::default()
        });
        ContributionsBuilder::new(meta)
    });
    load_declaration(&runtime, "@specforge/software").unwrap();
    assert_eq!(
        runtime.limits(),
        vec![(
            "@specforge/software".to_string(),
            Limits {
                execution_ms: 5000,
                memory_mb: 512
            }
        )]
    );
}

#[specforge_test_macros::test(
    behavior = "load_extension_declaration",
    verify = "the loader applies the execution budget its handshake declares, and a handshake call alone applies none"
)]
fn a_handshake_call_applies_no_limits() {
    let runtime = InProcessRuntime::new().with(|| {
        let mut meta = ExtensionMeta::new("@specforge/software", "1.0.0");
        meta.sandbox_policy = Some(SandboxPolicy {
            max_execution_ms: Some(5000),
            ..Default::default()
        });
        ContributionsBuilder::new(meta)
    });
    let handshake = ExtensionCalls::new(&runtime)
        .handshake("@specforge/software")
        .unwrap();
    // It reads what the policy comes to; applying it is the loader's.
    assert_eq!(handshake.sandbox.limits.execution_ms, 5000);
    assert!(runtime.limits().is_empty(), "{:?}", runtime.limits());
}

#[specforge_test_macros::test(
    behavior = "configure_sandbox_policy",
    verify = "an extension declaring no sandbox policy runs under the host's ceiling"
)]
fn an_extension_declaring_no_policy_runs_under_the_ceiling() {
    let runtime = InProcessRuntime::new().with(declaring("@specforge/formal"));
    load_declaration(&runtime, "@specforge/formal").unwrap();
    assert_eq!(
        runtime.limits(),
        vec![("@specforge/formal".to_string(), Limits::CEILING)]
    );
}

/// The handshake an older SDK built: `name`'s own, with `sandbox_policy`
/// as that SDK wrote it (permission fields the type no longer has).
fn older_handshake(name: &'static str, sandbox_policy: serde_json::Value) -> WasmCallResult {
    let mut handshake: serde_json::Value =
        serde_json::from_str(&declaring(name)().declaration().handshake_json()).unwrap();
    handshake["sandbox_policy"] = sandbox_policy;
    raw(handshake.to_string().as_bytes())
}

#[specforge_test_macros::test(
    behavior = "configure_sandbox_policy",
    verify = "Configure Sandbox Policy: sandbox policy configuration holds — handshake_read, limits_held_to_ceiling, above_ceiling_warned, capabilities_never_granted, limits_applied_at_handshake"
)]
#[specforge_test_macros::test(
    behavior = "configure_sandbox_policy",
    verify = "a sandbox_policy key that asks for a capability is W153"
)]
fn a_sandbox_policy_key_that_asks_for_a_capability_is_w153() {
    let runtime = InProcessRuntime::new()
        .with(declaring("@acme/asks"))
        .answer_raw(
            "@acme/asks",
            "__handshake",
            older_handshake(
                "@acme/asks",
                json!({
                    "max_memory_mb": 64,
                    "max_execution_ms": 60_000,
                    "network_access": true,
                    "allowed_paths": ["/etc"],
                }),
            ),
        );
    let loaded = load_declaration(&runtime, "@acme/asks").unwrap();

    // One W153 per key, and no W138: the handshake is not a describe item.
    let keys: Vec<&str> = loaded
        .warnings
        .iter()
        .map(|warning| {
            assert_eq!(warning.code, "W153", "{warning:?}");
            ["allowed_paths", "max_execution_ms", "network_access"]
                .into_iter()
                .find(|key| warning.message.contains(&format!("sandbox_policy.{key} ")))
                .unwrap_or_else(|| panic!("a key not declared: {}", warning.message))
        })
        .collect();
    assert_eq!(
        keys,
        ["allowed_paths", "max_execution_ms", "network_access"]
    );

    // The limits were applied at the handshake: the declared one as
    // declared, the one above the ceiling held to it.
    assert_eq!(
        runtime.limits(),
        vec![(
            "@acme/asks".to_string(),
            Limits {
                execution_ms: 30_000,
                memory_mb: 64
            }
        )]
    );
}

#[test]
fn a_policy_of_two_limits_loads_clean() {
    let runtime = InProcessRuntime::new().with(|| {
        let mut meta = ExtensionMeta::new("@acme/tidy", "1.0.0");
        meta.sandbox_policy = Some(SandboxPolicy {
            max_memory_mb: Some(64),
            max_execution_ms: Some(1000),
        });
        ContributionsBuilder::new(meta)
    });
    let loaded = load_declaration(&runtime, "@acme/tidy").unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
}

#[specforge_test_macros::test(
    behavior = "configure_sandbox_policy",
    verify = "a surface's sandbox override is W153"
)]
fn a_surface_sandbox_override_is_w153() {
    let runtime = InProcessRuntime::new().with(|| {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/asks", "1.0.0"));
        c.command("x", |cmd| {
            cmd.title("X").handler(|_| CommandOutput::default());
        });
        // Served as an older guest describes it: the command with the
        // `sandbox` override the descriptor no longer has.
        c.raw_category(
            DeclaredCategory::Surfaces,
            json!([{
                "commands": [{
                    "id": "x", "title": "X", "description": "", "export": "cmd__x",
                    "sandbox": { "fs_write": true }
                }]
            }]),
        );
        c
    });
    let loaded = load_declaration(&runtime, "@acme/asks").unwrap();
    assert_eq!(loaded.warnings.len(), 1, "{:?}", loaded.warnings);
    assert_eq!(loaded.warnings[0].code, "W153");
    assert!(
        loaded.warnings[0].message.contains("#0.commands[x]"),
        "{}",
        loaded.warnings[0].message
    );
}

// ── Handshake error handling ──

#[test]
fn handshake_trap_returns_error() {
    let runtime = InProcessRuntime::new().answer_raw(
        "@test/ext",
        "__handshake",
        trap("unreachable", "module crashed", "__handshake"),
    );
    let err = load_declaration(&runtime, "@test/ext").unwrap_err();
    match err {
        ProtocolError::HandshakeFailed(msg) => {
            assert!(msg.contains("unreachable"));
            assert!(msg.contains("module crashed"));
        }
        other => panic!("expected HandshakeFailed, got {:?}", other),
    }
}

#[test]
fn handshake_invalid_json_returns_deserialization_error() {
    let runtime = InProcessRuntime::new().answer_raw(
        "@test/ext",
        "__handshake",
        raw(b"not valid json at all"),
    );
    let err = load_declaration(&runtime, "@test/ext").unwrap_err();
    match err {
        ProtocolError::DeserializationError(_) => {}
        other => panic!("expected DeserializationError, got {:?}", other),
    }
}

// ── The protocol version: same major is compatible (M11) ──

#[test]
fn compatible_protocol_versions_load() {
    for version in ["1.0.0", "1.0.1", "1.1.0", "1.1.7", "1.9.3"] {
        let runtime = InProcessRuntime::new()
            .with(declaring("@test/ext"))
            .answer_raw("@test/ext", "__handshake", raw(&handshake_with(version)));
        assert!(
            load_declaration(&runtime, "@test/ext").is_ok(),
            "{version} is compatible with the host's {PROTOCOL_VERSION}"
        );
    }
}

#[specforge_test_macros::test(
    behavior = "validate_extension_manifest",
    verify = "a handshake whose protocol major differs from the host's produces E028"
)]
fn an_incompatible_protocol_version_fails_the_load() {
    for version in ["2.0.0", "0.9.0", "99.0"] {
        let runtime = InProcessRuntime::new()
            .with(declaring("@test/ext"))
            .answer_raw("@test/ext", "__handshake", raw(&handshake_with(version)));
        match load_declaration(&runtime, "@test/ext").unwrap_err() {
            ProtocolError::IncompatibleVersion {
                host_version,
                extension_version,
            } => {
                assert_eq!(host_version, PROTOCOL_VERSION);
                assert_eq!(extension_version, version);
            }
            other => panic!("{version}: expected IncompatibleVersion, got {:?}", other),
        }
    }
}

// ── Describe ──

#[test]
fn describe_answers_become_the_declaration() {
    let runtime = InProcessRuntime::new().with(declaring("@test/ext"));
    let declaration = load_declaration(&runtime, "@test/ext").unwrap().declaration;
    assert_eq!(declaration.entities.len(), 1);
    assert_eq!(declaration.entities[0].name, "behavior");
    assert!(declaration.entities[0].testable);
    assert_eq!(declaration.edges[0].label, "Implements");
    assert_eq!(declaration.validation_rules[0].code, "W001");
    assert!(declaration.collectors.is_empty());
}

#[test]
fn describe_trap_returns_error() {
    let runtime = InProcessRuntime::new()
        .with(declaring("@test/ext"))
        .answer_raw(
            "@test/ext",
            "__describe",
            trap("trap", "out of memory", "__describe"),
        );
    let err = load_declaration(&runtime, "@test/ext").unwrap_err();
    match err {
        ProtocolError::DescribeFailed { category, reason } => {
            assert_eq!(category, "entities");
            assert!(reason.contains("out of memory"));
        }
        other => panic!("expected DescribeFailed, got {:?}", other),
    }
}

/// An answer that is not a describe response fails naming its category.
#[test]
fn a_describe_answer_that_is_not_json_names_its_category() {
    let runtime = InProcessRuntime::new()
        .with(declaring("@test/ext"))
        .answer_raw_to(
            "@test/ext",
            "__describe",
            json!({"category": "edges"}),
            raw(b"not json"),
        );
    match load_declaration(&runtime, "@test/ext").unwrap_err() {
        ProtocolError::DescribeFailed { category, .. } => assert_eq!(category, "edges"),
        other => panic!("expected DescribeFailed, got {:?}", other),
    }
}

/// Load an extension whose surfaces declare an arg type outside
/// CommandArgType ("list"), which cannot be represented (an SDK guest
/// cannot declare it, so the answer is given raw).
fn load_with_unknown_arg_type() -> Result<Loaded, ProtocolError> {
    let surfaces = json!({"category": "surfaces", "items": [{"commands": [{
        "id": "c", "title": "C", "description": "d", "export": "cmd__c",
        "args": [{"name": "tags", "arg_type": "list"}]
    }]}]});
    let runtime = InProcessRuntime::new()
        .with(declaring("@test/ext"))
        .answer_raw_to(
            "@test/ext",
            "__describe",
            json!({"category": "surfaces"}),
            raw(surfaces.to_string().as_bytes()),
        );
    load_declaration(&runtime, "@test/ext")
}

#[specforge_test_macros::test(
    behavior = "register_surface_contributions",
    verify = "a surfaces description that does not parse fails the extension's load"
)]
fn a_surfaces_description_that_does_not_parse_fails_the_load() {
    let err = load_with_unknown_arg_type().unwrap_err();
    assert!(format!("{err}").contains("surfaces"), "{err}");
}

#[specforge_test_macros::test(
    invariant = "surface_schema_validity",
    verify = "a surfaces description with an unknown arg type fails the extension's load"
)]
fn an_unknown_arg_type_fails_the_load() {
    assert!(load_with_unknown_arg_type().is_err());
}
