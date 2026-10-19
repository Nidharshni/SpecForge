//! `specforge remove` and `specforge.remove_extension`.

use super::lock_plan::{Ask, Disabled, LockPlan, Purpose};
use super::{NOT_FOUND, Origin, ProjectExtension, Selection, not_installed};
use crate::view::ProjectView;
use crate::{Caller, OpError, OpErrorKind, Refusal, Writes, Wrote};
use serde::Serialize;
use specforge_common::ExtensionEntry;
use specforge_common::shape::Shape;
use specforge_graph::Graph;
use specforge_installed::{Installed, LockFileEntry, LockState};
use specforge_package::PackageName;
use specforge_project::EnabledExtension;
use specforge_registry::KindRegistry;
use std::path::Path;

/// What to remove from the project the view was compiled from.
pub struct RemoveRequest<'a> {
    pub name: &'a str,
    /// Remove even when another extension requires it.
    pub force: bool,
    /// Report what would be removed; change nothing.
    pub dry_run: bool,
    /// What the removal may reach (`caller.containment()`, ADR 0104): a file it would write that
    /// lands outside its project, by a symlink in it, refuses it (ADR 0137).
    pub caller: Caller<'a>,
}

/// What a removal reads of its project: the view, its root, its kinds and
/// its graph.
struct Removing<'a> {
    view: &'a ProjectView<'a>,
    force: bool,
    dry_run: bool,
    caller: Caller<'a>,
    kinds: &'a KindRegistry,
    graph: &'a Graph,
    /// The project's installed extensions, as the compile read them.
    installed: &'a Installed,
}

/// A stranded entity (CONTEXT.md): one whose kind only the removed
/// extension declares; the next compile reports it E024.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
pub struct StrandedEntity {
    pub entity_id: String,
    pub kind: String,
}

impl StrandedEntity {
    /// `<kind> '<id>' uses a kind only <extension> defines`.
    pub fn warning(&self, extension: &str) -> String {
        format!(
            "{} '{}' uses a kind only {extension} defines",
            self.kind, self.entity_id
        )
    }
}

/// What a removal did (or, on a dry run, would do).
#[derive(Debug, Clone, PartialEq)]
pub struct RemoveOutcome {
    pub name: String,
    /// Its version as the inventory gives it ([`ProjectExtension::version`]).
    pub version: Option<String>,
    pub origin: Origin,
    /// The entities the removal strands, in id order: their kind only the
    /// removed extension defines, so they fail E024 on the next compile.
    pub stranded: Vec<StrandedEntity>,
    pub dry_run: bool,
    /// The files the removal changed: `specforge.json` when an entry was
    /// dropped, and for an uninstall `specforge.lock` and each file deleted
    /// under the extension's directory. Nothing for a dry run.
    pub writes: Writes,
}

impl RemoveOutcome {
    /// Whether the removal only previewed, or ran and wrote [`Self::writes`].
    pub fn wrote(&self) -> Wrote {
        Wrote::of(self.dry_run, self.writes.clone())
    }

    /// The document both surfaces print.
    pub fn document(&self) -> RemoveDocument {
        RemoveDocument {
            action: if self.dry_run {
                RemoveAction::Planned
            } else {
                RemoveAction::Removed
            },
            extension: self.name.clone(),
            source: self.origin.source(),
            version: self.version.clone(),
            stranded: self.stranded.clone(),
        }
    }
}

/// What `specforge remove --format json` and `specforge.remove_extension`
/// print (`McpRemoveExtensionResult`).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct RemoveDocument {
    pub action: RemoveAction,
    pub extension: String,
    /// `builtin`, the lock entry's source, `file:<path>` or `unknown`
    /// ([`Origin::source`]).
    pub source: String,
    /// The locked version; for a builtin, the loaded one. Absent when
    /// neither is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The entities the removal strands, in id order.
    pub stranded: Vec<StrandedEntity>,
}

/// Whether the removal was made or only previewed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "snake_case")]
pub enum RemoveAction {
    Removed,
    /// A dry run: nothing changed.
    Planned,
}

/// The field of a removal's request a refusal can be about (ADR 0072).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoveField {
    /// `RemoveRequest::name`: the extension to remove.
    Name,
}

/// Remove `name` from the project the view was compiled from: a locked
/// install is uninstalled (binary, lock entry and `specforge.json` entry);
/// a builtin is disabled (its `specforge.json` entry); a `.wasm` file
/// entry, named by the extension it loaded as, its entry or its path, is
/// dropped from `specforge.json` (the file and the lock are left alone); an
/// enabled entry that does not load (E028) is dropped by its name.
/// Refused with E027 while a member of the project's peer set (the
/// extensions its entries load) would be left without a non-optional peer
/// it requires, unless `force`; with `extension_conflict` when more than
/// one entry enables `name`.
///
/// Every refusal is decided from the view before anything is written: a
/// `specforge.json` the compile could not edit (a problem that
/// [`blocks_edits`](specforge_common::ConfigProblem::blocks_edits), which
/// the compile reported as E069) refuses every removal with
/// `config_invalid`, without reading the file again. A removal is all or
/// nothing: the binary, the lock entry and the `specforge.json` entry go as
/// one change, and a failure at any step puts every file back (the error's
/// [`OpError::writes`] names what the rollback could not, normally
/// nothing). Without a root: `no_project`. The removal is one lock-plan change
/// ([`LockPlan::removing`]): a dry run is the same plan checked and not
/// committed.
pub fn remove(
    view: &ProjectView,
    req: &RemoveRequest,
) -> Result<RemoveOutcome, Refusal<RemoveField>> {
    view.project_root()?;
    if let Some(problem) = view
        .env()
        .config_problems()
        .iter()
        .find(|problem| problem.blocks_edits())
    {
        return Err(crate::config::refusal(problem).into());
    }
    let inventory = super::inventory(view);
    let removing = &Removing {
        view,
        force: req.force,
        dry_run: req.dry_run,
        caller: req.caller,
        kinds: view.registries().kinds(),
        graph: view.graph(),
        installed: view.installed(),
    };
    match inventory.select(req.name) {
        Selection::File { entry, extension } => remove_file(removing, entry, extension),
        Selection::Named(extension) => remove_named(removing, extension),
        Selection::Ambiguous(entries) => {
            Err(ambiguous(req.name, &entries).about(RemoveField::Name))
        }
        Selection::Nothing => {
            Err(nothing_named(req.name, view.installed().lock()).about(RemoveField::Name))
        }
    }
}

/// The refusal for an argument that more than one entry enables, naming
/// the entries in the order specforge.json lists them.
fn ambiguous(name: &str, entries: &[&EnabledExtension]) -> OpError {
    let entries: Vec<&str> = entries.iter().map(|e| e.entry.as_str()).collect();
    let quoted: Vec<String> = entries.iter().map(|e| format!("'{e}'")).collect();
    let error = OpError::new(
        OpErrorKind::Conflict,
        "extension_conflict",
        format!(
            "'{name}' is enabled by {} specforge.json entries: {}",
            quoted.len(),
            quoted.join(", ")
        ),
    )
    .with_suggestion(
        "remove one entry by its text as specforge.json writes it, or edit specforge.json",
    );
    error.with_data(serde_json::json!({"extension": name, "entries": entries}))
}

/// The refusal for an argument the project has nothing under: a file no
/// entry names; E072 for a name that could never be installed; else
/// `extension_not_found`, saying why nothing is locked when there is no
/// readable lock.
fn nothing_named(name: &str, lock: &LockState) -> OpError {
    if ExtensionEntry::parse(name).file(Path::new("")).is_some() {
        return OpError::new(
            OpErrorKind::ExtensionNotFound,
            NOT_FOUND,
            format!("extension '{name}' is not enabled: no specforge.json entry names that file"),
        )
        .with_suggestion("`specforge extensions` lists what the project enables")
        .with_data(serde_json::json!({ "extension": name }));
    }
    if let Err(why) = PackageName::parse(name) {
        return OpError::of(
            OpErrorKind::InvalidInput,
            specforge_common::package::invalid(&why),
        );
    }
    let why = match lock {
        LockState::Absent => Some("no lock file found"),
        LockState::Unreadable(problem) => Some(problem.message.as_str()),
        LockState::Read(_) => None,
    };
    not_installed(name, why)
}

/// Remove the extension `extension` names: its lock entry, binary and
/// `specforge.json` entries as one change when it is locked; else its
/// `specforge.json` entries (a builtin, or an entry that does not load).
fn remove_named(
    req: &Removing,
    extension: &ProjectExtension,
) -> Result<RemoveOutcome, Refusal<RemoveField>> {
    // Neither builtin nor locked while the lock can't be read: whether it
    // is installed is not known, so nothing is removed.
    if extension.origin == Origin::Unknown
        && let Some(problem) = req.installed.lock().problem()
    {
        return Err(
            not_installed(extension.name, Some(problem.message.as_str())).about(RemoveField::Name),
        );
    }
    // Loaded, but neither enabled nor locked (a view assembled in a test).
    if !extension.enabled() && extension.lock.is_none() {
        return Err(not_installed(extension.name, None).about(RemoveField::Name));
    }
    let writes = change(
        req,
        extension.lock,
        Disabled::Named(extension.name.to_string()),
    )?;
    Ok(RemoveOutcome {
        name: extension.name.to_string(),
        version: extension.version().map(str::to_string),
        origin: extension.origin.clone(),
        stranded: stranded(req.graph, req.kinds, extension.name),
        dry_run: req.dry_run,
        writes,
    })
}

/// The removal as one lock-plan change: `locked`'s package is uninstalled (its binary and lock
/// entry) and the entries `disabled` selects are dropped from `specforge.json`, judged by the
/// peer rule over the project's peer set as the removal leaves it, then written all or nothing
/// (nothing for a dry run). The files it wrote.
fn change(
    req: &Removing,
    locked: Option<&LockFileEntry>,
    disabled: Disabled,
) -> Result<Writes, Refusal<RemoveField>> {
    let purpose = if req.dry_run {
        Purpose::Preview
    } else {
        Purpose::Commit
    };
    let mut plan = LockPlan::removing(req.view, req.caller, purpose, req.force, locked.is_some())?;
    // Its directory is named by the lock entry's package name, which the lock read validated
    // (ADR 0036): nothing outside `.specforge/extensions` can be deleted.
    if let Some(locked) = locked {
        plan.uninstall(Ask(0), &locked.name);
    }
    plan.disable(Ask(0), disabled);
    let done = plan
        .check()
        .map_err(|refused| refused.decisive().1.about(RemoveField::Name))?
        .finish()?;
    Ok(done.writes)
}

/// Remove the `.wasm` file entry `file`: only its `specforge.json` entry
/// goes. The extension it loaded as (if it loaded) is what dependents and
/// stranded entities are checked against.
fn remove_file(
    req: &Removing,
    file: &EnabledExtension,
    extension: &ProjectExtension,
) -> Result<RemoveOutcome, Refusal<RemoveField>> {
    let writes = change(req, None, Disabled::Entry(file.entry.clone()))?;
    Ok(RemoveOutcome {
        name: file.name.clone(),
        version: extension.version().map(str::to_string),
        origin: Origin::File {
            path: file.file.clone().unwrap_or_default(),
        },
        stranded: stranded(req.graph, req.kinds, &file.name),
        dry_run: req.dry_run,
        writes,
    })
}

/// The entities whose kind only `extension` defines, in id order.
fn stranded(graph: &Graph, kinds: &KindRegistry, extension: &str) -> Vec<StrandedEntity> {
    // `Graph::nodes` is in id order.
    graph
        .nodes()
        .into_iter()
        .filter(|node| {
            kinds
                .get(node.kind.raw.as_str())
                .is_some_and(|kind| kind.source_extension == extension)
        })
        .map(|node| StrandedEntity {
            entity_id: node.id.raw.to_string(),
            kind: node.kind.raw.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests;
