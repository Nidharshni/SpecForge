//! `specforge.extensions`: the installed extensions (`specforge_ops::extension::list`).

use std::collections::BTreeSet;

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_ops::extension::ExtensionInfo;

use crate::args::NoArgs;
use crate::reply::Answered;
use crate::target::ProjectRef;

/// `specforge.extensions`'s reply (`McpExtensionsResult`).
#[derive(Debug, Serialize, Shape)]
pub struct Reply {
    extensions: Vec<ExtensionInfo>,
    lock_file_entries: Vec<LockEntry>,
    entity_kinds_in_graph: BTreeSet<String>,
}

/// One entry of the project's `specforge.lock`.
#[derive(Debug, Serialize, Shape)]
pub struct LockEntry {
    name: String,
    version: String,
}

pub(crate) fn call(project: &ProjectRef<'_>, _args: NoArgs) -> Answered<Reply> {
    // The shared listing, over the project view: what the project
    // compiled, its lock and the kinds its graph uses.
    let listing = specforge_ops::extension::list(&project.view());
    Ok(Reply {
        extensions: listing.extensions,
        lock_file_entries: listing
            .locked
            .iter()
            .map(|e| LockEntry {
                name: e.name.clone(),
                version: e.version.clone(),
            })
            .collect(),
        entity_kinds_in_graph: listing.kinds_in_graph,
    }
    .into())
}
