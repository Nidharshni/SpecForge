//! `specforge add` and `specforge.add_extension`.

use super::lock_plan::{Ask, Asked, Fetching, Install, LockPlan, Opened, Purpose};
use super::names::no_specifier;
use super::{Answer, Names, Place};
use crate::registry::{Publisher, Registry};
use crate::{Caller, Refusal, Writes, Wrote};
use specforge_installed::Installed;
use specforge_installed::Origin;
use specforge_wasm::WasmRuntime;

/// The field of an add's request a refusal can be about (ADR 0072).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddField {
    /// `AddRequest::extensions`: the specifiers to add, read by `extension::names`.
    Extensions,
}

/// What to add, and where.
#[derive(Debug, Clone)]
pub struct AddRequest<'a> {
    pub root: &'a specforge_common::ProjectRoot,
    /// The extensions to add, one specifier per string, in the order given: one change
    /// (`extension::names`, ADR 0105).
    pub extensions: Vec<String>,
    /// What a relative local path is read from: the project's root when `None`. The CLI passes
    /// its working directory.
    pub base: Option<&'a std::path::Path>,
    /// Accept a registry package with no publisher signature.
    pub allow_unsigned: bool,
    /// The add's lock plan judged and not committed: report what the add would do, or the refusal it
    /// would give; write, pin and download nothing. A registry package is judged by the
    /// declaration it was published with (its manifest). An extension already present answers as
    /// the real add does (`AlreadyPresent`, or `Builtin { changed: false }`): its lock plan answers
    /// it unchanged in both.
    pub dry_run: bool,
    /// What the add may reach (a local extension is read, every file written, through
    /// `caller.containment()`, ADR 0104) and who can be asked about a publisher key change
    /// (`caller.asking()`, ADR 0110).
    pub caller: Caller<'a>,
}

/// What an add did (or, on a dry run, would do) for one specifier.
#[derive(Debug, Clone, PartialEq)]
pub enum AddOutcome {
    /// A builtin enabled; `changed` is false when it already was.
    /// `peers_enabled` are the required builtin peers enabled first.
    Builtin {
        name: &'static str,
        changed: bool,
        peers_enabled: Vec<&'static str>,
    },
    /// Installed under `.specforge/extensions/` and locked. `peers_enabled` are the required
    /// builtins enabled with it, dependencies first.
    Installed {
        name: String,
        version: String,
        sha256: String,
        /// How the registry's publisher was accepted; `None` for a local file.
        publisher: Option<Publisher>,
        origin: Origin,
        peers_enabled: Vec<&'static str>,
    },
    /// Already installed at this version (or, from a local file, with
    /// these exact bytes) and enabled: nothing changed.
    AlreadyPresent { name: String, version: String },
    /// A dry run: what would be installed or enabled, and the required builtins that would be
    /// enabled with it, dependencies first.
    Planned {
        name: String,
        version: Option<String>,
        origin: Origin,
        peers_enabled: Vec<&'static str>,
    },
    /// An extension an earlier specifier of the same add names, as that one adds it: added
    /// once, by `first` (that specifier).
    Duplicate { name: String, first: String },
}

impl AddOutcome {
    /// The add changed nothing for it: a builtin already enabled, a package
    /// already present, or a specifier repeating an earlier one (`extension_added`'s
    /// `wasDuplicate`, ADR 0022 D5).
    pub fn is_duplicate(&self) -> bool {
        match self {
            AddOutcome::Builtin { changed, .. } => !changed,
            AddOutcome::AlreadyPresent { .. } | AddOutcome::Duplicate { .. } => true,
            AddOutcome::Installed { .. } | AddOutcome::Planned { .. } => false,
        }
    }
}

/// What an add did for one specifier.
#[derive(Debug, Clone, PartialEq)]
pub struct AddedExtension {
    /// The specifier as the caller gave it.
    pub specifier: String,
    pub outcome: AddOutcome,
}

/// What an add did, and what it changed on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct Added {
    /// What it did for each specifier, in the order given.
    pub added: Vec<AddedExtension>,
    /// The files the add changed: `specforge.json` when an entry was added,
    /// and for the installs each module and `specforge.lock` when their bytes
    /// changed. Nothing for a dry run or extensions already present. A
    /// publisher key pinned for the user is not one: it is pinned with the
    /// change, last.
    pub writes: Writes,
    /// How many extensions `specforge.json` enables after the add (`0`
    /// without one): `extension_added`'s `totalExtensions`.
    pub extensions_enabled: usize,
    /// The request asked for a preview (`AddRequest::dry_run`).
    pub dry_run: bool,
}

impl Added {
    /// Whether the add only previewed, or ran and wrote [`Self::writes`].
    pub fn wrote(&self) -> Wrote {
        Wrote::of(self.dry_run, self.writes.clone())
    }
}

/// Add the extensions `req` names to the project at `req.root.dir()`, as one change.
///
/// - A builtin is enabled in `specforge.json`, after the builtins it
///   requires as non-optional peers.
/// - A local `.wasm` is validated by its handshake, copied under
///   `.specforge/extensions/`, locked with the version it declares and
///   `source: "local:<path>"`, and enabled.
/// - A registry package is resolved, downloaded, integrity- and
///   signature-checked, installed, locked and enabled.
///
/// Every specifier is asked of one lock plan through `extension::names` (ADR 0134), judged once by the peer rule over the whole set
/// (ADR 0041: the order of the specifiers decides nothing), and each fetched binary is loaded
/// only once the plan has passed. An installed extension is enabled by its bare name, which the
/// runtime loads from the lock (ADR 0004 D3-b).
///
/// The add is all or nothing: the modules, the lock (written once) and the `specforge.json`
/// entries are one change, and a refusal of any specifier writes nothing; with several
/// specifiers, its message names the one refused. A failure while writing puts every file back
/// (the error's [`OpError::writes`] names what the rollback could not, normally nothing).
///
/// A specifier naming an extension an earlier one names, as that one adds it, is a
/// [`AddOutcome::Duplicate`]; two that add one extension differently (another version, other
/// bytes, another source) are refused with `extension_conflict`. An extension a `.wasm` file
/// entry already loads is refused the same way (ADR 0160).
pub fn add(
    req: &AddRequest,
    registry: &dyn Registry,
    runtime: &dyn WasmRuntime,
) -> Result<Added, Refusal<AddField>> {
    // The project must exist, with a config the writer can edit, before
    // anything is installed into it: the refusal `update` and `remove`
    // give an unusable specforge.json too (`config_invalid`).
    let read = crate::config::required(req.root.dir())?;
    let (config, found) = (read.config, read.found);
    if req.extensions.is_empty() {
        return Err(no_specifier().about(AddField::Extensions));
    }
    let place = Place {
        root: req.root.dir(),
        base: req.base.unwrap_or(req.root.dir()),
        containment: req.caller.containment(),
    };
    let names = Names::read(&req.extensions, place).map_err(|e| e.about(AddField::Extensions))?;
    let installed = Installed::at(req.root.dir());
    // `names` refuses a specforge.lock that can't be read before it reads a file or asks a registry.
    let plan = LockPlan::adding(
        &installed,
        Opened::read(&config, found),
        runtime,
        Fetching {
            registry,
            allow_unsigned: req.allow_unsigned,
        },
        req.caller,
        if req.dry_run {
            Purpose::Preview
        } else {
            Purpose::Commit
        },
    );
    let (checked, answers) = names
        .ask(
            plan,
            runtime,
            &mut |_, package| super::resolve(registry, package),
            &mut Package::of,
        )
        .map_err(|e| e.about(AddField::Extensions))?;
    let done = checked.finish()?;
    let added = answers
        .into_iter()
        .enumerate()
        .map(|(at, answer)| AddedExtension {
            specifier: names.specifier(at).to_string(),
            outcome: outcome(answer, done.peers_enabled(Ask(at)), req.dry_run),
        })
        .collect();
    Ok(Added {
        added,
        extensions_enabled: done.enabled,
        writes: done.writes,
        dry_run: req.dry_run,
    })
}

/// What the add records of a package install, to report it once the plan has answered its ask.
struct Package {
    name: String,
    version: String,
    sha256: String,
    publisher: Option<Publisher>,
    origin: Origin,
}

impl Package {
    fn of(install: &Install<'_>) -> Package {
        Package {
            name: install.name().to_string(),
            version: install.version().to_string(),
            sha256: install.sha256().to_string(),
            publisher: install.publisher().cloned(),
            origin: Origin::Installed {
                source: install.source().clone(),
            },
        }
    }
}

/// What the add did (or, for a dry run, would do) for one specifier: names' answer for it and the
/// builtins the finished plan enabled for its ask.
fn outcome(answer: Answer<Package>, peers_enabled: Vec<&'static str>, dry_run: bool) -> AddOutcome {
    match answer {
        Answer::Duplicate { name, first } => AddOutcome::Duplicate { name, first },
        Answer::Builtin {
            name,
            asked: Asked::Unchanged,
        } => AddOutcome::Builtin {
            name,
            changed: false,
            peers_enabled: Vec::new(),
        },
        Answer::Builtin { name, .. } if dry_run => AddOutcome::Planned {
            name: name.to_string(),
            version: None,
            origin: Origin::Builtin,
            peers_enabled,
        },
        Answer::Builtin { name, .. } => AddOutcome::Builtin {
            name,
            changed: true,
            peers_enabled,
        },
        Answer::Installed {
            recorded: package,
            asked: Asked::Unchanged,
            ..
        } => AddOutcome::AlreadyPresent {
            name: package.name,
            version: package.version,
        },
        Answer::Installed {
            recorded: package, ..
        } if dry_run => AddOutcome::Planned {
            name: package.name,
            version: Some(package.version),
            origin: package.origin,
            peers_enabled,
        },
        Answer::Installed {
            recorded: package, ..
        } => AddOutcome::Installed {
            name: package.name,
            version: package.version,
            sha256: package.sha256,
            publisher: package.publisher,
            origin: package.origin,
            peers_enabled,
        },
    }
}

mod document;
#[cfg(test)]
mod tests;

pub use document::{
    AddDocument, AddResult, BuiltinSource, DuplicateSpecifier, EnabledBuiltin, InstalledPackage,
    PlannedAdd, PresentExtension,
};
