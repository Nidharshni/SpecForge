//! Scanning the project's source files for public items, through the
//! enabled extensions' analyzers (each `scan__<language>` export, an
//! `ExtensionCalls::scan`).

use std::collections::HashMap;
use std::path::Path;

use crate::infer::SourceItem;
use specforge_protocol_types::{ExtensionDeclaration, ScanRequest};
use specforge_wasm::runtime::WasmRuntime;
use specforge_wasm::{CallError, CallFailure, ExtensionCalls, Operation};

struct ScannerEntry {
    extension_name: String,
    scan_export: String,
}

/// What a scan found, and which scans failed.
#[derive(Debug, Clone, Default)]
pub struct ScanOutcome {
    /// The public items every scanner that answered found.
    pub items: Vec<SourceItem>,
    /// The extensions whose scanner answered, in first-use order.
    pub scanners_used: Vec<String>,
    /// The files whose scanner did not answer (it trapped, or answered
    /// something that is not a scan response): their items are unknown.
    pub failures: Vec<ScanFailure>,
}

/// A file a scanner failed on.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanFailure {
    /// The file, relative to the project root.
    pub file: String,
    pub error: CallError,
}

/// Scan `source_files` (relative to `project_root`) with the analyzer that
/// claims each one's extension. A file no analyzer claims, or that cannot
/// be read, is skipped; a scanner that fails on a file is reported, never
/// dropped.
pub fn scan_source_files(
    runtime: Option<&dyn WasmRuntime>,
    declarations: &[ExtensionDeclaration],
    project_root: &Path,
    source_files: &[String],
) -> ScanOutcome {
    let mut ext_lookup: HashMap<String, ScannerEntry> = HashMap::new();
    for declaration in declarations {
        for ac in &declaration.analyzers {
            for ext in &ac.file_extensions {
                let normalized = if ext.starts_with('.') {
                    ext.clone()
                } else {
                    format!(".{}", ext)
                };
                ext_lookup
                    .entry(normalized)
                    .or_insert_with(|| ScannerEntry {
                        extension_name: declaration.name().to_string(),
                        scan_export: ac.scan_export.clone(),
                    });
            }
        }
    }

    let calls = runtime.map(ExtensionCalls::new);
    let mut outcome = ScanOutcome::default();
    for file_path in source_files {
        let file_ext = match file_path.rfind('.') {
            Some(i) => &file_path[i..],
            None => continue,
        };

        let entry = match ext_lookup.get(file_ext) {
            Some(e) => e,
            None => continue,
        };

        let abs_path = project_root.join(file_path);
        let content = match std::fs::read_to_string(&abs_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let request = ScanRequest {
            file_path: file_path.clone(),
            content,
        };
        let Some(calls) = &calls else {
            outcome.failures.push(ScanFailure {
                file: file_path.clone(),
                error: CallError::new(
                    Operation::Scan,
                    &entry.extension_name,
                    &entry.scan_export,
                    CallFailure::NotLoaded,
                ),
            });
            continue;
        };
        match calls.scan(&entry.extension_name, &entry.scan_export, &request) {
            Ok(response) => {
                for item in response.items {
                    outcome.items.push(SourceItem {
                        name: item.name,
                        item_kind: item.item_kind,
                        file: file_path.clone(),
                        line: item.line,
                        scanner: Some(entry.extension_name.clone()),
                    });
                }
                if !outcome.scanners_used.contains(&entry.extension_name) {
                    outcome.scanners_used.push(entry.extension_name.clone());
                }
            }
            Err(error) => outcome.failures.push(ScanFailure {
                file: file_path.clone(),
                error,
            }),
        }
    }
    outcome
}

#[cfg(test)]
mod tests;
