//! @rust — SDK-authored Wasm twin of the Rust language analyzer builtin.
//!
//! The analyzer is declared with the SDK builder (`c.analyzer`); the
//! handshake is derived by the SDK from the extension metadata and the
//! contributions. The scan/classify/map
//! exports port `crates/specforge-emitter/src/builtins/rust.rs` verbatim
//! (pure string logic — no tree-sitter), and their JSON wire types come from
//! `specforge-protocol-types`, the same definitions the host uses.

use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::{
    ClassifiedItem, ClassifyRequest, ClassifyResponse, MapSymbolRequest, MapSymbolResponse,
    ScanRequest, ScanResponse, ScannedItem,
};

#[specforge_extension_sdk::extension(
    name = "@specforge/rust",
    version = "1.0.0",
    description = "Rust analyzer: finds the public items of Rust sources for specforge infer"
)]
struct RustAnalyzer;

impl Contributions for RustAnalyzer {
    fn contribute(c: &mut ContributionsBuilder) {
        c.meta.sandbox_policy = Some(SandboxPolicy {
            max_memory_mb: Some(512),
            max_execution_ms: Some(30000),
        });
        c.analyzer("rust", |a| {
            a.file_extensions(&[".rs"])
                .excluded_dirs(&["target"])
                .description(
                    "Scans Rust source files for public items (functions, structs, enums, traits)",
                )
                .scan(scan_rust);
        });
    }
}

// ── Analyzer exports ───────────────────────────────────────────────────────
// The scanner is declared with its handler above (`scan__rust`). The
// declared `classify__rust` and `map__rust` exports, which the host does
// not call, are the guest handler's (ADR 0013, D12).

fn dispatch(export: &str, input: &[u8]) -> Option<Result<Vec<u8>, String>> {
    use specforge_extension_sdk::answer_export;
    match export {
        "classify__rust" => Some(answer_export(export, input, classify_rust)),
        "map__rust" => Some(answer_export(export, input, map_rust)),
        _ => None,
    }
}

specforge_extension_sdk::component_guest!(build = specforge_extension_build, handler = dispatch);

// ── Analyzer logic (ported verbatim from the native builtin) ───────────────

const PUB_PATTERNS: &[(&str, &str)] = &[
    ("pub fn ", "function"),
    ("pub async fn ", "function"),
    ("pub struct ", "struct"),
    ("pub enum ", "enum"),
    ("pub trait ", "trait"),
    ("pub type ", "type_alias"),
    ("pub const ", "constant"),
    ("pub static ", "static"),
];

fn scan_rust(req: &ScanRequest) -> ScanResponse {
    let mut items = Vec::new();

    for (line_num, line) in req.content.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
            continue;
        }

        if let Some(item) = parse_pub_item(trimmed, line_num + 1) {
            items.push(item);
        }
    }

    ScanResponse {
        items,
        language: Some("rust".into()),
    }
}

fn parse_pub_item(line: &str, line_num: usize) -> Option<ScannedItem> {
    for (prefix, kind) in PUB_PATTERNS {
        if !line.starts_with(prefix) {
            continue;
        }
        let rest = &line[prefix.len()..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }

        let signature = extract_signature(line);

        return Some(ScannedItem {
            name,
            item_kind: kind.to_string(),
            line: line_num,
            visibility: Some("pub".into()),
            signature: Some(signature),
        });
    }
    None
}

fn extract_signature(line: &str) -> String {
    if let Some(brace) = line.find('{') {
        line[..brace].trim().to_string()
    } else {
        line.trim_end_matches(';').trim().to_string()
    }
}

fn classify_rust(req: &ClassifyRequest) -> ClassifyResponse {
    let items = req
        .items
        .iter()
        .map(|item| {
            let (suggested, confidence) =
                classify_item(&item.item_kind, &item.name, &req.file_path);
            ClassifiedItem {
                name: item.name.clone(),
                item_kind: item.item_kind.clone(),
                suggested_entity_kind: suggested,
                confidence,
                line: item.line,
            }
        })
        .collect();

    ClassifyResponse { items }
}

fn classify_item(item_kind: &str, name: &str, file_path: &str) -> (Option<String>, f64) {
    if is_test_or_build_file(file_path) {
        return (None, 0.0);
    }

    match item_kind {
        "function" => {
            if name.starts_with("handle_")
                || name.starts_with("process_")
                || name.ends_with("_handler")
                || name.starts_with("create_")
                || name.starts_with("update_")
                || name.starts_with("delete_")
                || name.starts_with("get_")
            {
                (Some("behavior".into()), 0.8)
            } else {
                (Some("behavior".into()), 0.5)
            }
        }
        "struct" => {
            if name.ends_with("Error") || name.ends_with("Event") || name.ends_with("Message") {
                (Some("event".into()), 0.7)
            } else if name.ends_with("Port")
                || name.ends_with("Client")
                || name.ends_with("Service")
            {
                (Some("port".into()), 0.7)
            } else {
                (Some("type".into()), 0.6)
            }
        }
        "enum" => (Some("type".into()), 0.7),
        "trait" => (Some("port".into()), 0.8),
        _ => (None, 0.3),
    }
}

fn map_rust(req: &MapSymbolRequest) -> MapSymbolResponse {
    let snake = to_snake_case(&req.name);

    if req.existing_entity_ids.contains(&snake) {
        return MapSymbolResponse {
            entity_id: Some(snake),
            mapping_strategy: "exact_snake_case".into(),
        };
    }

    if req.existing_entity_ids.contains(&req.name) {
        return MapSymbolResponse {
            entity_id: Some(req.name.clone()),
            mapping_strategy: "exact_original".into(),
        };
    }

    MapSymbolResponse {
        entity_id: Some(snake),
        mapping_strategy: "generated_snake_case".into(),
    }
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_lowercase().next().unwrap_or(c));
    }
    result
}

fn is_test_or_build_file(path: &str) -> bool {
    path.contains("/tests/")
        || path.contains("/test/")
        || path.ends_with("_test.rs")
        || path.ends_with("/build.rs")
        || path.contains("/benches/")
        || path.contains("/examples/")
}

#[cfg(test)]
mod tests {
    use super::*;
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
            serde_json::from_str(&specforge_extension_build().handshake_json()).unwrap();
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
        let json = specforge_extension_build()
            .describe_response_json("analyzers")
            .expect("analyzers category");
        let resp: DescribeResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp.category, "analyzers");
        let descriptors: Vec<AnalyzerDescriptor> = resp.parse_items().unwrap();
        assert_eq!(descriptors.len(), 1);
        assert_eq!(descriptors[0].language, "rust");
        assert_eq!(descriptors[0].scan_export, "scan__rust");
    }
}
