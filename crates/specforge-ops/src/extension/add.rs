//! `specforge add` and `specforge.add_extension`.

use super::candidate::LocalFile;
use super::lock_plan::{Ask, Asked, Fetching, Install, LockPlan, Purpose};
use super::{Origin, builtin_name};
use crate::registry::{Acceptance, Publisher, Registry};
use crate::{OpError, OpErrorKind, Writes};
use serde::Serialize;
use specforge_common::codes;
use specforge_common::shape::Shape;
use specforge_installed::Installed;
use specforge_package::SpecifierError;
use specforge_package::{PackageName, PackageRef};
use specforge_wasm::WasmRuntime;
use std::path::PathBuf;

/// Where an extension to add comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A builtin, embedded in the binary.
    Builtin(&'static str),
    /// A `.wasm` file on disk.
    Local(PathBuf),
    /// A registry package and the version asked for (`latest` when none).
    Registry(PackageRef),
    /// A git repository (not supported yet: E064).
    Git { url: String },
}

/// Read an `add` argument once (ADR 0036): a builtin name, a `.wasm` path
/// (or any `./`, `../` or `/` path), a `git+<url>`, or a package reference
/// `@scope/name[@requirement]`. Anything else is E054; a requirement that
/// is none is R-RES-003. No registry is asked for either.
pub fn parse(specifier: &str) -> Result<Source, OpError> {
    let specifier = specifier.trim();
    if specifier.is_empty() {
        return Err(OpError::coded(
            crate::OpErrorKind::InvalidInput,
            codes::E054,
            "empty extension specifier",
        )
        .with_suggestion(SPECIFIER_FORMS));
    }
    if let Some(builtin) = builtin_name(specifier) {
        return Ok(Source::Builtin(builtin));
    }
    if let Some(url) = specifier.strip_prefix("git+") {
        // A `#rev` names a revision; a git source is not installable yet.
        let url = url.rfind('#').map_or(url, |hash| &url[..hash]);
        return Ok(Source::Git {
            url: url.to_string(),
        });
    }
    if specifier.ends_with(".wasm")
        || ["./", "../", "/"]
            .iter()
            .any(|prefix| specifier.starts_with(prefix))
    {
        return Ok(Source::Local(PathBuf::from(specifier)));
    }
    Ok(Source::Registry(PackageRef::parse(specifier)?))
}

/// What an `add` argument may be, as the suggestion of E054.
const SPECIFIER_FORMS: &str =
    "use a builtin's name, './local/path.wasm', 'git+https://...', or '@scope/name[@version]'";

/// E054 for a package reference that is not one, R-RES-003 for a
/// requirement that is none.
impl From<SpecifierError> for OpError {
    fn from(error: SpecifierError) -> Self {
        match error {
            SpecifierError::Requirement(_) => OpError::coded(
                crate::OpErrorKind::InvalidInput,
                codes::R_RES_003,
                error.to_string(),
            )
            .with_suggestion(
                "use a version such as 1.2.0, a requirement such as ^1.0, ~2.3, 1.x or \
                     >=1.0.0, <2.0.0, or latest",
            ),
            _ => OpError::coded(
                crate::OpErrorKind::InvalidInput,
                codes::E054,
                format!("invalid extension specifier: {error}"),
            )
            .with_suggestion(SPECIFIER_FORMS),
        }
    }
}

/// One extension to add: its specifier as the caller gave it, and the source it reads as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Specified {
    pub specifier: String,
    pub source: Source,
}

impl Specified {
    /// This specifier with a relative local path read from `base`: a surface with no working
    /// directory of its own (MCP) reads such a path from the project's root.
    pub fn relative_to(mut self, base: &std::path::Path) -> Self {
        if let Source::Local(path) = &mut self.source
            && path.is_relative()
        {
            *path = base.join(&*path);
        }
        self
    }
}

/// A source as its specifier would spell it.
impl From<Source> for Specified {
    fn from(source: Source) -> Self {
        Specified {
            specifier: source.to_string(),
            source,
        }
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Builtin(name) => f.write_str(name),
            Source::Local(path) => write!(f, "{}", path.display()),
            Source::Registry(package) => write!(f, "{package}"),
            Source::Git { url } => write!(f, "git+{url}"),
        }
    }
}

impl Source {
    /// Whether adding it installs a module under `.specforge/extensions/` and locks it.
    fn installs(&self) -> bool {
        matches!(self, Source::Local(_) | Source::Registry(_))
    }
}

/// Read each of `specifiers`, in order, as [`parse`] reads it. E054 when there is none; the
/// first that can't be read refuses, named when there are several.
pub fn specified<S: AsRef<str>>(specifiers: &[S]) -> Result<Vec<Specified>, OpError> {
    if specifiers.is_empty() {
        return Err(no_specifier());
    }
    let several = specifiers.len() > 1;
    specifiers
        .iter()
        .map(|specifier| {
            let specifier = specifier.as_ref();
            parse(specifier)
                .map(|source| Specified {
                    specifier: specifier.to_string(),
                    source,
                })
                .map_err(|error| named(several, specifier, error))
        })
        .collect()
}

/// E054: an add with nothing to add.
fn no_specifier() -> OpError {
    OpError::coded(
        crate::OpErrorKind::InvalidInput,
        codes::E054,
        "no extension specifier",
    )
    .with_suggestion(SPECIFIER_FORMS)
}

/// `error`, naming the specifier it refuses when the add has several.
fn named(several: bool, specifier: &str, error: OpError) -> OpError {
    if several {
        error.prefixed(format!("adding '{specifier}': "))
    } else {
        error
    }
}

/// How a key change in a signed package is decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    /// Refuse it (MCP, `--format json`): nobody can be asked.
    Refuse,
    /// Accept it (`--yes`).
    AssumeYes,
    /// Ask on the terminal, refusing when there is none.
    Prompt,
}

/// What to add, and where.
#[derive(Debug, Clone)]
pub struct AddRequest<'a> {
    pub root: &'a specforge_common::ProjectRoot,
    /// The extensions to add, in the order given: one change.
    pub extensions: Vec<Specified>,
    /// Accept a registry package with no publisher signature.
    pub allow_unsigned: bool,
    pub trust: Trust,
    /// The add's lock plan judged and not committed: report what the add would do, or the refusal it
    /// would give; write, pin and download nothing. A registry package is judged by the
    /// declaration it was published with (its manifest). An extension already present answers as
    /// the real add would (`AlreadyPresent`, or `Builtin { changed: false }`).
    pub dry_run: bool,
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

impl AddedExtension {
    /// Its entry in the add document.
    pub fn document(&self) -> AddResult {
        let specifier = self.specifier.clone();
        match &self.outcome {
            AddOutcome::Builtin {
                name,
                changed,
                peers_enabled,
            } => AddResult::Enabled(EnabledBuiltin {
                specifier,
                extension: (*name).to_string(),
                source: BuiltinSource::Builtin,
                changed: *changed,
                peers_enabled: peers_enabled
                    .iter()
                    .map(|peer| (*peer).to_string())
                    .collect(),
            }),
            AddOutcome::Installed {
                name,
                version,
                sha256,
                publisher,
                origin,
                peers_enabled,
            } => AddResult::Installed(InstalledPackage {
                specifier,
                extension: name.clone(),
                version: version.clone(),
                sha256: sha256.clone(),
                source: origin.source(),
                key_id: publisher
                    .as_ref()
                    .and_then(Publisher::key_id)
                    .map(str::to_string),
                publisher: publisher.as_ref().map(Publisher::acceptance),
                peers_enabled: peers_enabled
                    .iter()
                    .map(|peer| (*peer).to_string())
                    .collect(),
            }),
            AddOutcome::AlreadyPresent { name, version } => {
                AddResult::AlreadyPresent(PresentExtension {
                    specifier,
                    extension: name.clone(),
                    version: version.clone(),
                })
            }
            AddOutcome::Planned {
                name,
                version,
                origin,
                peers_enabled,
            } => AddResult::Planned(PlannedAdd {
                specifier,
                extension: name.clone(),
                version: version.clone(),
                source: origin.source(),
                peers_enabled: peers_enabled
                    .iter()
                    .map(|peer| (*peer).to_string())
                    .collect(),
            }),
            AddOutcome::Duplicate { name, first } => AddResult::Duplicate(DuplicateSpecifier {
                specifier,
                extension: name.clone(),
                duplicate_of: first.clone(),
            }),
        }
    }
}

/// What `specforge add --format json` and `specforge.add_extension` print
/// (`McpAddExtensionResult`): what the add did for each specifier.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct AddDocument {
    /// One entry per specifier, in the order given.
    pub added: Vec<AddResult>,
}

/// What the add did for one specifier, by `action` (`McpAddedExtension`).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AddResult {
    /// A builtin enabled in `specforge.json` (`changed: false` when it
    /// already was), after the required builtin peers it lists.
    Enabled(EnabledBuiltin),
    /// A package installed under `.specforge/extensions/` and locked.
    Installed(InstalledPackage),
    /// Already installed at this version and enabled: nothing changed.
    AlreadyPresent(PresentExtension),
    /// A dry run: what would be installed or enabled.
    Planned(PlannedAdd),
    /// An extension an earlier specifier of the same add names: added once.
    Duplicate(DuplicateSpecifier),
}

/// The one spelling of a builtin's `source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
pub enum BuiltinSource {
    #[serde(rename = "builtin")]
    Builtin,
}

/// A builtin enabled.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct EnabledBuiltin {
    pub specifier: String,
    pub extension: String,
    pub source: BuiltinSource,
    pub changed: bool,
    /// The required builtin peers enabled first.
    pub peers_enabled: Vec<String>,
}

/// A package installed and locked.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct InstalledPackage {
    pub specifier: String,
    pub extension: String,
    pub version: String,
    pub sha256: String,
    /// `registry`, or `local:<path>` for a local file ([`Origin::source`]).
    pub source: String,
    /// The publisher key id; absent for an unsigned package or a local file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
    /// How the publisher was accepted; absent for a local file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<Acceptance>,
    /// The required builtins enabled with it, dependencies first.
    pub peers_enabled: Vec<String>,
}

/// An extension already installed and enabled.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct PresentExtension {
    pub specifier: String,
    pub extension: String,
    pub version: String,
}

/// What a dry run would install or enable.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct PlannedAdd {
    pub specifier: String,
    pub extension: String,
    /// Absent for a builtin, which tracks the binary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// `builtin`, `registry` or `local:<path>`.
    pub source: String,
    /// The required builtins the add would enable with it, dependencies first.
    pub peers_enabled: Vec<String>,
}

/// A specifier naming an extension an earlier one of the same add names.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct DuplicateSpecifier {
    pub specifier: String,
    pub extension: String,
    /// The earlier specifier that adds it.
    pub duplicate_of: String,
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

impl Added {
    /// The add document: [`AddedExtension::document`] for each specifier.
    pub fn document(&self) -> AddDocument {
        AddDocument {
            added: self.added.iter().map(AddedExtension::document).collect(),
        }
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
) -> Result<Added, OpError> {
    // The project must exist, with a config the writer can edit, before
    // anything is installed into it: the refusal `update` and `remove`
    // give an unusable specforge.json too (`config_invalid`).
    let config = crate::config::required(req.root.dir())?.config;
    if req.extensions.is_empty() {
        return Err(no_specifier());
    }
    let several = req.extensions.len() > 1;
    let installed = Installed::at(req.root.dir());
    // A specforge.lock that can't be read refuses before a file is read or a registry asked.
    let plan = LockPlan::adding(
        &installed,
        &config.extensions,
        runtime,
        Fetching {
            registry,
            allow_unsigned: req.allow_unsigned,
            trust: req.trust,
        },
        if req.dry_run {
            Purpose::Preview
        } else {
            Purpose::Commit
        },
        req.extensions.iter().any(|e| e.source.installs()),
    )?;
    let mut resolver = Resolver {
        req,
        config: &config,
        installed: &installed,
        registry,
        runtime,
        claims: Vec::new(),
    };
    let mut steps = Vec::with_capacity(req.extensions.len());
    for (at, wanted) in req.extensions.iter().enumerate() {
        let step = resolver
            .step(&req.extensions[..at], wanted)
            .map_err(|error| match error {
                Resolved::Refused(error) => named(several, &wanted.specifier, error),
                Resolved::Conflict(error) => error,
            })?;
        steps.push(step);
    }
    apply(req, &config, plan, steps)
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

/// An extension a specifier of the add names, and how it adds it.
struct Claim {
    name: String,
    /// What makes two specifiers add it the same way: `builtin`, the registry version, or the
    /// local file's SHA-256.
    key: String,
    /// How the refusal of a conflict says it: `builtin`, `<version> from <source>`.
    shown: String,
    specifier: String,
}

/// Why a specifier can't be resolved: refused on its own (named when the add has several), or in
/// conflict with an earlier one (the refusal names both).
enum Resolved {
    Refused(OpError),
    Conflict(OpError),
}

impl From<OpError> for Resolved {
    fn from(error: OpError) -> Self {
        Resolved::Refused(error)
    }
}

/// Resolves each specifier in turn, remembering what the earlier ones claimed.
struct Resolver<'r, 'a> {
    req: &'r AddRequest<'r>,
    config: &'r specforge_common::ProjectConfig,
    installed: &'r Installed,
    registry: &'a dyn Registry,
    runtime: &'a dyn WasmRuntime,
    claims: Vec<Claim>,
}

impl<'a> Resolver<'_, 'a> {
    /// What the add does for `wanted`, given the specifiers before it.
    fn step(&mut self, before: &[Specified], wanted: &Specified) -> Result<Step<'a>, Resolved> {
        match &wanted.source {
            Source::Builtin(name) => self.builtin(wanted, name),
            Source::Local(path) => {
                let local = LocalFile::read(self.runtime, path)?;
                self.local(wanted, Install::local(self.req.root.dir(), local)?)
            }
            Source::Registry(package) => self.package(before, wanted, package),
            Source::Git { url } => Err(Resolved::Refused(OpError::coded(
                crate::OpErrorKind::InvalidInput,
                codes::E064,
                format!("git source '{url}' not yet supported"),
            ))),
        }
    }

    /// `name` claimed by `wanted` as `key`: `None` the first time, the duplicate when an earlier
    /// specifier claimed it the same way, the conflict when differently.
    fn claim(
        &mut self,
        wanted: &Specified,
        name: &str,
        key: String,
        shown: String,
    ) -> Result<Option<Step<'a>>, Resolved> {
        match self.claims.iter().find(|claim| claim.name == name) {
            Some(first) if first.key == key => Ok(Some(Step::Unchanged(AddOutcome::Duplicate {
                name: name.to_string(),
                first: first.specifier.clone(),
            }))),
            Some(first) => Err(Resolved::Conflict(
                OpError::new(
                    OpErrorKind::Conflict,
                    "extension_conflict",
                    format!(
                        "'{}' and '{}' both add '{name}', as {} and as {shown}",
                        first.specifier, wanted.specifier, first.shown
                    ),
                )
                .with_suggestion(format!("add '{name}' once")),
            )),
            None => {
                self.claims.push(Claim {
                    name: name.to_string(),
                    key,
                    shown,
                    specifier: wanted.specifier.clone(),
                });
                Ok(None)
            }
        }
    }

    fn builtin(&mut self, wanted: &Specified, name: &'static str) -> Result<Step<'a>, Resolved> {
        if let Some(duplicate) = self.claim(wanted, name, "builtin".into(), "builtin".into())? {
            return Ok(duplicate);
        }
        // An already-enabled extension is left exactly as it is, a dry run included.
        if super::enabled_builtins(self.config).contains(&name) {
            return Ok(Step::Unchanged(AddOutcome::Builtin {
                name,
                changed: false,
                peers_enabled: Vec::new(),
            }));
        }
        Ok(Step::Enable { name })
    }

    fn local(&mut self, wanted: &Specified, install: Install<'a>) -> Result<Step<'a>, Resolved> {
        let shown = format!("{} from {}", install.version(), install.source());
        let name = install.name().clone();
        if let Some(duplicate) =
            self.claim(wanted, name.as_str(), install.sha256().to_string(), shown)?
        {
            return Ok(duplicate);
        }
        if let Some(present) = already_present(self.installed, self.config, &name, |e| {
            e.wasm_hash == install.sha256() && e.source.local_path().is_some()
        }) {
            return Ok(Step::Unchanged(present));
        }
        Ok(Step::Local(install))
    }

    fn package(
        &mut self,
        before: &[Specified],
        wanted: &Specified,
        package: &PackageRef,
    ) -> Result<Step<'a>, Resolved> {
        let name = package.name.as_str();
        // The same package reference again resolves the same way: nothing is asked.
        if before.iter().any(|earlier| earlier.source == wanted.source)
            && let Some(first) = self.claims.iter().find(|claim| claim.name == name)
        {
            return Ok(Step::Unchanged(AddOutcome::Duplicate {
                name: name.to_string(),
                first: first.specifier.clone(),
            }));
        }
        let version = super::resolve(self.registry, package)?;
        let shown = format!("{version} from registry");
        if let Some(duplicate) = self.claim(wanted, name, format!("registry {version}"), shown)? {
            return Ok(duplicate);
        }
        if let Some(present) = already_present(self.installed, self.config, &package.name, |e| {
            e.version == version.to_string() && e.source.is_registry()
        }) {
            return Ok(Step::Unchanged(present));
        }
        Ok(Step::Fetch(package.name.clone(), version))
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
    config: &specforge_common::ProjectConfig,
    mut plan: LockPlan<'a>,
    steps: Vec<Step<'a>>,
) -> Result<Added, OpError> {
    let several = req.extensions.len() > 1;
    let specifier = |at: usize| req.extensions[at].specifier.as_str();
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
                    .map_err(|error| named(several, specifier(at), error))?;
                asked.push((at, Wanted::of(&install)));
                plan.install(ask, install);
            }
        }
    }
    if asked.is_empty() {
        return Ok(Added {
            added: added(req, outcomes),
            writes: Writes::none(),
            extensions_enabled: config.extensions.len(),
        });
    }

    let done = plan
        .check()
        .map_err(|refused| {
            let (Ask(at), error) = refused.decisive();
            named(several, specifier(at), error)
        })?
        .finish(req.root.dir())?;
    for (at, wanted) in asked {
        outcomes[at] = Some(wanted.outcome(done.asked(Ask(at)), req.dry_run));
    }
    Ok(Added {
        added: added(req, outcomes),
        extensions_enabled: done.enabled,
        writes: done.writes,
    })
}

/// Each specifier of `req` with its outcome.
fn added(req: &AddRequest, outcomes: Vec<Option<AddOutcome>>) -> Vec<AddedExtension> {
    req.extensions
        .iter()
        .zip(outcomes)
        .map(|(wanted, outcome)| AddedExtension {
            specifier: wanted.specifier.clone(),
            outcome: outcome.expect("every specifier has an outcome"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{
        declaration_of, enabled_project, entry, greet, installed, project,
    };
    use super::*;
    use crate::OpErrorKind;
    use crate::registry::testing::{MemoryRegistry, Published, declaration};
    use specforge_installed::LockFileEntry;
    use specforge_protocol_types::ExtensionDeclaration;
    use specforge_registry::PeerDependency;
    use specforge_test_macros::test as specforge_test;
    use std::path::Path;

    use crate::testing::{self, GREET, declaring, serving_builtin};
    use specforge_wasm::testing::InProcessRuntime;

    /// What the tests serve in process: `@sdk/greet` and `@test/probe`.
    fn runtime() -> InProcessRuntime {
        testing::candidates()
    }

    #[test]
    fn parse_names_each_source() {
        assert_eq!(
            parse("@specforge/product"),
            Ok(Source::Builtin("@specforge/product"))
        );
        assert_eq!(
            parse("ext/greet.wasm"),
            Ok(Source::Local(PathBuf::from("ext/greet.wasm")))
        );
        assert_eq!(
            parse("./ext/dir"),
            Ok(Source::Local(PathBuf::from("./ext/dir")))
        );
        assert_eq!(
            parse("@acme/tool@^1.2"),
            Ok(Source::Registry(
                PackageRef::parse("@acme/tool@^1.2").unwrap()
            ))
        );
        assert_eq!(
            parse("git+https://example.com/x.git"),
            Ok(Source::Git {
                url: "https://example.com/x.git".into()
            })
        );
        assert_eq!(parse("").unwrap_err().code, "E054");
        assert_eq!(parse("not-scoped").unwrap_err().code, "E054");
        assert_eq!(parse("@acme/").unwrap_err().code, "E054");
    }

    /// What `parse` makes of every input of plan 12's table (§2.2): a
    /// package reference, or the code of the refusal. Nothing reaches a
    /// registry that is not a scoped package name with a requirement.
    #[specforge_test(
        behavior = "parse_extension_specifier",
        verify = "each add argument reads as one extension source"
    )]
    fn parse_reads_each_input() {
        fn registry(reference: &str) -> Result<Source, String> {
            Ok(Source::Registry(PackageRef::parse(reference).unwrap()))
        }
        let cases: Vec<(&str, Result<Source, String>)> = vec![
            ("@acme/tool", registry("@acme/tool")),                 // I1
            ("@acme/tool@", Err("E054".into())),                    // I2
            ("@acme/tool@1.2.0", registry("@acme/tool@1.2.0")),     // I3
            ("@acme/tool@^1.2", registry("@acme/tool@^1.2")),       // I4
            ("@acme/tool@1.x", registry("@acme/tool@1.x")),         // I5
            ("@acme/tool@1.2", registry("@acme/tool@1.2")),         // I6
            ("@acme/tool@1.0.0/x", Err("R-RES-003".into())),        // I7
            ("@acme/tool@1.0.0?x=1", Err("R-RES-003".into())),      // I8
            ("foo@/bar", Err("E054".into())),                       // I9
            ("tool@1.0.0", Err("E054".into())),                     // I10
            ("tool", Err("E054".into())),                           // I11
            ("@acme/..", Err("E054".into())),                       // I12
            ("@acme/aa/bb", Err("E054".into())),                    // I13
            ("@acme/a/b", Err("E054".into())),                      // I14
            ("@a/x", registry("@a/x")),                             // I15
            ("@acme/T ool", Err("E054".into())),                    // I16
            ("Acme@1", Err("E054".into())),                         // I17
            ("@acme/tool@latest", registry("@acme/tool")),          // I18
            ("@acme/tool@*", registry("@acme/tool")),               // I18
            ("@acme/tool@>=1, <2", registry("@acme/tool@>=1, <2")), // I19
            ("@acme/tool@^bogus", Err("R-RES-003".into())),         // I20
            (
                "@acme/tool@2.0.0+build.1",
                registry("@acme/tool@2.0.0+build.1"),
            ), // I21
            ("@scope", Err("E054".into())),                         // I22
            (
                "@specforge/software",
                Ok(Source::Builtin("@specforge/software")),
            ), // I24
            (
                "git+https://h/r#v",
                Ok(Source::Git {
                    url: "https://h/r".into(),
                }),
            ), // I25
            (" @acme/tool ", registry("@acme/tool")),               // I26
        ];
        for (input, want) in cases {
            let got = parse(input).map_err(|error| error.code.to_string());
            assert_eq!(got, want, "{input:?}");
        }
    }

    #[specforge_test(
        behavior = "management_operations_over_the_project_view",
        verify = "add, update and remove refuse an unusable specforge.json with one refusal, before they write"
    )]
    fn an_unusable_config_is_refused_before_anything_is_installed() {
        use crate::config::testing::{UNUSABLE, files_under};

        for config in UNUSABLE {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("specforge.json"), config).unwrap();
            let before = files_under(dir.path());
            let read = specforge_common::read_project_config(dir.path());
            let refused = crate::config::refusal(&read.problems[0]);

            for source in [
                Source::Builtin("@specforge/product"),
                // A file that is not there: the config is refused first.
                Source::Local(dir.path().join("missing.wasm")),
                Source::Registry(PackageRef::parse("@acme/tool").unwrap()),
            ] {
                for dry_run in [false, true] {
                    let request = AddRequest {
                        root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                        extensions: vec![source.clone().into()],
                        allow_unsigned: false,
                        trust: Trust::Refuse,
                        dry_run,
                    };
                    let error = add(&request, &crate::registry::Unconfigured("add"), &runtime())
                        .unwrap_err();

                    assert_eq!(error, refused, "{config}: {source:?}");
                    assert_eq!(files_under(dir.path()), before, "{config}: {source:?}");
                }
            }
        }
    }

    #[specforge_test(
        behavior = "install_wasm_extension",
        verify = "an install of an extension whose binary changed after install replaces it"
    )]
    fn add_reinstalls_a_binary_that_changed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("specforge.json"),
            r#"{"name": "p", "version": "0.1.0", "extensions": []}"#,
        )
        .unwrap();
        let files = tempfile::tempdir().unwrap();
        let greet = files.path().join("greet.wasm");
        std::fs::write(&greet, GREET).unwrap();
        let request = AddRequest {
            root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            extensions: vec![Source::Local(greet).into()],
            allow_unsigned: false,
            trust: Trust::Refuse,
            dry_run: false,
        };
        let unconfigured = crate::registry::Unconfigured("add");
        add(&request, &unconfigured, &runtime()).unwrap();
        let module = dir
            .path()
            .join(".specforge/extensions/@sdk/greet/extension.wasm");
        let pinned = std::fs::read(&module).unwrap();
        let mut changed = pinned.clone();
        changed.extend_from_slice(b"changed after install");
        std::fs::write(&module, &changed).unwrap();

        let added = add(&request, &unconfigured, &runtime()).unwrap();

        assert!(
            matches!(added.added[0].outcome, AddOutcome::Installed { .. }),
            "{:?}",
            added.added[0].outcome
        );
        assert_eq!(std::fs::read(&module).unwrap(), pinned);
        assert_eq!(added.writes.len(), 1, "only the module differed");

        // Now that it is the pinned binary again, adding it is a no-op.
        let again = add(&request, &unconfigured, &runtime()).unwrap();
        assert!(matches!(
            again.added[0].outcome,
            AddOutcome::AlreadyPresent { .. }
        ));
    }

    /// A project directory whose `specforge.json` holds `config` verbatim.
    fn project_with(config: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("specforge.json"), config).unwrap();
        dir
    }

    const EMPTY_PROJECT: &str = r#"{"name": "p", "version": "0.1.0", "extensions": []}"#;

    fn local_request<'a>(root: &'a specforge_common::ProjectRoot, file: &Path) -> AddRequest<'a> {
        AddRequest {
            root,
            extensions: vec![Source::Local(file.to_path_buf()).into()],
            allow_unsigned: false,
            trust: Trust::Refuse,
            dry_run: false,
        }
    }

    fn builtin_request<'a>(
        root: &'a specforge_common::ProjectRoot,
        name: &'static str,
    ) -> AddRequest<'a> {
        AddRequest {
            root,
            extensions: vec![Source::Builtin(name).into()],
            allow_unsigned: false,
            trust: Trust::Refuse,
            dry_run: false,
        }
    }

    #[test]
    fn add_refuses_a_binary_that_claims_a_builtins_name() {
        use crate::config::testing::files_under;
        let dir = project_with(EMPTY_PROJECT);
        let file = tempfile::tempdir().unwrap();
        let wasm = file.path().join("product.wasm");
        std::fs::write(&wasm, b"\0asm impostor").unwrap();
        let impostor = runtime().binary(
            b"\0asm impostor",
            declaring("@specforge/product", "9.9.9", &[]),
        );
        let before = files_under(dir.path());

        let error = add(
            &local_request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                &wasm,
            ),
            &crate::registry::Unconfigured("add"),
            &impostor,
        )
        .unwrap_err();

        assert_eq!(error.code, "extension_conflict");
        assert_eq!(error.kind, OpErrorKind::Conflict);
        assert_eq!(
            error.message,
            "the extension declares the name of the builtin '@specforge/product'"
        );
        assert_eq!(
            error.suggestion.as_deref(),
            Some("enable the builtin instead: specforge add @specforge/product")
        );
        assert_eq!(files_under(dir.path()), before);
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "add unresolvable extension rejects with diagnostic"
    )]
    fn a_local_file_add_cannot_read_is_e054() {
        use crate::config::testing::files_under;
        let dir = project_with(EMPTY_PROJECT);
        let registry = crate::registry::Unconfigured("add");

        let before = files_under(dir.path());
        let missing = dir.path().join("missing.wasm");
        let error = add(
            &local_request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                &missing,
            ),
            &registry,
            &runtime(),
        )
        .unwrap_err();
        assert_eq!(error.code, "E054");
        assert_eq!(
            error.message,
            format!("file not found: {}", missing.display())
        );
        assert_eq!(files_under(dir.path()), before);
        assert_eq!(files_under(dir.path()), before);
    }

    #[test]
    fn an_empty_directory_is_no_extension_crate() {
        use crate::config::testing::files_under;
        let dir = project_with(EMPTY_PROJECT);
        let before = files_under(dir.path());
        let scratch = tempfile::tempdir().unwrap();
        let directory = scratch.path().join("a-directory");
        std::fs::create_dir(&directory).unwrap();

        let error = add(
            &local_request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                &directory,
            ),
            &crate::registry::Unconfigured("add"),
            &runtime(),
        )
        .unwrap_err();

        assert_eq!(error.code, "E040");
        assert!(
            error.message.starts_with("no built component in"),
            "{}",
            error.message
        );
        assert_eq!(files_under(dir.path()), before);
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "a local crate directory installs the component its crate builds"
    )]
    fn a_crate_directory_installs_its_built_component() {
        let dir = project_with(EMPTY_PROJECT);
        let scratch = tempfile::tempdir().unwrap();
        let krate = scratch.path().join("greet-ext");
        let release = krate.join("target/wasm32-wasip2/release");
        std::fs::create_dir_all(&release).unwrap();
        std::fs::write(
            krate.join("Cargo.toml"),
            "[package]\nname = \"greet-ext\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let component = release.join("greet_ext.wasm");
        std::fs::write(&component, GREET).unwrap();

        let added = add(
            &local_request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                &krate,
            ),
            &crate::registry::Unconfigured("add"),
            &runtime(),
        )
        .unwrap();

        assert!(
            matches!(added.added[0].outcome, AddOutcome::Installed { .. }),
            "{:?}",
            added.added[0].outcome
        );
        let lock = std::fs::read_to_string(dir.path().join("specforge.lock")).unwrap();
        assert!(
            lock.contains("local:")
                && lock.contains("greet-ext/target/wasm32-wasip2/release/greet_ext.wasm\""),
            "{lock}"
        );
    }

    #[test]
    fn a_local_file_that_is_no_extension_is_e028() {
        use crate::config::testing::files_under;
        let dir = project_with(EMPTY_PROJECT);
        let file = tempfile::tempdir().unwrap();
        let wasm = file.path().join("junk.wasm");
        std::fs::write(&wasm, b"not wasm").unwrap();
        let before = files_under(dir.path());

        let error = add(
            &local_request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                &wasm,
            ),
            &crate::registry::Unconfigured("add"),
            &InProcessRuntime::new(),
        )
        .unwrap_err();

        assert_eq!(error.code, "E028");
        assert!(
            error
                .message
                .starts_with("not a loadable SpecForge extension:"),
            "{}",
            error.message
        );
        assert!(
            error
                .suggestion
                .as_deref()
                .unwrap()
                .contains("specforge-extension-sdk")
        );
        assert_eq!(files_under(dir.path()), before);
    }

    #[specforge_test(
        behavior = "install_wasm_extension",
        verify = "an install over an unreadable specforge.lock is refused before anything is written"
    )]
    fn an_unreadable_lock_refuses_before_the_local_file_is_read() {
        let dir = project_with(EMPTY_PROJECT);
        std::fs::write(dir.path().join("specforge.lock"), "not a lock {{{").unwrap();
        let missing = dir.path().join("missing.wasm");

        let error = add(
            &local_request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                &missing,
            ),
            &crate::registry::Unconfigured("add"),
            &runtime(),
        )
        .unwrap_err();

        assert_eq!(error.code, "E033");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("specforge.lock")).unwrap(),
            "not a lock {{{"
        );
    }

    /// `runtime()`, also serving the builtins `@specforge/cargo-test` (which
    /// requires `@specforge/testing`) and `@specforge/testing`.
    fn runtime_with_cargo_test() -> InProcessRuntime {
        serving_builtin(
            serving_builtin(
                runtime(),
                "@specforge/cargo-test",
                declaring(
                    "@specforge/cargo-test",
                    "1.0.0",
                    &[("@specforge/testing", "^1.0", false)],
                ),
            ),
            "@specforge/testing",
            declaring("@specforge/testing", "1.0.0", &[]),
        )
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "add enables a builtin's required peers but not its optional ones"
    )]
    fn a_builtin_is_enabled_after_its_required_builtin_peers() {
        let dir = project_with(EMPTY_PROJECT);

        let added = add(
            &builtin_request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                "@specforge/cargo-test",
            ),
            &crate::registry::Unconfigured("add"),
            &runtime_with_cargo_test(),
        )
        .unwrap();

        assert_eq!(
            added.added[0].outcome,
            AddOutcome::Builtin {
                name: "@specforge/cargo-test",
                changed: true,
                peers_enabled: vec!["@specforge/testing"],
            }
        );
        assert_eq!(added.writes.names_under(dir.path()), ["specforge.json"]);
        assert_eq!(
            specforge_common::read_project_config(dir.path())
                .config
                .extensions,
            ["@specforge/testing", "@specforge/cargo-test"]
        );
        assert_eq!(added.extensions_enabled, 2);
    }

    #[test]
    fn extensions_enabled_counts_the_string_entries_after_the_add() {
        let dir =
            project_with(r#"{"name":"p","version":"0.1.0","extensions":[1,"@specforge/rust"]}"#);
        let registry = crate::registry::Unconfigured("add");
        let project = specforge_common::ProjectRoot::of(dir.path()).unwrap();
        let request = builtin_request(&project, "@specforge/product");
        let runtime = serving_builtin(
            serving_builtin(
                runtime(),
                "@specforge/product",
                declaring("@specforge/product", "1.0.0", &[]),
            ),
            "@specforge/rust",
            declaring("@specforge/rust", "1.0.0", &[]),
        );

        let added = add(&request, &registry, &runtime).unwrap();
        assert_eq!(added.extensions_enabled, 2);

        let again = add(&request, &registry, &runtime).unwrap();
        assert!(matches!(
            again.added[0].outcome,
            AddOutcome::Builtin { changed: false, .. }
        ));
        assert!(again.writes.is_empty());
        assert_eq!(again.extensions_enabled, 2);
    }

    #[specforge_test(
        behavior = "install_wasm_extension",
        verify = "add, init and publish read a candidate's declaration in the runtime their surface passes"
    )]
    fn a_candidate_is_read_in_the_runtime_the_caller_passes() {
        let dir = project_with(EMPTY_PROJECT);
        let files = tempfile::tempdir().unwrap();
        let greet = files.path().join("greet.wasm");
        std::fs::write(&greet, GREET).unwrap();
        let runtime = runtime();

        add(
            &local_request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                &greet,
            ),
            &crate::registry::Unconfigured("add"),
            &runtime,
        )
        .unwrap();

        let handshakes: Vec<_> = runtime
            .calls()
            .into_iter()
            .filter(|call| call.extension == "__candidate" && call.export == "__handshake")
            .collect();
        assert_eq!(handshakes.len(), 1, "{handshakes:?}");
        assert!(
            !specforge_wasm::WasmRuntime::unload(&runtime, "__candidate"),
            "the candidate is left unloaded"
        );
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "a builtin whose declaration cannot be read is refused before anything is written"
    )]
    fn a_builtin_whose_declaration_cannot_be_read_is_refused() {
        use crate::config::testing::files_under;
        let dir = project_with(EMPTY_PROJECT);
        let before = files_under(dir.path());

        let error = add(
            &builtin_request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                "@specforge/cargo-test",
            ),
            &crate::registry::Unconfigured("add"),
            &InProcessRuntime::new(),
        )
        .unwrap_err();

        assert_eq!(error.code, "E028");
        assert!(
            error
                .message
                .starts_with("the builtin '@specforge/cargo-test' does not load: "),
            "{}",
            error.message
        );
        assert_eq!(files_under(dir.path()), before);
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "add enables the builtins a required builtin peer requires, dependencies first"
    )]
    fn add_enables_what_a_required_builtin_requires_first() {
        let dir = project_with(EMPTY_PROJECT);
        let registry = crate::registry::Unconfigured("add");
        let chain = serving_builtin(
            serving_builtin(
                serving_builtin(
                    runtime(),
                    "@specforge/formal",
                    declaring(
                        "@specforge/formal",
                        "1.0.0",
                        &[("@specforge/software", "^1.0", false)],
                    ),
                ),
                "@specforge/software",
                declaring(
                    "@specforge/software",
                    "1.0.0",
                    &[("@specforge/product", "^1.0", false)],
                ),
            ),
            "@specforge/product",
            declaring("@specforge/product", "1.0.0", &[]),
        );

        let added = add(
            &builtin_request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                "@specforge/formal",
            ),
            &registry,
            &chain,
        )
        .unwrap();

        assert_eq!(
            added.added[0].outcome,
            AddOutcome::Builtin {
                name: "@specforge/formal",
                changed: true,
                peers_enabled: vec!["@specforge/product", "@specforge/software"],
            }
        );
        assert_eq!(
            specforge_common::read_project_config(dir.path())
                .config
                .extensions,
            [
                "@specforge/product",
                "@specforge/software",
                "@specforge/formal"
            ]
        );
        assert_eq!(added.writes.names_under(dir.path()), ["specforge.json"]);

        // A cycle among required builtins stops the walk: the registry build
        // reports it, and the add succeeds.
        let cyclic_dir = project_with(EMPTY_PROJECT);
        let cycle = serving_builtin(
            serving_builtin(
                runtime(),
                "@specforge/formal",
                declaring(
                    "@specforge/formal",
                    "1.0.0",
                    &[("@specforge/software", "^1.0", false)],
                ),
            ),
            "@specforge/software",
            declaring(
                "@specforge/software",
                "1.0.0",
                &[("@specforge/formal", "^1.0", false)],
            ),
        );
        let added = add(
            &builtin_request(
                &specforge_common::ProjectRoot::of(cyclic_dir.path()).unwrap(),
                "@specforge/formal",
            ),
            &registry,
            &cycle,
        )
        .unwrap();
        assert!(matches!(
            added.added[0].outcome,
            AddOutcome::Builtin { changed: true, .. }
        ));
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "a required builtin peer the embedded builtin does not satisfy is refused before anything is written"
    )]
    fn a_required_builtin_peer_the_embedded_one_does_not_satisfy_is_refused() {
        use crate::config::testing::files_under;
        let dir = project_with(EMPTY_PROJECT);
        let before = files_under(dir.path());
        // formal wants software ^2.0; the software this specforge embeds is 1.0.0. (Init also
        // enables testing beside software.)
        let runtime = serving_builtin(
            serving_builtin(
                serving_builtin(
                    runtime(),
                    "@specforge/formal",
                    declaring(
                        "@specforge/formal",
                        "1.0.0",
                        &[("@specforge/software", "^2.0", false)],
                    ),
                ),
                "@specforge/software",
                declaring("@specforge/software", "1.0.0", &[]),
            ),
            "@specforge/testing",
            declaring("@specforge/testing", "1.0.0", &[]),
        );

        let error = add(
            &builtin_request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                "@specforge/formal",
            ),
            &crate::registry::Unconfigured("add"),
            &runtime,
        )
        .unwrap_err();

        assert_eq!(error.code, "E027");
        assert_eq!(
            error.message,
            "extension '@specforge/formal' requires peer dependency '@specforge/software' ^2.0 but version 1.0.0 is installed"
        );
        assert_eq!(
            error.suggestion.as_deref(),
            Some("this specforge build's builtin extensions disagree: reinstall specforge")
        );
        assert_eq!(files_under(dir.path()), before);

        // init refuses the same way, before it writes anything.
        let target = tempfile::tempdir().unwrap();
        let extensions = vec!["@specforge/formal".to_string()];
        let error = crate::init::plan(
            &crate::init::Request {
                dir: &target.path().join("demo"),
                name: Some("demo"),
                version: crate::init::DEFAULT_VERSION,
                extensions: &extensions,
            },
            &runtime,
        )
        .unwrap_err();
        assert_eq!(error.code, "E027");
    }

    /// The project's peer set (ADR 0041, amendment round 7) is what an add is judged against.
    /// These helpers serve the probe extensions of `testing::probes`.
    fn wasm_file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    /// A registry publishing `bytes` as the package they declare.
    fn registry_publishing(bytes: &[u8]) -> MemoryRegistry {
        let declared = crate::publish::declare(&testing::probes(), bytes)
            .unwrap()
            .0;
        MemoryRegistry::new().serving(Published::new(bytes.to_vec(), declared))
    }

    /// Add `source` to the project at `dir`, allowing unsigned packages.
    fn add_one(
        dir: &Path,
        source: Source,
        registry: &dyn Registry,
    ) -> Result<Added, crate::OpError> {
        let root = specforge_common::ProjectRoot::of(dir).unwrap();
        add(
            &AddRequest {
                root: &root,
                extensions: vec![source.into()],
                allow_unsigned: true,
                trust: Trust::Refuse,
                dry_run: false,
            },
            registry,
            &testing::probes(),
        )
    }

    fn package(name: &str) -> Source {
        Source::Registry(PackageRef::parse(name).unwrap())
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "add judges the project's peer set as the add leaves it: its builtins, installed extensions and .wasm file entries"
    )]
    fn an_install_requiring_a_builtin_out_of_range_is_refused_from_any_source() {
        use crate::config::testing::files_under;
        let files = tempfile::tempdir().unwrap();
        let config = r#"{"name": "p", "version": "0.1.0", "extensions": ["@specforge/software"]}"#;
        let local = Source::Local(wasm_file(
            files.path(),
            "needs.wasm",
            testing::NEEDS_SOFTWARE_9,
        ));
        let registry = registry_publishing(testing::NEEDS_SOFTWARE_9);
        for (source, registry) in [
            (
                local,
                &crate::registry::Unconfigured("add") as &dyn Registry,
            ),
            (package("@probe/needs-software"), &registry),
        ] {
            let dir = project_with(config);
            let before = files_under(dir.path());

            let error = add_one(dir.path(), source, registry).unwrap_err();

            assert_eq!(error.code, "E027", "{error:?}");
            assert_eq!(
                error.message,
                "extension '@probe/needs-software' requires peer dependency '@specforge/software' ^9.0 but version 1.0.0 is installed"
            );
            assert_eq!(
                error.suggestion.as_deref(),
                Some(
                    "'@specforge/software' is the builtin this specforge embeds, at 1.0.0: install a version of '@probe/needs-software' that accepts it"
                )
            );
            assert_eq!(files_under(dir.path()), before);
        }
        let software = PackageName::parse("@specforge/software").unwrap();
        assert!(
            !registry
                .asked()
                .contains(&crate::registry::testing::Asked::Versions(software)),
            "a builtin is never unified: {:?}",
            registry.asked()
        );
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "an install whose required peer is not installed is refused with E027 before anything is written"
    )]
    fn an_install_whose_required_peer_is_not_installed_is_refused() {
        use crate::config::testing::files_under;
        let files = tempfile::tempdir().unwrap();
        let local = Source::Local(wasm_file(files.path(), "dep.wasm", testing::DEP_1));
        let registry = registry_publishing(testing::DEP_1);
        for (source, registry) in [
            (
                local,
                &crate::registry::Unconfigured("add") as &dyn Registry,
            ),
            (package("@probe/dep"), &registry),
        ] {
            let dir = project_with(EMPTY_PROJECT);
            let before = files_under(dir.path());

            let error = add_one(dir.path(), source, registry).unwrap_err();

            assert_eq!(error.code, "E027", "{error:?}");
            assert_eq!(
                error.message,
                "extension '@probe/dep' requires peer dependency '@probe/base' ^1.0 which is not installed"
            );
            assert_eq!(
                error.suggestion.as_deref(),
                Some("install it with: specforge add @probe/base")
            );
            assert_eq!(files_under(dir.path()), before);
        }
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "enabling a builtin that an enabled extension's peer range does not accept is refused with E027"
    )]
    fn enabling_a_builtin_an_enabled_extension_rejects_is_refused() {
        use crate::config::testing::files_under;
        // `@probe/needs-software` installed by hand, requiring software ^9.
        let mut locked = entry("@probe/needs-software", "1.0.0", "local:needs.wasm", &[]);
        locked.wasm_hash = specforge_installed::hex_sha256(testing::NEEDS_SOFTWARE_9);
        locked.peer_dependencies = vec![PeerDependency {
            name: "@specforge/software".to_string(),
            version: "^9.0".to_string(),
            optional: false,
        }];
        let dir = project(vec![locked]);
        std::fs::write(
            installed(dir.path(), "@probe/needs-software"),
            testing::NEEDS_SOFTWARE_9,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("specforge.json"),
            r#"{"name": "p", "version": "0.1.0", "extensions": ["@probe/needs-software"]}"#,
        )
        .unwrap();
        let before = files_under(dir.path());

        let error = add_one(
            dir.path(),
            Source::Builtin("@specforge/software"),
            &crate::registry::Unconfigured("add"),
        )
        .unwrap_err();

        assert_eq!(error.code, "E027", "{error:?}");
        assert!(
            error.message.starts_with(
                "enabling '@specforge/software' 1.0.0 breaks '@probe/needs-software': "
            ),
            "{}",
            error.message
        );
        assert_eq!(files_under(dir.path()), before);
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "an install that breaks a .wasm file entry, or that a .wasm file entry does not satisfy, is refused with E027"
    )]
    fn an_install_that_breaks_a_file_entry_or_that_a_file_entry_does_not_satisfy_is_refused() {
        use crate::config::testing::files_under;
        let files = tempfile::tempdir().unwrap();
        let unconfigured = crate::registry::Unconfigured("add");

        // (a) `dep.wasm` wants `@probe/base` ^1; installing base 2.0.0 breaks it.
        let dir = project_with(r#"{"name": "p", "version": "0.1.0", "extensions": ["dep.wasm"]}"#);
        wasm_file(dir.path(), "dep.wasm", testing::DEP_1);
        let base = Source::Local(wasm_file(files.path(), "base.wasm", testing::BASE_2));
        let before = files_under(dir.path());
        let error = add_one(dir.path(), base, &unconfigured).unwrap_err();
        assert_eq!(error.code, "E027", "{error:?}");
        assert!(
            error
                .message
                .starts_with("installing '@probe/base' 2.0.0 breaks '@probe/dep': "),
            "{}",
            error.message
        );
        assert_eq!(files_under(dir.path()), before);

        // (b) `base.wasm` is 2.0.0; the dep installed beside it wants ^1.
        let dir = project_with(r#"{"name": "p", "version": "0.1.0", "extensions": ["base.wasm"]}"#);
        wasm_file(dir.path(), "base.wasm", testing::BASE_2);
        let dep = Source::Local(wasm_file(files.path(), "dep.wasm", testing::DEP_1));
        let before = files_under(dir.path());
        let error = add_one(dir.path(), dep, &unconfigured).unwrap_err();
        assert_eq!(error.code, "E027", "{error:?}");
        assert_eq!(
            error.message,
            "extension '@probe/dep' requires peer dependency '@probe/base' ^1.0 but version 2.0.0 is installed"
        );
        assert_eq!(files_under(dir.path()), before);
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "an install enables the builtins it requires first, whatever its source"
    )]
    fn an_install_enables_the_builtins_it_requires_first() {
        let files = tempfile::tempdir().unwrap();
        let local = Source::Local(wasm_file(
            files.path(),
            "needs.wasm",
            testing::NEEDS_SOFTWARE_1,
        ));
        let registry = registry_publishing(testing::NEEDS_SOFTWARE_1);
        for (source, registry) in [
            (
                local,
                &crate::registry::Unconfigured("add") as &dyn Registry,
            ),
            (package("@probe/needs-software"), &registry),
        ] {
            let dir = project_with(EMPTY_PROJECT);

            let added = add_one(dir.path(), source, registry).unwrap();

            assert!(
                matches!(
                    &added.added[0].outcome,
                    AddOutcome::Installed { peers_enabled, .. }
                        if peers_enabled == &["@specforge/software"]
                ),
                "{:?}",
                added.added[0].outcome
            );
            let config = specforge_common::read_project_config(dir.path()).config;
            assert_eq!(
                config.extensions,
                ["@specforge/software", "@probe/needs-software"]
            );
            assert!(
                added
                    .writes
                    .names_under(dir.path())
                    .contains(&"specforge.json".to_string()),
                "{:?}",
                added.writes
            );
        }
    }

    #[test]
    fn add_reads_specforge_json_once_before_its_edit() {
        let dir = project_with(
            r#"{"name": "p", "version": "0.1.0", "sentinel": {"keep": [1, 2]}, "extensions": []}"#,
        );
        let files = tempfile::tempdir().unwrap();
        let greet = files.path().join("greet.wasm");
        std::fs::write(&greet, GREET).unwrap();

        let added = add(
            &local_request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                &greet,
            ),
            &crate::registry::Unconfigured("add"),
            &runtime(),
        )
        .unwrap();

        assert!(matches!(
            added.added[0].outcome,
            AddOutcome::Installed { .. }
        ));
        assert_eq!(added.extensions_enabled, 1);
        let config: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join("specforge.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(config["sentinel"], serde_json::json!({"keep": [1, 2]}));
        assert_eq!(config["extensions"], serde_json::json!(["@sdk/greet"]));
    }

    #[cfg(unix)]
    #[specforge_test(
        behavior = "install_wasm_extension",
        verify = "a failed install puts back the binary, specforge.lock and specforge.json it changed"
    )]
    fn a_failed_builtin_edit_enables_no_peer() {
        use std::os::unix::fs::PermissionsExt;
        let dir = project_with(EMPTY_PROJECT);
        let config = dir.path().join("specforge.json");
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o444)).unwrap();
        let before = std::fs::read(&config).unwrap();

        let error = add(
            &builtin_request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                "@specforge/cargo-test",
            ),
            &crate::registry::Unconfigured("add"),
            &runtime_with_cargo_test(),
        )
        .unwrap_err();
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert_eq!(error.code, "config_write_failed", "{error:?}");
        assert!(error.writes.is_empty());
        assert_eq!(std::fs::read(&config).unwrap(), before);
    }

    #[test]
    fn a_missing_config_is_config_not_found_with_the_hint_to_init() {
        let dir = tempfile::tempdir().unwrap();
        let request = AddRequest {
            root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            extensions: vec![Source::Builtin("@specforge/product").into()],
            allow_unsigned: false,
            trust: Trust::Refuse,
            dry_run: false,
        };

        let error = add(&request, &crate::registry::Unconfigured("add"), &runtime()).unwrap_err();

        assert_eq!(error.code, "config_not_found");
        assert!(error.suggestion.unwrap().contains("specforge init"));
        assert!(!dir.path().join("specforge.json").exists());
    }

    /// A project enabling nothing yet, with `lock` locked.
    /// A project locking `lock`, with a `specforge.json` enabling none of it: adding a locked
    /// package installs it again.
    fn disabled_project(lock: Vec<LockFileEntry>) -> tempfile::TempDir {
        let dir = project(lock);
        std::fs::write(
            dir.path().join("specforge.json"),
            r#"{"name": "p", "version": "0.1.0", "extensions": []}"#,
        )
        .unwrap();
        dir
    }

    /// A project locking `lock`, with a `specforge.json` enabling every locked extension.
    fn add_project(lock: Vec<LockFileEntry>) -> tempfile::TempDir {
        enabled_project(lock)
    }

    fn add_greet(root: &Path, registry: &MemoryRegistry) -> Result<AddOutcome, OpError> {
        add(
            &AddRequest {
                root: &specforge_common::ProjectRoot::of(root).unwrap(),
                extensions: vec![
                    Source::Registry(PackageRef::parse("@sdk/greet@0.1.0").unwrap()).into(),
                ],
                allow_unsigned: true,
                trust: Trust::Refuse,
                dry_run: false,
            },
            registry,
            &runtime(),
        )
        .map(|added| added.added[0].outcome.clone())
    }

    fn with_peer(
        mut declaration: ExtensionDeclaration,
        name: &str,
        range: &str,
    ) -> ExtensionDeclaration {
        declaration
            .handshake
            .peer_dependencies
            .push(PeerDependency {
                name: name.to_string(),
                version: range.to_string(),
                optional: false,
            });
        declaration
    }

    #[specforge_test(
        behavior = "check_registry_reply",
        verify = "add refuses a package whose binary declares other than its published declaration"
    )]
    fn a_binary_that_declares_other_than_its_published_declaration_is_refused() {
        // R4/R5: the published declaration differs from the binary's, in
        // its handshake (another description) or in a category (no kinds).
        // A served declaration is trusted for nothing the binary doesn't
        // declare: the package is refused before anything is installed.
        let mut description = declaration_of(&greet());
        description.handshake.description = Some("Something else".to_string());
        for published in [description, {
            let mut kinds = declaration_of(&greet());
            kinds.entities.clear();
            kinds
        }] {
            let dir = add_project(Vec::new());
            let registry = MemoryRegistry::new().serving(Published::new(greet(), published));
            let err = add_greet(dir.path(), &registry).unwrap_err();
            assert!(err.is(crate::registry::METADATA_MISMATCH), "{err:?}");
            assert!(err.message.contains("@sdk/greet@0.1.0"), "{err:?}");
            assert!(
                !installed(dir.path(), "@sdk/greet").exists(),
                "nothing is installed"
            );
        }
        // The message names the first part that differs.
        let mut kinds = declaration_of(&greet());
        kinds.entities.clear();
        let dir = add_project(Vec::new());
        let registry = MemoryRegistry::new().serving(Published::new(greet(), kinds));
        let err = add_greet(dir.path(), &registry).unwrap_err();
        assert!(err.message.contains("another entities"), "{err:?}");
        // The published declaration equal to the binary's installs.
        let dir = add_project(Vec::new());
        let registry =
            MemoryRegistry::new().serving(Published::new(greet(), declaration_of(&greet())));
        add_greet(dir.path(), &registry).unwrap();
    }

    #[specforge_test(
        behavior = "check_registry_reply",
        verify = "the diamond gate decides on the published declaration's peers"
    )]
    fn the_diamond_gate_reads_the_published_declarations_peers() {
        // R4: the published declaration names a peer @acme/x ^2 that the
        // lock holds at 1.0.0. The gate refuses before the binary (which
        // declares no peer) is loaded.
        let dir = add_project(vec![entry("@acme/x", "1.0.0", "registry", &[])]);
        let registry = MemoryRegistry::new()
            .serving(Published::new(
                greet(),
                with_peer(declaration_of(&greet()), "@acme/x", "^2"),
            ))
            .serving(Published::new(
                b"\0asm x".to_vec(),
                declaration("@acme/x", "1.0.0", &[]),
            ));
        let err = add_greet(dir.path(), &registry).unwrap_err();
        assert!(!err.is(crate::registry::METADATA_MISMATCH), "{err:?}");
        assert!(err.code.starts_with("R-RES"), "{err:?}");
        assert!(err.message.contains("@acme/x"), "{err:?}");
    }

    #[test]
    fn adding_an_installed_exact_version_asks_the_registry_nothing() {
        let dir = add_project(Vec::new());
        let registry =
            MemoryRegistry::new().serving(Published::new(greet(), declaration_of(&greet())));
        let first = add_greet(dir.path(), &registry).unwrap();
        assert!(matches!(first, AddOutcome::Installed { .. }), "{first:?}");
        let asked = registry.asked();
        assert_eq!(asked.len(), 1, "{asked:?}");

        // The same version again: installed and enabled, so nothing is asked.
        let again = add_greet(dir.path(), &registry).unwrap();
        assert!(
            matches!(again, AddOutcome::AlreadyPresent { .. }),
            "{again:?}"
        );
        assert_eq!(
            registry.asked(),
            asked,
            "the registry was asked nothing more"
        );
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "an add that upgrades a locked package judges its peers against the lock as the add leaves it"
    )]
    fn an_add_that_upgrades_a_locked_package_judges_its_peers_without_its_previous_entry() {
        let dir = add_project(vec![
            entry("@x/b", "1.0.0", "registry", &[]),
            entry("@x/a", "1.0.0", "registry", &[("@x/b", "^1")]),
        ]);
        let registry = MemoryRegistry::new()
            .serving(Published::new(
                b"\0asm b1".to_vec(),
                declaration("@x/b", "1.0.0", &[]),
            ))
            .serving(Published::new(
                b"\0asm b2".to_vec(),
                declaration("@x/b", "2.0.0", &[]),
            ))
            .serving(Published::new(
                b"\0asm a2".to_vec(),
                declaration("@x/a", "2.0.0", &[("@x/b", "^2")]),
            ));
        let lock_before = std::fs::read(dir.path().join("specforge.lock")).unwrap();

        let error = add(
            &AddRequest {
                root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                extensions: vec![Source::Registry(PackageRef::parse("@x/a@2.0.0").unwrap()).into()],
                allow_unsigned: true,
                trust: Trust::Refuse,
                dry_run: false,
            },
            &registry,
            &runtime(),
        )
        .unwrap_err();

        assert_eq!(error.code, "R-RES-006", "{error:?}");
        assert_eq!(
            error.message,
            "version diamond: '@x/a' requires peer '@x/b' ^2 but 1.0.0 is locked; @x/b 2.0.0 would satisfy every requirer"
        );
        assert_eq!(
            std::fs::read(dir.path().join("specforge.lock")).unwrap(),
            lock_before
        );
        assert_eq!(
            registry.asked(),
            [
                crate::registry::testing::Asked::Fetch(
                    PackageName::parse("@x/a").unwrap(),
                    specforge_package::Version::new(2, 0, 0)
                ),
                crate::registry::testing::Asked::Versions(PackageName::parse("@x/b").unwrap()),
            ]
        );
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "a version diamond names each requirer once"
    )]
    fn a_version_diamond_names_each_requirer_once() {
        let dir = add_project(vec![entry(
            "@acme/app",
            "1.0.0",
            "registry",
            &[("@sdk/greet", "^2.0")],
        )]);
        let registry =
            MemoryRegistry::new().serving(Published::new(greet(), declaration_of(&greet())));

        let error = add_greet(dir.path(), &registry).unwrap_err();

        assert_eq!(error.code, "R-RES-005", "{error:?}");
        assert!(
            error
                .message
                .starts_with("installing '@sdk/greet' 0.1.0 breaks '@acme/app': "),
            "{error:?}"
        );
        assert_eq!(
            error
                .message
                .matches("@acme/app wants @sdk/greet ^2.0")
                .count(),
            1,
            "{error:?}"
        );
        assert!(
            !installed(dir.path(), "@sdk/greet").exists(),
            "nothing is installed"
        );
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "a dry run of an extension already present answers as the add would, whatever its source"
    )]
    fn a_dry_run_of_a_present_extension_answers_as_the_add_would() {
        fn dry(root: &specforge_common::ProjectRoot, source: Source) -> AddRequest<'_> {
            AddRequest {
                root,
                extensions: vec![source.into()],
                allow_unsigned: true,
                trust: Trust::Refuse,
                dry_run: true,
            }
        }

        // A registry install, present: already present first.
        let dir = add_project(Vec::new());
        let registry =
            MemoryRegistry::new().serving(Published::new(greet(), declaration_of(&greet())));
        add_greet(dir.path(), &registry).unwrap();
        let source = Source::Registry(PackageRef::parse("@sdk/greet@0.1.0").unwrap());
        let outcome = add(
            &dry(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                source,
            ),
            &registry,
            &runtime(),
        )
        .unwrap()
        .added[0]
            .outcome
            .clone();
        assert_eq!(
            outcome,
            AddOutcome::AlreadyPresent {
                name: "@sdk/greet".into(),
                version: "0.1.0".into()
            }
        );

        // A local install, present: already present too.
        let dir = project_with(EMPTY_PROJECT);
        let files = tempfile::tempdir().unwrap();
        let wasm = files.path().join("greet.wasm");
        std::fs::write(&wasm, GREET).unwrap();
        let unconfigured = crate::registry::Unconfigured("add");
        add(
            &local_request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                &wasm,
            ),
            &unconfigured,
            &runtime(),
        )
        .unwrap();
        let outcome = add(
            &dry(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                Source::Local(wasm),
            ),
            &unconfigured,
            &runtime(),
        )
        .unwrap()
        .added[0]
            .outcome
            .clone();
        assert_eq!(
            outcome,
            AddOutcome::AlreadyPresent {
                name: "@sdk/greet".into(),
                version: "0.1.0".into()
            }
        );

        // A builtin, enabled: the no-op the real add returns.
        let dir =
            project_with(r#"{"name":"p","version":"0.1.0","extensions":["@specforge/product"]}"#);
        let source = Source::Builtin("@specforge/product");
        let before = crate::config::testing::files_under(dir.path());
        let added = add(
            &dry(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                source,
            ),
            &unconfigured,
            &runtime(),
        )
        .unwrap();
        assert_eq!(
            added.added[0].outcome,
            AddOutcome::Builtin {
                name: "@specforge/product",
                changed: false,
                peers_enabled: Vec::new()
            }
        );
        assert!(added.writes.is_empty());
        assert_eq!(crate::config::testing::files_under(dir.path()), before);
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "the add document lists what the add did for each specifier by action, with source always and key_id only for a signed package"
    )]
    fn add_document_names_what_the_add_did() {
        use specforge_installed::LockSource;

        let installed_from = |publisher, source| AddOutcome::Installed {
            name: "@acme/x".into(),
            version: "1.0.0".into(),
            sha256: "ab".into(),
            publisher,
            origin: Origin::Installed { source },
            peers_enabled: Vec::new(),
        };
        let signed = Publisher::Signed {
            key_id: "k1".into(),
            first_use: true,
        };
        let cases = [
            (
                AddOutcome::Builtin {
                    name: "@specforge/product",
                    changed: true,
                    peers_enabled: vec!["@specforge/software"],
                },
                serde_json::json!({"specifier": "s", "action": "enabled", "extension": "@specforge/product",
                    "source": "builtin", "changed": true, "peers_enabled": ["@specforge/software"]}),
            ),
            (
                installed_from(Some(Publisher::Unsigned), LockSource::Registry),
                serde_json::json!({"specifier": "s", "action": "installed", "extension": "@acme/x", "version": "1.0.0",
                    "sha256": "ab", "source": "registry", "publisher": "unsigned", "peers_enabled": []}),
            ),
            (
                installed_from(Some(signed), LockSource::Registry),
                serde_json::json!({"specifier": "s", "action": "installed", "extension": "@acme/x", "version": "1.0.0",
                    "sha256": "ab", "source": "registry", "key_id": "k1", "publisher": "pinned_now", "peers_enabled": []}),
            ),
            (
                installed_from(None, LockSource::Local("x.wasm".into())),
                serde_json::json!({"specifier": "s", "action": "installed", "extension": "@acme/x", "version": "1.0.0",
                    "sha256": "ab", "source": "local:x.wasm", "peers_enabled": []}),
            ),
            (
                AddOutcome::AlreadyPresent {
                    name: "@acme/x".into(),
                    version: "1.0.0".into(),
                },
                serde_json::json!({"specifier": "s", "action": "already_present", "extension": "@acme/x", "version": "1.0.0"}),
            ),
            (
                AddOutcome::Planned {
                    name: "@specforge/product".into(),
                    version: None,
                    origin: Origin::Builtin,
                    peers_enabled: vec!["@specforge/software"],
                },
                serde_json::json!({"specifier": "s", "action": "planned", "extension": "@specforge/product", "source": "builtin", "peers_enabled": ["@specforge/software"]}),
            ),
        ];
        let cases = cases.into_iter().chain([(
            AddOutcome::Duplicate {
                name: "@acme/x".into(),
                first: "@acme/x@1".into(),
            },
            serde_json::json!({"specifier": "s", "action": "duplicate", "extension": "@acme/x",
                "duplicate_of": "@acme/x@1"}),
        )]);
        let (outcomes, documents): (Vec<_>, Vec<_>) = cases.unzip();
        let added = Added {
            added: outcomes
                .into_iter()
                .map(|outcome| AddedExtension {
                    specifier: "s".into(),
                    outcome,
                })
                .collect(),
            writes: Writes::none(),
            extensions_enabled: 1,
        };
        assert_eq!(
            serde_json::to_value(added.document()).unwrap(),
            serde_json::json!({ "added": documents })
        );
        assert!(
            AddOutcome::AlreadyPresent {
                name: "a".into(),
                version: "1".into()
            }
            .is_duplicate()
        );
        assert!(!installed_from(None, LockSource::Registry).is_duplicate());
        assert!(
            AddOutcome::Duplicate {
                name: "a".into(),
                first: "a".into()
            }
            .is_duplicate()
        );
    }

    // --- several specifiers: one change ---

    /// The outcomes of `added`, in specifier order.
    fn outcomes(added: &Added) -> Vec<AddOutcome> {
        added.added.iter().map(|a| a.outcome.clone()).collect()
    }

    /// An add of `extensions` into `root`.
    fn several(
        root: &specforge_common::ProjectRoot,
        extensions: Vec<Specified>,
        dry_run: bool,
    ) -> AddRequest<'_> {
        AddRequest {
            root,
            extensions,
            allow_unsigned: true,
            trust: Trust::Refuse,
            dry_run,
        }
    }

    /// `runtime()`, also serving the builtin `@specforge/product`.
    fn runtime_with_product() -> InProcessRuntime {
        serving_builtin(
            runtime(),
            "@specforge/product",
            declaring("@specforge/product", "1.0.0", &[]),
        )
    }

    /// `greet.wasm` written in a directory of its own.
    fn greet_file() -> (tempfile::TempDir, PathBuf) {
        let files = tempfile::tempdir().unwrap();
        let wasm = files.path().join("greet.wasm");
        std::fs::write(&wasm, GREET).unwrap();
        (files, wasm)
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "add takes several specifiers and adds them as one change"
    )]
    fn several_specifiers_are_added_as_one_change() {
        let dir = project_with(EMPTY_PROJECT);
        let (_files, wasm) = greet_file();
        let shown = wasm.display().to_string();
        let extensions = specified(&["@specforge/product", shown.as_str()]).unwrap();

        let added = add(
            &several(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                extensions,
                false,
            ),
            &crate::registry::Unconfigured("add"),
            &runtime_with_product(),
        )
        .unwrap();

        let specifiers: Vec<&str> = added.added.iter().map(|a| a.specifier.as_str()).collect();
        assert_eq!(specifiers, ["@specforge/product", shown.as_str()]);
        let outcomes = outcomes(&added);
        assert_eq!(
            outcomes[0],
            AddOutcome::Builtin {
                name: "@specforge/product",
                changed: true,
                peers_enabled: Vec::new(),
            }
        );
        assert!(
            matches!(&outcomes[1], AddOutcome::Installed { name, .. } if name == "@sdk/greet"),
            "{outcomes:?}"
        );
        assert_eq!(
            added.writes.names_under(dir.path()),
            [
                ".specforge/extensions/@sdk/greet/extension.wasm",
                "specforge.json",
                "specforge.lock"
            ]
        );
        assert_eq!(
            specforge_common::read_project_config(dir.path())
                .config
                .extensions,
            ["@specforge/product", "@sdk/greet"]
        );
        assert_eq!(added.extensions_enabled, 2);
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "an add of several specifiers that refuses one writes nothing, its refusal naming that specifier"
    )]
    fn a_refused_specifier_writes_nothing_and_is_named() {
        use crate::config::testing::files_under;
        let dir = project_with(EMPTY_PROJECT);
        let (_files, wasm) = greet_file();
        let missing = dir.path().join("missing.wasm");
        let before = files_under(dir.path());

        for extensions in [
            vec![
                Specified::from(Source::Builtin("@specforge/product")),
                Specified::from(Source::Local(missing.clone())),
            ],
            vec![
                Specified::from(Source::Local(missing.clone())),
                Specified::from(Source::Local(wasm.clone())),
            ],
        ] {
            let error = add(
                &several(
                    &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                    extensions,
                    false,
                ),
                &crate::registry::Unconfigured("add"),
                &runtime_with_product(),
            )
            .unwrap_err();

            assert_eq!(error.code, "E054", "{error:?}");
            assert_eq!(
                error.message,
                format!(
                    "adding '{}': file not found: {}",
                    missing.display(),
                    missing.display()
                )
            );
            assert_eq!(files_under(dir.path()), before);
        }

        // One specifier: its refusal is the add's, as it was.
        let error = add(
            &several(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                vec![Source::Local(missing.clone()).into()],
                false,
            ),
            &crate::registry::Unconfigured("add"),
            &runtime(),
        )
        .unwrap_err();
        assert_eq!(
            error.message,
            format!("file not found: {}", missing.display())
        );
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "the specifiers of an add are read in order, the first unreadable one refused and named among several"
    )]
    fn specifiers_are_read_in_order_and_named_among_several() {
        let read = specified(&["@specforge/product", "@acme/tool@^1"]).unwrap();
        assert_eq!(
            read,
            [
                Specified {
                    specifier: "@specforge/product".into(),
                    source: Source::Builtin("@specforge/product"),
                },
                Specified {
                    specifier: "@acme/tool@^1".into(),
                    source: Source::Registry(PackageRef::parse("@acme/tool@^1").unwrap()),
                },
            ]
        );

        let error = specified(&["@specforge/product", "tool", "@acme/"]).unwrap_err();
        assert_eq!(error.code, "E054");
        assert!(
            error
                .message
                .starts_with("adding 'tool': invalid extension specifier: "),
            "{error:?}"
        );
        let alone = specified(&["tool"]).unwrap_err();
        assert!(
            alone.message.starts_with("invalid extension specifier: "),
            "{alone:?}"
        );

        let none = specified::<&str>(&[]).unwrap_err();
        assert_eq!(none.code, "E054");
        assert_eq!(none.message, "no extension specifier");
        let dir = project_with(EMPTY_PROJECT);
        let error = add(
            &several(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                Vec::new(),
                false,
            ),
            &crate::registry::Unconfigured("add"),
            &runtime(),
        )
        .unwrap_err();
        assert_eq!(error, none);
    }

    const A2: &[u8] = b"\0asm in-process @x/a 2.0.0";
    const B2: &[u8] = b"\0asm in-process @x/b 2.0.0";

    /// `runtime()`, also serving `@x/a` 2.0.0 (wanting `@x/b` ^2) and `@x/b` 2.0.0.
    fn coupled_runtime() -> InProcessRuntime {
        runtime()
            .binary(A2, declaring("@x/a", "2.0.0", &[("@x/b", "^2", false)]))
            .binary(B2, declaring("@x/b", "2.0.0", &[]))
    }

    /// `@x/a` 2.0.0, `@x/b` 1.0.0 (no extension: it never loads) and `@x/b` 2.0.0 published.
    fn coupled_registry(runtime: &InProcessRuntime) -> MemoryRegistry {
        let served = |wasm: &[u8]| {
            Published::new(
                wasm.to_vec(),
                crate::publish::declare(runtime, wasm).unwrap().0,
            )
        };
        MemoryRegistry::new()
            .serving(served(A2))
            .serving(Published::new(
                b"\0asm b1".to_vec(),
                declaration("@x/b", "1.0.0", &[]),
            ))
            .serving(served(B2))
    }

    fn registry_specified(reference: &str) -> Specified {
        Specified {
            specifier: reference.to_string(),
            source: Source::Registry(PackageRef::parse(reference).unwrap()),
        }
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "an add of several specifiers judges their peers on the whole set, whatever their order"
    )]
    fn peers_are_judged_on_the_whole_set_whatever_the_order() {
        use crate::config::testing::files_under;
        let runtime = coupled_runtime();

        // `@x/a` 2.0.0 alone wants `@x/b` ^2, and 1.0.0 is locked.
        let dir = add_project(vec![entry("@x/b", "1.0.0", "registry", &[])]);
        let error = add(
            &several(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                vec![registry_specified("@x/a@2.0.0")],
                false,
            ),
            &coupled_registry(&runtime),
            &runtime,
        )
        .unwrap_err();
        assert_eq!(error.code, "R-RES-006", "{error:?}");

        // With `@x/b` 2.0.0 in the same add, the set is accepted, in either order, leaving the
        // same lock.
        let mut locks = Vec::new();
        for order in [["@x/a@2.0.0", "@x/b@2.0.0"], ["@x/b@2.0.0", "@x/a@2.0.0"]] {
            let dir = add_project(vec![entry("@x/b", "1.0.0", "registry", &[])]);
            let extensions = order.iter().map(|r| registry_specified(r)).collect();
            let added = add(
                &several(
                    &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                    extensions,
                    false,
                ),
                &coupled_registry(&runtime),
                &runtime,
            )
            .unwrap();
            assert!(
                outcomes(&added)
                    .iter()
                    .all(|o| matches!(o, AddOutcome::Installed { .. })),
                "{order:?}: {added:?}"
            );
            let lock_path = dir.path().join("specforge.lock");
            let lock = specforge_installed::read_lock_file(&lock_path).unwrap();
            let locked: Vec<(&str, &str)> = lock
                .entries
                .iter()
                .map(|e| (e.name.as_str(), e.version.as_str()))
                .collect();
            assert_eq!(locked, [("@x/b", "2.0.0"), ("@x/a", "2.0.0")], "{order:?}");
            locks.push(std::fs::read(&lock_path).unwrap());
        }
        assert_eq!(locks[0], locks[1], "the order decides nothing");

        // With `@x/b` 1.0.0 instead, the set is refused in either order, naming `@x/a`'s
        // specifier, and nothing is written.
        for order in [["@x/a@2.0.0", "@x/b@1.0.0"], ["@x/b@1.0.0", "@x/a@2.0.0"]] {
            let dir = disabled_project(vec![entry("@x/b", "1.0.0", "registry", &[])]);
            let before = files_under(dir.path());
            let extensions = order.iter().map(|r| registry_specified(r)).collect();
            let error = add(
                &several(
                    &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                    extensions,
                    false,
                ),
                &coupled_registry(&runtime),
                &runtime,
            )
            .unwrap_err();
            assert_eq!(error.code, "R-RES-006", "{order:?}: {error:?}");
            assert_eq!(
                error.message,
                "adding '@x/a@2.0.0': version diamond: '@x/a' requires peer '@x/b' ^2 but this install locks 1.0.0; @x/b 2.0.0 would satisfy every requirer",
                "{order:?}"
            );
            assert_eq!(files_under(dir.path()), before, "{order:?}");
        }
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "a specifier naming an extension an earlier one adds is a duplicate, and two adding it differently are refused"
    )]
    fn a_repeated_extension_is_a_duplicate_and_a_different_one_a_conflict() {
        use crate::config::testing::files_under;

        // The same builtin twice, by two spellings: enabled once.
        let dir = project_with(EMPTY_PROJECT);
        let extensions = specified(&["@specforge/product", "@specforge/product@latest"]).unwrap();
        let added = add(
            &several(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                extensions,
                false,
            ),
            &crate::registry::Unconfigured("add"),
            &runtime_with_product(),
        )
        .unwrap();
        assert_eq!(
            outcomes(&added)[1],
            AddOutcome::Duplicate {
                name: "@specforge/product".into(),
                first: "@specforge/product".into(),
            }
        );
        assert!(added.added[1].outcome.is_duplicate());
        assert_eq!(
            specforge_common::read_project_config(dir.path())
                .config
                .extensions,
            ["@specforge/product"]
        );

        // A registry package resolving to the same version: fetched once.
        let dir = add_project(Vec::new());
        let registry =
            MemoryRegistry::new().serving(Published::new(greet(), declaration_of(&greet())));
        let extensions = vec![
            registry_specified("@sdk/greet@0.1.0"),
            registry_specified("@sdk/greet"),
            registry_specified("@sdk/greet@0.1.0"),
        ];
        let added = add(
            &several(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                extensions,
                false,
            ),
            &registry,
            &runtime(),
        )
        .unwrap();
        let outcomes = outcomes(&added);
        assert!(matches!(outcomes[0], AddOutcome::Installed { .. }));
        for duplicate in &outcomes[1..] {
            assert_eq!(
                *duplicate,
                AddOutcome::Duplicate {
                    name: "@sdk/greet".into(),
                    first: "@sdk/greet@0.1.0".into(),
                }
            );
        }
        let fetched = registry
            .asked()
            .into_iter()
            .filter(|asked| matches!(asked, crate::registry::testing::Asked::Fetch(..)))
            .count();
        assert_eq!(fetched, 1);

        // The same package at two versions: refused, naming both, nothing written.
        let runtime = coupled_runtime();
        let dir = add_project(Vec::new());
        let before = files_under(dir.path());
        let extensions = vec![
            registry_specified("@x/b@1.0.0"),
            registry_specified("@x/b@2.0.0"),
        ];
        let error = add(
            &several(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                extensions,
                false,
            ),
            &coupled_registry(&runtime),
            &runtime,
        )
        .unwrap_err();
        assert_eq!(error.code, "extension_conflict");
        assert_eq!(error.kind, OpErrorKind::Conflict);
        assert_eq!(
            error.message,
            "'@x/b@1.0.0' and '@x/b@2.0.0' both add '@x/b', as 1.0.0 from registry and as 2.0.0 from registry"
        );
        assert_eq!(error.suggestion.as_deref(), Some("add '@x/b' once"));
        assert_eq!(files_under(dir.path()), before);
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "a dry run of several specifiers previews each and writes nothing"
    )]
    fn a_dry_run_of_several_specifiers_previews_each() {
        use crate::config::testing::files_under;
        let dir = project_with(EMPTY_PROJECT);
        let (_files, wasm) = greet_file();
        let before = files_under(dir.path());
        let extensions = vec![
            Specified::from(Source::Builtin("@specforge/product")),
            Specified::from(Source::Local(wasm.clone())),
            Specified::from(Source::Builtin("@specforge/product")),
        ];

        let added = add(
            &several(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                extensions,
                true,
            ),
            &crate::registry::Unconfigured("add"),
            &runtime_with_product(),
        )
        .unwrap();

        let outcomes = outcomes(&added);
        assert_eq!(
            outcomes[0],
            AddOutcome::Planned {
                name: "@specforge/product".into(),
                version: None,
                origin: Origin::Builtin,
                peers_enabled: vec![],
            }
        );
        assert!(
            matches!(&outcomes[1], AddOutcome::Planned { name, version: Some(v), .. }
                if name == "@sdk/greet" && v == "0.1.0"),
            "{outcomes:?}"
        );
        assert!(matches!(outcomes[2], AddOutcome::Duplicate { .. }));
        assert!(added.writes.is_empty());
        assert_eq!(files_under(dir.path()), before);

        // A dry run refuses what the add would refuse, naming it.
        let missing = dir.path().join("missing.wasm");
        let extensions = vec![
            Source::Builtin("@specforge/product").into(),
            Source::Local(missing).into(),
        ];
        let error = add(
            &several(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                extensions,
                true,
            ),
            &crate::registry::Unconfigured("add"),
            &runtime_with_product(),
        )
        .unwrap_err();
        assert_eq!(error.code, "E054");
        assert!(error.message.starts_with("adding '"), "{error:?}");
        assert_eq!(files_under(dir.path()), before);
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "a dry run of several specifiers previews each and writes nothing"
    )]
    fn a_dry_run_reports_the_builtins_it_would_enable() {
        let dir = project_with(EMPTY_PROJECT);
        let chain = serving_builtin(
            serving_builtin(
                runtime_with_product(),
                "@specforge/formal",
                declaring(
                    "@specforge/formal",
                    "1.0.0",
                    &[("@specforge/software", "^1.0", false)],
                ),
            ),
            "@specforge/software",
            declaring("@specforge/software", "1.0.0", &[]),
        );

        let added = add(
            &several(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                vec![Source::Builtin("@specforge/formal").into()],
                true,
            ),
            &crate::registry::Unconfigured("add"),
            &chain,
        )
        .unwrap();

        let document = serde_json::to_value(added.document()).unwrap();
        assert_eq!(document["added"][0]["action"], "planned");
        assert_eq!(
            document["added"][0]["peers_enabled"],
            serde_json::json!(["@specforge/software"]),
            "{document}"
        );
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "a dry run judges a registry package by the declaration it was published with, downloading nothing"
    )]
    fn a_dry_run_judges_a_registry_package_by_its_published_declaration() {
        let published = || {
            MemoryRegistry::new()
                .serving(Published::new(
                    b"\0asm base1".to_vec(),
                    declaration("@acme/base", "1.0.0", &[]),
                ))
                .serving(Published::new(
                    b"\0asm base2".to_vec(),
                    declaration("@acme/base", "2.0.0", &[]),
                ))
                .serving(Published::new(
                    b"\0asm app".to_vec(),
                    declaration("@acme/app", "1.0.0", &[("@acme/base", "^2.0")]),
                ))
        };
        let dir = add_project(vec![entry("@acme/base", "1.0.0", "registry", &[])]);
        let root = specforge_common::ProjectRoot::of(dir.path()).unwrap();
        let request = |dry_run| AddRequest {
            root: &root,
            extensions: vec![
                Source::Registry(PackageRef::parse("@acme/app@1.0.0").unwrap()).into(),
            ],
            allow_unsigned: true,
            trust: Trust::Refuse,
            dry_run,
        };

        let real = add(&request(false), &published(), &runtime()).unwrap_err();
        assert_eq!(real.code, "R-RES-006", "{real:?}");
        assert!(real.message.contains("2.0.0"), "{real:?}");

        let registry = published();
        let dry = add(&request(true), &registry, &runtime()).unwrap_err();
        assert_eq!((dry.code, dry.message), (real.code, real.message));
        let asked = registry.asked();
        assert!(
            asked.contains(&crate::registry::testing::Asked::Published(
                PackageName::parse("@acme/app").unwrap(),
                specforge_package::Version::new(1, 0, 0)
            )),
            "{asked:?}"
        );
        assert!(
            !asked
                .iter()
                .any(|asked| matches!(asked, crate::registry::testing::Asked::Fetch(..))),
            "a dry run downloads nothing: {asked:?}"
        );
    }

    #[specforge_test(
        behavior = "parse_extension_specifier",
        verify = "a specifier that is no builtin's name, git+ URL or @scope/name reference is a local path, a bare crate directory included"
    )]
    #[ignore = "fixed by T9"]
    fn a_bare_path_is_a_local_source() {
        assert_eq!(parse("ext").unwrap(), Source::Local("ext".into()));
        assert_eq!(
            parse("tools/greet").unwrap(),
            Source::Local("tools/greet".into())
        );
        assert!(matches!(parse("@acme/x").unwrap(), Source::Registry(_)));
        assert_eq!(
            parse("@acme/x.wasm").unwrap(),
            Source::Local("@acme/x.wasm".into())
        );

        let dir = project_with(EMPTY_PROJECT);
        let krate = dir.path().join("ext");
        let release = krate.join("target/wasm32-wasip2/release");
        std::fs::create_dir_all(&release).unwrap();
        std::fs::write(krate.join("Cargo.toml"), "[package]\nname = \"greet\"\n").unwrap();
        std::fs::write(release.join("greet.wasm"), GREET).unwrap();
        let root = specforge_common::ProjectRoot::of(dir.path()).unwrap();
        let specified = Specified {
            specifier: "ext".into(),
            source: parse("ext").unwrap(),
        }
        .relative_to(dir.path());

        let added = add(
            &several(&root, vec![specified], false),
            &crate::registry::Unconfigured("add"),
            &runtime(),
        )
        .unwrap();

        let AddOutcome::Installed { name, origin, .. } = &added.added[0].outcome else {
            panic!("{:?}", added.added[0].outcome);
        };
        assert_eq!(name, "@sdk/greet");
        assert_eq!(
            origin.source(),
            "local:ext/target/wasm32-wasip2/release/greet.wasm"
        );
    }
}
