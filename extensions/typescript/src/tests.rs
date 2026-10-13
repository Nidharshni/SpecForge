use super::*;
use specforge_extension_sdk::testing::MockHost;
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
        content: "export const MAX_SIZE = 100;\nexport let counter = 0;\nexport var legacy = true;"
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
        serde_json::from_str(&specforge_extension_build().declaration().handshake_json()).unwrap();
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
    let json = MockHost::new(specforge_extension_build())
        .describe_json("analyzers")
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
