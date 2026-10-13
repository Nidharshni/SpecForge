use super::*;
use specforge_extension_sdk::testing::MockHost;
use specforge_protocol_types::{AnalyzerDescriptor, DescribeResponse};
#[test]
fn scan_finds_pub_items() {
    let req = ScanRequest {
        file_path: "src/lib.rs".into(),
        content: "pub fn hello() {}\npub struct Config {}\nfn private() {}".into(),
    };
    let resp = scan_rust(&req);
    assert_eq!(resp.items.len(), 2);
    assert_eq!(resp.items[0].name, "hello");
    assert_eq!(resp.items[0].item_kind, "function");
    assert_eq!(resp.items[0].line, 1);
    assert_eq!(resp.items[1].name, "Config");
    assert_eq!(resp.items[1].item_kind, "struct");
}
#[test]
fn scan_skips_comments() {
    let req = ScanRequest {
        file_path: "src/lib.rs".into(),
        content: "// pub fn commented() {}\npub fn real() {}".into(),
    };
    let resp = scan_rust(&req);
    assert_eq!(resp.items.len(), 1);
    assert_eq!(resp.items[0].name, "real");
}

#[test]
fn scan_captures_signature() {
    let req = ScanRequest {
        file_path: "src/lib.rs".into(),
        content: "pub fn process(input: &str) -> Result<Output, Error> {".into(),
    };
    let resp = scan_rust(&req);
    assert_eq!(resp.items.len(), 1);
    assert_eq!(
        resp.items[0].signature.as_deref(),
        Some("pub fn process(input: &str) -> Result<Output, Error>")
    );
}

#[test]
fn scan_finds_async_fn() {
    let req = ScanRequest {
        file_path: "src/lib.rs".into(),
        content: "pub async fn fetch_data() {}".into(),
    };
    let resp = scan_rust(&req);
    assert_eq!(resp.items.len(), 1);
    assert_eq!(resp.items[0].name, "fetch_data");
    assert_eq!(resp.items[0].item_kind, "function");
}

#[test]
fn classify_handler_as_behavior() {
    let req = ClassifyRequest {
        file_path: "src/handlers.rs".into(),
        items: vec![ScannedItem {
            name: "handle_login".into(),
            item_kind: "function".into(),
            line: 10,
            visibility: Some("pub".into()),
            signature: None,
        }],
    };
    let resp = classify_rust(&req);
    assert_eq!(resp.items.len(), 1);
    assert_eq!(
        resp.items[0].suggested_entity_kind.as_deref(),
        Some("behavior")
    );
    assert!(resp.items[0].confidence >= 0.8);
}

#[test]
fn classify_trait_as_port() {
    let req = ClassifyRequest {
        file_path: "src/ports.rs".into(),
        items: vec![ScannedItem {
            name: "Repository".into(),
            item_kind: "trait".into(),
            line: 1,
            visibility: Some("pub".into()),
            signature: None,
        }],
    };
    let resp = classify_rust(&req);
    assert_eq!(resp.items[0].suggested_entity_kind.as_deref(), Some("port"));
}

#[test]
fn classify_skips_test_files() {
    let req = ClassifyRequest {
        file_path: "src/tests/helpers.rs".into(),
        items: vec![ScannedItem {
            name: "setup_db".into(),
            item_kind: "function".into(),
            line: 1,
            visibility: Some("pub".into()),
            signature: None,
        }],
    };
    let resp = classify_rust(&req);
    assert!(resp.items[0].suggested_entity_kind.is_none());
}

#[test]
fn map_matches_existing_snake_case() {
    let req = MapSymbolRequest {
        name: "MyService".into(),
        item_kind: "struct".into(),
        file_path: "src/lib.rs".into(),
        existing_entity_ids: vec!["my_service".into()],
    };
    let resp = map_rust(&req);
    assert_eq!(resp.entity_id.as_deref(), Some("my_service"));
    assert_eq!(resp.mapping_strategy, "exact_snake_case");
}

#[test]
fn map_generates_snake_case_when_no_match() {
    let req = MapSymbolRequest {
        name: "ConfigLoader".into(),
        item_kind: "struct".into(),
        file_path: "src/lib.rs".into(),
        existing_entity_ids: vec![],
    };
    let resp = map_rust(&req);
    assert_eq!(resp.entity_id.as_deref(), Some("config_loader"));
    assert_eq!(resp.mapping_strategy, "generated_snake_case");
}

#[test]
fn handshake_declares_analyzer() {
    let handshake: specforge_protocol_types::HandshakeResponse =
        serde_json::from_str(&specforge_extension_build().declaration().handshake_json()).unwrap();
    assert_eq!(handshake.name, "@specforge/rust");
    assert!(!handshake.providers);
    let declaration = specforge_extension_build().declaration();
    assert!(declaration.declares(specforge_extension_sdk::DeclaredCategory::Analyzers));
    assert!(!declaration.declares(specforge_extension_sdk::DeclaredCategory::Entities));
    let policy = handshake.sandbox_policy.expect("sandbox policy");
    assert_eq!(policy.max_memory_mb, Some(512));
    assert_eq!(policy.max_execution_ms, Some(30000));
}

#[test]
fn describe_returns_analyzer_descriptor() {
    let json = MockHost::new(specforge_extension_build())
        .describe_json("analyzers")
        .expect("analyzers category");
    let resp: DescribeResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(resp.category, "analyzers");
    let descriptors: Vec<AnalyzerDescriptor> = resp.parse_items().unwrap();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].language, "rust");
    assert_eq!(descriptors[0].scan_export, "scan__rust");
}
