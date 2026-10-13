//! `extensions/<dir>/wasm/inputs.json`: what one builtin blob was built from. `build-builtins` writes and checks it
//! through this module; a quick run asks it which blobs a change set feeds. The JSON is unchanged byte for byte.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::change_set::ChangeSet;

pub const FILE: &str = "inputs.json";
/// The key for the root manifest's `[workspace.package]` and `[workspace.dependencies]` tables: the shared crates
/// inherit edition, version, license and dependency specs from them, and only those tables can change what they
/// compile to.
pub const WORKSPACE_INHERITED: &str = "Cargo.toml#workspace-inherited";
pub const INHERITED_TABLES: &[&str] = &["[workspace.package]", "[workspace.dependencies]"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobRecord {
    pub artifact: String,
    pub artifact_sha256: String,
    pub built_with: String,
    /// Recorded input key -> SHA-256 of what the key names.
    pub inputs: BTreeMap<String, String>,
}

impl BlobRecord {
    pub fn read(repo_root: &Path, builtin: &str) -> Result<BlobRecord, String> {
        let path = record_path(repo_root, builtin);
        let bytes = std::fs::read(&path)
            .map_err(|_| format!("{} is missing or unreadable", shown(builtin)))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("{} is malformed: {e}", shown(builtin)))
    }

    /// Pretty JSON and a final newline, as `build-builtins --install` wrote it at the base.
    pub fn write(&self, repo_root: &Path, builtin: &str) -> Result<(), String> {
        let mut text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        text.push('\n');
        let path = record_path(repo_root, builtin);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

fn record_path(repo_root: &Path, builtin: &str) -> PathBuf {
    repo_root.join(shown(builtin))
}

fn shown(builtin: &str) -> String {
    format!("extensions/{builtin}/wasm/{FILE}")
}

/// The repo path an input key names: `Cargo.toml` for [`WORKSPACE_INHERITED`], else the key.
pub fn path_of(key: &str) -> &str {
    if key == WORKSPACE_INHERITED {
        "Cargo.toml"
    } else {
        key
    }
}

/// The tables of the root manifest a staged crate inherits.
pub fn inherited_tables(manifest: &str) -> String {
    let mut out = String::new();
    let mut keep = false;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            keep = INHERITED_TABLES.contains(&trimmed);
        }
        if keep {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Every builtin with a record: the directories `extensions/*/wasm/inputs.json` exists in (sorted).
pub fn builtins(repo_root: &Path) -> Result<Vec<String>, String> {
    let dir = repo_root.join("extensions");
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut found: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().join("wasm").join(FILE).is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    Ok(found)
}

/// The builtins (directory names under `extensions/`, sorted) whose recorded inputs hold a changed path.
pub fn fed_by(repo_root: &Path, changes: &ChangeSet) -> Result<Vec<String>, String> {
    let mut fed = Vec::new();
    for builtin in builtins(repo_root)? {
        let record = BlobRecord::read(repo_root, &builtin)?;
        if changes
            .paths()
            .any(|path| record.inputs.keys().any(|key| path_of(key) == path))
        {
            fed.push(builtin);
        }
    }
    Ok(fed)
}

#[cfg(test)]
mod tests;
