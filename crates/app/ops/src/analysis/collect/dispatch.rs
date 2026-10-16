//! Reading a runner's report and handing it to the extension's `collect__<name>` export.

use super::collector::Collector;
use crate::{Containment, Held, OpError, OpErrorKind};
use specforge_common::codes;
use specforge_protocol_types::{CollectInput, CollectOutput, CollectReportFile};
use specforge_wasm::{CallError, CallFailure, ExtensionCalls, Operation};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    files
}

// ── reading and dispatch ────────────────────────────────────────────────────

/// Read the report at `report`: the file itself, or every `*.json` file
/// directly inside a directory, only those modified at or after `since`
/// when given. Each file is held by `containment` before it is opened (one that
/// lands outside is `outside_root`); a read that fails is E045. Paths are reported
/// relative to `root`.
pub(super) fn read_report(
    report: &Held,
    root: &Path,
    since: Option<SystemTime>,
    containment: &Containment,
) -> Result<Vec<CollectReportFile>, OpError> {
    let fresh = |path: &PathBuf| {
        since.is_none_or(|since| {
            std::fs::metadata(path)
                .and_then(|m| m.modified())
                .is_ok_and(|modified| modified >= since)
        })
    };
    let mut held = Vec::new();
    if report.real.is_dir() {
        for path in json_files(&report.real).into_iter().filter(fresh) {
            let name = path.file_name().unwrap_or_default();
            held.push(containment.resolve(&report.spelled.join(name))?);
        }
    } else if report.real.is_file() {
        held.push(report.clone());
    }
    held.iter()
        .map(|file| {
            let content = file.read_to_string().map_err(|e| {
                OpError::coded(
                    OpErrorKind::SchemaMismatch,
                    codes::E045,
                    format!("failed to read report {}: {e}", file.spelled.display()),
                )
            })?;
            let shown = file.spelled.strip_prefix(root).unwrap_or(&file.spelled);
            Ok(CollectReportFile {
                path: shown.display().to_string(),
                content,
            })
        })
        .collect()
}

/// Hand the report files, and the captured standard output if any, to the
/// collector's pure export: the protocol's `CollectInput` in, its
/// `CollectOutput` out. Err: the export trapped, or answered something
/// that is not a `CollectOutput` (E028 naming the collector).
pub fn dispatch(
    runtime: Option<&dyn specforge_wasm::runtime::WasmRuntime>,
    collector: &Collector,
    reports: &[CollectReportFile],
    stdout: Option<&str>,
) -> Result<CollectOutput, CallError> {
    let input = CollectInput {
        reports: reports.to_vec(),
        stdout: stdout.map(str::to_string),
    };
    let Some(runtime) = runtime else {
        return Err(CallError::new(
            Operation::Collect,
            &collector.extension,
            &collector.export,
            CallFailure::NotLoaded,
        ));
    };
    ExtensionCalls::new(runtime).collect(&collector.extension, &collector.export, &input)
}
