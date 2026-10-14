//! Proof that an extension authored with `specforge-extension-sdk` passes the
//! host's protocol (handshake + describe) end to end through the component
//! runtime. The fixture's component blob is vendored at
//! `specforge_component::committed::GREET` (refresh:
//! `cargo run -p xtask --bin revendor`).
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use std::path::PathBuf;

use specforge_component::ComponentRuntime;
use specforge_wasm::runtime::{WasmCallResult, WasmRuntime};

fn greet_wasm_path() -> PathBuf {
    specforge_component::committed::GREET.path()
}

#[test]
fn sdk_greet_extension_passes_protocol() {
    let blob = std::fs::read(greet_wasm_path()).expect("vendored greet component blob");
    let runtime = ComponentRuntime::new();
    runtime
        .load("@sdk/greet", &blob)
        .expect("greet component instantiates");

    // __handshake: identity, derived flags, protocol version.
    let hs = runtime.call_export("@sdk/greet", "__handshake", b"");
    let bytes = match hs {
        WasmCallResult::Ok(bytes) => bytes,
        other => panic!("handshake failed: {other:?}"),
    };
    let hs: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(hs["name"], "@sdk/greet");
    assert_eq!(hs["version"], "0.1.0");
    assert_eq!(
        hs["protocol_version"],
        specforge_protocol_types::PROTOCOL_VERSION
    );
    assert!(hs.get("contribution_flags").is_none());
    assert!(hs.get("providers").is_none());

    // __describe entities: the greeting kind with its style field.
    let de = runtime.call_export("@sdk/greet", "__describe", br#"{"category":"entities"}"#);
    let bytes = match de {
        WasmCallResult::Ok(bytes) => bytes,
        other => panic!("describe failed: {other:?}"),
    };
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["category"], "entities");
    assert_eq!(v["items"][0]["name"], "greeting");
    assert_eq!(v["items"][0]["fields"][0]["name"], "style");
    assert_eq!(v["items"][0]["fields"][0]["field_type"], "enum");
    assert_eq!(v["items"][0]["fields"][0]["enum_values"][0], "warm");

    // __describe validation_rules: the contributed rule round-trips.
    let dr = runtime.call_export(
        "@sdk/greet",
        "__describe",
        br#"{"category":"validation_rules"}"#,
    );
    let bytes = match dr {
        WasmCallResult::Ok(bytes) => bytes,
        other => panic!("describe rules failed: {other:?}"),
    };
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["items"][0]["code"], "E901");
    assert_eq!(v["items"][0]["severity"], "error");
    assert_eq!(v["items"][0]["check"], "field_value_constraint");

    // __describe: unsupported category mirrors the builtin error behavior.
    let bad = runtime.call_export("@sdk/greet", "__describe", br#"{"category":"nope"}"#);
    assert!(
        matches!(bad, WasmCallResult::Trap(_)),
        "expected trap for unsupported category"
    );
}

/// The greet fixture's declarations, from its source: what its vendored
/// component is built from.
#[path = "../../../../fixtures/greet-extension/src/contributions.rs"]
mod greet;

/// Every call a host makes to greet, with its input: the handshake, a
/// describe of every category the protocol has and one it does not, and
/// each export its declaration names (its commands, MCP tools and
/// resources, passes, collectors, custom rules and scanners), plus one it
/// does not route.
fn every_call(
    declaration: &specforge_protocol_types::ExtensionDeclaration,
) -> Vec<(String, Vec<u8>)> {
    use specforge_protocol_types::DECLARED_CATEGORIES;
    let request = |category: &str| format!(r#"{{"category":"{category}"}}"#).into_bytes();
    let mut calls = vec![("__handshake".to_string(), b"{}".to_vec())];
    for category in DECLARED_CATEGORIES.iter().copied().chain(["nope"]) {
        calls.push(("__describe".to_string(), request(category)));
    }
    let surfaces = &declaration.surfaces;
    for command in &surfaces.commands {
        let input = serde_json::json!({"args": {}, "format": "json", "today": "2026-10-05",
            "graph": {"nodes": [], "edges": []}});
        calls.push((command.export.clone(), input.to_string().into_bytes()));
    }
    for tool in &surfaces.mcp_tools {
        calls.push((tool.export.clone(), b"{}".to_vec()));
    }
    for resource in &surfaces.mcp_resources {
        let input = serde_json::json!({"uri": resource.uri_template});
        calls.push((resource.export.clone(), input.to_string().into_bytes()));
    }
    for pass in &declaration.passes {
        let input =
            br#"{"entities":[{"id":"hi","kind":"greeting","fields":{"style":"warm"}}],"edges":[]}"#;
        calls.push((
            specforge_protocol_types::pass_export(&pass.name),
            input.to_vec(),
        ));
    }
    for collector in &declaration.collectors {
        let input = br#"{"reports":[{"path":"r.txt","content":"hi ok\n"}]}"#;
        calls.push((collector.export.clone(), input.to_vec()));
    }
    for rule in &declaration.validation_rules {
        if let Some(function) = &rule.wasm_function {
            let input = br#"{"entity":{"id":"hi","kind":"greeting","fields":[],"methods":[]},"referenced":[],"declared_types":[],"primitives":[]}"#;
            calls.push((function.clone(), input.to_vec()));
        }
    }
    for analyzer in &declaration.analyzers {
        let input = br#"{"file_path":"a.txt","content":"hi\n"}"#;
        calls.push((analyzer.scan_export.clone(), input.to_vec()));
    }
    calls.push(("greet__no_such_export".to_string(), b"{}".to_vec()));
    calls
}

/// The greet fixture answers every call alike through the component
/// runtime, from its vendored blob, and through the in-process runtime,
/// from its source: the same bytes, or the same trap.
#[specforge_test_macros::test(
    behavior = "call_extension_exports",
    verify = "an SDK-declared extension answers the same through the in-process runtime as through the component runtime"
)]
fn greet_answers_alike_through_both_runtimes() {
    use specforge_wasm::testing::InProcessRuntime;

    let blob = std::fs::read(greet_wasm_path()).expect("vendored greet component blob");
    let component = ComponentRuntime::new();
    component
        .load("@sdk/greet", &blob)
        .expect("greet component instantiates");
    let in_process = InProcessRuntime::new().with(greet::build);

    let calls = every_call(&greet::build().declaration());
    assert!(calls.len() > SUPPORTED_CATEGORY_COUNT, "{calls:?}");
    for (export, input) in calls {
        let from_blob = component.call_export("@sdk/greet", &export, &input);
        let from_source = in_process.call_export("@sdk/greet", &export, &input);
        match (&from_blob, &from_source) {
            (WasmCallResult::Ok(a), WasmCallResult::Ok(b)) => assert_eq!(
                String::from_utf8_lossy(a),
                String::from_utf8_lossy(b),
                "{export} {}",
                String::from_utf8_lossy(&input)
            ),
            (WasmCallResult::Trap(a), WasmCallResult::Trap(b)) => {
                assert_eq!(
                    (&a.kind, &a.export_name),
                    (&b.kind, &b.export_name),
                    "{export}"
                );
                if a.kind == "guest_error" {
                    assert_eq!(a.message, b.message, "{export}");
                }
            }
            _ => {
                panic!("{export}: the component answered {from_blob:?}, in process {from_source:?}")
            }
        }
    }
}

const SUPPORTED_CATEGORY_COUNT: usize = specforge_protocol_types::DECLARED_CATEGORIES.len();
