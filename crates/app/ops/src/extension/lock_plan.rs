//! The lock plan (ADR 0041, amendment round 7; ADR 0065): the one way `add`, `update`, `init` and
//! `remove` change what a project installs and enables.
//!
//! An operation opens the plan for its verb ([`LockPlan::adding`], [`LockPlan::updating`],
//! [`LockPlan::initializing`], [`LockPlan::removing`]) and names what it asks for, each charged to
//! an [`Ask`]: it installs, enables, disables, uninstalls, or fetches from the plan's registry. It
//! never drives the plan's own steps: the plan
//! stages each install into the lock as the operation will leave it (the package's previous entry
//! is replaced, never counted beside it), closes over the builtins what it adds requires, and judges,
//! once, the **project peer set** as the change leaves it (the extensions the project's
//! `specforge.json` entries load, `peer_set`): every declared peer whose dependent or peer the
//! change touches, a missing required peer included. An unsatisfied peer installed from a registry
//! is unified against the registry the install came from (R-RES-006/005, ADR 0001); anything else
//! out of range (a local install, a builtin, a `.wasm` file entry) is refused with E027. Only then
//! is each binary fetched from a registry loaded, and it must declare what was published
//! (ADR 0012). What passes is committed as one change (ADR 0028 D4): the modules, the lock, the
//! appended `specforge.json` entries and, last, the publisher keys its fetches accepted (ADR
//! 0059). A refused or failed plan writes and pins nothing. Refusals and outcomes are answered per
//! ask.

use crate::view::ProjectView;
use serde_json::Value;
use specforge_installed::{Installed, LockFile, LockFileEntry, LockState, Module, Pin};
use specforge_package::PackageName;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_wasm::WasmRuntime;

use super::peer_set::Declarations;
use crate::asking::Asking;
use crate::registry::{KeyPin, Publisher, Registry};
use crate::{OpError, OpErrorKind};

/// What a lock plan is for. It words the refusal of an install that a locked extension does not
/// accept ("installing '@x/b' 2.0.0 breaks '@x/a': ...", "updating ...").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verb {
    /// `add` and `init`.
    Install,
    /// `update`.
    Update,
    /// `remove`.
    Remove,
}

impl Verb {
    /// Whether the verb enables what it installs and the builtins those require (`add`, `init`).
    /// `update` moves versions and enables nothing.
    fn enables(self) -> bool {
        matches!(self, Verb::Install)
    }

    fn participle(self) -> &'static str {
        match self {
            Verb::Install => "installing",
            Verb::Update => "updating",
            Verb::Remove => "removing",
        }
    }

    fn noun(self) -> &'static str {
        match self {
            Verb::Install => "install",
            Verb::Update => "update",
            Verb::Remove => "removal",
        }
    }
}

/// One thing an operation asked for, by its position among the operation's own requests (add's and
/// init's specifiers, update's lock entries). Refusals and outcomes are answered per ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Ask(pub(crate) usize);

/// Whether a checked plan is written or only answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Purpose {
    Commit,
    /// A dry run: judged exactly as a commit is, written, pinned and downloaded never.
    Preview,
}

/// Which `specforge.json` entries a removal drops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Disabled {
    /// Every entry naming this extension.
    Named(String),
    /// The entries written exactly so (a `.wasm` file entry).
    Entry(String),
}

impl Disabled {
    /// Whether `entry` (a `specforge.json` entry) is dropped by it.
    fn drops(&self, entry: &str) -> bool {
        match self {
            Disabled::Named(name) => specforge_common::extension_entry_name(entry) == name,
            Disabled::Entry(text) => entry.trim() == text.trim(),
        }
    }
}

/// Where a plan's registry installs come from, and how their publishers are accepted: the
/// registry port and the caller's choices.
#[derive(Clone, Copy)]
pub(crate) struct Fetching<'a> {
    pub(crate) registry: &'a dyn Registry,
    pub(crate) allow_unsigned: bool,
    /// Who can be asked about a publisher key change (ADR 0110).
    pub(crate) asking: Asking<'a>,
}

/// One install a lock plan stages: a module (none for a preview's registry package, which has not
/// been downloaded) and the lock entry that pins it.
pub(crate) struct Install<'a> {
    module: Option<Module>,
    /// The SHA-256 the lock pins: the module's, or for a preview the one its registry publishes.
    sha256: String,
    pin: Pin,
    from: From<'a>,
}

/// Where an install comes from, and what is left to check once the gate has passed.
enum From<'a> {
    /// A `.wasm` file: loaded and checked when it was read.
    File,
    /// A preview's registry package: judged by the declaration it was published with, never
    /// downloaded, loaded or committed.
    Published { registry: &'a dyn Registry },
    /// A registry: `bytes` (a copy of the module) load in `runtime` after the gate, and must
    /// declare `declaration`.
    Registry {
        registry: &'a dyn Registry,
        runtime: &'a dyn WasmRuntime,
        declaration: Box<ExtensionDeclaration>,
        publisher: Publisher,
        bytes: Module,
    },
    /// What the project already has, read from its lock entry: never staged, downloaded or loaded.
    Locked,
}

/// An install the plan staged, for the ask that wanted it.
struct Staged<'a> {
    ask: Ask,
    name: PackageName,
    version: String,
    module: Option<Module>,
    pin: Pin,
    from: From<'a>,
}

/// An entry the change appends to `specforge.json`'s `extensions`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Appended {
    name: String,
    /// The ask it is charged to: the one that asked for it, or, for a builtin the plan enabled
    /// because an extension requires it, the ask of that extension.
    ask: Ask,
    /// For a builtin the plan enabled because an extension the change asked for requires it, that
    /// extension (the root of the chain: `formal` for `software` and for `product` under it);
    /// `None` for an entry asked for.
    required_by: Option<String>,
}

/// The lock and the `extensions` entries as the operation leaves them, staged in memory.
pub(crate) struct LockPlan<'a> {
    installed: &'a Installed,
    /// The lock as the change leaves it; `None` for a plan that installs nothing (it accepts any
    /// lock: an unreadable one hides the installed extensions, as it hides them from the compile).
    lock: Option<LockFile>,
    staged: Vec<Staged<'a>>,
    /// `specforge.json` as the plan opened it: what it enables before the change.
    config: Opened,
    appended: Vec<Appended>,
    /// The answer each ask got when it was asked, in the order they were asked; a `Changes` holds
    /// no peers until `finish` fills them.
    asked: Vec<(Ask, Asked)>,
    declarations: Declarations<'a>,
    /// The runtime the plan's fetched binaries load in; `None` for a plan that fetches nothing
    /// and reads its declarations from a compiled project (`remove`).
    runtime: Option<&'a dyn WasmRuntime>,
    verb: Verb,
    purpose: Purpose,
    /// A removal is not judged (`remove --force`).
    force: bool,
    /// `(trigger, companion)`: the companion is enabled whenever the change leaves the trigger
    /// enabled.
    companions: Vec<(&'static str, &'static str)>,
    /// The entries a removal drops, and the packages it uninstalls, each for its ask.
    disabled: Vec<(Ask, Disabled)>,
    uninstalled: Vec<(Ask, PackageName)>,
    /// Where registry installs come from; `None` for a plan that fetches nothing.
    fetching: Option<Fetching<'a>>,
    /// The operation's name, for the refusal of a fetch from no registry.
    operation: &'static str,
}

impl<'a> LockPlan<'a> {
    /// `add`'s plan over the project `installed` is rooted at, whose `specforge.json` enables
    /// `entries`. Builtins and `.wasm` file entries are read through `runtime`, as are the local
    /// files it installs and, once the gate passed, what it fetched. `installs`: whether a
    /// specifier installs; then an unreadable lock refuses here (E033), before a file is read or a
    /// registry asked. A preview fetches nothing: a registry package is judged by its [`Manifest`].
    pub(crate) fn adding(
        installed: &'a Installed,
        config: Opened,
        runtime: &'a dyn WasmRuntime,
        fetching: Fetching<'a>,
        purpose: Purpose,
        installs: bool,
    ) -> Result<LockPlan<'a>, OpError> {
        let mut plan = LockPlan::open(
            installed,
            config,
            Some(runtime),
            Declarations::through(runtime),
            Verb::Install,
            Some(fetching),
            "add",
            installs,
        )?;
        plan.purpose = purpose;
        Ok(plan)
    }

    /// `update`'s plan: as [`Self::adding`], never enabling; E033 for an unreadable lock.
    pub(crate) fn updating(
        installed: &'a Installed,
        config: Opened,
        runtime: &'a dyn WasmRuntime,
        fetching: Fetching<'a>,
    ) -> Result<LockPlan<'a>, OpError> {
        LockPlan::open(
            installed,
            config,
            Some(runtime),
            Declarations::through(runtime),
            Verb::Update,
            Some(fetching),
            "update",
            true,
        )
    }

    /// `init`'s plan: a project with no `specforge.json` yet, which the commit creates from `document`
    /// with the entries the change leaves. It installs local files only (no registry); `installs` as
    /// in [`Self::adding`].
    pub(crate) fn initializing(
        installed: &'a Installed,
        runtime: &'a dyn WasmRuntime,
        installs: bool,
        document: Value,
    ) -> Result<LockPlan<'a>, OpError> {
        LockPlan::open(
            installed,
            Opened::creating(document),
            Some(runtime),
            Declarations::through(runtime),
            Verb::Install,
            None,
            "init",
            installs,
        )
    }

    /// `remove`'s plan over the compiled project `view`: its lock as the compile read it, its
    /// entries, the declarations its compile loaded (read again through the view's runtime only
    /// for an entry the removal lets load). `uninstalls`: whether it uninstalls (then an
    /// unreadable lock is E033). `force`: the peer rule is not applied (`remove --force`).
    pub(crate) fn removing(
        view: &ProjectView<'a>,
        purpose: Purpose,
        force: bool,
        uninstalls: bool,
    ) -> Result<LockPlan<'a>, OpError> {
        let mut plan = LockPlan::open(
            view.installed(),
            Opened::read(view.env().config(), view.env().config_found()),
            view.runtime().map(|runtime| &**runtime),
            Declarations::loaded_by(view),
            Verb::Remove,
            None,
            "remove",
            uninstalls,
        )?;
        plan.purpose = purpose;
        plan.force = force;
        Ok(plan)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the plan's whole input: the installed set, the entries, the runtime, the declarations, the verb, the registry, the operation and whether it installs"
    )]
    fn open(
        installed: &'a Installed,
        config: Opened,
        runtime: Option<&'a dyn WasmRuntime>,
        declarations: Declarations<'a>,
        verb: Verb,
        fetching: Option<Fetching<'a>>,
        operation: &'static str,
        installs: bool,
    ) -> Result<LockPlan<'a>, OpError> {
        let lock = if installs {
            Some(
                installed
                    .writable_lock()
                    .map_err(|why| OpError::of(OpErrorKind::SchemaMismatch, why))?,
            )
        } else {
            None
        };
        Ok(LockPlan {
            installed,
            lock,
            staged: Vec::new(),
            config,
            appended: Vec::new(),
            asked: Vec::new(),
            declarations,
            runtime,
            verb,
            purpose: Purpose::Commit,
            force: false,
            companions: Vec::new(),
            disabled: Vec::new(),
            uninstalled: Vec::new(),
            fetching,
            operation,
        })
    }

    /// Install `install` for `ask`: its lock entry replaces the package's previous one. A verb that
    /// enables also enables it by its bare name (nothing when an entry already does). Nothing is
    /// judged yet.
    ///
    /// A plan stages one install per name: the operation claims its specifiers first
    /// (`extension::claims`, ADR 0098), so two that name one extension never both reach the plan.
    ///
    /// # Panics
    /// On a plan opened with `installs: false` (it changes no lock, so it accepted any lock).
    /// In a debug build, on a second install of one name.
    pub(crate) fn install(&mut self, ask: Ask, install: Install<'a>) {
        debug_assert!(
            !self
                .staged
                .iter()
                .any(|staged| staged.name == install.pin.name),
            "'{}' is installed twice by one plan: claim the specifiers first (ADR 0098)",
            install.pin.name
        );
        if let Some(locked) = self
            .unchanged(&install.pin.name, |entry| {
                entry.wasm_hash == install.sha256
                    && std::mem::discriminant(&entry.source)
                        == std::mem::discriminant(&install.pin.source)
            })
            .map(|entry| entry.version.clone())
        {
            self.answer(
                ask,
                Asked::Unchanged {
                    locked: Some(locked),
                },
            );
            return;
        }
        debug_assert!(
            !matches!(install.from, From::Locked),
            "a locked install is always unchanged"
        );
        let Install {
            module,
            sha256,
            pin,
            from,
        } = install;
        self.lock
            .as_mut()
            .expect("a plan opened with installs: false stages no install")
            .put(pin.entry(&sha256));
        if self.verb.enables() {
            self.append(ask, pin.name.as_str());
        }
        self.answer(
            ask,
            Asked::Changes {
                peers_enabled: Vec::new(),
            },
        );
        self.staged.push(Staged {
            ask,
            name: pin.name.clone(),
            version: pin.version.clone(),
            module,
            pin,
            from,
        });
    }

    /// Enable `name` (a builtin, or an install's bare name) for `ask` with an entry of its own,
    /// after the entries `specforge.json` holds; the ask is answered unchanged when an entry
    /// already enables it.
    pub(crate) fn enable(&mut self, ask: Ask, name: &str) {
        let answer = if self.append(ask, name) {
            Asked::Changes {
                peers_enabled: Vec::new(),
            }
        } else {
            Asked::Unchanged { locked: None }
        };
        self.answer(ask, answer);
    }

    /// Append an entry for `name` unless one already enables it; `false` when an entry of
    /// `specforge.json` (as opened) does. An earlier ask of this change appending it still makes
    /// the change enable it.
    fn append(&mut self, ask: Ask, name: &str) -> bool {
        let named = |entry: &String| specforge_common::extension_entry_name(entry) == name;
        if self.config.entries().iter().any(named) {
            return false;
        }
        if !self.appended.iter().any(|appended| named(&appended.name)) {
            self.appended.push(Appended {
                name: name.to_string(),
                ask,
                required_by: None,
            });
        }
        true
    }

    /// `name`'s lock entry when the project already has it as `same` accepts: the lock the plan
    /// opened pins it, its module on disk is the one pinned, and an entry of the `specforge.json`
    /// the plan opened enables it. What the project has before the change, never the staged lock.
    fn unchanged(
        &self,
        name: &PackageName,
        same: impl Fn(&LockFileEntry) -> bool,
    ) -> Option<&LockFileEntry> {
        let entry = self.installed.verified(name.as_str()).filter(|e| same(e))?;
        let enabled = self
            .config
            .entries()
            .iter()
            .any(|e| specforge_common::extension_entry_name(e) == name.as_str());
        enabled.then_some(entry)
    }

    /// Whether the asks so far leave the project as it is: nothing staged, appended, dropped,
    /// uninstalled or created.
    fn changes_nothing(&self) -> bool {
        self.staged.is_empty()
            && self.appended.is_empty()
            && self.disabled.is_empty()
            && self.uninstalled.is_empty()
            && !self.config.creates()
    }

    /// Record `asked` as the answer to `ask`, unless it already has one.
    fn answer(&mut self, ask: Ask, asked: Asked) {
        if !self.asked.iter().any(|(known, _)| *known == ask) {
            self.asked.push((ask, asked));
        }
    }

    /// Drop the entries `disabled` selects, for `ask`.
    pub(crate) fn disable(&mut self, ask: Ask, disabled: Disabled) {
        self.disabled.push((ask, disabled));
    }

    /// Uninstall `name` (its module and lock entry), for `ask`. A name the lock does not hold
    /// changes nothing.
    ///
    /// # Panics
    /// On a plan opened with `uninstalls: false` (it changes no lock, so it accepted any lock).
    pub(crate) fn uninstall(&mut self, ask: Ask, name: &PackageName) {
        let lock = self
            .lock
            .as_mut()
            .expect("a plan opened with uninstalls: false uninstalls nothing");
        if lock.without(name) {
            self.uninstalled.push((ask, name.clone()));
        }
    }

    /// Enable `companion` whenever the change leaves `trigger` enabled (`init`: `@specforge/testing`
    /// beside `@specforge/software`, a test runner's extension beside `@specforge/testing`).
    /// Charged to the ask that enabled `trigger`.
    pub(crate) fn accompany(&mut self, trigger: &'static str, companion: &'static str) {
        self.companions.push((trigger, companion));
    }

    /// The project's installed extensions as the change leaves them.
    fn view(&self) -> Installed {
        match &self.lock {
            Some(lock) => {
                Installed::with_lock(self.installed.root(), LockState::Read(lock.clone()))
            }
            None => self.installed.clone(),
        }
    }

    /// What `specforge.json` enables as the change leaves it.
    fn entries_after(&self) -> Vec<String> {
        let items = self.config.items_after(
            |entry| {
                self.disabled
                    .iter()
                    .any(|(_, disabled)| disabled.drops(entry))
            },
            &self.appended,
        );
        Opened::entries_of(&items)
    }
}

/// What a change touches: each staged install, then each appended entry that is not also staged
/// (an `add`'s install is both, and counts once, as staged).
struct Changing {
    names: Vec<String>,
    /// The ask each of `names` is charged to.
    asks: Vec<Ask>,
    /// How many of `names` are staged installs: the first ones.
    staged: usize,
}

impl Changing {
    fn of(staged: &[Staged], appended: &[Appended]) -> Changing {
        let mut names: Vec<String> = staged.iter().map(|s| s.name.to_string()).collect();
        let mut asks: Vec<Ask> = staged.iter().map(|s| s.ask).collect();
        let installs = names.len();
        for entry in appended {
            let name = specforge_common::extension_entry_name(&entry.name);
            if !names.iter().any(|touched| touched == name) {
                names.push(name.to_string());
                asks.push(entry.ask);
            }
        }
        Changing {
            names,
            asks,
            staged: installs,
        }
    }

    fn none() -> Changing {
        Changing {
            names: Vec::new(),
            asks: Vec::new(),
            staged: 0,
        }
    }

    fn position(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|touched| touched == name)
    }

    fn contains(&self, name: &str) -> bool {
        self.position(name).is_some()
    }

    fn is_staged(&self, at: usize) -> bool {
        at < self.staged
    }
}

/// A lock plan the peer rule accepts and whose binaries all loaded as published: what may be
/// written, and the publisher keys to pin once it is.
pub(crate) struct Checked<'a> {
    installed: &'a Installed,
    /// Whether the change writes the lock (the plan was opened to install or uninstall).
    changes_lock: bool,
    /// The modules to place, each with the lock entry that pins it, in staging order.
    installs: Vec<(Module, Pin)>,
    /// The packages to uninstall.
    uninstalls: Vec<PackageName>,
    /// The `specforge.json` entries to drop.
    disabled: Vec<Disabled>,
    purpose: Purpose,
    /// The operation's name, for a refusal.
    operation: &'static str,
    /// The pins its registry installs call for, and the registry port that pins them; none for a
    /// plan that fetches nothing.
    pins: Option<(&'a dyn Registry, Vec<KeyPin>)>,
    /// `specforge.json` as the plan opened it.
    config: Opened,
    appended: Vec<Appended>,
    asked: Vec<(Ask, Asked)>,
    /// Whether the change writes anything (`LockPlan::changes_nothing` was false); when not, it is
    /// neither judged nor written (ADR 0138).
    changes: bool,
}

mod close;
mod commit;
mod config;
mod install;
mod judge;
#[cfg(test)]
mod tests;

pub(crate) use commit::{Also, Asked};
pub(crate) use config::Opened;
