//! Extensions as a project has them: what it installs and enables (`specforge add`, `update`,
//! `remove`, one lock plan, ADR 0065), what it lists (`specforge extensions`, `providers`), what an
//! author publishes (ADR 0045), and the commands extensions declare ([`command`], ADR 0011). The
//! group uses only the base.

mod add;
mod candidate;
mod claims;
pub mod command;
#[cfg(test)]
mod fixtures;
mod inventory;
mod list;
mod lock_plan;
mod names;
mod peer_set;
mod publish;
mod remove;
mod resolve;
mod update;

pub use add::{
    AddDocument, AddField, AddOutcome, AddRequest, AddResult, Added, AddedExtension, BuiltinSource,
    DuplicateSpecifier, EnabledBuiltin, InstalledPackage, PlannedAdd, PresentExtension, add,
};
pub use candidate::{Candidate, Installable, LocalFile, binary_at};
pub use inventory::{Inventory, ProjectExtension, Selection, Status, inventory};
pub use list::{
    ExtensionInfo, ExtensionsDocument, LockedExtension, ProviderRow, ProviderStatus,
    ProvidersDocument, list, providers,
};
pub use publish::{
    PublishAction, PublishDocument, PublishOutcome, PublishReport, ValidateDocument, Validated,
    declare, publish, validate,
};
pub use remove::{
    RemoveDocument, RemoveField, RemoveOutcome, RemoveRequest, StrandedEntity, remove,
};
pub use resolve::{resolve, upgrade};
pub use update::{
    BatchUpdateCompleted, BatchUpdateEvent, ExtensionUpdate, NO_LOCK, UpdateDocument,
    UpdateOutcome, UpdateRequest, UpdateRow, UpdateStatus, UpgradeCompleted, update,
};

pub(crate) use claims::{Claimed, Claims};
pub(crate) use lock_plan::{Also, Install, LockPlan};
pub(crate) use names::{Answer, Names, Place};

use specforge_installed::LockSource;

/// The code an operation reports for an extension the project doesn't
/// have.
pub const NOT_FOUND: &str = "extension_not_found";

/// The refusal for an extension the project has nothing under: `extension_not_found` "extension
/// '<name>' is not installed", saying `why` when it is known.
pub(crate) fn not_installed(name: &str, why: Option<&str>) -> crate::OpError {
    let mut message = format!("extension '{name}' is not installed");
    if let Some(why) = why {
        message.push_str(&format!(" ({why})"));
    }
    crate::OpError::new(crate::OpErrorKind::ExtensionNotFound, NOT_FOUND, message)
        .with_data(serde_json::json!({ "extension": name }))
}

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
    /// Where the entry at `at` loads its extension from, by the load's one rule
    /// (`Installed::resolve`): a builtin's name is `Builtin`; a locked name `Installed { source }`;
    /// a `.wasm` file entry `File { path }` (as written); a name nothing loads `Unknown`; an entry
    /// naming what an earlier one enables, that entry's origin.
    pub fn of(resolution: &specforge_installed::Resolution<'_>, at: usize) -> Origin {
        use specforge_installed::Resolved;
        match &resolution.entries()[at] {
            Resolved::Builtin { .. } => Origin::Builtin,
            Resolved::Locked { entry, .. } => Origin::Installed {
                source: entry.source.clone(),
            },
            Resolved::File(file) => Origin::File {
                path: file.path.to_string(),
            },
            Resolved::Unloadable { .. } => Origin::Unknown,
            Resolved::Again { first } => Origin::of(resolution, *first),
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

/// The builtin a named `specifier` names (`@specforge/product`, optionally with an
/// `@version` suffix, which is ignored: builtins track the binary); a `.wasm` file entry names none.
pub fn builtin_name(specifier: &str) -> Option<&'static str> {
    let specforge_common::ExtensionEntry::Named(name) =
        specforge_common::ExtensionEntry::parse(specifier)
    else {
        return None;
    };
    specforge_project::builtins()
        .names()
        .find(|builtin| *builtin == name)
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
        assert_eq!(builtin_name("@specforge/product=./x.wasm"), None);
    }
}
