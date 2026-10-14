//! @typescript — SDK-authored Wasm twin of the TypeScript language analyzer
//! builtin.
//!
//! The analyzer is declared with the SDK builder (`c.analyzer`); the
//! handshake is derived by the SDK from the extension metadata and the
//! contributions. The scan/classify/map
//! exports port `crates/pipeline/emitter/src/builtins/typescript.rs`
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
mod tests;
