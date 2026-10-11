//! @typescript — SDK-authored Wasm twin of the TypeScript language analyzer
//! builtin.
//!
//! The analyzer is declared with the SDK builder (`c.analyzer`); the
//! handshake is derived by the SDK from the extension metadata and the
//! contributions. The scan/classify/map
//! exports port `crates/specforge-emitter/src/builtins/typescript.rs`
//! verbatim (pure string logic — no tree-sitter), and their JSON wire types
//! come from `specforge-protocol-types`, the same definitions the host uses.
use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::{
    ClassifiedItem, ClassifyRequest, ClassifyResponse, MapSymbolRequest, MapSymbolResponse,
    ScanRequest, ScanResponse, ScannedItem,
};

#[specforge_extension_sdk::extension(
    name = "@specforge/typescript",
    version = "1.0.0",
    description = "TypeScript and JavaScript analyzer: finds the exported symbols of sources for specforge infer"
)]
struct TypeScriptAnalyzer;

impl Contributions for TypeScriptAnalyzer {
    fn contribute(c: &mut ContributionsBuilder) {
        c.meta.sandbox_policy = Some(SandboxPolicy {
            max_memory_mb: Some(512),
            max_execution_ms: Some(30000),
        });
        c.analyzer("typescript", |a| {
            a.file_extensions(&[".ts", ".tsx", ".js", ".jsx"])
                .excluded_dirs(&["node_modules", "dist", "build", ".next", ".nuxt"])
                .description(
                    "Scans TypeScript/JavaScript source files for exported symbols (functions, classes, interfaces, types)",
                )
                .scan(scan_typescript);
        });
    }
}

// ── Analyzer exports ───────────────────────────────────────────────────────
// The scanner is declared with its handler above (`scan__typescript`). The
// declared `classify__typescript` and `map__typescript` exports, which the host does
// not call, are the guest handler's (ADR 0013, D12).

fn dispatch(export: &str, input: &[u8]) -> Option<Result<Vec<u8>, String>> {
    use specforge_extension_sdk::answer_export;
    match export {
        "classify__typescript" => Some(answer_export(export, input, classify_typescript)),
        "map__typescript" => Some(answer_export(export, input, map_typescript)),
        _ => None,
    }
}

specforge_extension_sdk::component_guest!(build = specforge_extension_build, handler = dispatch);

// ── Analyzer logic (ported verbatim from the native builtin) ───────────────

// Longest prefix first to avoid false matches
const EXPORT_PATTERNS: &[(&str, &str)] = &[
    ("export default async function ", "function"),
    ("export default function ", "function"),
    ("export default class ", "class"),
    ("export async function ", "function"),
    ("export function ", "function"),
    ("export abstract class ", "class"),
    ("export class ", "class"),
    ("export interface ", "interface"),
    ("export const enum ", "enum"),
    ("export type ", "type_alias"),
    ("export enum ", "enum"),
    ("export const ", "constant"),
    ("export let ", "variable"),
    ("export var ", "variable"),
];

fn scan_typescript(req: &ScanRequest) -> ScanResponse {
    let mut items = Vec::new();

    for (line_num, line) in req.content.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
            continue;
        }

        if trimmed.starts_with("export {") || trimmed.starts_with("export *") {
            continue;
        }

        if let Some(item) = parse_export(trimmed, line_num + 1) {
            items.push(item);
        }
    }

    ScanResponse {
        items,
        language: Some("typescript".into()),
    }
}

fn parse_export(line: &str, line_num: usize) -> Option<ScannedItem> {
    for (prefix, kind) in EXPORT_PATTERNS {
        if !line.starts_with(prefix) {
            continue;
        }
        let rest = &line[prefix.len()..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
            .collect();
        if name.is_empty() {
            continue;
        }

        let visibility = if line.contains("export default") {
            "default"
        } else {
            "export"
        };

        let signature = extract_signature(line);

        return Some(ScannedItem {
            name,
            item_kind: kind.to_string(),
            line: line_num,
            visibility: Some(visibility.into()),
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

fn classify_typescript(req: &ClassifyRequest) -> ClassifyResponse {
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
    if is_test_file(file_path) {
        return (None, 0.0);
    }

    match item_kind {
        "function" => {
            if name.starts_with("handle")
                || name.starts_with("process")
                || name.ends_with("Handler")
                || name.starts_with("create")
                || name.starts_with("update")
                || name.starts_with("delete")
                || name.starts_with("get")
            {
                (Some("behavior".into()), 0.8)
            } else {
                (Some("behavior".into()), 0.5)
            }
        }
        "class" => {
            if name.ends_with("Error") || name.ends_with("Event") || name.ends_with("Message") {
                (Some("event".into()), 0.7)
            } else if name.ends_with("Service")
                || name.ends_with("Client")
                || name.ends_with("Port")
                || name.ends_with("Repository")
                || name.ends_with("Gateway")
            {
                (Some("port".into()), 0.7)
            } else {
                (Some("type".into()), 0.6)
            }
        }
        "interface" => {
            if name.ends_with("Service")
                || name.ends_with("Port")
                || name.ends_with("Repository")
                || name.ends_with("Gateway")
                || (name.starts_with('I')
                    && name.len() > 1
                    && name.chars().nth(1).is_some_and(|c| c.is_uppercase()))
            {
                (Some("port".into()), 0.8)
            } else {
                (Some("type".into()), 0.6)
            }
        }
        "type_alias" => (Some("type".into()), 0.7),
        "enum" => (Some("type".into()), 0.7),
        "constant" | "variable" => (None, 0.3),
        _ => (None, 0.3),
    }
}

fn map_typescript(req: &MapSymbolRequest) -> MapSymbolResponse {
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

fn is_test_file(path: &str) -> bool {
    path.contains("/tests/")
        || path.contains("/test/")
        || path.contains("/__tests__/")
        || path.contains("/__mocks__/")
        || path.ends_with(".test.ts")
        || path.ends_with(".test.tsx")
        || path.ends_with(".test.js")
        || path.ends_with(".test.jsx")
        || path.ends_with(".spec.ts")
        || path.ends_with(".spec.tsx")
        || path.ends_with(".spec.js")
        || path.ends_with(".spec.jsx")
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_protocol_types::{AnalyzerDescriptor, DescribeResponse};
    // --- Scan tests ---

    #[test]
    fn scan_finds_exported_functions() {
        let req = ScanRequest {
            file_path: "src/handlers.ts".into(),
            content: "export function hello() {}\nfunction private() {}".into(),
        };
        let resp = scan_typescript(&req);
        assert_eq!(resp.items.len(), 1);
        assert_eq!(resp.items[0].name, "hello");
        assert_eq!(resp.items[0].item_kind, "function");
        assert_eq!(resp.items[0].line, 1);
    }

    #[test]
    fn scan_finds_exported_classes() {
        let req = ScanRequest {
            file_path: "src/service.ts".into(),
            content: "export class MyService {}".into(),
        };
        let resp = scan_typescript(&req);
        assert_eq!(resp.items.len(), 1);
        assert_eq!(resp.items[0].name, "MyService");
        assert_eq!(resp.items[0].item_kind, "class");
    }

    #[test]
    fn scan_finds_exported_interfaces() {
        let req = ScanRequest {
            file_path: "src/ports.ts".into(),
            content: "export interface UserPort {}".into(),
        };
        let resp = scan_typescript(&req);
        assert_eq!(resp.items.len(), 1);
        assert_eq!(resp.items[0].name, "UserPort");
        assert_eq!(resp.items[0].item_kind, "interface");
    }

    #[test]
    fn scan_finds_exported_types() {
        let req = ScanRequest {
            file_path: "src/types.ts".into(),
            content: "export type Result = Success | Failure;".into(),
        };
        let resp = scan_typescript(&req);
        assert_eq!(resp.items.len(), 1);
        assert_eq!(resp.items[0].name, "Result");
        assert_eq!(resp.items[0].item_kind, "type_alias");
    }

    #[test]
    fn scan_finds_exported_enums() {
        let req = ScanRequest {
            file_path: "src/status.ts".into(),
            content: "export enum Status { Active, Inactive }".into(),
        };
        let resp = scan_typescript(&req);
        assert_eq!(resp.items.len(), 1);
        assert_eq!(resp.items[0].name, "Status");
        assert_eq!(resp.items[0].item_kind, "enum");
    }

    #[test]
    fn scan_finds_async_functions() {
        let req = ScanRequest {
            file_path: "src/api.ts".into(),
            content: "export async function fetchData() {}".into(),
        };
        let resp = scan_typescript(&req);
        assert_eq!(resp.items.len(), 1);
        assert_eq!(resp.items[0].name, "fetchData");
        assert_eq!(resp.items[0].item_kind, "function");
    }

    #[test]
    fn scan_finds_default_exports() {
        let req = ScanRequest {
            file_path: "src/main.ts".into(),
            content: "export default function main() {}".into(),
        };
        let resp = scan_typescript(&req);
        assert_eq!(resp.items.len(), 1);
        assert_eq!(resp.items[0].name, "main");
        assert_eq!(resp.items[0].item_kind, "function");
        assert_eq!(resp.items[0].visibility.as_deref(), Some("default"));
    }

    #[test]
    fn scan_finds_abstract_classes() {
        let req = ScanRequest {
            file_path: "src/base.ts".into(),
            content: "export abstract class Base {}".into(),
        };
        let resp = scan_typescript(&req);
        assert_eq!(resp.items.len(), 1);
        assert_eq!(resp.items[0].name, "Base");
        assert_eq!(resp.items[0].item_kind, "class");
    }

    #[test]
    fn scan_finds_const_enum() {
        let req = ScanRequest {
            file_path: "src/direction.ts".into(),
            content: "export const enum Direction { Up, Down }".into(),
        };
        let resp = scan_typescript(&req);
        assert_eq!(resp.items.len(), 1);
        assert_eq!(resp.items[0].name, "Direction");
        assert_eq!(resp.items[0].item_kind, "enum");
    }

    #[test]
    fn scan_skips_comments() {
        let req = ScanRequest {
            file_path: "src/lib.ts".into(),
            content: "// export function fake() {}\nexport function real() {}".into(),
        };
        let resp = scan_typescript(&req);
        assert_eq!(resp.items.len(), 1);
        assert_eq!(resp.items[0].name, "real");
    }

    #[test]
    fn scan_skips_reexports() {
        let req = ScanRequest {
            file_path: "src/index.ts".into(),
            content: "export { UserService } from './user';".into(),
        };
        let resp = scan_typescript(&req);
        assert_eq!(resp.items.len(), 0);
    }

    #[test]
    fn scan_skips_barrel_exports() {
        let req = ScanRequest {
            file_path: "src/index.ts".into(),
            content: "export * from './module';".into(),
        };
        let resp = scan_typescript(&req);
        assert_eq!(resp.items.len(), 0);
    }

    #[test]
    fn scan_skips_anonymous_default() {
        let req = ScanRequest {
            file_path: "src/handler.ts".into(),
            content: "export default () => {};".into(),
        };
        let resp = scan_typescript(&req);
        assert_eq!(resp.items.len(), 0);
    }

    #[test]
    fn scan_captures_signature() {
        let req = ScanRequest {
            file_path: "src/api.ts".into(),
            content: "export function process(input: string): Result {".into(),
        };
        let resp = scan_typescript(&req);
        assert_eq!(resp.items.len(), 1);
        assert_eq!(
            resp.items[0].signature.as_deref(),
            Some("export function process(input: string): Result")
        );
    }

    #[test]
    fn scan_finds_const_and_let() {
        let req = ScanRequest {
            file_path: "src/config.ts".into(),
            content:
                "export const MAX_SIZE = 100;\nexport let counter = 0;\nexport var legacy = true;"
                    .into(),
        };
        let resp = scan_typescript(&req);
        assert_eq!(resp.items.len(), 3);
        assert_eq!(resp.items[0].name, "MAX_SIZE");
        assert_eq!(resp.items[0].item_kind, "constant");
        assert_eq!(resp.items[1].name, "counter");
        assert_eq!(resp.items[1].item_kind, "variable");
        assert_eq!(resp.items[2].name, "legacy");
        assert_eq!(resp.items[2].item_kind, "variable");
    }

    #[test]
    fn scan_finds_default_async_function() {
        let req = ScanRequest {
            file_path: "src/main.ts".into(),
            content: "export default async function bootstrap() {}".into(),
        };
        let resp = scan_typescript(&req);
        assert_eq!(resp.items.len(), 1);
        assert_eq!(resp.items[0].name, "bootstrap");
        assert_eq!(resp.items[0].item_kind, "function");
        assert_eq!(resp.items[0].visibility.as_deref(), Some("default"));
    }

    // --- Classify tests ---

    #[test]
    fn classify_handler_as_behavior() {
        let req = ClassifyRequest {
            file_path: "src/handlers.ts".into(),
            items: vec![ScannedItem {
                name: "handleLogin".into(),
                item_kind: "function".into(),
                line: 10,
                visibility: Some("export".into()),
                signature: None,
            }],
        };
        let resp = classify_typescript(&req);
        assert_eq!(
            resp.items[0].suggested_entity_kind.as_deref(),
            Some("behavior")
        );
        assert!(resp.items[0].confidence >= 0.8);
    }

    #[test]
    fn classify_interface_as_port() {
        let req = ClassifyRequest {
            file_path: "src/ports.ts".into(),
            items: vec![ScannedItem {
                name: "UserRepository".into(),
                item_kind: "interface".into(),
                line: 1,
                visibility: Some("export".into()),
                signature: None,
            }],
        };
        let resp = classify_typescript(&req);
        assert_eq!(resp.items[0].suggested_entity_kind.as_deref(), Some("port"));
        assert!(resp.items[0].confidence >= 0.8);
    }

    #[test]
    fn classify_i_prefixed_interface_as_port() {
        let req = ClassifyRequest {
            file_path: "src/ports.ts".into(),
            items: vec![ScannedItem {
                name: "ILogger".into(),
                item_kind: "interface".into(),
                line: 1,
                visibility: Some("export".into()),
                signature: None,
            }],
        };
        let resp = classify_typescript(&req);
        assert_eq!(resp.items[0].suggested_entity_kind.as_deref(), Some("port"));
    }

    #[test]
    fn classify_class_as_type() {
        let req = ClassifyRequest {
            file_path: "src/models.ts".into(),
            items: vec![ScannedItem {
                name: "Config".into(),
                item_kind: "class".into(),
                line: 1,
                visibility: Some("export".into()),
                signature: None,
            }],
        };
        let resp = classify_typescript(&req);
        assert_eq!(resp.items[0].suggested_entity_kind.as_deref(), Some("type"));
        assert!(resp.items[0].confidence >= 0.5);
    }

    #[test]
    fn classify_skips_test_files() {
        let req = ClassifyRequest {
            file_path: "src/__tests__/helper.ts".into(),
            items: vec![ScannedItem {
                name: "setupDb".into(),
                item_kind: "function".into(),
                line: 1,
                visibility: Some("export".into()),
                signature: None,
            }],
        };
        let resp = classify_typescript(&req);
        assert!(resp.items[0].suggested_entity_kind.is_none());
    }

    #[test]
    fn classify_skips_spec_files() {
        let req = ClassifyRequest {
            file_path: "src/service.spec.ts".into(),
            items: vec![ScannedItem {
                name: "testHelper".into(),
                item_kind: "function".into(),
                line: 1,
                visibility: Some("export".into()),
                signature: None,
            }],
        };
        let resp = classify_typescript(&req);
        assert!(resp.items[0].suggested_entity_kind.is_none());
    }

    #[test]
    fn classify_error_class_as_event() {
        let req = ClassifyRequest {
            file_path: "src/errors.ts".into(),
            items: vec![ScannedItem {
                name: "ValidationError".into(),
                item_kind: "class".into(),
                line: 1,
                visibility: Some("export".into()),
                signature: None,
            }],
        };
        let resp = classify_typescript(&req);
        assert_eq!(
            resp.items[0].suggested_entity_kind.as_deref(),
            Some("event")
        );
    }

    // --- Map tests ---

    #[test]
    fn map_matches_existing_snake_case() {
        let req = MapSymbolRequest {
            name: "MyService".into(),
            item_kind: "class".into(),
            file_path: "src/service.ts".into(),
            existing_entity_ids: vec!["my_service".into()],
        };
        let resp = map_typescript(&req);
        assert_eq!(resp.entity_id.as_deref(), Some("my_service"));
        assert_eq!(resp.mapping_strategy, "exact_snake_case");
    }

    #[test]
    fn map_generates_snake_case() {
        let req = MapSymbolRequest {
            name: "ConfigLoader".into(),
            item_kind: "class".into(),
            file_path: "src/config.ts".into(),
            existing_entity_ids: vec![],
        };
        let resp = map_typescript(&req);
        assert_eq!(resp.entity_id.as_deref(), Some("config_loader"));
        assert_eq!(resp.mapping_strategy, "generated_snake_case");
    }

    #[test]
    fn map_camel_case_function() {
        let req = MapSymbolRequest {
            name: "fetchUserData".into(),
            item_kind: "function".into(),
            file_path: "src/api.ts".into(),
            existing_entity_ids: vec!["fetch_user_data".into()],
        };
        let resp = map_typescript(&req);
        assert_eq!(resp.entity_id.as_deref(), Some("fetch_user_data"));
        assert_eq!(resp.mapping_strategy, "exact_snake_case");
    }

    // --- Protocol tests ---

    #[test]
    fn handshake_declares_analyzer() {
        let handshake: specforge_protocol_types::HandshakeResponse =
            serde_json::from_str(&specforge_extension_build().handshake_json()).unwrap();
        assert_eq!(handshake.name, "@specforge/typescript");
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
        assert_eq!(descriptors[0].language, "typescript");
        assert_eq!(
            descriptors[0].file_extensions,
            vec![".ts", ".tsx", ".js", ".jsx"]
        );
        assert_eq!(descriptors[0].scan_export, "scan__typescript");
    }
}
