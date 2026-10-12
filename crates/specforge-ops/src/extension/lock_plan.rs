//! The lock plan (ADR 0041, amendment 2026-10-11): the one way `add`, `update` and `init`
//! install extensions.
//!
//! Each install is staged into the lock as the operation will leave it. The package's previous
//! entry is replaced, never counted beside it. Once everything is staged, the peer rule judges,
//! once, every declared peer that touches a staged package: a staged extension's own peers, and
//! every locked extension's peers on a staged one. An unsatisfied peer is unified against the
//! registry the install came from (R-RES-006/005, ADR 0001) or, for a local file, refused with
//! E027. A peer that is not locked is `check`'s. Only then is each binary fetched from a
//! registry loaded, and it must declare what was published (ADR 0012). What passes is committed
//! as one change (ADR 0028 D4), whose last step pins the publisher keys its fetches accepted
//! (ADR 0059): a refused or failed plan pins nothing.

use std::path::Path;

use specforge_common::{Diagnostic, codes};
use specforge_installed::{Change, Installed, LockFile, LockSource, Module, Pin};
use specforge_package::PackageName;
use specforge_package::Version;
use specforge_package::peers::{Member, PeerRequirement, Peers, Verdict};
use specforge_protocol_types::{DeclaredCategory, ExtensionDeclaration, PeerDependency};
use specforge_wasm::WasmRuntime;

use super::Trust;
use super::candidate::{Installable, LocalFile};
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
}

impl Verb {
    fn participle(self) -> &'static str {
        match self {
            Verb::Install => "installing",
            Verb::Update => "updating",
        }
    }

    fn noun(self) -> &'static str {
        match self {
            Verb::Install => "install",
            Verb::Update => "update",
        }
    }
}

/// Where registry installs come from and how they are accepted: the registry port, the runtime
/// their binaries load in once the gate has passed, the lock whose keys they are held to, and the
/// caller's two trust choices.
#[derive(Clone, Copy)]
pub(crate) struct Fetch<'a> {
    pub(crate) registry: &'a dyn Registry,
    pub(crate) runtime: &'a dyn WasmRuntime,
    /// The project's installed extensions: the publisher key its lock records for a name is the
    /// pin when the user has none.
    pub(crate) installed: &'a Installed,
    pub(crate) allow_unsigned: bool,
    pub(crate) trust: Trust,
}

/// One install a lock plan stages: a module and the lock entry that pins it.
pub(crate) struct Install<'a> {
    module: Module,
    pin: Pin,
    from: From<'a>,
}

/// Where an install comes from, and what is left to check once the gate has passed.
enum From<'a> {
    /// A `.wasm` file: loaded and checked when it was read.
    File,
    /// A registry: `bytes` (a copy of the module) load after the gate, and must declare
    /// `declaration`.
    Registry {
        fetch: Fetch<'a>,
        declaration: Box<ExtensionDeclaration>,
        publisher: Publisher,
        bytes: Module,
    },
}

impl<'a> Fetch<'a> {
    /// `name@version` fetched as an install. The registry adapter checks integrity and the
    /// publisher signature under the TOFU policy. Its pin is read from the published declaration:
    /// its version and its peers, the publisher's key id, `source: registry`. Its binary is not
    /// loaded: [`LockPlan::check`] loads it after the gate. It is held to the key the lock records
    /// for `name` when the user has no pin of their own. Nothing is pinned for the user: the
    /// publisher the fetch decided is pinned when the plan commits.
    pub(crate) fn install(
        &self,
        name: &PackageName,
        version: &Version,
    ) -> Result<Install<'a>, OpError> {
        let policy = TrustPolicy::new(self.allow_unsigned, self.trust)
            .locked(self.installed.locked_key(name));
        let mut package = self.registry.fetch(name, version, policy)?;
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
                fetch: *self,
                declaration: Box::new(package.declaration),
                publisher: package.publisher,
                bytes: module.clone(),
            },
            module,
            pin,
        })
    }
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
        Ok(Install {
            module: local.binary.into_module(),
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
        self.module.digest()
    }

    pub(crate) fn source(&self) -> &LockSource {
        &self.pin.source
    }

    /// How the registry accepted its publisher; `None` for a local file.
    pub(crate) fn publisher(&self) -> Option<&Publisher> {
        match &self.from {
            From::Registry { publisher, .. } => Some(publisher),
            From::File => None,
        }
    }
}

/// What the plan keeps of an install once its module and pin are in the change.
struct Staged<'a> {
    name: PackageName,
    version: String,
    from: From<'a>,
}

impl Staged<'_> {
    /// The pin its registry fetch decided, once it is written; none for a local file, a key already
    /// pinned or an unsigned package.
    fn key_pin(&self) -> Option<KeyPin> {
        match &self.from {
            From::Registry { publisher, .. } => publisher.pin(&self.name),
            From::File => None,
        }
    }

    /// The registry an unsatisfied peer touching this install is unified against; `None` for a
    /// local file (E027).
    fn registry(&self) -> Option<&dyn Registry> {
        match &self.from {
            From::Registry { fetch, .. } => Some(fetch.registry),
            From::File => None,
        }
    }

    /// A fetched binary loaded in its runtime and held to what was published (ADR 0012). A local
    /// file was checked when it was read.
    fn load(&self) -> Result<(), OpError> {
        let From::Registry {
            fetch,
            declaration,
            bytes,
            ..
        } = &self.from
        else {
            return Ok(());
        };
        let binary = Installable::read(fetch.runtime, bytes.clone())?;
        let declared = binary.candidate();
        if declared.name() != self.name.as_str() || declared.version() != self.version {
            return Err(OpError::diagnostic(
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

/// The lock as the operation's installs leave it, staged in memory.
pub(crate) struct LockPlan<'a> {
    change: Change<'a>,
    staged: Vec<Staged<'a>>,
    verb: Verb,
    /// The registry port its registry installs came from: an operation fetches through one.
    registry: Option<&'a dyn Registry>,
}

impl<'a> LockPlan<'a> {
    /// A plan over the lock `installed` read. E033 when `specforge.lock` is there and can't be
    /// read: writing one would drop what it records.
    pub(crate) fn new(installed: &'a Installed, verb: Verb) -> Result<LockPlan<'a>, OpError> {
        Ok(LockPlan {
            change: installed.change()?,
            staged: Vec::new(),
            verb,
            registry: None,
        })
    }

    /// Stage `install`. Its entry replaces the package's previous one, if the lock has one.
    /// Nothing is judged yet.
    ///
    /// # Panics
    /// When a registry install comes through another registry port than one staged before: an
    /// operation passes one port, which routes each name itself (ADR 0045).
    pub(crate) fn stage(&mut self, install: Install<'a>) {
        let Install { module, pin, from } = install;
        if let From::Registry { fetch, .. } = &from {
            match self.registry {
                None => self.registry = Some(fetch.registry),
                Some(first) => assert!(
                    std::ptr::addr_eq(first, fetch.registry),
                    "a lock plan stages installs from one registry port"
                ),
            }
        }
        let staged = Staged {
            name: pin.name.clone(),
            version: pin.version.clone(),
            from,
        };
        self.change.install(module, pin);
        self.staged.push(staged);
    }

    /// Judge the plan, then load what it fetched.
    ///
    /// 1. The peer rule (`Peers::unsatisfied`) over the staged lock. It considers every declared
    ///    peer whose dependent or peer is staged, except a peer that is not locked (`check`'s
    ///    E027). E073 for a range that can't be read. For a peer out of range or not SemVer:
    ///    unified against the registry the staged install came from (R-RES-006 naming a version
    ///    every requirer accepts, R-RES-005 naming each requirer once), or E027 for a local file.
    ///    A locked extension a staged one breaks is named in the refusal: "installing '<package>'
    ///    <version> breaks '<dependent>': ...".
    /// 2. Each fetched install the gate did not refuse: its binary loaded in the fetch's runtime
    ///    (E028, `extension_conflict`), declaring the name and version it was published as
    ///    (E028) and the declaration it was published with (R-TRUST-004).
    ///
    /// What passes carries the pins its registry fetches decided (`Publisher::pin`), in staging
    /// order, for [`Checked::commit`] to save. Lock order decides nothing.
    pub(crate) fn check(self) -> Result<Checked<'a>, Refused> {
        let LockPlan {
            change,
            staged,
            verb,
            registry,
        } = self;
        let lock = change.lock();
        let changing: Vec<&str> = staged.iter().map(|s| s.name.as_str()).collect();
        let mut failed: Vec<Option<(OpError, bool)>> = staged.iter().map(|_| None).collect();
        for edge in unsatisfied(lock, &changing) {
            let owner = edge.owner(&changing);
            if failed[owner].is_none() {
                let error = refusal(lock, &staged, &changing, owner, &edge, verb);
                failed[owner] = Some((error, true));
            }
        }
        for (i, s) in staged.iter().enumerate() {
            if failed[i].is_none() {
                failed[i] = s.load().err().map(|error| (error, false));
            }
        }
        let (failures, judged): (Vec<(PackageName, OpError)>, Vec<bool>) = staged
            .iter()
            .zip(failed)
            .filter_map(|(s, f)| f.map(|(e, judged)| ((s.name.clone(), e), judged)))
            .unzip();
        if failures.is_empty() {
            let pins = staged.iter().filter_map(Staged::key_pin).collect();
            Ok(Checked {
                change,
                pins: registry.map(|registry| (registry, pins)),
            })
        } else {
            Err(Refused { failures, judged })
        }
    }
}

/// A lock plan the peer rule accepts and whose binaries all loaded as published: what may be
/// written, and the publisher keys to pin once it is.
pub(crate) struct Checked<'a> {
    change: Change<'a>,
    /// The pins its registry installs call for, and the registry port that pins them; none for a
    /// plan with no registry install.
    pins: Option<(&'a dyn Registry, Vec<KeyPin>)>,
}

impl Checked<'_> {
    /// Write it: each module placed, the lock written once (ADR 0028 D4), then the publisher keys
    /// pinned, last (ADR 0059). A failure at any step, the pins' included (R-TRUST-006), puts
    /// every file back and pins nothing. The error's writes name what could not be put back
    /// (normally nothing).
    pub(crate) fn commit(self) -> Result<Writes, OpError> {
        let Checked { change, pins } = self;
        change
            .commit_with(None, || record(pins))
            .map(|committed| Writes::of(committed.changed))
            .map_err(|failed| failed.error.with_writes(Writes::of(failed.left)))
    }

    /// [`Self::commit`] with `edit`, the `specforge.json` write at `config` that goes with it, run
    /// after the lock and before the pins. All or nothing, `edit` and the pins included.
    pub(crate) fn commit_with(
        self,
        config: &Path,
        edit: impl FnOnce() -> Result<(), OpError>,
    ) -> Result<Writes, OpError> {
        let Checked { change, pins } = self;
        change
            .commit_with(Some(config), || {
                edit()?;
                record(pins)
            })
            .map(|committed| Writes::of(committed.changed))
            .map_err(|failed| failed.error.with_writes(Writes::of(failed.left)))
    }
}

/// Pin what a committed plan's fetches decided, through the port that fetched them.
fn record(pins: Option<(&dyn Registry, Vec<KeyPin>)>) -> Result<(), OpError> {
    match pins {
        Some((registry, pins)) if !pins.is_empty() => registry.pin(&pins),
        _ => Ok(()),
    }
}

/// Why a lock plan was refused: each staged package that can't be installed, with its first
/// reason, in staging order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Refused {
    pub(crate) failures: Vec<(PackageName, OpError)>,
    /// For each failure, whether the peer rule refused it (else its binary did not load).
    judged: Vec<bool>,
}

impl Refused {
    /// What an operation installing a set all or nothing reports (`add` of several): the first
    /// install the peer rule refused, else the first whose binary did not load. A refusal the set
    /// earns is not hidden by a binary the plan would never have loaded.
    pub(crate) fn decisive(self) -> (PackageName, OpError) {
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

    /// The first failure: what an operation that stages one install reports.
    pub(crate) fn first(self) -> OpError {
        self.failures
            .into_iter()
            .next()
            .map(|(_, error)| error)
            .expect("a refused plan has a failure")
    }
}

/// A declared peer the staged lock leaves unsatisfied.
struct Edge<'l> {
    dependent: &'l str,
    declared: &'l PeerDependency,
    verdict: Verdict,
}

impl Edge<'_> {
    /// The staged install the refusal belongs to: the dependent when it is staged, else the peer.
    fn owner(&self, changing: &[&str]) -> usize {
        let at = |name: &str| changing.iter().position(|c| *c == name);
        at(self.dependent)
            .or_else(|| at(&self.declared.name))
            .expect("an edge touching the plan")
    }
}

/// The unsatisfied peers of `lock` that touch `changing`, in this order: a staged extension's own
/// peers first (in staging order, each in its declared order), then the locked extensions a staged
/// one breaks (in lock order). A peer that is not locked is left to `check` (ADR 0041).
fn unsatisfied<'l>(lock: &'l LockFile, changing: &[&str]) -> Vec<Edge<'l>> {
    let members = lock.entries.iter().map(|e| Member {
        name: e.name.as_str(),
        version: &e.version,
        peers: &e.peer_dependencies,
    });
    let mut edges: Vec<Edge<'l>> = Peers::of(members)
        .unsatisfied()
        .into_iter()
        .filter(|u| u.verdict != Verdict::Missing)
        .filter(|u| changing.contains(&u.dependent) || changing.contains(&u.declared.name.as_str()))
        .map(|u| Edge {
            dependent: u.dependent,
            declared: u.declared,
            verdict: u.verdict,
        })
        .collect();
    edges.sort_by_key(|e| {
        changing
            .iter()
            .position(|c| *c == e.dependent)
            .map_or((1, 0), |i| (0, i))
    });
    edges
}

/// Why `edge` refuses the install at `owner`.
fn refusal(
    lock: &LockFile,
    staged: &[Staged],
    changing: &[&str],
    owner: usize,
    edge: &Edge,
    verb: Verb,
) -> OpError {
    let peer = edge.declared.name.as_str();
    let error = match &edge.verdict {
        Verdict::Unreadable(why) => OpError::from(specforge_common::peers::unreadable(
            edge.dependent,
            edge.declared,
            why,
        )),
        Verdict::OutOfRange { installed } | Verdict::NotSemver { installed } => {
            match staged[owner].registry() {
                None => OpError::from(
                    specforge_common::peers::of(edge.dependent, edge.declared, &edge.verdict)
                        .expect("an unsatisfied peer is a diagnostic"),
                ),
                Some(registry) => {
                    // What the plan leaves the peer at: this operation's version, or the lock's.
                    let locked = if changing.contains(&peer) {
                        format!("this {} locks {installed}", verb.noun())
                    } else {
                        format!("{installed} is locked")
                    };
                    diamond(registry, lock, edge, &locked)
                }
            }
        }
        Verdict::Satisfied | Verdict::Missing => unreachable!("filtered by `unsatisfied`"),
    };
    if changing.contains(&edge.dependent) {
        error
    } else {
        let install = &staged[owner];
        error.prefixed(format!(
            "{} '{}' {} breaks '{}': ",
            verb.participle(),
            install.name,
            install.version,
            edge.dependent
        ))
    }
}

/// R-RES-006 naming the version of `edge`'s peer every requirer in `lock` accepts, or R-RES-005
/// (E073) when none does (ADR 0001).
fn diamond(registry: &dyn Registry, lock: &LockFile, edge: &Edge, locked: &str) -> OpError {
    let peer = edge.declared.name.as_str();
    let versions = match published_versions(registry, peer) {
        Ok(versions) => versions,
        Err(error) => return error,
    };
    match unify(peer, &versions, &requirers(lock, peer)) {
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
        Err(diagnostic) => OpError::from(diagnostic),
    }
}

/// Every entry of `lock` that declares a peer on `peer`, with that declaration, in lock order.
/// Each requirer appears once: the staged entry replaced the previous one.
fn requirers<'l>(lock: &'l LockFile, peer: &str) -> Vec<(&'l str, &'l PeerDependency)> {
    lock.entries
        .iter()
        .flat_map(|e| {
            e.peer_dependencies
                .iter()
                .filter(move |p| p.name == peer)
                .map(move |p| (e.name.as_str(), p))
        })
        .collect()
}

/// The versions `registry` publishes of `peer`. E072 when `peer` is no package name.
fn published_versions(registry: &dyn Registry, peer: &str) -> Result<Vec<String>, OpError> {
    let name = PackageName::parse(peer)
        .map_err(|why| OpError::from(specforge_common::package::invalid(&why)))?;
    Ok(registry
        .versions(&name)?
        .iter()
        .map(Version::to_string)
        .collect())
}

/// Unify a version diamond: several requirers each declare a SemVer range for the same package.
/// Pick the highest of its published `versions` that satisfies every requirer's range
/// (intersection, not backtracking: when none does, R-RES-005 names each requirer; a range that
/// is not SemVer is E073).
fn unify(
    name: &str,
    versions: &[String],
    requirers: &[(&str, &PeerDependency)],
) -> Result<String, Diagnostic> {
    let mut reqs = Vec::with_capacity(requirers.len());
    for (requirer, declared) in requirers {
        let requirement = PeerRequirement::read(declared)
            .map_err(|why| specforge_common::peers::unreadable(requirer, declared, &why))?;
        reqs.push((*requirer, declared.version.as_str(), requirement));
    }

    let mut candidates: Vec<Version> = versions
        .iter()
        .filter_map(|v| Version::parse(v).ok())
        .collect();
    candidates.sort();

    let unified = candidates.into_iter().rev().find(|v| {
        reqs.iter()
            .all(|(_, _, requirement)| requirement.accepts(&v.to_string()) == Some(true))
    });

    unified.map(|v| v.to_string()).ok_or_else(|| {
        let wanted = reqs
            .iter()
            .map(|(requirer, range, _)| format!("{requirer} wants {name} {range}"))
            .collect::<Vec<_>>()
            .join("; ");
        Diagnostic::new(
            codes::R_RES_005,
            format!(
                "version diamond for '{}': no single version satisfies every requirer ({}). Available: {}",
                name,
                wanted,
                versions.join(", ")
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

/// `locals` staged over the lock at `root`.
fn staged_locals<'a>(
    installed: &'a Installed,
    root: &Path,
    locals: Vec<LocalFile>,
) -> Result<LockPlan<'a>, OpError> {
    let mut plan = LockPlan::new(installed, Verb::Install)?;
    for local in locals {
        plan.stage(Install::local(root, local)?);
    }
    Ok(plan)
}

/// Install `locals`, read once by `init`'s plan, into the project at `root` as one change,
/// without reading them again. No writes when `locals` is empty.
pub(crate) fn install_local(root: &Path, locals: Vec<LocalFile>) -> Result<Writes, OpError> {
    if locals.is_empty() {
        return Ok(Writes::none());
    }
    let installed = Installed::at(root);
    staged_locals(&installed, root, locals)?
        .check()
        .map_err(Refused::first)?
        .commit()
}

/// Whether [`install_local`] would accept `locals` over the lock at `root`. `init`'s plan asks
/// this before anything is written (E033, E072, E073, E027).
pub(crate) fn check_local(root: &Path, locals: &[LocalFile]) -> Result<(), OpError> {
    if locals.is_empty() {
        return Ok(());
    }
    let installed = Installed::at(root);
    staged_locals(&installed, root, locals.to_vec())?
        .check()
        .map(drop)
        .map_err(Refused::first)
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::entry;
    use super::*;
    use crate::registry::testing::{Asked, MemoryRegistry, Published, declaration};
    use crate::testing::declaring;
    use specforge_installed::{LockFileEntry, LockState};
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

    fn fetch<'a>(
        registry: &'a MemoryRegistry,
        runtime: &'a InProcessRuntime,
        installed: &'a Installed,
    ) -> Fetch<'a> {
        Fetch {
            registry,
            runtime,
            installed,
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

    fn refused(plan: LockPlan<'_>) -> OpError {
        plan.check().map(drop).unwrap_err().first()
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
        let mut plan = LockPlan::new(&installed, Verb::Install).unwrap();
        plan.stage(
            fetch(&registry, &runtime, &installed)
                .install(
                    &PackageName::parse("@acme/app").unwrap(),
                    &Version::new(1, 0, 0),
                )
                .unwrap(),
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
        let mut plan = LockPlan::new(&installed, Verb::Install).unwrap();
        plan.stage(
            fetch(&registry, &runtime, &installed)
                .install(
                    &PackageName::parse("@acme/app").unwrap(),
                    &Version::new(1, 0, 0),
                )
                .unwrap(),
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
        let mut plan = LockPlan::new(&installed, Verb::Install).unwrap();
        plan.stage(local(&runtime, ONE_ISH, "app.wasm"));

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
        let mut plan = LockPlan::new(&installed, Verb::Install).unwrap();
        plan.stage(
            fetch(&registry, &runtime, &installed)
                .install(
                    &PackageName::parse("@acme/app").unwrap(),
                    &Version::new(1, 0, 0),
                )
                .unwrap(),
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
        let mut plan = LockPlan::new(&installed, Verb::Install).unwrap();
        plan.stage(local(&runtime, APP_NEEDS_BASE_2, "app.wasm"));

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
        let mut plan = LockPlan::new(&installed, Verb::Install).unwrap();
        plan.stage(
            fetch(&registry, &runtime, &installed)
                .install(
                    &PackageName::parse("@acme/app").unwrap(),
                    &Version::new(1, 0, 0),
                )
                .unwrap(),
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
        let mut plan = LockPlan::new(&installed, Verb::Install).unwrap();
        plan.stage(
            fetch(&registry, &runtime, &installed)
                .install(
                    &PackageName::parse("@acme/app").unwrap(),
                    &Version::new(1, 0, 0),
                )
                .unwrap(),
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
        let mut plan = LockPlan::new(&installed, Verb::Install).unwrap();
        plan.stage(
            fetch(&registry, &runtime, &installed)
                .install(
                    &PackageName::parse("@acme/app").unwrap(),
                    &Version::new(1, 0, 0),
                )
                .unwrap(),
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
        let mut plan = LockPlan::new(&installed, Verb::Install).unwrap();
        plan.stage(
            fetch(&registry, &runtime, &installed)
                .install(
                    &PackageName::parse("@acme/app").unwrap(),
                    &Version::new(2, 0, 0),
                )
                .unwrap(),
        );

        // The staged entry replaced the previous one: base has one requirer, at ^2.
        let listed = requirers(plan.change.lock(), "@acme/base");
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
        let mut plan = LockPlan::new(&installed, Verb::Install).unwrap();
        plan.stage(local(&runtime, BASE, "base.wasm"));

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
        let mut plan = LockPlan::new(&installed, Verb::Update).unwrap();
        plan.stage(local(&runtime, BASE, "base.wasm"));

        let err = refused(plan);

        assert!(
            err.message
                .starts_with("updating '@acme/base' 2.0.0 breaks '@acme/app': "),
            "{err:?}"
        );
    }

    #[test]
    fn a_peer_that_is_not_locked_is_left_to_check() {
        let installed = nowhere(Vec::new());
        let runtime = runtime();
        let mut plan = LockPlan::new(&installed, Verb::Install).unwrap();
        plan.stage(local(&runtime, APP_NEEDS_BASE_2, "app.wasm"));

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
        let mut plan = LockPlan::new(&installed, Verb::Install).unwrap();
        plan.stage(local(&runtime, APP, "app.wasm"));

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
            let mut plan = LockPlan::new(&installed, Verb::Update).unwrap();
            let (app, base) = (
                local(&runtime, APP_NEEDS_BASE_2, "app.wasm"),
                local(&runtime, BASE, "base.wasm"),
            );
            if app_first {
                plan.stage(app);
                plan.stage(base);
            } else {
                plan.stage(base);
                plan.stage(app);
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
        let mut plan = LockPlan::new(&installed, Verb::Install).unwrap();
        plan.stage(local(&runtime, APP_NEEDS_BASE_2, "app.wasm"));

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
        let mut plan = LockPlan::new(&installed, Verb::Install).unwrap();
        plan.stage(
            fetch(&registry, &runtime, &installed)
                .install(
                    &PackageName::parse("@acme/app").unwrap(),
                    &Version::new(1, 0, 0),
                )
                .unwrap(),
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
        std::fs::write(dir.path().join("specforge.json"), "{\"extensions\":[]}").unwrap();
        dir
    }

    /// `@acme/app` 1.0.0 fetched from `registry` and staged over the lock in `installed`.
    fn stage_app<'a>(
        registry: &'a MemoryRegistry,
        runtime: &'a InProcessRuntime,
        installed: &'a Installed,
    ) -> LockPlan<'a> {
        let mut plan = LockPlan::new(installed, Verb::Install).unwrap();
        plan.stage(
            fetch(registry, runtime, installed)
                .install(
                    &PackageName::parse("@acme/app").unwrap(),
                    &Version::new(1, 0, 0),
                )
                .unwrap(),
        );
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
        let mut plan = LockPlan::new(&installed, Verb::Install).unwrap();
        plan.stage(
            fetch(&refused_app, &runtime, &installed)
                .install(&app(), &Version::new(1, 0, 0))
                .unwrap(),
        );
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
        checked.commit().unwrap();
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
            .commit()
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
            .commit_with(&config, || {
                std::fs::write(&config, "{\"extensions\":[\"@acme/app\"]}").unwrap();
                Ok(())
            })
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
            .commit()
            .unwrap();
        assert_eq!(registry.pins(), [(app(), "k1".to_string())].into());

        // Unsigned, accepted with allow_unsigned.
        let dir = base_project();
        let installed = Installed::at(dir.path());
        let registry = app_registry(None);
        stage_app(&registry, &runtime, &installed)
            .check()
            .unwrap()
            .commit()
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
        let mut plan = LockPlan::new(&installed, Verb::Install).unwrap();
        plan.stage(
            fetch(&registry, &runtime, &installed)
                .install(
                    &PackageName::parse("@acme/app").unwrap(),
                    &Version::new(1, 0, 0),
                )
                .unwrap(),
        );

        let err = refused(plan);

        assert!(err.is(METADATA_MISMATCH), "{err:?}");
    }

    #[test]
    fn refused_names_each_staged_package_once_in_staging_order() {
        // The app wants base ^3 and the base breaks `other`: both are refused, once each.
        let installed = nowhere(base_and_other("^1.0"));
        let runtime = runtime();
        let mut plan = LockPlan::new(&installed, Verb::Install).unwrap();
        plan.stage(local(&runtime, APP_NEEDS_BASE_3, "app.wasm"));
        plan.stage(local(&runtime, BASE, "base.wasm"));

        let refused = plan.check().map(drop).unwrap_err();

        let names: Vec<String> = refused
            .failures
            .iter()
            .map(|(name, _)| name.to_string())
            .collect();
        assert_eq!(names, ["@acme/app", "@acme/base"]);
    }

    fn versions(vs: &[&str]) -> Vec<String> {
        vs.iter().map(|v| v.to_string()).collect()
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
        unify("@shared/lib", &versions(vs), &borrowed)
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
