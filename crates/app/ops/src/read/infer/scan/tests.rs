use super::*;
use crate::read::infer::sources::InferenceSources;
use serde_json::json;
use specforge_component::ComponentRuntime;
use specforge_protocol_types::{AnalyzerDescriptor, ExtensionDeclaration, HandshakeResponse};
use specforge_test_macros::test as specforge_test;
use specforge_wasm::WasmCallResult;
use specforge_wasm::testing::InProcessRuntime;
use tempfile::TempDir;

/// Build a Wasm runtime for a temp project listing `ext_names` — the only
/// way extensions exist now (WASM-only migration, Phase 7: the native
/// mirror tier is gone).
fn wasm_runtime_for(ext_names: &[&str]) -> specforge_component::ComponentRuntime {
    let runtime = specforge_component::ComponentRuntime::new();
    let names: Vec<String> = ext_names.iter().map(|name| name.to_string()).collect();
    specforge_component::builtins::load_builtins_for(&runtime, &names).unwrap();
    runtime
}

fn rust_only_runtime() -> ComponentRuntime {
    wasm_runtime_for(&["@specforge/rust"])
}

fn rust_manifest() -> ExtensionDeclaration {
    ExtensionDeclaration {
        handshake: HandshakeResponse {
            name: "@specforge/rust".into(),
            version: "1.0.0".into(),
            ..HandshakeResponse::default()
        },
        analyzers: vec![AnalyzerDescriptor {
            language: "rust".into(),
            file_extensions: vec![".rs".into()],
            excluded_dirs: vec!["target".into()],
            scan_export: "scan__rust".into(),
            classify_export: "classify__rust".into(),
            map_export: "map__rust".into(),
            description: None,
        }],
        ..ExtensionDeclaration::default()
    }
}

#[test]
fn scan_only_matching_extensions() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("lib.rs"), "pub fn hello() {}").unwrap();
    std::fs::write(dir.path().join("readme.md"), "# Hello").unwrap();
    std::fs::write(dir.path().join("app.txt"), "text file").unwrap();

    let runtime = rust_only_runtime();
    let manifests = vec![rust_manifest()];
    let source_files = vec!["lib.rs", "readme.md", "app.txt"];

    let ScanOutcome {
        items,
        scanners_used: scanners,
        ..
    } = scan_source_files(
        Some(&runtime),
        &InferenceSources::with_keys(dir.path(), &manifests, &source_files),
    );

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "hello");
    assert_eq!(items[0].item_kind, "function");
    assert_eq!(items[0].file, "lib.rs");
    assert_eq!(items[0].scanner.as_deref(), Some("@specforge/rust"));
    assert_eq!(scanners, vec!["@specforge/rust"]);
}

#[test]
fn scan_empty_source_list() {
    let dir = TempDir::new().unwrap();
    let runtime = rust_only_runtime();
    let manifests = vec![rust_manifest()];

    let ScanOutcome {
        items,
        scanners_used: scanners,
        ..
    } = scan_source_files(
        Some(&runtime),
        &InferenceSources::with_keys(dir.path(), &manifests, &[] as &[&str]),
    );

    assert!(items.is_empty());
    assert!(scanners.is_empty());
}

#[test]
fn scan_no_manifests_skips_all_files() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("lib.rs"), "pub fn hello() {}").unwrap();

    let runtime = rust_only_runtime();
    let source_files = vec!["lib.rs"];

    let ScanOutcome {
        items,
        scanners_used: scanners,
        ..
    } = scan_source_files(
        Some(&runtime),
        &InferenceSources::with_keys(dir.path(), &[], &source_files),
    );

    assert!(items.is_empty());
    assert!(scanners.is_empty());
}

#[test]
fn scan_missing_file_skipped_gracefully() {
    let dir = TempDir::new().unwrap();

    let runtime = rust_only_runtime();
    let manifests = vec![rust_manifest()];
    let source_files = vec!["nonexistent.rs"];

    let ScanOutcome {
        items,
        scanners_used: scanners,
        ..
    } = scan_source_files(
        Some(&runtime),
        &InferenceSources::with_keys(dir.path(), &manifests, &source_files),
    );

    assert!(items.is_empty());
    assert!(scanners.is_empty());
}

#[test]
fn default_runtime_scans_rust_files() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("main.rs"),
        "pub fn process_order() {}\npub struct Config {}",
    )
    .unwrap();

    let runtime = wasm_runtime_for(&["@specforge/rust"]);
    let manifests = vec![rust_manifest()];
    let source_files = vec!["main.rs"];

    let ScanOutcome {
        items,
        scanners_used: scanners,
        ..
    } = scan_source_files(
        Some(&runtime),
        &InferenceSources::with_keys(dir.path(), &manifests, &source_files),
    );

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].name, "process_order");
    assert_eq!(items[1].name, "Config");
    assert_eq!(scanners, vec!["@specforge/rust"]);
}

fn typescript_manifest() -> ExtensionDeclaration {
    ExtensionDeclaration {
        handshake: HandshakeResponse {
            name: "@specforge/typescript".into(),
            version: "1.0.0".into(),
            ..HandshakeResponse::default()
        },
        analyzers: vec![AnalyzerDescriptor {
            language: "typescript".into(),
            file_extensions: vec![".ts".into(), ".tsx".into(), ".js".into(), ".jsx".into()],
            excluded_dirs: vec!["node_modules".into(), "dist".into()],
            scan_export: "scan__typescript".into(),
            classify_export: "classify__typescript".into(),
            map_export: "map__typescript".into(),
            description: None,
        }],
        ..ExtensionDeclaration::default()
    }
}

#[test]
fn multi_scanner_mixed_project() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("lib.rs"), "pub fn hello() {}").unwrap();
    std::fs::write(
        dir.path().join("app.ts"),
        "export function handleRequest() {}\nexport class UserService {}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("component.tsx"),
        "export function render() {}",
    )
    .unwrap();
    std::fs::write(dir.path().join("utils.js"), "export const MAX = 10;").unwrap();
    std::fs::write(dir.path().join("readme.md"), "# Hello").unwrap();

    let runtime = wasm_runtime_for(&["@specforge/rust", "@specforge/typescript"]);
    let manifests = vec![rust_manifest(), typescript_manifest()];
    let source_files = vec!["lib.rs", "app.ts", "component.tsx", "utils.js", "readme.md"];

    let ScanOutcome {
        items,
        scanners_used: scanners,
        ..
    } = scan_source_files(
        Some(&runtime),
        &InferenceSources::with_keys(dir.path(), &manifests, &source_files),
    );

    assert_eq!(items.len(), 5);

    let rust_items: Vec<_> = items
        .iter()
        .filter(|i| i.scanner.as_deref() == Some("@specforge/rust"))
        .collect();
    assert_eq!(rust_items.len(), 1);
    assert_eq!(rust_items[0].name, "hello");

    let ts_items: Vec<_> = items
        .iter()
        .filter(|i| i.scanner.as_deref() == Some("@specforge/typescript"))
        .collect();
    assert_eq!(ts_items.len(), 4);
    assert!(ts_items.iter().any(|i| i.name == "handleRequest"));
    assert!(ts_items.iter().any(|i| i.name == "UserService"));
    assert!(ts_items.iter().any(|i| i.name == "render"));
    assert!(ts_items.iter().any(|i| i.name == "MAX"));

    assert_eq!(scanners.len(), 2);
    assert!(scanners.contains(&"@specforge/rust".to_string()));
    assert!(scanners.contains(&"@specforge/typescript".to_string()));
}

/// A scanner that traps, or answers what is not a scan response, on a
/// file: the file is reported as a failure (E028 naming the scanner), never
/// silently counted as having no public items, and the gap report it feeds
/// is approximate.
#[specforge_test_macros::test(
    behavior = "call_extension_exports",
    verify = "a scanner that traps or answers malformed output is reported, not dropped"
)]
fn a_scanner_that_fails_is_reported_not_dropped() {
    use specforge_wasm::testing::InProcessRuntime;
    use specforge_wasm::{CallFailure, WasmCallResult, WasmTrapInfo};

    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("a.rs"), "pub fn a() {}\n").unwrap();
    std::fs::write(dir.path().join("b.rs"), "pub fn b() {}\n").unwrap();
    let trapped = WasmCallResult::Trap(WasmTrapInfo {
        kind: "call_failed".into(),
        message: "unreachable: the scanner panicked".into(),
        export_name: "scan__rust".into(),
    });
    for answer in [trapped, WasmCallResult::Ok(b"garbage".to_vec())] {
        let runtime = InProcessRuntime::new().answer_raw("@specforge/rust", "scan__rust", answer);
        let outcome = scan_source_files(
            Some(&runtime),
            &InferenceSources::with_keys(dir.path(), &[rust_manifest()], &["a.rs", "b.rs"]),
        );
        assert!(outcome.items.is_empty() && outcome.scanners_used.is_empty());
        let failed: Vec<&str> = outcome.failures.iter().map(|f| f.file.as_str()).collect();
        assert_eq!(failed, ["a.rs", "b.rs"], "one failure per file");
        for failure in &outcome.failures {
            assert_eq!(failure.error.export, "scan__rust");
            assert_eq!(failure.error.extension, "@specforge/rust");
            let diagnostic = failure.error.diagnostic();
            assert_eq!(diagnostic.code, "E028");
            assert!(
                diagnostic
                    .message
                    .starts_with("scanner scan__rust() of '@specforge/rust' "),
                "{}",
                diagnostic.message
            );
        }
        assert!(!matches!(
            outcome.failures[0].error.failure,
            CallFailure::NotLoaded
        ));
    }
}

// ── The scanner's wire (plan 04 T1: compared with the goldens in
// `crates/runtime/wasm/tests/wire/`) ──

const EXT: &str = "@pin/ext";

fn wire_dir() -> std::path::PathBuf {
    specforge_repo_root::repo_root().join("crates/runtime/wasm/tests/wire")
}

fn sorted(value: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|k| (k.clone(), sorted(&map[k])))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

fn golden(name: &str, actual: &serde_json::Value) {
    let path = wire_dir().join(name);
    if std::env::var_os("SPECFORGE_BLESS").is_some() {
        let mut text = serde_json::to_string_pretty(&sorted(actual)).unwrap();
        text.push('\n');
        std::fs::write(&path, text).unwrap();
    }
    let expected: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("golden {}: {e}", path.display())),
    )
    .unwrap();
    assert_eq!(actual, &expected, "golden {name}");
}

fn answering(export: &str, result: WasmCallResult) -> InProcessRuntime {
    InProcessRuntime::new().answer_raw(EXT, export, result)
}

fn scanner() -> Vec<ExtensionDeclaration> {
    vec![ExtensionDeclaration {
        handshake: HandshakeResponse {
            name: EXT.into(),
            version: "1.0.0".into(),
            ..HandshakeResponse::default()
        },
        analyzers: vec![AnalyzerDescriptor {
            language: "rust".into(),
            file_extensions: vec![".rs".into()],
            excluded_dirs: Vec::new(),
            scan_export: "scan__rust".into(),
            classify_export: "classify__rust".into(),
            map_export: "map__rust".into(),
            description: None,
        }],
        ..ExtensionDeclaration::default()
    }]
}

fn sources() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("a.rs"), "pub fn a() {}\n").unwrap();
    dir
}

#[specforge_test(
    behavior = "call_extension_exports",
    verify = "every extension call encodes its input as the protocol type the SDK decodes"
)]
fn the_scan_request_and_its_answer() {
    let dir = sources();
    let answer = json!({"items": [{"name": "a", "item_kind": "function", "line": 1}],
                        "language": "rust"});
    let runtime = answering(
        "scan__rust",
        WasmCallResult::Ok(answer.to_string().into_bytes()),
    );
    let scanned = scan_source_files(
        Some(&runtime),
        &InferenceSources::with_keys(dir.path(), &scanner(), &["a.rs"]),
    );
    golden("scan.input.json", &runtime.calls()[0].input);
    assert_eq!(scanned.items.len(), 1);
    assert_eq!(
        (scanned.items[0].name.as_str(), scanned.items[0].line),
        ("a", 1)
    );
    assert_eq!(scanned.scanners_used, [EXT]);
    assert!(scanned.failures.is_empty());
}
