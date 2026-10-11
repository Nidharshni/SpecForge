//! Extension management: the operations behind `specforge add`, `remove`,
//! `extensions` and their MCP tools.

mod add;
mod candidate;
#[cfg(test)]
mod fixtures;
mod list;
mod lock_plan;
mod remove;
mod resolve;
mod update;

pub use add::{AddOutcome, AddRequest, Added, Source, Trust, add, parse};
pub use candidate::{Candidate, Installable, LocalFile};
pub use list::{
    ExtensionEntry, ExtensionInfo, ExtensionListing, LockedExtension, ProviderEntry,
    ProviderListing, ProviderRow, ProviderStatus, ProvidersDocument, Status, list, providers,
};
pub use remove::{RemoveOutcome, RemoveRequest, StrandedEntity, remove};
pub use resolve::{resolve, resolve_requirement};
pub use update::{
    BatchUpdateCompleted, ExtensionUpdate, NO_LOCK, UpdateOutcome, UpdateRequest, UpdateStatus,
    update,
};

pub(crate) use add::install_local;
pub(crate) use candidate::required_builtins;

use specforge_installed::{LockFile, LockSource};
use specforge_project::EnabledExtension;

/// The code an operation reports for an extension the project doesn't
/// have.
pub const NOT_FOUND: &str = "extension_not_found";

/// Where an extension comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Embedded in the binary; enabling it is only a config entry.
    Builtin,
    /// Installed under `.specforge/extensions/`, pinned in `specforge.lock`
    /// with this source.
    Installed { source: LockSource },
    /// Loaded from the `.wasm` file a `specforge.json` entry names; `path`
    /// as the entry writes it.
    File { path: String },
    /// Named by an entry, neither a builtin nor locked: it does not load
    /// (E028).
    Unknown,
}

impl Origin {
    /// Where `name` comes from in a project: the `.wasm` file an
    /// `extensions` entry names (over a lock entry), the lock entry's
    /// source, a builtin, else `Unknown`. The one
    /// rule `list` and `doctor` name sources by.
    pub fn of(name: &str, enabled: &[EnabledExtension], lock: Option<&LockFile>) -> Origin {
        if let Some(path) = enabled
            .iter()
            .find(|e| e.name == name)
            .and_then(|e| e.file.clone())
        {
            return Origin::File { path };
        }
        if let Some(entry) =
            lock.and_then(|lock| lock.entries.iter().find(|e| e.name.as_str() == name))
        {
            return Origin::Installed {
                source: entry.source.clone(),
            };
        }
        match builtin_name(name) {
            Some(_) => Origin::Builtin,
            None => Origin::Unknown,
        }
    }

    /// What the listings call it: `builtin`, the lock entry's source, or
    /// `file:<path>`.
    pub fn source(&self) -> String {
        match self {
            Origin::Builtin => "builtin".to_string(),
            Origin::Installed { source } => source.to_string(),
            Origin::File { path } => format!("file:{path}"),
            Origin::Unknown => "unknown".to_string(),
        }
    }
}

/// The builtin `specifier` names (`@specforge/product`, optionally with an
/// `@version` suffix, which is ignored: builtins track the binary).
pub fn builtin_name(specifier: &str) -> Option<&'static str> {
    let name = crate::config::entry_name(specifier);
    specforge_project::builtins()
        .names()
        .find(|builtin| *builtin == name)
}

/// Builtins `config` enables, in declaration order.
pub fn enabled_builtins(config: &specforge_common::ProjectConfig) -> Vec<&'static str> {
    config
        .extensions
        .iter()
        .filter_map(|entry| builtin_name(entry))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_name_accepts_bare_and_versioned_names() {
        assert_eq!(
            builtin_name("@specforge/product"),
            Some("@specforge/product")
        );
        assert_eq!(
            builtin_name("@specforge/formal@latest"),
            Some("@specforge/formal")
        );
        assert_eq!(builtin_name("@acme/thing"), None);
        assert_eq!(builtin_name("./local.wasm"), None);
    }
}
