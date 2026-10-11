//! The one loader: an extension's declaration is read through
//! `load_declaration`, its handshake and then every declared describe
//! category exactly once, whatever its contribution flags say. Extensions
//! are served in process from their SDK builders (`InProcessRuntime`).

use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::{DECLARED_CATEGORIES, DeclaredCategory, ProtocolError};
use specforge_wasm::protocol::load_declaration;
use specforge_wasm::testing::InProcessRuntime;
use specforge_wasm::{WasmCallResult, WasmRuntime, WasmTrapInfo};

fn reports() -> ContributionsBuilder {
    let mut meta = ExtensionMeta::new("@acme/reports", "0.1.0");
    meta.short = Some("rep".to_string());
    let mut b = ContributionsBuilder::new(meta);
    b.kind("report", |k| {
        k.description("r");
    });
    b
}

fn commands_only() -> ContributionsBuilder {
    let mut b = ContributionsBuilder::new(ExtensionMeta::new("@acme/cmds", "0.1.0"));
    b.command("hello", |c| {
        c.title("Hello")
            .description("Say hello")
            .handler(|_| CommandOutput {
                exit_code: 0,
                stdout: "hi".to_string(),
                stderr: String::new(),
            });
    });
    b
}

fn passes_only() -> ContributionsBuilder {
    let mut b = ContributionsBuilder::new(ExtensionMeta::new("@acme/audit", "0.1.0"));
    b.pass("audit", |p| {
        p.phase("check").run(no_findings);
    });
    b
}

#[specforge_test_macros::test(
    behavior = "load_extension_declaration",
    verify = "the loader reads the handshake and every describe category once"
)]
fn the_loader_reads_every_category_once() {
    // A passes-only extension raises no contribution flag that used to gate
    // a category, yet every category is read.
    let runtime = InProcessRuntime::new().with(passes_only);
    let loaded = load_declaration(&runtime, "@acme/audit").unwrap();
    let calls = runtime.calls();
    assert_eq!(calls.len(), 1 + DECLARED_CATEGORIES.len(), "{calls:?}");
    assert_eq!(calls[0].export, "__handshake");
    let described: Vec<&str> = calls[1..]
        .iter()
        .map(|c| {
            assert_eq!(c.export, "__describe");
            c.input["category"].as_str().unwrap()
        })
        .collect();
    assert_eq!(described, DECLARED_CATEGORIES);
    assert!(loaded.warnings.is_empty());
}

#[specforge_test_macros::test(
    behavior = "load_extension_declaration",
    verify = "an extension that only declares passes has them in its declaration"
)]
fn a_passes_only_extension_has_its_passes() {
    let runtime = InProcessRuntime::new().with(passes_only);
    let declaration = load_declaration(&runtime, "@acme/audit")
        .unwrap()
        .declaration;
    assert_eq!(declaration.passes.len(), 1);
    assert_eq!(declaration.passes[0].name, "audit");
    assert_eq!(declaration.passes[0].phase.as_deref(), Some("check"));
    assert_eq!(declaration, passes_only().declaration());
}

/// The same holds for commands, which no contribution flag covers (R2).
#[test]
fn a_commands_only_extension_has_its_commands() {
    let runtime = InProcessRuntime::new().with(commands_only);
    let declaration = load_declaration(&runtime, "@acme/cmds")
        .unwrap()
        .declaration;
    assert_eq!(declaration.surfaces.commands.len(), 1);
    assert_eq!(declaration.surfaces.commands[0].id, "hello");
}

fn typo() -> ContributionsBuilder {
    let mut b = ContributionsBuilder::new(ExtensionMeta::new("@acme/typo", "0.1.0"));
    b.raw_category(
        DeclaredCategory::Entities,
        serde_json::json!([{ "name": "x", "testabel": true }]),
    );
    b
}

#[specforge_test_macros::test(
    behavior = "load_extension_declaration",
    verify = "a describe item key the protocol does not define produces W138"
)]
fn an_unknown_describe_key_is_w138() {
    let runtime = InProcessRuntime::new().with(typo);
    let loaded = load_declaration(&runtime, "@acme/typo").unwrap();
    // The typo costs the field it meant: the kind is not testable.
    assert!(!loaded.declaration.entities[0].testable);
    let codes: Vec<&str> = loaded.warnings.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["W138"]);
    let message = &loaded.warnings[0].message;
    for part in ["@acme/typo", "'entities'", "'x'", "'testabel'"] {
        assert!(message.contains(part), "{message}");
    }
}

#[test]
fn the_declared_short_name_is_loaded() {
    let runtime = InProcessRuntime::new().with(reports);
    let declaration = load_declaration(&runtime, "@acme/reports")
        .unwrap()
        .declaration;
    assert_eq!(declaration.handshake.ext_short.as_deref(), Some("rep"));
    assert_eq!(declaration.short(), "rep");
}

/// A category whose answer fails, or does not parse, fails the load naming
/// the category.
#[test]
fn a_failing_category_fails_the_load_naming_it() {
    let runtime = InProcessRuntime::new().with(reports).answer_raw(
        "@acme/reports",
        "__describe",
        WasmCallResult::Ok(br#"{"category": "entities", "items": [{"nam": "x"}]}"#.to_vec()),
    );
    match load_declaration(&runtime, "@acme/reports").unwrap_err() {
        ProtocolError::DescribeFailed { category, reason } => {
            assert_eq!(category, "entities");
            assert!(reason.contains("missing field `name`"), "{reason}");
        }
        other => panic!("expected DescribeFailed, got {other:?}"),
    }
}

// ── The in-process runtime routes as the component guest does ──

fn dispatch(export: &str, _input: &[u8]) -> Option<Result<Vec<u8>, String>> {
    match export {
        "scan__x" => Some(Ok(b"scanned".to_vec())),
        "fails" => Some(Err("boom".to_string())),
        "panics" => panic!("guest panicked"),
        _ => None,
    }
}

fn trap(result: WasmCallResult) -> WasmTrapInfo {
    match result {
        WasmCallResult::Trap(trap) => trap,
        WasmCallResult::Ok(bytes) => panic!("expected a trap, got {bytes:?}"),
    }
}

#[test]
fn the_in_process_runtime_maps_guest_failures_to_traps() {
    let runtime = InProcessRuntime::new().with_handler(reports, dispatch);
    let ok = runtime.call_export("@acme/reports", "scan__x", b"");
    assert!(matches!(ok, WasmCallResult::Ok(bytes) if bytes == b"scanned"));

    let missing = trap(runtime.call_export("@acme/nope", "__handshake", b"{}"));
    assert_eq!(missing.kind, "extension_not_found");
    assert_eq!(missing.message, "Extension '@acme/nope' not loaded");

    let unknown = trap(runtime.call_export("@acme/reports", "nope", b""));
    assert_eq!(unknown.kind, "guest_error");
    assert_eq!(unknown.message, "unknown export 'nope'");

    let failed = trap(runtime.call_export("@acme/reports", "fails", b""));
    assert_eq!(
        (failed.kind.as_str(), failed.message.as_str()),
        ("guest_error", "boom")
    );

    let panicked = trap(runtime.call_export("@acme/reports", "panics", b""));
    assert_eq!(panicked.kind, "call_failed");
    assert_eq!(panicked.message, "unreachable: guest panicked");
    assert_eq!(panicked.export_name, "panics");

    let raw = InProcessRuntime::new().answer_raw(
        "@acme/x",
        "__handshake",
        WasmCallResult::Trap(WasmTrapInfo {
            kind: "deadline_exceeded".to_string(),
            message: "slow".to_string(),
            export_name: "__handshake".to_string(),
        }),
    );
    assert_eq!(
        trap(raw.call_export("@acme/x", "__handshake", b"")).kind,
        "deadline_exceeded"
    );
    assert_eq!(raw.calls().len(), 1);
}

/// A pass's handler that finds nothing.
fn no_findings(_: &PassInput) -> Vec<PassDiagnostic> {
    Vec::new()
}
