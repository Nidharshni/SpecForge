//! Scanning the project's source files for public items, through the
//! enabled extensions' analyzers (each `scan__<language>` export, an
//! `ExtensionCalls::scan`).

use super::sources::{InferenceSources, SourceRead};
use crate::read::infer::SourceItem;
use specforge_protocol_types::ScanRequest;
use specforge_wasm::runtime::WasmRuntime;
use specforge_wasm::{CallError, CallFailure, ExtensionCalls, Operation};

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

/// Scan the inference sources with the analyzer that claims each one's extension. A file that
/// cannot be read, or is not UTF-8, is skipped; a scanner that fails on a file is reported,
/// never dropped.
pub fn scan_source_files(
    runtime: Option<&dyn WasmRuntime>,
    sources: &InferenceSources,
) -> ScanOutcome {
    let calls = runtime.map(ExtensionCalls::new);
    let mut outcome = ScanOutcome::default();
    for key in sources.keys() {
        let Some(entry) = sources.claim(key) else {
            continue;
        };
        let content = match sources.files().read(key) {
            SourceRead::Bytes(bytes) => match String::from_utf8(bytes) {
                Ok(content) => content,
                Err(_) => continue,
            },
            _ => continue,
        };
        let file_path = key.as_str();

        let request = ScanRequest {
            file_path: file_path.to_string(),
            content,
        };
        let Some(calls) = &calls else {
            outcome.failures.push(ScanFailure {
                file: file_path.to_string(),
                error: CallError::new(
                    Operation::Scan,
                    &entry.analyzer,
                    &entry.scan_export,
                    CallFailure::NotLoaded,
                ),
            });
            continue;
        };
        match calls.scan(&entry.analyzer, &entry.scan_export, &request) {
            Ok(response) => {
                for item in response.items {
                    outcome.items.push(SourceItem {
                        name: item.name,
                        item_kind: item.item_kind,
                        file: file_path.to_string(),
                        line: item.line,
                        scanner: Some(entry.analyzer.clone()),
                    });
                }
                if !outcome.scanners_used.contains(&entry.analyzer) {
                    outcome.scanners_used.push(entry.analyzer.clone());
                }
            }
            Err(error) => outcome.failures.push(ScanFailure {
                file: file_path.to_string(),
                error,
            }),
        }
    }
    outcome
}

#[cfg(test)]
mod tests;
