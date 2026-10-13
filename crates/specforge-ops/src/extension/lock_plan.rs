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

use std::path::{Path, PathBuf};

use crate::view::ProjectView;
use specforge_common::{Diagnostic, codes};
use specforge_installed::{Installed, LockFile, LockSource, LockState, Module, Pin};
use specforge_package::peers::{PeerRequirement, Verdict};
use specforge_package::{PackageName, Version, highest_matching};
use specforge_protocol_types::{DeclaredCategory, ExtensionDeclaration, PeerDependency};
use specforge_wasm::WasmRuntime;

use super::candidate::{Installable, LocalFile};
use super::peer_set::{Declarations, ProjectPeers};
use super::{Origin, Trust};
use crate::registry::{KeyPin, METADATA_MISMATCH, Publisher, Registry, TrustPolicy};
use crate::{OpError, OpErrorKind, Writes};

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
    /// Every entry naming this extension (`config::remove_extension`).
    Named(String),
    /// The entries written exactly so (a `.wasm` file entry; `config::remove_entry`).
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
/// registry port and the caller's two trust choices.
#[derive(Clone, Copy)]
pub(crate) struct Fetching<'a> {
    pub(crate) registry: &'a dyn Registry,
    pub(crate) allow_unsigned: bool,
    pub(crate) trust: Trust,
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
}

impl<'a> Install<'a> {
    /// A `.wasm` file read once, installed as `local:<path>`. The path is relative to `root` when
    /// the file is under it, absolute otherwise. E072 when it declares no package name.
    pub(crate) fn local(root: &Path, local: LocalFile) -> Result<Install<'a>, OpError> {
        let name = local.binary.package()?;
        let candidate = local.binary.candidate();
        let pin = Pin {
            name,
            version: candidate.version().to_string(),
            source: LockSource::Local(shown_path(root, &local.path)),
            key_id: None,
            peers: candidate.peers().to_vec(),
        };
        let module = local.binary.into_module();
        Ok(Install {
            sha256: module.digest().to_string(),
            module: Some(module),
            pin,
            from: From::File,
        })
    }

    pub(crate) fn name(&self) -> &PackageName {
        &self.pin.name
    }

    /// The version it locks: what its declaration declares.
    pub(crate) fn version(&self) -> &str {
        &self.pin.version
    }

    /// The module's SHA-256: what the lock pins.
    pub(crate) fn sha256(&self) -> &str {
        &self.sha256
    }

    pub(crate) fn source(&self) -> &LockSource {
        &self.pin.source
    }

    /// How the registry accepted its publisher; `None` for a local file.
    pub(crate) fn publisher(&self) -> Option<&Publisher> {
        match &self.from {
            From::Registry { publisher, .. } => Some(publisher),
            From::File | From::Published { .. } => None,
        }
    }
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

impl Staged<'_> {
    /// The pin its registry fetch decided, once it is written; none for a local file, a key already
    /// pinned or an unsigned package.
    fn key_pin(&self) -> Option<KeyPin> {
        match &self.from {
            From::Registry { publisher, .. } => publisher.pin(&self.name),
            From::File | From::Published { .. } => None,
        }
    }

    /// The registry an unsatisfied peer touching this install is unified against; `None` for a
    /// local file (E027).
    fn registry(&self) -> Option<&dyn Registry> {
        match &self.from {
            From::Registry { registry, .. } | From::Published { registry } => Some(*registry),
            From::File => None,
        }
    }

    /// A fetched binary loaded in its runtime and held to what was published (ADR 0012). A local
    /// file was checked when it was read.
    fn load(&self) -> Result<(), OpError> {
        let From::Registry {
            runtime,
            declaration,
            bytes,
            ..
        } = &self.from
        else {
            // A local file was checked when it was read; a preview loads nothing.
            return Ok(());
        };
        let binary = Installable::read(*runtime, bytes.clone())?;
        let declared = binary.candidate();
        if declared.name() != self.name.as_str() || declared.version() != self.version {
            return Err(OpError::coded(
                crate::OpErrorKind::InvalidInput,
                codes::E028,
                format!(
                    "registry package {}@{} declares itself {}@{}",
                    self.name,
                    self.version,
                    declared.name(),
                    declared.version()
                ),
            ));
        }
        if let Some(category) = first_difference(declaration, declared.declaration()) {
            return Err(OpError::coded(
                OpErrorKind::SchemaMismatch,
                METADATA_MISMATCH,
                format!(
                    "registry package {}@{} declares another {category} than the binary it serves",
                    self.name, self.version
                ),
            )
            .with_suggestion("don't install the package, and check the registry"));
        }
        Ok(())
    }
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
    /// What `specforge.json` enables before the change.
    entries: Vec<String>,
    appended: Vec<Appended>,
    /// The extension each ask enables, in the order they were asked.
    wanted: Vec<(Ask, String)>,
    declarations: Declarations<'a>,
    /// The runtime the plan's fetched binaries load in; `None` for a plan that fetches nothing
    /// and reads its declarations from a compiled project (`remove`).
    runtime: Option<&'a dyn WasmRuntime>,
    verb: Verb,
    purpose: Purpose,
    /// A removal is not judged (`remove --force`).
    force: bool,
    /// Whether the change edits an existing `specforge.json`; `init` writes its own.
    edits_config: bool,
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
        entries: &[String],
        runtime: &'a dyn WasmRuntime,
        fetching: Fetching<'a>,
        purpose: Purpose,
        installs: bool,
    ) -> Result<LockPlan<'a>, OpError> {
        let mut plan = LockPlan::open(
            installed,
            entries,
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
        entries: &[String],
        runtime: &'a dyn WasmRuntime,
        fetching: Fetching<'a>,
    ) -> Result<LockPlan<'a>, OpError> {
        LockPlan::open(
            installed,
            entries,
            Some(runtime),
            Declarations::through(runtime),
            Verb::Update,
            Some(fetching),
            "update",
            true,
        )
    }

    /// `init`'s plan: a project with no `specforge.json` yet, whose config the caller's [`Also`]
    /// writes from [`Checked::entries`]. It installs local files only (no registry); `installs` as
    /// in [`Self::adding`].
    pub(crate) fn initializing(
        installed: &'a Installed,
        runtime: &'a dyn WasmRuntime,
        installs: bool,
    ) -> Result<LockPlan<'a>, OpError> {
        let mut plan = LockPlan::open(
            installed,
            &[],
            Some(runtime),
            Declarations::through(runtime),
            Verb::Install,
            None,
            "init",
            installs,
        )?;
        plan.edits_config = false;
        Ok(plan)
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
        let entries: Vec<String> = view
            .env()
            .enabled()
            .iter()
            .map(|entry| entry.entry.clone())
            .collect();
        let mut plan = LockPlan::open(
            view.installed(),
            &entries,
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

    #[allow(clippy::too_many_arguments)]
    fn open(
        installed: &'a Installed,
        entries: &[String],
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
            entries: entries.to_vec(),
            appended: Vec::new(),
            wanted: Vec::new(),
            declarations,
            runtime,
            verb,
            purpose: Purpose::Commit,
            force: false,
            edits_config: true,
            companions: Vec::new(),
            disabled: Vec::new(),
            uninstalled: Vec::new(),
            fetching,
            operation,
        })
    }

    /// `name@version` from the plan's registry, fetched: downloaded, integrity- and
    /// signature-checked under the plan's trust choices, held to the key the lock records for
    /// `name` when the user has no pin of their own, nothing written. Its pin is read from the
    /// published declaration: its version and its peers, the publisher's key id, `source:
    /// registry`. Its binary is not loaded: [`Self::check`] loads it after the gate. Nothing is
    /// pinned for the user: the publisher the fetch decided is pinned when the plan commits. For a
    /// preview, its [`Manifest`] only: nothing is downloaded and no publisher is decided. E063
    /// when the plan fetches from no registry.
    pub(crate) fn fetch(
        &self,
        name: &PackageName,
        version: &Version,
    ) -> Result<Install<'a>, OpError> {
        let Some(fetching) = self.fetching else {
            return Err(crate::registry::no_registry(self.operation));
        };
        if self.purpose == Purpose::Preview {
            let manifest = fetching.registry.published(name, version)?;
            let pin = Pin {
                name: manifest.name,
                version: manifest.version.to_string(),
                source: LockSource::Registry,
                key_id: None,
                peers: manifest.declaration.peers().to_vec(),
            };
            return Ok(Install {
                module: None,
                sha256: manifest.sha256,
                pin,
                from: From::Published {
                    registry: fetching.registry,
                },
            });
        }
        let policy = TrustPolicy::new(fetching.allow_unsigned, fetching.trust)
            .locked(self.installed.locked_key(name));
        let mut package = fetching.registry.fetch(name, version, policy)?;
        let module = Module::new(std::mem::take(&mut package.wasm));
        let pin = Pin {
            name: package.name.clone(),
            version: package.version.to_string(),
            source: LockSource::Registry,
            key_id: package.publisher.key_id().map(str::to_string),
            peers: package.declaration.peers().to_vec(),
        };
        Ok(Install {
            from: From::Registry {
                registry: fetching.registry,
                runtime: self.runtime.expect("a plan that fetches has a runtime"),
                declaration: Box::new(package.declaration),
                publisher: package.publisher,
                bytes: module.clone(),
            },
            sha256: module.digest().to_string(),
            module: Some(module),
            pin,
        })
    }

    /// Install `install` for `ask`: its lock entry replaces the package's previous one. A verb that
    /// enables also enables it by its bare name (nothing when an entry already does). Nothing is
    /// judged yet.
    ///
    /// # Panics
    /// On a plan opened with `installs: false` (it changes no lock, so it accepted any lock).
    pub(crate) fn install(&mut self, ask: Ask, install: Install<'a>) {
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
            self.enable(ask, pin.name.as_str());
        }
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
    /// after the entries `specforge.json` holds; nothing when an entry already enables it.
    pub(crate) fn enable(&mut self, ask: Ask, name: &str) {
        self.wanted.push((ask, name.to_string()));
        let enabled = self
            .entries
            .iter()
            .chain(self.appended.iter().map(|appended| &appended.name))
            .any(|entry| specforge_common::extension_entry_name(entry) == name);
        if !enabled {
            self.appended.push(Appended {
                name: name.to_string(),
                ask,
                required_by: None,
            });
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
        self.entries
            .iter()
            .filter(|entry| {
                !self
                    .disabled
                    .iter()
                    .any(|(_, disabled)| disabled.drops(entry))
            })
            .cloned()
            .chain(self.appended.iter().map(|appended| appended.name.clone()))
            .collect()
    }

    /// Enable what the change requires (a verb that `enables`; `update`'s plan enables nothing):
    /// each required peer of an extension the change touches that is a builtin no entry enables,
    /// and theirs, each appended just before the entry that first requires it (dependencies
    /// first). E028 when a builtin's declaration can't be read. Idempotent; [`Self::check`] runs
    /// it first.
    ///
    /// A cycle among required builtins ends it too: its builtins are enabled once, and the cycle's
    /// E027 stays the compile's (ADR 0041 D4).
    fn close(&mut self) -> Result<(), OpError> {
        if !self.verb.enables() {
            return Ok(());
        }
        loop {
            let set =
                ProjectPeers::of(&self.view(), &self.entries_after(), &mut self.declarations)?;
            let changing = Changing::of(&self.staged, &self.appended);
            let mut wanted: Vec<(String, &'static str)> = Vec::new();
            for name in &changing.names {
                let Some(member) = set.member(name) else {
                    continue;
                };
                for peer in member.peers.iter().filter(|peer| !peer.optional) {
                    if let Some(builtin) = super::builtin_name(&peer.name)
                        && set.member(builtin).is_none()
                        && !wanted.iter().any(|(_, wanted)| *wanted == builtin)
                    {
                        wanted.push((name.clone(), builtin));
                    }
                }
            }
            if wanted.is_empty() {
                if self.accompany_enabled() {
                    continue;
                }
                self.dependencies_first(&set);
                return Ok(());
            }
            for (dependent, builtin) in wanted {
                let (ask, root) = match self
                    .appended
                    .iter()
                    .find(|appended| appended.name == dependent)
                {
                    Some(appended) => (
                        appended.ask,
                        appended
                            .required_by
                            .clone()
                            .unwrap_or_else(|| dependent.clone()),
                    ),
                    None => (
                        self.staged
                            .iter()
                            .find(|staged| staged.name.as_str() == dependent)
                            .map_or(Ask(0), |staged| staged.ask),
                        dependent.clone(),
                    ),
                };
                self.appended.push(Appended {
                    name: builtin.to_string(),
                    ask,
                    required_by: Some(root),
                });
            }
        }
    }

    /// Append each companion whose trigger the change leaves enabled and that no entry enables:
    /// whether it appended any.
    fn accompany_enabled(&mut self) -> bool {
        let mut added = false;
        for (trigger, companion) in self.companions.clone() {
            let after = self.entries_after();
            let enabled = |name: &str| {
                after
                    .iter()
                    .any(|entry| specforge_common::extension_entry_name(entry) == name)
            };
            if enabled(trigger) && !enabled(companion) {
                let ask = self
                    .appended
                    .iter()
                    .find(|appended| appended.name == trigger)
                    .map(|appended| appended.ask)
                    .or_else(|| {
                        self.staged
                            .iter()
                            .find(|staged| staged.name.as_str() == trigger)
                            .map(|staged| staged.ask)
                    })
                    .unwrap_or(Ask(0));
                self.appended.push(Appended {
                    name: companion.to_string(),
                    ask,
                    required_by: None,
                });
                added = true;
            }
        }
        added
    }

    /// The appended entries, each after the appended builtins it requires (its non-optional
    /// peers): a depth-first order, otherwise the order they were enabled in. A cycle puts each of
    /// its entries once.
    fn dependencies_first(&mut self, set: &ProjectPeers) {
        let names: Vec<String> = self
            .appended
            .iter()
            .map(|appended| appended.name.clone())
            .collect();
        let mut order: Vec<usize> = Vec::with_capacity(names.len());
        let mut visited = vec![false; names.len()];
        fn visit(
            at: usize,
            names: &[String],
            set: &ProjectPeers,
            visited: &mut [bool],
            order: &mut Vec<usize>,
        ) {
            if std::mem::replace(&mut visited[at], true) {
                return;
            }
            if let Some(member) = set.member(&names[at]) {
                for peer in member.peers.iter().filter(|peer| !peer.optional) {
                    if super::builtin_name(&peer.name).is_none() {
                        continue;
                    }
                    if let Some(next) = names.iter().position(|name| *name == peer.name) {
                        visit(next, names, set, visited, order);
                    }
                }
            }
            order.push(at);
        }
        for at in 0..names.len() {
            visit(at, &names, set, &mut visited, &mut order);
        }
        let mut taken: Vec<Option<Appended>> = std::mem::take(&mut self.appended)
            .into_iter()
            .map(Some)
            .collect();
        self.appended = order
            .into_iter()
            .map(|at| taken[at].take().expect("each entry is placed once"))
            .collect();
    }

    /// Judge the plan, then load what it fetched.
    ///
    /// 1. [`Self::close`], then the peer rule over the project peer set as the change leaves it
    ///    ([`ProjectPeers::of`] over the staged lock and the entries plus the appended ones). It
    ///    judges every declared peer whose dependent or peer the change touches (a staged install,
    ///    an appended entry):
    ///    - E073 for a range that can't be read;
    ///    - E027 for a required peer no member has (check's message and suggestion);
    ///    - for a peer out of range or not SemVer, by where the peer comes from:
    ///      - installed, with the touching change fetched from a registry: unified against that
    ///        registry (R-RES-006 naming a version every requirer of the set accepts, R-RES-005
    ///        naming each once);
    ///      - otherwise E027: a local install's, or a builtin's or a `.wasm` file entry's, whose
    ///        versions no install changes.
    ///
    ///    A member the change breaks is named: "<installing|updating|enabling> '<name>' <version>
    ///    breaks '<dependent>': ...".
    /// 2. Each fetched install the gate did not refuse: its binary loaded in the plan's runtime
    ///    (E028, `extension_conflict`), declaring the name and version it was published as
    ///    (E028) and the declaration it was published with (R-TRUST-004).
    ///
    /// Each failure is charged to an ask: a staged install's or appended entry's own, a required
    /// builtin's to the ask whose extension required it. What passes carries the pins its registry
    /// fetches decided (`Publisher::pin`), in staging order, for [`Checked::finish`] to save. Lock
    /// order and entry order decide nothing.
    pub(crate) fn check(mut self) -> Result<Checked<'a>, Refused> {
        let changing = self.touched()?;
        let mut failed: Vec<Option<(OpError, bool)>> =
            changing.names.iter().map(|_| None).collect();
        if !changing.names.is_empty() && !(self.verb == Verb::Remove && self.force) {
            let (view, after) = (self.view(), self.entries_after());
            let set = match ProjectPeers::of(&view, &after, &mut self.declarations) {
                Ok(set) => set,
                Err(error) => return Err(Refused::of(&changing, error)),
            };
            let edges = unsatisfied(&set, &changing);
            for edge in &edges {
                let owner = edge.owner(&changing);
                if failed[owner].is_none() {
                    let error = refusal(
                        &set,
                        &self.staged,
                        &changing,
                        owner,
                        edge,
                        &edges,
                        self.verb,
                    );
                    failed[owner] = Some((error, true));
                }
            }
        }
        let LockPlan {
            installed,
            lock,
            staged,
            entries,
            appended,
            wanted,
            fetching,
            purpose,
            disabled,
            uninstalled,
            edits_config,
            ..
        } = self;
        for (i, s) in staged.iter().enumerate() {
            if failed[i].is_none() {
                failed[i] = s.load().err().map(|error| (error, false));
            }
        }
        let (failures, judged): (Vec<(Ask, OpError)>, Vec<bool>) = changing
            .asks
            .iter()
            .zip(failed)
            .filter_map(|(ask, f)| f.map(|(e, judged)| ((*ask, e), judged)))
            .unzip();
        if !failures.is_empty() {
            return Err(Refused { failures, judged });
        }
        let pins = staged.iter().filter_map(Staged::key_pin).collect();
        Ok(Checked {
            installed,
            changes_lock: lock.is_some(),
            installs: staged
                .into_iter()
                .filter_map(|staged| staged.module.map(|module| (module, staged.pin)))
                .collect(),
            uninstalls: uninstalled.into_iter().map(|(_, name)| name).collect(),
            disabled: disabled.into_iter().map(|(_, disabled)| disabled).collect(),
            purpose,
            edits_config,
            pins: fetching.map(|fetching| (fetching.registry, pins)),
            entries,
            appended,
            wanted,
        })
    }

    /// What the change touches, once what it enables is closed over (E028 refuses the plan when a
    /// builtin it needs does not load): a staged install, an appended entry, and for a removal the
    /// members it takes out of the project's peer set or lets load.
    fn touched(&mut self) -> Result<Changing, Refused> {
        if self.verb == Verb::Remove {
            return self.removal_touched();
        }
        if let Err(error) = self.close() {
            return Err(Refused::of(
                &Changing::of(&self.staged, &self.appended),
                error,
            ));
        }
        Ok(Changing::of(&self.staged, &self.appended))
    }

    /// The members that differ between the project peer set before the removal and after it: a
    /// member it removes, or one it lets load (a `.wasm` file entry another entry shadowed), each
    /// charged to the first ask.
    fn removal_touched(&mut self) -> Result<Changing, Refused> {
        let none = || Changing {
            names: Vec::new(),
            asks: Vec::new(),
            staged: 0,
        };
        let before = ProjectPeers::of(self.installed, &self.entries, &mut self.declarations)
            .map_err(|error| Refused::of(&none(), error))?;
        let after = ProjectPeers::of(&self.view(), &self.entries_after(), &mut self.declarations)
            .map_err(|error| Refused::of(&none(), error))?;
        let mut names: Vec<String> = Vec::new();
        for member in before.members().iter().chain(after.members()) {
            if before.member(&member.name) != after.member(&member.name)
                && !names.contains(&member.name)
            {
                names.push(member.name.clone());
            }
        }
        let ask = self
            .disabled
            .first()
            .map(|(ask, _)| *ask)
            .or_else(|| self.uninstalled.first().map(|(ask, _)| *ask))
            .unwrap_or(Ask(0));
        Ok(Changing {
            asks: names.iter().map(|_| ask).collect(),
            names,
            staged: 0,
        })
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
    edits_config: bool,
    /// The pins its registry installs call for, and the registry port that pins them; none for a
    /// plan that fetches nothing.
    pins: Option<(&'a dyn Registry, Vec<KeyPin>)>,
    /// What `specforge.json` enabled before the change.
    entries: Vec<String>,
    appended: Vec<Appended>,
    wanted: Vec<(Ask, String)>,
}

/// A file the change's last step writes after `specforge.json`'s edit and before the pins (`init`'s
/// config, `.gitignore` and starter). Put back, or removed when it was not there, when the change
/// fails.
pub(crate) struct Also<'f> {
    pub(crate) path: PathBuf,
    pub(crate) write: Box<dyn FnOnce() -> Result<(), OpError> + 'f>,
}

/// What a finished plan did, or for a preview would do.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Done {
    /// The files written; none for a preview.
    pub(crate) writes: Writes,
    /// How many extensions `specforge.json` enables after the change (would, for a preview).
    pub(crate) enabled: usize,
    asked: Vec<(Ask, Asked)>,
}

/// What a finished plan did for one ask.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Asked {
    /// The entry of the extension it asked for was appended by this change (would be, for a
    /// preview); `false` when an entry already enabled it, or another writer added it since the
    /// plan read the file.
    pub(crate) appended: bool,
    /// The builtins enabled because its extension requires them, dependencies first.
    pub(crate) peers_enabled: Vec<&'static str>,
}

impl Done {
    /// What the plan did for `ask` (nothing, for an ask the plan changed nothing for).
    pub(crate) fn asked(&self, ask: Ask) -> Asked {
        self.asked
            .iter()
            .find(|(asked, _)| *asked == ask)
            .map(|(_, asked)| asked.clone())
            .unwrap_or_default()
    }
}

/// `result`, a missing `specforge.json` being nothing to edit: a project without one (an install
/// only the lock knows) has no entry to drop.
fn edited(result: Result<bool, OpError>) -> Result<(), OpError> {
    match result {
        Err(error) if error.code == "config_not_found" => Ok(()),
        other => other.map(|_| ()),
    }
}

impl Checked<'_> {
    /// What `specforge.json` enables as the change leaves it, in order: the entries it held
    /// without the dropped ones, then the appended ones, dependencies first.
    pub(crate) fn entries(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|entry| !self.disabled.iter().any(|disabled| disabled.drops(entry)))
            .cloned()
            .chain(self.appended.iter().map(|appended| appended.name.clone()))
            .collect()
    }

    /// A preview answers without writing. A commit writes, all or nothing (ADR 0028 D4): each
    /// staged module placed and each uninstalled one moved aside, the lock once (when a module
    /// changed), then the last step: `specforge.json`'s edit (the appended entries, the dropped
    /// ones) and last the publisher keys its fetches accepted (ADR 0059). A plan that stages no
    /// module writes no lock. A failure at any step, the pins' included (R-TRUST-006), puts every
    /// file back and pins nothing. The error's writes name what could not be put back (normally
    /// nothing).
    pub(crate) fn finish(self, root: &Path, also: Vec<Also<'_>>) -> Result<Done, OpError> {
        let final_entries = self.entries();
        let Checked {
            installed,
            changes_lock,
            installs,
            uninstalls,
            disabled,
            purpose,
            edits_config,
            pins,
            appended,
            wanted,
            ..
        } = self;
        let names: Vec<&str> = appended
            .iter()
            .map(|appended| appended.name.as_str())
            .collect();
        let edited_config = edits_config && (!names.is_empty() || !disabled.is_empty());
        let also_paths: Vec<PathBuf> = also.iter().map(|also| also.path.clone()).collect();
        let mut enabled = None;
        let mut writes = Writes::none();
        if purpose == Purpose::Commit {
            let config_path = root.join(crate::config::CONFIG_FILE);
            let last = || -> Result<(), OpError> {
                if edits_config && !names.is_empty() {
                    enabled = Some(crate::config::enable(root, &names)?);
                }
                for dropped in &disabled {
                    edited(match dropped {
                        Disabled::Named(name) => crate::config::remove_extension(root, name),
                        Disabled::Entry(entry) => crate::config::remove_entry(root, entry),
                    })?;
                }
                for file in also {
                    (file.write)()?;
                }
                record(pins)
            };
            let mut change = if changes_lock {
                installed
                    .change()
                    .map_err(|why| OpError::of(OpErrorKind::SchemaMismatch, why))?
            } else {
                installed.change_entries()
            };
            for (module, pin) in installs {
                change.install(module, pin);
            }
            for name in &uninstalls {
                change.uninstall(name);
            }
            let committed = change
                .files(
                    edited_config
                        .then(|| config_path.clone())
                        .into_iter()
                        .chain(also_paths),
                )
                .commit_with(last, |why| OpError::of(OpErrorKind::Internal, why))
                .map_err(|failed| failed.error.with_writes(Writes::of(failed.left)))?;
            writes = Writes::of(committed.changed);
        }

        let written = |name: &str| match purpose {
            Purpose::Commit if !edits_config => names.contains(&name),
            Purpose::Commit => enabled
                .as_ref()
                .is_some_and(|enabled| enabled.appended.iter().any(|a| a == name)),
            Purpose::Preview => names.contains(&name),
        };
        let mut asked: Vec<(Ask, Asked)> = Vec::new();
        for (ask, name) in &wanted {
            if asked.iter().any(|(known, _)| known == ask) {
                continue;
            }
            asked.push((
                *ask,
                Asked {
                    appended: written(name),
                    // Each is reported once, by the ask whose extension first required it.
                    peers_enabled: appended
                        .iter()
                        .filter(|entry| entry.ask == *ask && entry.required_by.is_some())
                        .filter(|entry| written(&entry.name))
                        .filter_map(|entry| super::builtin_name(&entry.name))
                        .collect(),
                },
            ));
        }
        Ok(Done {
            writes,
            enabled: enabled
                .as_ref()
                .map_or(final_entries.len(), |enabled| enabled.total),
            asked,
        })
    }
}

/// Pin what a committed plan's fetches decided, through the port that fetched them.
fn record(pins: Option<(&dyn Registry, Vec<KeyPin>)>) -> Result<(), OpError> {
    match pins {
        Some((registry, pins)) if !pins.is_empty() => registry.pin(&pins),
        _ => Ok(()),
    }
}

/// Why a lock plan was refused: each ask that can't be done, with its first reason, in staging
/// order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Refused {
    failures: Vec<(Ask, OpError)>,
    /// For each failure, whether the peer rule refused it (else its binary did not load).
    judged: Vec<bool>,
}

impl Refused {
    /// `error`, refusing the plan as a whole: charged to the first ask it touches.
    fn of(changing: &Changing, error: OpError) -> Refused {
        Refused {
            failures: vec![(changing.asks.first().copied().unwrap_or(Ask(0)), error)],
            judged: vec![true],
        }
    }

    /// What an all-or-nothing operation reports (`add`, `init`): the first ask the peer rule
    /// refused, else the first whose binary did not load. A refusal the set earns is not hidden by
    /// a binary the plan would never have loaded.
    pub(crate) fn decisive(self) -> (Ask, OpError) {
        let at = self
            .judged
            .iter()
            .position(|judged| *judged)
            .unwrap_or_default();
        self.failures
            .into_iter()
            .nth(at)
            .expect("a refused plan has a failure")
    }

    /// Every refused ask, in staging order (`update` marks each row).
    pub(crate) fn failures(self) -> Vec<(Ask, OpError)> {
        self.failures
    }
}

/// A declared peer the project peer set leaves unsatisfied.
struct Edge<'s> {
    dependent: &'s str,
    declared: &'s PeerDependency,
    verdict: Verdict,
}

impl Edge<'_> {
    /// The touched name the refusal belongs to: the dependent when it is touched, else the peer.
    fn owner(&self, changing: &Changing) -> usize {
        changing
            .position(self.dependent)
            .or_else(|| changing.position(&self.declared.name))
            .expect("an edge touching the plan")
    }
}

/// The unsatisfied peers of `set` that touch `changing`, in this order: a touched extension's own
/// peers first (in touching order, each in its declared order), then the members a touched one
/// breaks (in entry order). A required peer no member has is among them (ADR 0041).
fn unsatisfied<'s>(set: &'s ProjectPeers, changing: &Changing) -> Vec<Edge<'s>> {
    let mut edges: Vec<Edge<'s>> = set
        .rule()
        .unsatisfied()
        .into_iter()
        .filter(|u| changing.contains(u.dependent) || changing.contains(&u.declared.name))
        .map(|u| Edge {
            dependent: u.dependent,
            declared: u.declared,
            verdict: u.verdict,
        })
        .collect();
    edges.sort_by_key(|e| changing.position(e.dependent).map_or((1, 0), |i| (0, i)));
    edges
}

/// Why `edge` refuses the touched package at `owner`.
fn refusal(
    set: &ProjectPeers,
    staged: &[Staged],
    changing: &Changing,
    owner: usize,
    edge: &Edge,
    edges: &[Edge],
    verb: Verb,
) -> OpError {
    if verb == Verb::Remove {
        return removal_refusal(changing, owner, edge, edges);
    }
    let peer = edge.declared.name.as_str();
    let checks = || {
        OpError::of(
            OpErrorKind::SchemaMismatch,
            specforge_common::peers::of(edge.dependent, edge.declared, &edge.verdict)
                .expect("an unsatisfied peer is a diagnostic"),
        )
    };
    let error = match &edge.verdict {
        Verdict::Unreadable(why) => OpError::of(
            OpErrorKind::SchemaMismatch,
            specforge_common::peers::unreadable(edge.dependent, edge.declared, why),
        ),
        Verdict::Missing => checks(),
        Verdict::OutOfRange { installed } | Verdict::NotSemver { installed } => {
            match set.member(peer).map(|member| &member.origin) {
                Some(Origin::Builtin) => {
                    let dependent_is_builtin = set
                        .member(edge.dependent)
                        .is_some_and(|member| member.origin == Origin::Builtin);
                    checks().with_suggestion(if dependent_is_builtin {
                        "this specforge build's builtin extensions disagree: reinstall specforge"
                            .to_string()
                    } else {
                        format!(
                            "'{peer}' is the builtin this specforge embeds, at {installed}: install a version of '{}' that accepts it",
                            edge.dependent
                        )
                    })
                }
                Some(Origin::Installed { .. }) => {
                    match staged.get(owner).and_then(Staged::registry) {
                        None => checks(),
                        Some(registry) => {
                            // What the plan leaves the peer at: this operation's version, or the
                            // lock's.
                            let locked = if changing.contains(peer) {
                                format!("this {} locks {installed}", verb.noun())
                            } else {
                                format!("{installed} is locked")
                            };
                            diamond(registry, set, edge, &locked)
                        }
                    }
                }
                Some(Origin::File { .. } | Origin::Unknown) | None => checks(),
            }
        }
        Verdict::Satisfied => unreachable!("filtered by `unsatisfied`"),
    };
    if changing.contains(edge.dependent) {
        error
    } else {
        let (participle, name, version) = if changing.is_staged(owner) {
            let install = &staged[owner];
            (
                verb.participle(),
                install.name.to_string(),
                install.version.clone(),
            )
        } else {
            let name = changing.names[owner].clone();
            let version = set
                .member(&name)
                .map_or_else(String::new, |member| member.version.clone());
            ("enabling", name, version)
        };
        error.prefixed(format!(
            "{participle} '{name}' {version} breaks '{}': ",
            edge.dependent
        ))
    }
}

/// Why a removal is refused for `edge`: what it takes out of the project's peer set (or lets load)
/// leaves a member's required peer unsatisfied. "removing '<name>' breaks '<dependent>': ...",
/// its suggestion naming every member the removal leaves that way.
fn removal_refusal(changing: &Changing, owner: usize, edge: &Edge, edges: &[Edge]) -> OpError {
    let diagnostic = match &edge.verdict {
        Verdict::Unreadable(why) => {
            specforge_common::peers::unreadable(edge.dependent, edge.declared, why)
        }
        verdict => specforge_common::peers::of(edge.dependent, edge.declared, verdict)
            .expect("an unsatisfied peer is a diagnostic"),
    };
    let error = OpError::of(OpErrorKind::Conflict, diagnostic);
    if changing.contains(edge.dependent) {
        return error;
    }
    let name = &changing.names[owner];
    let mut dependents: Vec<String> = edges
        .iter()
        .filter(|other| other.declared.name == *name && !changing.contains(other.dependent))
        .map(|other| format!("'{}'", other.dependent))
        .collect();
    dependents.sort();
    dependents.dedup();
    error
        .prefixed(format!("removing '{name}' breaks '{}': ", edge.dependent))
        .with_suggestion(format!(
            "remove {} first, or use --force to remove '{name}' anyway",
            dependents.join(", ")
        ))
}

/// R-RES-006 naming the version of `edge`'s peer every requirer in `set` accepts, or R-RES-005
/// (E073) when none does (ADR 0001).
fn diamond(registry: &dyn Registry, set: &ProjectPeers, edge: &Edge, locked: &str) -> OpError {
    let peer = edge.declared.name.as_str();
    let versions = match published_versions(registry, peer) {
        Ok(versions) => versions,
        Err(error) => return error,
    };
    match unify(peer, &versions, &set.requirers(peer)) {
        Ok(unified) => OpError::coded(
            OpErrorKind::Conflict,
            codes::R_RES_006,
            format!(
                "version diamond: '{}' requires peer '{peer}' {} but {locked}; {peer} {unified} would satisfy every requirer",
                edge.dependent, edge.declared.version
            ),
        )
        .with_suggestion(format!(
            "install '{peer}' at {unified} (specforge add {peer}@{unified}), or at another version every requirer accepts, then retry; when a locked extension must move with it, specforge update --major moves them together"
        )),
        Err(diagnostic) => OpError::of(OpErrorKind::Conflict, diagnostic),
    }
}

/// The versions `registry` publishes of `peer`, in the registry's order. E072 when `peer` is no
/// package name.
fn published_versions(registry: &dyn Registry, peer: &str) -> Result<Vec<Version>, OpError> {
    let name = PackageName::parse(peer).map_err(|why| {
        OpError::of(
            OpErrorKind::InvalidInput,
            specforge_common::package::invalid(&why),
        )
    })?;
    registry.versions(&name)
}

/// Unify a version diamond (ADR 0001): the highest of `published` that every requirer's range
/// matches, each range read on its own (`specforge_package::highest_matching`; a pre-release only
/// when that range names its `MAJOR.MINOR.PATCH`). R-RES-005 naming each requirer when none does;
/// E073 for a range that is not SemVer.
fn unify(
    name: &str,
    published: &[Version],
    requirers: &[(&str, &PeerDependency)],
) -> Result<Version, Diagnostic> {
    let mut reqs = Vec::with_capacity(requirers.len());
    for (requirer, declared) in requirers {
        let requirement = PeerRequirement::read(declared)
            .map_err(|why| specforge_common::peers::unreadable(requirer, declared, &why))?;
        reqs.push((*requirer, declared.version.as_str(), requirement.range));
    }
    let ranges: Vec<_> = reqs.iter().map(|(_, _, range)| range).collect();

    highest_matching(published, &ranges).cloned().ok_or_else(|| {
        let wanted = reqs
            .iter()
            .map(|(requirer, range, _)| format!("{requirer} wants {name} {range}"))
            .collect::<Vec<_>>()
            .join("; ");
        let available = published
            .iter()
            .map(Version::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        Diagnostic::new(
            codes::R_RES_005,
            format!(
                "version diamond for '{name}': no single version satisfies every requirer ({wanted}). Available: {available}"
            ),
        )
        .with_suggestion(
            "no version unifies these ranges; upgrade the requirer with the narrowest range or pin a compatible peer version manually"
                .to_string(),
        )
    })
}

/// The first part of a declaration that differs between `served` and `binary` (`handshake`, or a
/// describe category), or `None` when they are equal.
fn first_difference(
    served: &ExtensionDeclaration,
    binary: &ExtensionDeclaration,
) -> Option<&'static str> {
    if served.handshake != binary.handshake {
        return Some("handshake");
    }
    DeclaredCategory::ALL
        .into_iter()
        .find(|category| served.describe_items(*category) != binary.describe_items(*category))
        .map(DeclaredCategory::name)
}

/// `path` as the lock records it: relative to the project root when it is under it, absolute
/// otherwise.
fn shown_path(root: &Path, path: &Path) -> String {
    let absolute = |p: &Path| {
        std::path::absolute(p)
            .map(|p| p.canonicalize().unwrap_or(p))
            .unwrap_or_else(|_| p.to_path_buf())
    };
    let (root, path) = (absolute(root), absolute(path));
    path.strip_prefix(&root)
        .unwrap_or(&path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::entry;
    use super::*;
    use crate::registry::testing::{Asked, MemoryRegistry, Published, declaration};
    use crate::testing::declaring;
    use specforge_installed::{LockFile, LockFileEntry, LockState};
    use specforge_test_macros::test as specforge_test;
    use specforge_wasm::testing::InProcessRuntime;

    const BASE: &[u8] = b"\0asm in-process @acme/base 2.0.0";
    const APP: &[u8] = b"\0asm in-process @acme/app 1.0.0";
    const APP_NEEDS_BASE_2: &[u8] = b"\0asm in-process @acme/app 1.0.0 needs base 2";
    const ONE_ISH: &[u8] = b"\0asm in-process @acme/app 1.0.0 one-ish";
    const BASE_1: &[u8] = b"\0asm in-process @acme/base 1.0.0";
    const APP_NEEDS_BASE_1: &[u8] = b"\0asm in-process @acme/app 1.0.0 needs base 1";
    const APP_NEEDS_BASE_3: &[u8] = b"\0asm in-process @acme/app 1.0.0 needs base 3";

    fn nowhere(entries: Vec<LockFileEntry>) -> Installed {
        Installed::with_lock(
            Path::new("/nowhere"),
            LockState::Read(LockFile {
                lockfile_version: 1,
                entries,
            }),
        )
    }

    fn peer(name: &str, range: &str) -> PeerDependency {
        PeerDependency {
            name: name.to_string(),
            version: range.to_string(),
            optional: false,
        }
    }

    /// `@acme/base` locked at 1.0.0 and required by `@acme/other` at `other_range`.
    fn base_and_other(other_range: &str) -> Vec<LockFileEntry> {
        vec![
            entry("@acme/base", "1.0.0", "registry", &[]),
            entry(
                "@acme/other",
                "1.0.0",
                "registry",
                &[("@acme/base", other_range)],
            ),
        ]
    }

    /// The runtime serving every binary above.
    fn runtime() -> InProcessRuntime {
        InProcessRuntime::new()
            .binary(BASE, declaring("@acme/base", "2.0.0", &[]))
            .binary(BASE_1, declaring("@acme/base", "1.0.0", &[]))
            .binary(APP, declaring("@acme/app", "1.0.0", &[]))
            .binary(
                APP_NEEDS_BASE_2,
                declaring("@acme/app", "1.0.0", &[("@acme/base", "^2.0", false)]),
            )
            .binary(
                APP_NEEDS_BASE_1,
                declaring("@acme/app", "1.0.0", &[("@acme/base", "^1.0", false)]),
            )
            .binary(
                APP_NEEDS_BASE_3,
                declaring("@acme/app", "1.0.0", &[("@acme/base", "^3.0", false)]),
            )
            .binary(
                ONE_ISH,
                declaring("@acme/app", "1.0.0", &[("@acme/base", "one-ish", false)]),
            )
    }

    /// `@acme/base` 1.0.0, 1.4.0 and 2.0.0 published.
    fn base_registry() -> MemoryRegistry {
        let published = |version: &str, bytes: &[u8]| {
            Published::new(bytes.to_vec(), declaration("@acme/base", version, &[]))
        };
        MemoryRegistry::new()
            .serving(published("1.0.0", b"\0asm b1"))
            .serving(published("1.4.0", b"\0asm b14"))
            .serving(published("2.0.0", b"\0asm b2"))
    }

    /// The registry of a plan that fetches nothing.
    fn no_registry() -> &'static MemoryRegistry {
        static NONE: std::sync::LazyLock<MemoryRegistry> =
            std::sync::LazyLock::new(MemoryRegistry::new);
        &NONE
    }

    fn fetching(registry: &MemoryRegistry) -> Fetching<'_> {
        Fetching {
            registry,
            allow_unsigned: true,
            trust: Trust::Refuse,
        }
    }

    /// `bytes` read in `runtime` as a local file at `/nowhere/<name>.wasm`.
    fn local(runtime: &InProcessRuntime, bytes: &[u8], file: &str) -> Install<'static> {
        let installable = Installable::read(runtime, Module::new(bytes.to_vec())).unwrap();
        Install::local(
            Path::new("/nowhere"),
            LocalFile {
                path: Path::new("/nowhere").join(file),
                binary: installable,
            },
        )
        .unwrap()
    }

    /// A plan over `installed` whose `specforge.json` enables every locked extension, fetching
    /// from `registry`.
    fn plan_from<'a>(
        installed: &'a Installed,
        runtime: &'a InProcessRuntime,
        registry: &'a MemoryRegistry,
        verb: Verb,
    ) -> LockPlan<'a> {
        let entries: Vec<String> = installed
            .lock()
            .entries()
            .iter()
            .map(|e| e.name.to_string())
            .collect();
        match verb {
            Verb::Install => LockPlan::adding(
                installed,
                &entries,
                runtime,
                fetching(registry),
                Purpose::Commit,
                true,
            ),
            Verb::Update => LockPlan::updating(installed, &entries, runtime, fetching(registry)),
            Verb::Remove => unreachable!("a removal is planned over a compiled project"),
        }
        .unwrap()
    }

    /// [`plan_from`] a registry that serves nothing.
    fn plan<'a>(
        installed: &'a Installed,
        runtime: &'a InProcessRuntime,
        verb: Verb,
    ) -> LockPlan<'a> {
        plan_from(installed, runtime, no_registry(), verb)
    }

    /// Stage `install` for the first ask; `add` enables it by its bare name.
    fn stage<'a>(plan: &mut LockPlan<'a>, install: Install<'a>) {
        plan.install(Ask(0), install);
    }

    /// `name@version` fetched from the plan's registry and staged.
    fn stage_fetched(plan: &mut LockPlan<'_>, name: &PackageName, version: Version) {
        let install = plan.fetch(name, &version).unwrap();
        stage(plan, install);
    }

    fn refused(plan: LockPlan<'_>) -> OpError {
        plan.check().map(drop).unwrap_err().failures().remove(0).1
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "a peer locked outside the new extension's range fails R-RES-006 naming a version that satisfies every requirer"
    )]
    fn a_unifiable_diamond_is_refused_with_the_version_that_unifies() {
        let installed = nowhere(base_and_other(">=1.0.0"));
        let (registry, runtime) = (base_registry(), runtime());
        let app = Published::new(
            b"\0asm app".to_vec(),
            declaration("@acme/app", "1.0.0", &[("@acme/base", "^2.0")]),
        );
        let registry = registry.serving(app);
        let mut plan = plan_from(&installed, &runtime, &registry, Verb::Install);
        stage_fetched(
            &mut plan,
            &PackageName::parse("@acme/app").unwrap(),
            Version::new(1, 0, 0),
        );

        let err = refused(plan);

        assert_eq!(err.code, "R-RES-006");
        assert_eq!(
            err.message,
            "version diamond: '@acme/app' requires peer '@acme/base' ^2.0 but 1.0.0 is locked; @acme/base 2.0.0 would satisfy every requirer"
        );
        assert_eq!(
            err.suggestion.as_deref(),
            Some(
                "install '@acme/base' at 2.0.0 (specforge add @acme/base@2.0.0), or at another version every requirer accepts, then retry; when a locked extension must move with it, specforge update --major moves them together"
            )
        );
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "a peer no single version satisfies for every requirer fails R-RES-005"
    )]
    fn an_impossible_diamond_is_refused_naming_each_requirer() {
        let installed = nowhere(base_and_other("^1.0"));
        let registry = base_registry().serving(Published::new(
            b"\0asm app".to_vec(),
            declaration("@acme/app", "1.0.0", &[("@acme/base", "^2.0")]),
        ));
        let runtime = runtime();
        let mut plan = plan_from(&installed, &runtime, &registry, Verb::Install);
        stage_fetched(
            &mut plan,
            &PackageName::parse("@acme/app").unwrap(),
            Version::new(1, 0, 0),
        );

        let err = refused(plan);

        assert_eq!(err.code, "R-RES-005");
        assert!(
            err.message.contains("@acme/other wants @acme/base ^1.0")
                && err.message.contains("@acme/app wants @acme/base ^2.0"),
            "{err:?}"
        );
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "an extension whose peer range is not SemVer is refused with E073 before anything is installed"
    )]
    fn an_unreadable_candidate_range_is_refused_e073() {
        let installed = nowhere(base_and_other(">=1.0.0"));
        let runtime = runtime();
        let mut plan = plan(&installed, &runtime, Verb::Install);
        stage(&mut plan, local(&runtime, ONE_ISH, "app.wasm"));

        let err = refused(plan);

        assert_eq!(err.code, "E073", "{err:?}");
        assert!(
            err.message.contains("'@acme/app'") && err.message.contains("'one-ish'"),
            "{err:?}"
        );
    }

    #[test]
    fn an_unreadable_requirer_range_is_e073_naming_it() {
        let installed = nowhere(base_and_other("one-ish"));
        let registry = base_registry().serving(Published::new(
            b"\0asm app".to_vec(),
            declaration("@acme/app", "1.0.0", &[("@acme/base", "^2.0")]),
        ));
        let runtime = runtime();
        let mut plan = plan_from(&installed, &runtime, &registry, Verb::Install);
        stage_fetched(
            &mut plan,
            &PackageName::parse("@acme/app").unwrap(),
            Version::new(1, 0, 0),
        );

        let err = refused(plan);

        assert_eq!(err.code, "E073", "{err:?}");
        assert!(err.message.contains("'@acme/other'"), "{err:?}");
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "a local install whose peer is installed outside its range is refused with E027"
    )]
    fn a_local_candidate_out_of_range_is_e027() {
        let installed = nowhere(base_and_other("^1.0"));
        let runtime = runtime();
        let mut plan = plan(&installed, &runtime, Verb::Install);
        stage(&mut plan, local(&runtime, APP_NEEDS_BASE_2, "app.wasm"));

        let err = refused(plan);

        assert_eq!(err.code, "E027", "{err:?}");
        assert_eq!(
            err.message,
            "extension '@acme/app' requires peer dependency '@acme/base' ^2.0 but version 1.0.0 is installed"
        );
    }

    #[test]
    fn a_locked_peer_at_a_version_that_is_not_semver_is_unified_like_one_out_of_range() {
        let mut entries = base_and_other("^1.0");
        entries[0].version = "local".to_string();
        let installed = nowhere(entries);
        let registry = base_registry().serving(Published::new(
            b"\0asm app".to_vec(),
            declaration("@acme/app", "1.0.0", &[("@acme/base", ">=1.0.0")]),
        ));
        let runtime = runtime();
        let mut plan = plan_from(&installed, &runtime, &registry, Verb::Install);
        stage_fetched(
            &mut plan,
            &PackageName::parse("@acme/app").unwrap(),
            Version::new(1, 0, 0),
        );

        let err = refused(plan);

        assert_eq!(err.code, "R-RES-006", "{err:?}");
        assert!(err.message.contains("@acme/base 1.4.0"), "{err:?}");
    }

    #[test]
    fn a_satisfied_plan_asks_the_registry_nothing_more() {
        let installed = nowhere(base_and_other("^1.0"));
        let runtime = runtime();
        let registry = base_registry().serving(Published::new(
            APP_NEEDS_BASE_1.to_vec(),
            crate::publish::declare(&runtime, APP_NEEDS_BASE_1)
                .unwrap()
                .0,
        ));
        let mut plan = plan_from(&installed, &runtime, &registry, Verb::Install);
        stage_fetched(
            &mut plan,
            &PackageName::parse("@acme/app").unwrap(),
            Version::new(1, 0, 0),
        );

        plan.check().map(drop).unwrap();

        assert_eq!(
            registry.asked(),
            [Asked::Fetch(
                PackageName::parse("@acme/app").unwrap(),
                Version::new(1, 0, 0)
            )]
        );
    }

    #[test]
    fn a_registry_failure_listing_the_peers_versions_is_the_error() {
        let installed = nowhere(base_and_other("^1.0"));
        // `@acme/base` is not served: listing its versions fails R-RES-001.
        let registry = MemoryRegistry::new().serving(Published::new(
            b"\0asm app".to_vec(),
            declaration("@acme/app", "1.0.0", &[("@acme/base", "^2.0")]),
        ));
        let runtime = runtime();
        let mut plan = plan_from(&installed, &runtime, &registry, Verb::Install);
        stage_fetched(
            &mut plan,
            &PackageName::parse("@acme/app").unwrap(),
            Version::new(1, 0, 0),
        );

        let err = refused(plan);

        assert_eq!(err.code, "R-RES-001", "{err:?}");
    }

    #[test]
    fn a_staged_package_replaces_its_previous_entry() {
        // B1 at unit level: `@acme/app` 1.0.0 locked wanting base ^1; staging 2.0.0 (^2) must not
        // leave the old range among the requirers.
        let installed = nowhere(vec![
            entry("@acme/base", "1.0.0", "registry", &[]),
            entry("@acme/app", "1.0.0", "registry", &[("@acme/base", "^1")]),
        ]);
        let registry = base_registry().serving(Published::new(
            b"\0asm app2".to_vec(),
            declaration("@acme/app", "2.0.0", &[("@acme/base", "^2")]),
        ));
        let runtime = runtime();
        let mut plan = plan_from(&installed, &runtime, &registry, Verb::Install);
        stage_fetched(
            &mut plan,
            &PackageName::parse("@acme/app").unwrap(),
            Version::new(2, 0, 0),
        );

        // The staged entry replaced the previous one: base has one requirer, at ^2.
        let view = plan.view();
        let entries = ["@acme/base".to_string(), "@acme/app".to_string()];
        let set = ProjectPeers::of(&view, &entries, &mut Declarations::through(&runtime)).unwrap();
        let listed = set.requirers("@acme/base");
        assert_eq!(listed.len(), 1);
        assert_eq!(
            (listed[0].0, listed[0].1.version.as_str()),
            ("@acme/app", "^2")
        );

        let err = refused(plan);

        assert_eq!(err.code, "R-RES-006", "{err:?}");
    }

    #[test]
    fn a_locked_extension_a_staged_one_breaks_is_named_with_the_install() {
        let installed = nowhere(vec![
            entry("@acme/base", "1.0.0", "registry", &[]),
            entry("@acme/app", "1.0.0", "registry", &[("@acme/base", "^1")]),
        ]);
        let runtime = runtime();
        let mut plan = plan(&installed, &runtime, Verb::Install);
        stage(&mut plan, local(&runtime, BASE, "base.wasm"));

        let err = refused(plan);

        assert_eq!(err.code, "E027", "{err:?}");
        assert!(
            err.message
                .starts_with("installing '@acme/base' 2.0.0 breaks '@acme/app': "),
            "{err:?}"
        );
    }

    #[test]
    fn an_update_words_what_it_breaks() {
        let installed = nowhere(vec![
            entry("@acme/base", "1.0.0", "registry", &[]),
            entry("@acme/app", "1.0.0", "registry", &[("@acme/base", "^1")]),
        ]);
        let runtime = runtime();
        let mut plan = plan(&installed, &runtime, Verb::Update);
        stage(&mut plan, local(&runtime, BASE, "base.wasm"));

        let err = refused(plan);

        assert!(
            err.message
                .starts_with("updating '@acme/base' 2.0.0 breaks '@acme/app': "),
            "{err:?}"
        );
    }

    #[test]
    fn a_required_peer_no_member_has_is_refused_e027() {
        let installed = nowhere(Vec::new());
        let runtime = runtime();
        let mut plan = plan(&installed, &runtime, Verb::Install);
        stage(&mut plan, local(&runtime, APP_NEEDS_BASE_2, "app.wasm"));

        let err = refused(plan);

        assert_eq!(err.code, "E027", "{err:?}");
        assert_eq!(
            err.message,
            "extension '@acme/app' requires peer dependency '@acme/base' ^2.0 which is not installed"
        );
        assert_eq!(
            err.suggestion.as_deref(),
            Some("install it with: specforge add @acme/base")
        );
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "an extension no specforge.json entry enables is not judged"
    )]
    fn a_locked_extension_no_entry_enables_is_not_judged() {
        let installed = nowhere(vec![
            entry("@acme/base", "1.0.0", "registry", &[]),
            entry(
                "@acme/other",
                "1.0.0",
                "registry",
                &[("@acme/base", "^1.0")],
            ),
        ]);
        let runtime = runtime();
        // No entry enables `@acme/other`: it is in the lock only.
        let mut plan = LockPlan::adding(
            &installed,
            &[],
            &runtime,
            fetching(no_registry()),
            Purpose::Commit,
            true,
        )
        .unwrap();
        stage(&mut plan, local(&runtime, BASE, "base.wasm"));

        assert!(plan.check().is_ok());
    }

    #[test]
    fn an_unchanged_extension_left_broken_is_not_this_plans() {
        // `@acme/other` already wants a base that is not locked at ^9: the plan touches neither.
        let installed = nowhere(vec![
            entry("@acme/base", "1.0.0", "registry", &[]),
            entry("@acme/other", "1.0.0", "registry", &[("@acme/base", "^9")]),
        ]);
        let runtime = runtime();
        let mut plan = plan(&installed, &runtime, Verb::Install);
        stage(&mut plan, local(&runtime, APP, "app.wasm"));

        assert!(plan.check().is_ok());
    }

    #[test]
    fn edges_where_both_ends_move_are_judged_against_both_new_versions() {
        // app (wants base ^2) and base both move: judged against base 2.0.0, in either order.
        let runtime = runtime();
        for app_first in [true, false] {
            let installed = nowhere(vec![
                entry("@acme/base", "1.0.0", "registry", &[]),
                entry("@acme/app", "1.0.0", "registry", &[("@acme/base", "^1")]),
            ]);
            let mut plan = plan(&installed, &runtime, Verb::Update);
            let (app, base) = (
                local(&runtime, APP_NEEDS_BASE_2, "app.wasm"),
                local(&runtime, BASE, "base.wasm"),
            );
            if app_first {
                stage(&mut plan, app);
                stage(&mut plan, base);
            } else {
                stage(&mut plan, base);
                stage(&mut plan, app);
            }
            assert!(plan.check().is_ok(), "app first: {app_first}");
        }
    }

    #[test]
    fn a_staged_extensions_own_peers_are_reported_before_what_it_breaks() {
        // `@acme/app` 1.0.0 is staged wanting base ^2 (locked 1.0.0); the locked `@acme/dep`
        // wants app ^2. The app's own peer comes first.
        let installed = nowhere(vec![
            entry("@acme/base", "1.0.0", "registry", &[]),
            entry("@acme/dep", "1.0.0", "registry", &[("@acme/app", "^2")]),
            entry("@acme/app", "2.0.0", "registry", &[]),
        ]);
        let runtime = runtime();
        let mut plan = plan(&installed, &runtime, Verb::Install);
        stage(&mut plan, local(&runtime, APP_NEEDS_BASE_2, "app.wasm"));

        let err = refused(plan);

        assert_eq!(
            err.message,
            "extension '@acme/app' requires peer dependency '@acme/base' ^2.0 but version 1.0.0 is installed"
        );
    }

    #[test]
    fn a_refused_plan_loads_no_binary() {
        let installed = nowhere(base_and_other("^1.0"));
        let registry = base_registry().serving(Published::new(
            APP_NEEDS_BASE_2.to_vec(),
            declaration("@acme/app", "1.0.0", &[("@acme/base", "^2.0")]),
        ));
        let runtime = runtime();
        let mut plan = plan_from(&installed, &runtime, &registry, Verb::Install);
        stage_fetched(
            &mut plan,
            &PackageName::parse("@acme/app").unwrap(),
            Version::new(1, 0, 0),
        );

        let err = refused(plan);

        assert_eq!(err.code, "R-RES-005", "{err:?}");
        assert!(
            runtime
                .calls()
                .iter()
                .all(|call| call.extension != "__candidate"),
            "{:?}",
            runtime.calls()
        );
    }

    /// `@acme/app` 1.0.0 (needs base `^1.0`), published signed by `signer` (unsigned when `None`).
    fn app_registry(signer: Option<&str>) -> MemoryRegistry {
        let app = Published::new(
            APP_NEEDS_BASE_1.to_vec(),
            crate::publish::declare(&runtime(), APP_NEEDS_BASE_1)
                .expect("a publishable binary")
                .0,
        );
        MemoryRegistry::new().serving(match signer {
            Some(key) => app.signed_by(key),
            None => app,
        })
    }

    /// A project on disk locking base 1.0.0, with a `specforge.json`.
    fn base_project() -> tempfile::TempDir {
        let dir =
            super::super::fixtures::project(vec![entry("@acme/base", "1.0.0", "registry", &[])]);
        std::fs::write(
            dir.path().join("specforge.json"),
            "{\"extensions\":[\"@acme/base\"]}",
        )
        .unwrap();
        dir
    }

    /// `@acme/app` 1.0.0 fetched from `registry` and staged over the lock in `installed`.
    fn stage_app<'a>(
        registry: &'a MemoryRegistry,
        runtime: &'a InProcessRuntime,
        installed: &'a Installed,
    ) -> LockPlan<'a> {
        let mut plan = plan_from(installed, runtime, registry, Verb::Install);
        stage_fetched(&mut plan, &app(), Version::new(1, 0, 0));
        plan
    }

    fn app() -> PackageName {
        PackageName::parse("@acme/app").unwrap()
    }

    #[specforge_test(
        behavior = "pin_publisher_key",
        verify = "a lock plan pins the keys its fetches decided only when it commits"
    )]
    fn a_lock_plan_pins_what_its_fetches_decided_only_when_it_commits() {
        let runtime = runtime();

        // Refused by the gate (app wants base ^2, base 1.0.0 is locked): nothing is pinned.
        let installed = nowhere(base_and_other("^1.0"));
        let refused_app = base_registry().serving(
            Published::new(
                APP_NEEDS_BASE_2.to_vec(),
                declaration("@acme/app", "1.0.0", &[("@acme/base", "^2.0")]),
            )
            .signed_by("k1"),
        );
        let mut plan = plan_from(&installed, &runtime, &refused_app, Verb::Install);
        stage_fetched(&mut plan, &app(), Version::new(1, 0, 0));
        assert_eq!(refused(plan).code, "R-RES-005");
        assert!(refused_app.pins().is_empty());

        // Checked but not committed: nothing is pinned either.
        let dir = base_project();
        let installed = Installed::at(dir.path());
        let registry = app_registry(Some("k1"));
        let checked = stage_app(&registry, &runtime, &installed).check().unwrap();
        assert!(registry.pins().is_empty(), "a check pins nothing");
        drop(checked);
        assert!(registry.pins().is_empty());

        // Committed: pinned.
        let checked = stage_app(&registry, &runtime, &installed).check().unwrap();
        checked.finish(dir.path(), Vec::new()).unwrap();
        assert_eq!(registry.pins(), [(app(), "k1".to_string())].into());
    }

    #[specforge_test(
        invariant = "registry_integrity",
        verify = "the hash and the key id are recorded in the lock file"
    )]
    fn a_committed_registry_install_locks_its_hash_and_key_id() {
        let (runtime, registry) = (runtime(), app_registry(Some("k1")));
        let dir = base_project();
        let installed = Installed::at(dir.path());

        stage_app(&registry, &runtime, &installed)
            .check()
            .unwrap()
            .finish(dir.path(), Vec::new())
            .unwrap();

        let lock = specforge_installed::read_lock_file(&specforge_installed::lock_path(dir.path()))
            .unwrap();
        let locked = lock
            .entries
            .iter()
            .find(|e| e.name == app())
            .expect("the app is locked");
        assert_eq!(
            locked.wasm_hash,
            specforge_installed::hex_sha256(APP_NEEDS_BASE_1)
        );
        assert_eq!(locked.key_id.as_deref(), Some("k1"));
    }

    #[specforge_test(
        invariant = "extension_operation_atomicity",
        verify = "a pin that cannot be saved puts back everything the install wrote"
    )]
    fn a_pin_that_cannot_be_saved_puts_back_everything_the_install_wrote() {
        let (runtime, registry) = (runtime(), app_registry(Some("k1")));
        let registry = registry.refusing_pins();
        let dir = base_project();
        let installed = Installed::at(dir.path());
        let lock_file = specforge_installed::lock_path(dir.path());
        let config = dir.path().join("specforge.json");
        let module = installed.module_path(&app());
        let (lock_before, config_before) = (
            std::fs::read(&lock_file).unwrap(),
            std::fs::read(&config).unwrap(),
        );

        let error = stage_app(&registry, &runtime, &installed)
            .check()
            .unwrap()
            .finish(dir.path(), Vec::new())
            .unwrap_err();

        assert!(error.is(codes::R_TRUST_006), "{error:?}");
        assert_eq!(std::fs::read(&lock_file).unwrap(), lock_before);
        assert_eq!(std::fs::read(&config).unwrap(), config_before);
        assert!(!module.exists(), "the placed module is taken away");
        assert!(registry.pins().is_empty());
    }

    #[test]
    fn an_already_pinned_or_unsigned_install_pins_nothing() {
        let runtime = runtime();
        let dir = base_project();
        let installed = Installed::at(dir.path());

        // The user already pinned this key.
        let registry = app_registry(Some("k1")).pinning("@acme/app", "k1");
        stage_app(&registry, &runtime, &installed)
            .check()
            .unwrap()
            .finish(dir.path(), Vec::new())
            .unwrap();
        assert_eq!(registry.pins(), [(app(), "k1".to_string())].into());

        // Unsigned, accepted with allow_unsigned.
        let dir = base_project();
        let installed = Installed::at(dir.path());
        let registry = app_registry(None);
        stage_app(&registry, &runtime, &installed)
            .check()
            .unwrap()
            .finish(dir.path(), Vec::new())
            .unwrap();
        assert!(registry.pins().is_empty());
    }

    #[test]
    fn a_fetched_binary_that_declares_otherwise_than_published_is_refused_after_the_gate() {
        let installed = nowhere(Vec::new());
        // Published without the peer the binary declares.
        let registry = MemoryRegistry::new().serving(Published::new(
            APP_NEEDS_BASE_2.to_vec(),
            declaration("@acme/app", "1.0.0", &[]),
        ));
        let runtime = runtime();
        let mut plan = plan_from(&installed, &runtime, &registry, Verb::Install);
        stage_fetched(
            &mut plan,
            &PackageName::parse("@acme/app").unwrap(),
            Version::new(1, 0, 0),
        );

        let err = refused(plan);

        assert!(err.is(METADATA_MISMATCH), "{err:?}");
    }

    #[test]
    fn refused_names_each_staged_package_once_in_staging_order() {
        // The app wants base ^3 and the base breaks `other`: both are refused, once each.
        let installed = nowhere(base_and_other("^1.0"));
        let runtime = runtime();
        let mut plan = plan(&installed, &runtime, Verb::Install);
        plan.install(Ask(0), local(&runtime, APP_NEEDS_BASE_3, "app.wasm"));
        plan.install(Ask(1), local(&runtime, BASE, "base.wasm"));

        let refused = plan.check().map(drop).unwrap_err();

        let asks: Vec<Ask> = refused.failures().iter().map(|(ask, _)| *ask).collect();
        assert_eq!(asks, [Ask(0), Ask(1)]);
    }

    /// `bytes` published as the package the in-process probe runtime says they declare.
    fn published_probe(bytes: &[u8]) -> Published {
        let declared = crate::publish::declare(&crate::testing::probes(), bytes)
            .unwrap()
            .0;
        Published::new(bytes.to_vec(), declared)
    }

    #[test]
    fn a_builtin_peer_out_of_range_is_e027_naming_the_embedded_version() {
        let installed = nowhere(Vec::new());
        let runtime = crate::testing::probes();
        let registry =
            MemoryRegistry::new().serving(published_probe(crate::testing::NEEDS_SOFTWARE_9));
        let entries = ["@specforge/software".to_string()];
        let mut plan = LockPlan::adding(
            &installed,
            &entries,
            &runtime,
            fetching(&registry),
            Purpose::Commit,
            true,
        )
        .unwrap();
        stage_fetched(
            &mut plan,
            &PackageName::parse("@probe/needs-software").unwrap(),
            Version::new(1, 0, 0),
        );

        let err = refused(plan);

        assert_eq!(err.code, "E027", "{err:?}");
        assert_eq!(
            err.message,
            "extension '@probe/needs-software' requires peer dependency '@specforge/software' ^9.0 but version 1.0.0 is installed"
        );
        assert_eq!(
            err.suggestion.as_deref(),
            Some(
                "'@specforge/software' is the builtin this specforge embeds, at 1.0.0: install a version of '@probe/needs-software' that accepts it"
            )
        );
        assert_eq!(
            registry.asked(),
            [Asked::Fetch(
                PackageName::parse("@probe/needs-software").unwrap(),
                Version::new(1, 0, 0)
            )],
            "a builtin is never unified"
        );
    }

    #[test]
    fn enabling_a_builtin_a_member_rejects_is_named_with_the_enable() {
        let installed = nowhere(vec![entry(
            "@probe/needs-software",
            "1.0.0",
            "local:needs.wasm",
            &[("@specforge/software", "^9.0")],
        )]);
        let runtime = crate::testing::probes();
        let mut plan = plan(&installed, &runtime, Verb::Install);
        plan.enable(Ask(0), "@specforge/software");

        let err = refused(plan);

        assert_eq!(err.code, "E027", "{err:?}");
        assert!(
            err.message.starts_with(
                "enabling '@specforge/software' 1.0.0 breaks '@probe/needs-software': "
            ),
            "{err:?}"
        );
    }

    #[test]
    fn a_file_entry_is_a_member_both_ways() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = crate::testing::probes();
        let installed = Installed::with_lock(
            dir.path(),
            LockState::Read(LockFile {
                lockfile_version: 1,
                entries: Vec::new(),
            }),
        );

        // `dep.wasm` wants `@probe/base` ^1: installing base 2.0.0 breaks it.
        std::fs::write(dir.path().join("dep.wasm"), crate::testing::DEP_1).unwrap();
        let entries = ["dep.wasm".to_string()];
        let mut plan = LockPlan::adding(
            &installed,
            &entries,
            &runtime,
            fetching(no_registry()),
            Purpose::Commit,
            true,
        )
        .unwrap();
        stage(
            &mut plan,
            local(&runtime, crate::testing::BASE_2, "base.wasm"),
        );
        let err = refused(plan);
        assert_eq!(err.code, "E027", "{err:?}");
        assert!(
            err.message
                .starts_with("installing '@probe/base' 2.0.0 breaks '@probe/dep': "),
            "{err:?}"
        );

        // `base.wasm` is 2.0.0: the dep installed beside it does not get the ^1 it wants.
        std::fs::write(dir.path().join("base.wasm"), crate::testing::BASE_2).unwrap();
        let entries = ["base.wasm".to_string()];
        let mut plan = LockPlan::adding(
            &installed,
            &entries,
            &runtime,
            fetching(no_registry()),
            Purpose::Commit,
            true,
        )
        .unwrap();
        stage(
            &mut plan,
            local(&runtime, crate::testing::DEP_1, "dep.wasm"),
        );
        let err = refused(plan);
        assert_eq!(
            err.message,
            "extension '@probe/dep' requires peer dependency '@probe/base' ^1.0 but version 2.0.0 is installed"
        );
    }

    #[test]
    fn a_diamond_counts_a_file_entrys_range_among_the_requirers() {
        const X_NEEDS_BASE_1: &[u8] = b"\0asm in-process @acme/x 1.0.0 needs base 1";
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("x.wasm"), X_NEEDS_BASE_1).unwrap();
        let installed = Installed::with_lock(
            dir.path(),
            LockState::Read(LockFile {
                lockfile_version: 1,
                entries: vec![entry("@acme/base", "1.0.0", "registry", &[])],
            }),
        );
        let runtime = runtime().binary(
            X_NEEDS_BASE_1,
            declaring("@acme/x", "1.0.0", &[("@acme/base", "^1", false)]),
        );
        // The app wants base ^2: base 2.0.0 alone would unify, but the file entry wants ^1.
        let registry = base_registry().serving(Published::new(
            b"\0asm app".to_vec(),
            declaration("@acme/app", "1.0.0", &[("@acme/base", "^2.0")]),
        ));
        let entries = ["@acme/base".to_string(), "x.wasm".to_string()];
        let mut plan = LockPlan::adding(
            &installed,
            &entries,
            &runtime,
            fetching(&registry),
            Purpose::Commit,
            true,
        )
        .unwrap();
        stage_fetched(
            &mut plan,
            &PackageName::parse("@acme/app").unwrap(),
            Version::new(1, 0, 0),
        );

        let err = refused(plan);

        assert_eq!(err.code, "R-RES-005", "{err:?}");
        assert!(
            err.message.contains("@acme/x wants @acme/base ^1")
                && err.message.contains("@acme/app wants @acme/base ^2.0"),
            "{err:?}"
        );
    }

    #[test]
    fn a_builtin_that_does_not_load_refuses_the_plan_e028() {
        let installed = nowhere(Vec::new());
        // The runtime serves `APP`, not the builtin `@specforge/software`.
        let runtime = runtime();
        let entries = ["@specforge/software".to_string()];
        let mut plan = LockPlan::adding(
            &installed,
            &entries,
            &runtime,
            fetching(no_registry()),
            Purpose::Commit,
            true,
        )
        .unwrap();
        stage(&mut plan, local(&runtime, APP, "app.wasm"));

        let err = refused(plan);

        assert_eq!(err.code, "E028", "{err:?}");
        assert!(
            err.message
                .starts_with("the builtin '@specforge/software' does not load"),
            "{err:?}"
        );
    }

    #[test]
    fn an_enabling_plan_accepts_an_unreadable_lock() {
        let installed = Installed::with_lock(
            Path::new("/nowhere"),
            LockState::Unreadable(specforge_common::Diagnostic::new(codes::E033, "unreadable")),
        );
        let runtime = crate::testing::probes();

        // A plan that only enables opens no change: the lock hides the installed extensions.
        let mut enabling = LockPlan::adding(
            &installed,
            &[],
            &runtime,
            fetching(no_registry()),
            Purpose::Commit,
            false,
        )
        .unwrap();
        enabling.enable(Ask(0), "@specforge/software");
        assert!(enabling.check().is_ok());

        // One that installs refuses: writing a lock would drop what it records.
        let refused = LockPlan::adding(
            &installed,
            &[],
            &runtime,
            fetching(no_registry()),
            Purpose::Commit,
            true,
        )
        .map(drop);
        assert_eq!(refused.unwrap_err().code, "E033");
    }

    #[test]
    fn close_enables_required_builtins_dependencies_first() {
        const APP_NEEDS_FORMAL: &[u8] = b"\0asm in-process @acme/app 1.0.0 needs formal";
        let builtin = |runtime, name: &'static str, peers| {
            crate::testing::serving_builtin(runtime, name, declaring(name, "1.0.0", peers))
        };
        // formal requires software, which has product as an optional peer.
        let runtime = runtime().binary(
            APP_NEEDS_FORMAL,
            declaring(
                "@acme/app",
                "1.0.0",
                &[("@specforge/formal", "^1.0", false)],
            ),
        );
        let runtime = builtin(
            runtime,
            "@specforge/formal",
            &[("@specforge/software", "^1.0", false)],
        );
        let runtime = builtin(
            runtime,
            "@specforge/software",
            &[("@specforge/product", "^1.0", true)],
        );
        let installed = nowhere(Vec::new());

        let mut plan = LockPlan::adding(
            &installed,
            &[],
            &runtime,
            fetching(no_registry()),
            Purpose::Commit,
            true,
        )
        .unwrap();
        stage(&mut plan, local(&runtime, APP_NEEDS_FORMAL, "app.wasm"));
        plan.close().unwrap();

        let appended: Vec<(&str, Option<&str>)> = plan
            .appended
            .iter()
            .map(|a| (a.name.as_str(), a.required_by.as_deref()))
            .collect();
        assert_eq!(
            appended,
            [
                ("@specforge/software", Some("@acme/app")),
                ("@specforge/formal", Some("@acme/app")),
                ("@acme/app", None),
            ],
            "the optional product is not enabled"
        );
        plan.close().unwrap();
        assert_eq!(plan.appended.len(), 3, "idempotent");

        // Asked in the wrong order, a builtin still comes after the builtin it requires.
        let mut plan = LockPlan::adding(
            &installed,
            &[],
            &runtime,
            fetching(no_registry()),
            Purpose::Commit,
            false,
        )
        .unwrap();
        plan.enable(Ask(0), "@specforge/formal");
        plan.enable(Ask(0), "@specforge/software");
        plan.close().unwrap();
        let names: Vec<&str> = plan.appended.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["@specforge/software", "@specforge/formal"]);
    }

    #[test]
    fn update_judges_without_enabling() {
        let installed = nowhere(vec![entry("@acme/app", "1.0.0", "registry", &[])]);
        let runtime = runtime();
        let mut plan = plan(&installed, &runtime, Verb::Update);
        stage(&mut plan, local(&runtime, APP_NEEDS_BASE_2, "app.wasm"));

        let err = refused(plan);

        assert_eq!(err.code, "E027", "{err:?}");
        assert_eq!(
            err.message,
            "extension '@acme/app' requires peer dependency '@acme/base' ^2.0 which is not installed"
        );
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "add enables the builtins a required builtin peer requires, dependencies first"
    )]
    fn each_ask_is_answered_with_the_builtins_its_extension_required() {
        let builtin = |runtime, name: &'static str, peers| {
            crate::testing::serving_builtin(runtime, name, declaring(name, "1.0.0", peers))
        };
        let runtime = builtin(
            builtin(
                runtime(),
                "@specforge/formal",
                &[("@specforge/software", "^1.0", false)],
            ),
            "@specforge/software",
            &[],
        );
        let enabling = |asked: &[&'static str]| {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(
                dir.path().join("specforge.json"),
                "{\"name\":\"p\",\"version\":\"0.1.0\",\"extensions\":[]}",
            )
            .unwrap();
            let installed = Installed::at(dir.path());
            let mut plan = LockPlan::adding(
                &installed,
                &[],
                &runtime,
                fetching(no_registry()),
                Purpose::Commit,
                false,
            )
            .unwrap();
            for (at, name) in asked.iter().enumerate() {
                plan.enable(Ask(at), name);
            }
            plan.check()
                .ok()
                .unwrap()
                .finish(dir.path(), Vec::new())
                .unwrap()
        };

        // software first: formal's requirement is already met by the earlier ask.
        let done = enabling(&["@specforge/software", "@specforge/formal"]);
        assert_eq!(done.asked(Ask(0)).peers_enabled, Vec::<&str>::new());
        assert_eq!(done.asked(Ask(1)).peers_enabled, Vec::<&str>::new());
        assert!(done.asked(Ask(0)).appended && done.asked(Ask(1)).appended);

        // formal alone enables software with it.
        let done = enabling(&["@specforge/formal"]);
        assert_eq!(done.asked(Ask(0)).peers_enabled, ["@specforge/software"]);
        assert_eq!(done.enabled, 2);
    }

    fn req(requirer: &'static str, range: &str) -> (&'static str, PeerDependency) {
        (requirer, peer("@shared/lib", range))
    }

    fn unified(
        vs: &[&str],
        requirers: &[(&'static str, PeerDependency)],
    ) -> Result<String, Diagnostic> {
        let borrowed: Vec<(&str, &PeerDependency)> =
            requirers.iter().map(|(n, p)| (*n, p)).collect();
        let published: Vec<Version> = vs.iter().map(|v| Version::parse(v).unwrap()).collect();
        unify("@shared/lib", &published, &borrowed).map(|version| version.to_string())
    }

    /// Pinned: a pre-release is unified only when every requirer's range names its
    /// `MAJOR.MINOR.PATCH`, each range read on its own; build metadata is ordered; R-RES-005 lists
    /// `Available:` in the registry's order. T5 keeps every answer.
    #[test]
    fn pin_unify_takes_a_pre_release_only_when_every_range_names_it() {
        let published = ["1.0.0", "1.5.0", "2.0.0-beta.1"];
        // (a) no range names the beta's triple.
        let result = unified(
            &published,
            &[req("@a/ext", ">=1.0.0"), req("@b/ext", "^1.0")],
        );
        assert_eq!(result.unwrap(), "1.5.0");
        // (b) both do.
        let result = unified(
            &published,
            &[
                req("@a/ext", ">=2.0.0-beta.1"),
                req("@b/ext", "^2.0.0-beta.1"),
            ],
        );
        assert_eq!(result.unwrap(), "2.0.0-beta.1");
        // (c) one does: merged into one requirement the beta would pass, read apart it does not.
        let result = unified(
            &published,
            &[req("@a/ext", ">=2.0.0-beta.1"), req("@b/ext", ">=1.0.0")],
        );
        assert_eq!(result.unwrap_err().code, "R-RES-005");
        // (d) semver's total order.
        let result = unified(
            &["1.0.0+build.2", "1.0.0+build.1"],
            &[req("@a/ext", "^1.0")],
        );
        assert_eq!(result.unwrap(), "1.0.0+build.2");
        // (e) the registry's order.
        let err = unified(&["2.0.0", "1.0.0"], &[req("@a/ext", "^3.0")]).unwrap_err();
        assert!(err.message.ends_with("Available: 2.0.0, 1.0.0"), "{err:?}");
    }

    #[test]
    fn unify_picks_highest_version_satisfying_every_requirer() {
        let result = unified(
            &["1.0.0", "1.5.0", "2.0.0", "2.5.0", "3.0.0"],
            &[req("@a/ext", "^2.0.0"), req("@b/ext", ">=2.0.0, <3.0.0")],
        )
        .unwrap();
        assert_eq!(result, "2.5.0");
    }

    #[test]
    fn unify_single_requirer_behaves_like_resolve_version() {
        let result = unified(&["1.0.0", "1.2.0", "2.0.0"], &[req("@a/ext", "^1.0.0")]).unwrap();
        assert_eq!(result, "1.2.0");
    }

    #[test]
    fn unify_incompatible_ranges_reports_diamond_conflict() {
        let err = unified(
            &["1.0.0", "1.5.0", "2.0.0", "2.5.0"],
            &[req("@a/ext", "^1.0.0"), req("@b/ext", "^2.0.0")],
        )
        .unwrap_err();
        assert_eq!(err.code, "R-RES-005");
        assert!(err.message.contains("@a/ext"));
        assert!(err.message.contains("@b/ext"));
        assert!(err.message.contains("^1.0.0"));
        assert!(err.message.contains("^2.0.0"));
    }

    #[test]
    fn unify_malformed_range_names_the_requirer() {
        let err = unified(&["1.0.0"], &[req("@a/ext", "not-a-range")]).unwrap_err();
        assert_eq!(err.code, "E073");
        assert!(err.message.contains("@a/ext"));
    }

    #[test]
    fn unify_no_requirers_picks_highest_available() {
        let result = unified(&["1.0.0", "2.0.0"], &[]).unwrap();
        assert_eq!(result, "2.0.0");
    }
}
