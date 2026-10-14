//! `<crate_dir>/wasm/inputs.json`: what one committed blob (a builtin under `extensions/` or a fixture under
//! `fixtures/`) was built from. `build-blobs` writes and checks it through this module; a quick run asks it which
//! blobs a change set feeds. A blob is named by its crate directory from the repository root. The JSON is unchanged
//! byte for byte.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::change_set::ChangeSet;

pub const FILE: &str = "inputs.json";
/// The key for the root manifest's tables a staged crate inherits: `[workspace.package]` (edition, version,
/// license), `[workspace.dependencies]` (dependency specs) and the two `[workspace.lints]` tables (the lint levels
/// its `[lints] workspace = true` reads). Only these can change how a staged crate builds.
pub const WORKSPACE_INHERITED: &str = "Cargo.toml#workspace-inherited";
pub const INHERITED_TABLES: &[&str] = &[
    "[workspace.package]",
    "[workspace.dependencies]",
    "[workspace.lints.rust]",
    "[workspace.lints.clippy]",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobRecord {
    pub artifact: String,
    pub artifact_sha256: String,
    pub built_with: String,
    /// Recorded input key -> SHA-256 of what the key names.
    pub inputs: BTreeMap<String, String>,
}

impl BlobRecord {
    pub fn read(repo_root: &Path, crate_dir: &str) -> Result<BlobRecord, String> {
        let path = record_path(repo_root, crate_dir);
        let bytes = std::fs::read(&path)
            .map_err(|_| format!("{} is missing or unreadable", shown(crate_dir)))?;
        serde_json::from_slice(&bytes)
            .map_err(|e| format!("{} is malformed: {e}", shown(crate_dir)))
    }

    /// Pretty JSON and a final newline, as `build-blobs --install` wrote it at the base.
    pub fn write(&self, repo_root: &Path, crate_dir: &str) -> Result<(), String> {
        let mut text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        text.push('\n');
        let path = record_path(repo_root, crate_dir);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

fn record_path(repo_root: &Path, crate_dir: &str) -> PathBuf {
    repo_root.join(shown(crate_dir))
}

fn shown(crate_dir: &str) -> String {
    format!("{crate_dir}/wasm/{FILE}")
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

/// The parents of the crates that commit a blob: the builtins and the fixtures.
pub const BLOB_PARENTS: &[&str] = &["extensions", "fixtures"];

/// Every committed blob with a record: the crate directories (`extensions/<dir>`, `fixtures/<dir>`, sorted) whose
/// `wasm/inputs.json` exists.
pub fn blobs(repo_root: &Path) -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    for parent in BLOB_PARENTS {
        let dir = repo_root.join(parent);
        let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        found.extend(
            entries
                .flatten()
                .filter(|entry| entry.path().join("wasm").join(FILE).is_file())
                .map(|entry| format!("{parent}/{}", entry.file_name().to_string_lossy())),
        );
    }
    found.sort();
    Ok(found)
}

/// The blobs (crate directories, sorted) whose recorded inputs hold a changed path.
pub fn fed_by(repo_root: &Path, changes: &ChangeSet) -> Result<Vec<String>, String> {
    let mut fed = Vec::new();
    for crate_dir in blobs(repo_root)? {
        let record = BlobRecord::read(repo_root, &crate_dir)?;
        if changes
            .paths()
            .any(|path| record.inputs.keys().any(|key| path_of(key) == path))
        {
            fed.push(crate_dir);
        }
    }
    Ok(fed)
}

#[cfg(test)]
mod tests;
