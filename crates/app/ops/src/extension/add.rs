//! `specforge add` and `specforge.add_extension`.

use super::Origin;
use super::lock_plan::{Ask, Asked, Fetching, Install, LockPlan, Purpose};
use super::names::no_specifier;
use super::{Named, Names, Place};
use crate::registry::{Publisher, Registry};
use crate::{Caller, Refusal, Writes};
use specforge_installed::Installed;
use specforge_package::PackageName;
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
    /// the real add would (`AlreadyPresent`, or `Builtin { changed: false }`).
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
/// Every install is staged in one lock plan, judged once by the peer rule over the whole set
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
/// bytes, another source) are refused with `extension_conflict`.
pub fn add(
    req: &AddRequest,
    registry: &dyn Registry,
    runtime: &dyn WasmRuntime,
) -> Result<Added, Refusal<AddField>> {
    // The project must exist, with a config the writer can edit, before
    // anything is installed into it: the refusal `update` and `remove`
    // give an unusable specforge.json too (`config_invalid`).
    let config = crate::config::required(req.root.dir())?.config;
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
    // A specforge.lock that can't be read refuses before a file is read or a registry asked.
    let plan = LockPlan::adding(
        &installed,
        &config.extensions,
        runtime,
        Fetching {
            registry,
            allow_unsigned: req.allow_unsigned,
            asking: req.caller.asking(),
        },
        if req.dry_run {
            Purpose::Preview
        } else {
            Purpose::Commit
        },
        names.installs(),
    )?;
    let named = names
        .name_each(runtime, &mut |_, package| super::resolve(registry, package))
        .map_err(|e| e.about(AddField::Extensions))?;
    let steps = named
        .into_iter()
        .map(|named| step(&config, &installed, named))
        .collect();
    apply(req, &names, &config, plan, steps)
}

/// What the add does for one specifier, once it is resolved.
enum Step<'a> {
    /// Nothing to write: already enabled or present, a dry run's plan, or a duplicate.
    Unchanged(AddOutcome),
    /// A builtin to enable (the lock plan enables the builtins it requires first).
    Enable { name: &'static str },
    /// A local file to install.
    Local(Install<'a>),
    /// A registry package to fetch and install.
    Fetch(PackageName, specforge_package::Version),
}

/// What the add does for what one specifier names: nothing when it is a duplicate, already enabled
/// or already present (an already-enabled extension is left exactly as it is, a dry run included).
fn step<'a>(
    config: &specforge_common::ProjectConfig,
    installed: &Installed,
    named: Named<'a>,
) -> Step<'a> {
    match named {
        Named::Duplicate { name, first } => Step::Unchanged(AddOutcome::Duplicate { name, first }),
        Named::Builtin(name) => {
            if super::enabled_builtins(config).contains(&name) {
                Step::Unchanged(AddOutcome::Builtin {
                    name,
                    changed: false,
                    peers_enabled: Vec::new(),
                })
            } else {
                Step::Enable { name }
            }
        }
        Named::Local { install, .. } => {
            let name = install.name().clone();
            match already_present(installed, config, &name, |e| {
                e.wasm_hash == install.sha256() && e.source.local_path().is_some()
            }) {
                Some(present) => Step::Unchanged(present),
                None => Step::Local(*install),
            }
        }
        Named::Package { name, version } => {
            match already_present(installed, config, &name, |e| {
                e.version == version.to_string() && e.source.is_registry()
            }) {
                Some(present) => Step::Unchanged(present),
                None => Step::Fetch(name, version),
            }
        }
    }
}

/// `AlreadyPresent` when the lock holds `name` as `same` accepts, its
/// binary is in place and is the one the lock pins, and `specforge.json`
/// enables it. A binary that changed after install is not present: adding
/// it again reinstalls it.
fn already_present(
    installed: &Installed,
    config: &specforge_common::ProjectConfig,
    name: &PackageName,
    same: impl Fn(&specforge_installed::LockFileEntry) -> bool,
) -> Option<AddOutcome> {
    let entry = installed.verified(name.as_str()).filter(|e| same(e))?;
    let enabled = config
        .extensions
        .iter()
        .any(|e| specforge_common::extension_entry_name(e) == name.as_str());
    enabled.then(|| AddOutcome::AlreadyPresent {
        name: name.to_string(),
        version: entry.version.clone(),
    })
}

/// What a specifier asks the plan for, to turn the plan's answer for its ask into an outcome.
enum Wanted {
    Builtin(&'static str),
    Package {
        name: String,
        version: String,
        sha256: String,
        publisher: Option<Publisher>,
        origin: Origin,
    },
}

impl Wanted {
    fn of(install: &Install<'_>) -> Wanted {
        Wanted::Package {
            name: install.name().to_string(),
            version: install.version().to_string(),
            sha256: install.sha256().to_string(),
            publisher: install.publisher().cloned(),
            origin: Origin::Installed {
                source: install.source().clone(),
            },
        }
    }

    /// What the add did (or, for a dry run, would do) for it, given what the plan did for its ask.
    fn outcome(self, asked: Asked, dry_run: bool) -> AddOutcome {
        match self {
            Wanted::Builtin(name) if dry_run => AddOutcome::Planned {
                name: name.to_string(),
                version: None,
                origin: Origin::Builtin,
                peers_enabled: asked.peers_enabled,
            },
            Wanted::Package {
                name,
                version,
                origin,
                ..
            } if dry_run => AddOutcome::Planned {
                name,
                version: Some(version),
                origin,
                peers_enabled: asked.peers_enabled,
            },
            Wanted::Builtin(name) => AddOutcome::Builtin {
                name,
                changed: asked.appended,
                peers_enabled: asked.peers_enabled,
            },
            Wanted::Package {
                name,
                version,
                sha256,
                publisher,
                origin,
            } => AddOutcome::Installed {
                name,
                version,
                sha256,
                publisher,
                origin,
                peers_enabled: asked.peers_enabled,
            },
        }
    }
}

/// Ask `plan` for what `steps` name, check it once and commit it with the `specforge.json`
/// entries (each builtin after the builtins it requires, each install by its bare name): one
/// change that puts everything back when a step fails. A plan the peer rule refuses writes
/// nothing, its refusal naming the specifier when there are several (ADR 0041).
fn apply<'a>(
    req: &AddRequest,
    names: &Names<'_>,
    config: &specforge_common::ProjectConfig,
    mut plan: LockPlan<'a>,
    steps: Vec<Step<'a>>,
) -> Result<Added, Refusal<AddField>> {
    let mut outcomes: Vec<Option<AddOutcome>> = Vec::with_capacity(steps.len());
    // Each specifier that adds something, and what it asked for.
    let mut asked: Vec<(usize, Wanted)> = Vec::new();
    for (at, step) in steps.into_iter().enumerate() {
        outcomes.push(None);
        let ask = Ask(at);
        match step {
            Step::Unchanged(outcome) => outcomes[at] = Some(outcome),
            Step::Enable { name } => {
                plan.enable(ask, name);
                asked.push((at, Wanted::Builtin(name)));
            }
            Step::Local(install) => {
                asked.push((at, Wanted::of(&install)));
                plan.install(ask, install);
            }
            Step::Fetch(name, version) => {
                let install = plan
                    .fetch(&name, &version)
                    .map_err(|error| names.named(at, error).about(AddField::Extensions))?;
                asked.push((at, Wanted::of(&install)));
                plan.install(ask, install);
            }
        }
    }
    if asked.is_empty() {
        return Ok(Added {
            added: added(names, outcomes),
            writes: Writes::none(),
            extensions_enabled: config.extensions.len(),
        });
    }

    let done = plan
        .check()
        .map_err(|refused| {
            let (Ask(at), error) = refused.decisive();
            names.named(at, error).about(AddField::Extensions)
        })?
        .finish(
            req.root.dir(),
            Vec::new(),
            &req.caller.containment().narrowed(req.root.dir()),
        )?;
    for (at, wanted) in asked {
        outcomes[at] = Some(wanted.outcome(done.asked(Ask(at)), req.dry_run));
    }
    Ok(Added {
        added: added(names, outcomes),
        extensions_enabled: done.enabled,
        writes: done.writes,
    })
}

/// Each specifier of `names` with its outcome.
fn added(names: &Names<'_>, outcomes: Vec<Option<AddOutcome>>) -> Vec<AddedExtension> {
    outcomes
        .into_iter()
        .enumerate()
        .map(|(at, outcome)| AddedExtension {
            specifier: names.specifier(at).to_string(),
            outcome: outcome.expect("every specifier has an outcome"),
        })
        .collect()
}

mod document;
#[cfg(test)]
mod tests;

pub use document::{
    AddDocument, AddResult, BuiltinSource, DuplicateSpecifier, EnabledBuiltin, InstalledPackage,
    PlannedAdd, PresentExtension,
};
