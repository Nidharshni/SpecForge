//! The add document both surfaces print (`McpAddExtensionResult`).

use crate::registry::{Acceptance, Publisher};
use serde::Serialize;
use specforge_common::shape::Shape;

use super::{AddOutcome, Added, AddedExtension};

impl AddedExtension {
    /// Its entry in the add document.
    pub fn document(&self) -> AddResult {
        let specifier = self.specifier.clone();
        match &self.outcome {
            AddOutcome::Builtin {
                name,
                changed,
                peers_enabled,
            } => AddResult::Enabled(EnabledBuiltin {
                specifier,
                extension: (*name).to_string(),
                source: BuiltinSource::Builtin,
                changed: *changed,
                peers_enabled: peers_enabled
                    .iter()
                    .map(|peer| (*peer).to_string())
                    .collect(),
            }),
            AddOutcome::Installed {
                name,
                version,
                sha256,
                publisher,
                origin,
                peers_enabled,
            } => AddResult::Installed(InstalledPackage {
                specifier,
                extension: name.clone(),
                version: version.clone(),
                sha256: sha256.clone(),
                source: origin.source(),
                key_id: publisher
                    .as_ref()
                    .and_then(Publisher::key_id)
                    .map(str::to_string),
                publisher: publisher.as_ref().map(Publisher::acceptance),
                peers_enabled: peers_enabled
                    .iter()
                    .map(|peer| (*peer).to_string())
                    .collect(),
            }),
            AddOutcome::AlreadyPresent { name, version } => {
                AddResult::AlreadyPresent(PresentExtension {
                    specifier,
                    extension: name.clone(),
                    version: version.clone(),
                })
            }
            AddOutcome::Planned {
                name,
                version,
                origin,
                peers_enabled,
            } => AddResult::Planned(PlannedAdd {
                specifier,
                extension: name.clone(),
                version: version.clone(),
                source: origin.source(),
                peers_enabled: peers_enabled
                    .iter()
                    .map(|peer| (*peer).to_string())
                    .collect(),
            }),
            AddOutcome::Duplicate { name, first } => AddResult::Duplicate(DuplicateSpecifier {
                specifier,
                extension: name.clone(),
                duplicate_of: first.clone(),
            }),
        }
    }
}

/// What `specforge add --format json` and `specforge.add_extension` print
/// (`McpAddExtensionResult`): what the add did for each specifier.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct AddDocument {
    /// One entry per specifier, in the order given.
    pub added: Vec<AddResult>,
}

/// What the add did for one specifier, by `action` (`McpAddedExtension`).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AddResult {
    /// A builtin enabled in `specforge.json` (`changed: false` when it
    /// already was), after the required builtin peers it lists.
    Enabled(EnabledBuiltin),
    /// A package installed under `.specforge/extensions/` and locked.
    Installed(InstalledPackage),
    /// Already installed at this version and enabled: nothing changed.
    AlreadyPresent(PresentExtension),
    /// A dry run: what would be installed or enabled.
    Planned(PlannedAdd),
    /// An extension an earlier specifier of the same add names: added once.
    Duplicate(DuplicateSpecifier),
}

/// The one spelling of a builtin's `source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
pub enum BuiltinSource {
    #[serde(rename = "builtin")]
    Builtin,
}

/// A builtin enabled.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct EnabledBuiltin {
    pub specifier: String,
    pub extension: String,
    pub source: BuiltinSource,
    pub changed: bool,
    /// The required builtin peers enabled first.
    pub peers_enabled: Vec<String>,
}

/// A package installed and locked.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct InstalledPackage {
    pub specifier: String,
    pub extension: String,
    pub version: String,
    pub sha256: String,
    /// `registry`, or `local:<path>` for a local file ([`Origin::source`]).
    pub source: String,
    /// The publisher key id; absent for an unsigned package or a local file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
    /// How the publisher was accepted; absent for a local file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<Acceptance>,
    /// The required builtins enabled with it, dependencies first.
    pub peers_enabled: Vec<String>,
}

/// An extension already installed and enabled.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct PresentExtension {
    pub specifier: String,
    pub extension: String,
    pub version: String,
}

/// What a dry run would install or enable.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct PlannedAdd {
    pub specifier: String,
    pub extension: String,
    /// Absent for a builtin, which tracks the binary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// `builtin`, `registry` or `local:<path>`.
    pub source: String,
    /// The required builtins the add would enable with it, dependencies first.
    pub peers_enabled: Vec<String>,
}

/// A specifier naming an extension an earlier one of the same add names.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct DuplicateSpecifier {
    pub specifier: String,
    pub extension: String,
    /// The earlier specifier that adds it.
    pub duplicate_of: String,
}

impl Added {
    /// The add document: [`AddedExtension::document`] for each specifier.
    pub fn document(&self) -> AddDocument {
        AddDocument {
            added: self.added.iter().map(AddedExtension::document).collect(),
        }
    }
}
