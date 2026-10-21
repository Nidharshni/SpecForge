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
pub use candidate::{
    Installable, LocalComponent, LocalFile, WASM_TARGET, builtin_declaration, not_a_builtins_name,
    target_dir,
};
pub use inventory::{Inventory, ProjectExtension, Selection, Status, inventory};
pub use list::{
    ExtensionInfo, ExtensionsDocument, LockedExtension, ProviderRow, ProviderStatus,
    ProvidersDocument, list, providers,
};
pub use publish::{
    PublishAction, PublishDocument, PublishReport, ValidateDocument, Validated, declare, publish,
    validate,
};
pub use remove::{
    RemoveDocument, RemoveField, RemoveOutcome, RemoveRequest, StrandedEntity, remove,
};
pub use resolve::{resolve, upgrade};
pub use update::{
    BatchUpdateCompleted, ExtensionUpdate, NO_LOCK, UpdateDocument, UpdateOutcome, UpdateRequest,
    UpdateRow, UpdateStatus, UpgradeCompleted, update,
};

pub(crate) use claims::{Claimed, Claims};
pub(crate) use lock_plan::{Also, Install, LockPlan};
pub(crate) use names::{Answer, Names, Place};

use specforge_graph::{Graph, Node};
use specforge_registry::KindRegistry;

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

/// The builtin a named `specifier` names (`@specforge/product`, optionally with an
/// `@version` suffix, which this reads as the entry rule does: the name; an add specifier's version is read by `names` and
/// held to the embedded builtin); a `.wasm` file entry names none.
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

/// The graph's entities whose kind `extension` registered (the kind
/// registry's `source_extension`: the first extension to declare a kind,
/// E026), in id order. `specforge extensions` counts them as the
/// extension's `entity_count`; removing the extension strands them (E024 on
/// the next compile).
pub(crate) fn entities_of<'a>(
    graph: &'a Graph,
    kinds: &'a KindRegistry,
    extension: &'a str,
) -> impl Iterator<Item = &'a Node> + 'a {
    graph.nodes().into_iter().filter(move |node| {
        kinds
            .get(node.kind.raw.as_str())
            .is_some_and(|kind| kind.source_extension == extension)
    })
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
