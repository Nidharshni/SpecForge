//! Consent to run a collector's command, keyed by project, extension and command.

use super::collector::Collector;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Overrides the user-level consent store (tests, custom homes).
const CONSENT_FILE_ENV: &str = "SPECFORGE_CONSENT_FILE";

// ── consent ─────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct ConsentStore {
    #[serde(default)]
    pub(super) approved: Vec<Approval>,
}

/// One approval: this project may run this extension's collector command.
/// A changed command is a different approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Approval {
    project: String,
    extension: String,
    collector: String,
    run: Vec<String>,
}

/// The user-level consent store. It lives outside the project so a cloned
/// repository can't pre-approve its own commands.
pub(super) fn consent_path() -> PathBuf {
    if let Some(path) = std::env::var_os(CONSENT_FILE_ENV) {
        return PathBuf::from(path);
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    home.join(".specforge").join("collector-consent.json")
}

fn consent_for(collector: &Collector, root: &Path) -> Approval {
    let project = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    Approval {
        project: project.display().to_string(),
        extension: collector.extension.clone(),
        collector: collector.name.clone(),
        run: collector.run.clone(),
    }
}

pub(super) fn load_consent(path: &Path) -> ConsentStore {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// Whether the user already approved this collector's command here.
pub(super) fn is_approved(store: &Path, collector: &Collector, root: &Path) -> bool {
    let wanted = consent_for(collector, root);
    load_consent(store).approved.contains(&wanted)
}

/// Record the user's approval of this collector's command here.
pub(super) fn approve(store: &Path, collector: &Collector, root: &Path) -> Result<(), String> {
    let mut consents = load_consent(store);
    let wanted = consent_for(collector, root);
    consents.approved.retain(|c| {
        !(c.project == wanted.project
            && c.extension == wanted.extension
            && c.collector == wanted.collector)
    });
    consents.approved.push(wanted);
    if let Some(dir) = store.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let json = serde_json::to_string_pretty(&consents).expect("consent serialization cannot fail");
    std::fs::write(store, json).map_err(|e| format!("{}: {e}", store.display()))
}
