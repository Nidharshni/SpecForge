//! Protocol extension loading: extensions are loaded via the Wasm protocol
//! (__handshake / __describe) through a WasmRuntime implementation.

use specforge_extension_sdk::{ContributionsBuilder, ExtensionMeta, no_other_exports};
use specforge_protocol_types::{ContributionFlags, HandshakeResponse};
use specforge_test::prelude::*;
use specforge_wasm::testing::InProcessRuntime;
use specforge_wasm::{WasmCallResult, WasmTrapInfo};
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper: create a temp project dir with specforge.json and optional extensions.
fn setup_project(extensions: &[&str], spec_content: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    let ext_json: Vec<String> = extensions.iter().map(|e| format!("\"{}\"", e)).collect();
    let config = format!(
        r#"{{"name":"test","version":"0.1.0","extensions":[{}]}}"#,
        ext_json.join(",")
    );
    fs::write(dir.path().join("specforge.json"), config).unwrap();
    specforge_installed::testing::install_configured(dir.path(), &specforge_project::builtins());
    fs::write(dir.path().join("core.spec"), spec_content).unwrap();
    dir
}

// --- Step 6: Protocol extension loaded through a runtime ---

// B:dual_mode_loading — verify unit "protocol extension loaded via runtime"
// Not linked to "installed extension manifest is loaded": the mock answers
// a handshake for any name and never goes through the project runtime.
// cli/tests/installed_extensions.rs proves that obligation with a real
// install.
#[test]
fn protocol_extension_loaded_with_runtime() {
    let dir = setup_project(
        &["@test/proto"],
        "behavior hello \"Hello\" {\n    status planned\n}\n",
    );

    // The project names it; the runtime serves it under that name.
    let runtime = InProcessRuntime::new().serving(
        "@test/proto",
        || {
            let mut c = ContributionsBuilder::new(ExtensionMeta::new("@test/proto", "1.0.0"));
            c.kind("gadget", |k| {
                k.description("A test gadget");
            });
            c
        },
        no_other_exports,
    );

    let ctx = specforge_project::CompiledProject::compile(dir.path(), Some(Arc::new(runtime)));

    let diagnostics = ctx.diagnostics();

    // No W031 — runtime was provided
    let w031: Vec<_> = diagnostics.iter().filter(|d| d.code == "W031").collect();
    assert!(
        w031.is_empty(),
        "should not have W031 when runtime is provided: {:?}",
        w031
    );

    // No E028 — protocol loading should succeed
    let e028: Vec<_> = diagnostics.iter().filter(|d| d.code == "E028").collect();
    assert!(e028.is_empty(), "should not have E028: {:?}", e028);

    // Its declaration should appear in ctx.env.registries().declarations()
    assert_eq!(
        ctx.environment().registries().declarations().len(),
        1,
        "expected 1 declaration from protocol extension"
    );
    assert_eq!(
        ctx.environment().registries().declarations()[0].name(),
        "@test/proto"
    );

    // KindRegistry should have "gadget"
    assert!(
        ctx.environment().registries().kinds().contains("gadget"),
        "expected gadget kind in registry"
    );
}

// --- Step 7: Mixed manifest + protocol extensions ---
// (Removed: dual-mode coexistence is no longer supported. When a runtime is provided,
// all extensions are loaded via the protocol path. Manifest-only extensions are being removed.)

// --- Step 8: Error handling — protocol failures become diagnostics ---

// B:dual_mode_loading — verify unit "protocol handshake trap produces E031 diagnostic"
#[specforge_test(
    behavior = "load_extension_manifests",
    verify = "unloadable extension binary produces diagnostic instead of crash"
)]
fn protocol_handshake_trap_produces_e028() {
    let dir = setup_project(
        &["@test/broken"],
        "behavior hello \"Hello\" {\n    status planned\n}\n",
    );

    let runtime = InProcessRuntime::new().answer_raw(
        "@test/broken",
        "__handshake",
        WasmCallResult::Trap(WasmTrapInfo {
            kind: "unreachable".to_string(),
            message: "extension panicked during handshake".to_string(),
            export_name: "__handshake".to_string(),
        }),
    );

    let ctx = specforge_project::CompiledProject::compile(dir.path(), Some(Arc::new(runtime)));

    let diagnostics = ctx.diagnostics();

    // E028 diagnostic should be emitted
    let e028: Vec<_> = diagnostics.iter().filter(|d| d.code == "E028").collect();
    assert_eq!(
        e028.len(),
        1,
        "expected exactly 1 E028 diagnostic, got: {:?}",
        e028
    );
    assert!(
        e028[0].message.contains("@test/broken"),
        "E028 should mention extension name"
    );
    assert!(
        e028[0].message.contains("protocol loading failed"),
        "E028 should describe the error"
    );

    // No declaration from the broken extension
    assert!(
        ctx.environment().registries().declarations().is_empty(),
        "broken extension should not produce a declaration"
    );
}

// B:dual_mode_loading — verify unit "protocol version mismatch produces E031"
#[specforge_test(
    behavior = "load_extension_manifests",
    verify = "unloadable extension binary produces diagnostic instead of crash"
)]
fn protocol_version_mismatch_produces_e028() {
    let dir = setup_project(
        &["@test/badver"],
        "behavior hello \"Hello\" {\n    status planned\n}\n",
    );

    // Return a handshake with wrong protocol version
    let bad_handshake = HandshakeResponse {
        protocol_version: "99.0".to_string(),
        name: "@test/badver".to_string(),
        version: "1.0.0".to_string(),
        contribution_flags: ContributionFlags::default(),
        peer_dependencies: vec![],
        sandbox_policy: None,
        starter_template: None,
        theme_color: None,
        migration_hook: None,
        ..Default::default()
    };
    let runtime = InProcessRuntime::new().answer_raw(
        "@test/badver",
        "__handshake",
        WasmCallResult::Ok(serde_json::to_vec(&bad_handshake).unwrap()),
    );

    let ctx = specforge_project::CompiledProject::compile(dir.path(), Some(Arc::new(runtime)));

    let diagnostics = ctx.diagnostics();

    let e028: Vec<_> = diagnostics.iter().filter(|d| d.code == "E028").collect();
    assert_eq!(
        e028.len(),
        1,
        "expected exactly 1 E028 for version mismatch"
    );
    assert!(
        e028[0].message.contains("protocol loading failed"),
        "E028 should describe error"
    );
    assert!(
        e028[0].message.contains("version mismatch"),
        "E028 should mention version mismatch"
    );
}

// (Removed: "protocol error does not prevent manifest extensions from loading" tested dual-mode
// coexistence which is no longer supported. Error isolation for protocol extensions is covered
// by protocol_handshake_trap_produces_e028.)
