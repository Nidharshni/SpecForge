//! The compiled project (architecture plan 01, ADR 0004 D1-d).
//!
//! Every surface (`specforge check` and the other CLI commands, watch, the
//! LSP, MCP) used to assemble "config, extensions, registries, resolve,
//! graph, checks" on its own, and each copy drifted. This crate owns that
//! assembly:
//! - an [`Environment`] is everything derived from `specforge.json` and the
//!   loaded extensions before any `.spec` file is read, assembled from those
//!   parts ([`Environment::assemble`]; [`Environment::load`] reads them);
//! - a [`CompiledProject`] is an environment plus the sources read in it,
//!   their graph build and what the imports and the checks reported. Its
//!   [`CompiledProject::diagnostics`] are, by definition, what `specforge
//!   check` reports, in the one report order;
//! - a [`ProjectSession`] is a compiled project kept current
//!   ([`ProjectSession::project`]) plus what it is built from: its inputs
//!   ([`SessionInputs`]) say what a changed path is ([`InputRole`]), and it
//!   applies changes as an update, an environment reload or a re-check
//!   (watch, the LSP and MCP each hold one). After every update that runs
//!   the checks it reports what a fresh compile reports, in the same order.

mod buffers;
mod build_cache;
mod check_passes;
mod compiled;
pub mod coverage;
mod freshness;
mod inputs;
pub mod passes;
mod policy;
pub mod providers;
mod session;
mod setup;
pub mod snapshot;
mod sources;
mod verdicts;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sources::SourceCache;

use snapshot::EntitySnapshot;
use specforge_common::{
    ConfigProblem, ConfigRead, Diagnostic, ProjectConfig, ProjectRoot, codes, discover_spec_files,
    is_discovered, read_project_config,
};
use specforge_graph::{Graph, GraphBuild, GraphConfig};
use specforge_installed::{Builtins, Installed};
use specforge_registry::{
    RegistryBuild, build_registries,
    rules::{CustomVerdicts, NoVerdicts},
};
use specforge_wasm::WasmRuntime;
use verdicts::WasmVerdicts;

pub use buffers::Buffer;
pub use build_cache::{BUILD_CACHE_FILE, BUILD_CACHE_FORMAT, BuildCache, CachedStatus};
pub use compiled::CompiledProject;
pub use inputs::{Changes, InputRole, SessionInputs, UpdateKind, WatchRoot, Watched, source_key};
pub use policy::{DiagnosticPolicy, LINT_PROFILE_NAMES, LintProfile, UnknownLintProfile};
pub use providers::Providers;
pub use session::{CheckMode, OpeningProject, ProjectSession, RuntimeSource, SourceChange, Update};
pub use setup::{SetupDiagnostic, SetupPart};
pub use specforge_graph::{
    EdgeChange, GraphDelta, ModifiedNodeChange, NodeAspect, NodeChange, compute_graph_delta,
};
pub use specforge_installed::{EnabledExtension, LoadSubject, Loaded};

/// The runtime a project's extensions run in (every [`WasmRuntime`] is
/// `Send + Sync`).
pub type SharedRuntime = Arc<dyn WasmRuntime>;

/// The builtin extensions this host embeds.
pub fn builtins() -> Builtins<'static> {
    Builtins(specforge_component::builtins::BUILTIN_EXTENSIONS)
}

/// What the extension load gave an environment (CONTEXT "Extension load"):
/// nothing, when it ran without a runtime, or what it loaded into a
/// runtime, which the environment then holds (ADR 0015 R1, R3).
pub enum ExtensionLoad {
    /// No runtime, so no extension loads: each `extensions` entry enables
    /// what it names, unloaded ([`Loaded::unloaded`]).
    NoRuntime,
    /// What [`Installed::load`] loaded into `runtime`: each entry's
    /// outcome, the declarations in entry order, the load's diagnostics.
    In {
        runtime: SharedRuntime,
        loaded: Loaded,
    },
}

/// Everything derived from `specforge.json` and the loaded extensions,
/// before any `.spec` file is read. Assembled from those parts
/// ([`Self::assemble`]).
pub struct Environment {
    /// The config, every way `specforge.json` is not used as written (each
    /// reported as E069, first; with one that
    /// [`loads_nothing`](ConfigProblem::loads_nothing), no extension
    /// loaded), and whether the file exists at the root. No file was read
    /// for [`Self::empty`] and [`Self::with_registries`].
    read: ConfigRead,
    /// The project's installed extensions: what `specforge.lock` held when
    /// the environment was read (absent, read, or unreadable with its
    /// problem), once per environment, which every operation over the
    /// project reads instead of the disk. Its root is the project root. A
    /// changed lock reloads the environment ([`SessionInputs`]).
    installed: Installed,
    /// What each `specforge.json` `extensions` entry enables, in order, as
    /// the runtime loaded it (a `.wasm` file entry by the name its
    /// component declares).
    enabled: Vec<EnabledExtension>,
    /// Where `.spec` files are discovered: `spec_root` from the config,
    /// relative to the project root, or the project root itself.
    spec_root: PathBuf,
    /// The registries, rules, passes and graph inputs built from the loaded
    /// declarations.
    registries: RegistryBuild,
    /// The `providers` specforge.json configures, registered once against
    /// the loaded declarations (ADR 0004 D3-c): with any scheme registered,
    /// a ref with another scheme is I005.
    providers: Providers,
    /// One E069 per config problem. The extension load's diagnostics are on
    /// [`Self::enabled`] ([`Installed::load_diagnostics`] orders them).
    config_diagnostics: Vec<Diagnostic>,
    /// I002, when no extension loaded.
    structural_only: Option<Diagnostic>,
    /// The runtime the extensions were loaded in: every extension call an
    /// operation makes over this project runs there, a custom rule's
    /// verdict and a check pass included. None: the environment was loaded
    /// without one, so no extension loaded ([`Self::empty`] has none;
    /// [`Self::with_registries`] has the one it is given).
    runtime: Option<SharedRuntime>,
    /// The project root it was loaded from ([`Self::from_read`]); `None` for one
    /// assembled in memory ([`Self::assemble`], [`Self::empty`],
    /// [`Self::with_registries`]). For a loaded environment, [`Self::root`] is its
    /// `dir()`.
    root: Option<ProjectRoot>,
}

impl Environment {
    /// No project: the default config, no spec root, no extension.
    pub fn empty() -> Self {
        Environment {
            read: ConfigRead::default(),
            installed: Installed::none(),
            enabled: Vec::new(),
            spec_root: PathBuf::new(),
            registries: RegistryBuild::default(),
            providers: Providers::default(),
            config_diagnostics: Vec::new(),
            structural_only: None,
            runtime: None,
            root: None,
        }
    }

    /// No project, a registry build made in memory, and the runtime its
    /// extension calls go to, if any: a bare build for a graph built in a
    /// test (ADR 0015 "The view, completed"). Nothing reported, no I002.
    pub fn with_registries(registries: RegistryBuild, runtime: Option<SharedRuntime>) -> Self {
        Environment {
            registries,
            runtime,
            ..Environment::empty()
        }
    }

    /// Read the config of the project at `root` and load its extensions into
    /// `runtime` (none without one), which the environment then holds, then
    /// build the registries from them. The environment keeps `root`.
    pub fn load(root: &ProjectRoot, runtime: Option<SharedRuntime>) -> Self {
        Self::from_read(root, read_project_config(root.dir()), runtime)
    }

    /// The environment of the config `read`, the installed extensions
    /// `installed` and what the extension load gave, rooted where
    /// `installed` is:
    /// - one E069 per config problem, in file order, then the load's
    ///   diagnostics;
    /// - the registry build of the loaded declarations; in a runtime, each
    ///   custom rule's `wasm_function` called once there on a probe entity
    ///   (W112 for one that does not answer a verdict);
    /// - the configured `providers` registered against the loaded
    ///   declarations (W118, E057);
    /// - I002 when no extension loaded, naming why (the config, nothing
    ///   configured, or the load failures);
    /// - the spec root: the config's `spec_root` under the root, else the
    ///   root.
    ///
    /// Reads nothing from disk. Its one effect is the probe's calls in the
    /// runtime it is given (the `WasmRuntime` port).
    pub fn assemble(read: ConfigRead, installed: Installed, load: ExtensionLoad) -> Self {
        let config_diagnostics: Vec<Diagnostic> = read
            .problems
            .iter()
            .map(config_problem_diagnostic)
            .collect();
        let (loaded, runtime) = match load {
            ExtensionLoad::NoRuntime => (Loaded::unloaded(&read.config.extensions), None),
            ExtensionLoad::In { runtime, loaded } => (loaded, Some(runtime)),
        };
        let Loaded {
            enabled,
            declarations,
        } = loaded;
        let mut registries = build_registries(declarations);
        // A custom rule's wasm_function is resolved against its extension
        // now, so a name it does not export is reported once (W112).
        if let Some(runtime) = runtime.as_deref() {
            registries.probe(&WasmVerdicts::probe_only(runtime));
        }
        let providers = Providers::register(read.config.raw.as_ref(), registries.declarations());
        let structural_only = registries.declarations().is_empty().then(|| {
            structural_only_notice(
                read.found,
                installed.root(),
                &read.config.extensions,
                &read.problems,
            )
        });
        let spec_root = read.config.spec_root_in(installed.root());
        Environment {
            read,
            installed,
            enabled,
            spec_root,
            registries,
            providers,
            config_diagnostics,
            structural_only,
            runtime,
            root: None,
        }
    }

    /// The environment of the config `read` (the one read of
    /// `specforge.json`): the lock read at `root` once, the extensions
    /// loaded into `runtime` through the production policy, then
    /// [`Self::assemble`]d. The disk adapter of the assembly; the session's
    /// open and [`Self::load`] are its callers.
    pub(crate) fn from_read(
        root: &ProjectRoot,
        read: ConfigRead,
        runtime: Option<SharedRuntime>,
    ) -> Self {
        let installed = Installed::at(root.dir());
        let load = match runtime {
            Some(runtime) => ExtensionLoad::In {
                loaded: installed.load(&read.config.extensions, &builtins(), runtime.as_ref()),
                runtime,
            },
            None => ExtensionLoad::NoRuntime,
        };
        Environment {
            root: Some(root.clone()),
            ..Self::assemble(read, installed, load)
        }
    }

    /// The project root it was loaded from; `None` for one assembled in memory.
    pub fn project_root(&self) -> Option<&ProjectRoot> {
        self.root.as_ref()
    }

    /// The project root (where `specforge.json` lives); empty with no project.
    pub fn root(&self) -> &Path {
        self.installed.root()
    }

    /// The config as used: the default for each unusable key or file.
    pub fn config(&self) -> &ProjectConfig {
        &self.read.config
    }

    /// How `specforge.json` is not used as written, in file order (each one
    /// E069). With one that [`loads_nothing`](ConfigProblem::loads_nothing),
    /// no extension loaded.
    pub fn config_problems(&self) -> &[ConfigProblem] {
        &self.read.problems
    }

    /// `specforge.json` exists at the root. `false` with no project.
    pub fn config_found(&self) -> bool {
        self.read.found
    }

    /// What `specforge.lock` held when the environment was read, once, for
    /// every operation over the project (ADR 0028 D2).
    pub fn installed(&self) -> &Installed {
        &self.installed
    }

    /// What each `extensions` entry enables, in entry order, as the
    /// extension load left it.
    pub fn enabled(&self) -> &[EnabledExtension] {
        &self.enabled
    }

    /// Where `.spec` files are discovered.
    pub fn spec_root(&self) -> &Path {
        &self.spec_root
    }

    /// The registries, rules, passes and surfaces built from the loaded
    /// declarations.
    pub fn registries(&self) -> &RegistryBuild {
        &self.registries
    }

    /// The configured providers, registered once against the loaded
    /// declarations (ADR 0004 D3-c).
    pub fn providers(&self) -> &Providers {
        &self.providers
    }

    /// The runtime the extensions were loaded in, where every extension
    /// call over this project runs; none: nothing loaded (ADR 0015 R1, R3).
    pub fn runtime(&self) -> Option<&SharedRuntime> {
        self.runtime.as_ref()
    }

    /// The inputs every graph of this project is built with: every surface
    /// that builds a graph (`check`, watch, the LSP) takes its `GraphConfig`
    /// from here, so none can drift.
    pub fn graph_config(&self) -> GraphConfig {
        let build = &self.registries;
        GraphConfig {
            known_provider_schemes: self.providers.schemes(),
            inputs: build.graph_inputs(),
        }
    }

    /// The entity snapshot of `graph`, built in this environment: its
    /// registries decide each entity's standing and its spec root resolves
    /// the relative paths the rules read. The one place a snapshot is
    /// taken from an environment.
    pub fn entity_snapshot(&self, graph: &Graph) -> EntitySnapshot {
        EntitySnapshot::of(graph, &self.registries, &self.spec_root)
    }

    /// Every check a compile runs on a built graph, over its entity
    /// snapshot `entities`: the registry build's checks (the structural
    /// checks and the extensions' rules, in the order
    /// [`RegistryBuild::check`] runs them), then the check-phase passes;
    /// the custom verdicts and the passes run in [`Self::runtime`].
    pub fn run_checks(&self, graph: &Graph, entities: &EntitySnapshot) -> Vec<Diagnostic> {
        let runtime = self.runtime.as_deref();
        let mut diagnostics = Vec::new();
        let verdicts: Box<dyn CustomVerdicts + '_> = match runtime {
            Some(runtime) => Box::new(WasmVerdicts::new(runtime, entities)),
            None => Box::new(NoVerdicts),
        };
        diagnostics.extend(
            self.registries
                .check(&entities.rule_input(), verdicts.as_ref()),
        );
        if let Some(runtime) = runtime {
            diagnostics.extend(check_passes::run(self, graph, entities, runtime));
        }
        diagnostics
    }

    /// The diagnostics reported before any source, each with the part of
    /// the setup it is about, in `specforge check`'s order: why
    /// `specforge.json` is not used as written (E069), the extension load's
    /// entry by entry (an entry's failure, or what it reported though it
    /// loaded; the lock's E033 once, before the first entry it left
    /// unloaded), provider registration (W118; E057 is a collision), I002,
    /// then the registry build's, in its order
    /// ([`specforge_registry::RegistryBuild::reported`]).
    pub fn setup_diagnostics(&self) -> Vec<SetupDiagnostic<'_>> {
        let of = |part, diagnostic| SetupDiagnostic { part, diagnostic };
        let mut all: Vec<SetupDiagnostic<'_>> = self
            .config_diagnostics
            .iter()
            .map(|d| of(SetupPart::Config, d))
            .collect();
        all.extend(
            self.installed
                .load_diagnostics(&self.enabled)
                .into_iter()
                .map(|(subject, d)| {
                    of(
                        match subject {
                            LoadSubject::Lock => SetupPart::Lock,
                            LoadSubject::Entry(i) => SetupPart::Entry(i),
                        },
                        d,
                    )
                }),
        );
        all.extend(self.providers.reported().map(|(part, d)| of(part, d)));
        all.extend(
            self.structural_only
                .iter()
                .map(|d| of(SetupPart::StructuralOnly, d)),
        );
        all.extend(
            self.registries
                .reported()
                .map(|(part, d)| of(part.into(), d)),
        );
        all
    }

    /// [`Self::setup_diagnostics`] without the parts: what `specforge check`
    /// reports before any source.
    pub fn diagnostics(&self) -> impl Iterator<Item = &Diagnostic> {
        self.setup_diagnostics()
            .into_iter()
            .map(|setup| setup.diagnostic)
    }

    /// Whether a file (its path relative to the spec root) is left out of
    /// the project, as discovery leaves it out: not a `.spec` file, under a
    /// skipped directory (`target`, `node_modules`, ...) or matched by an
    /// `exclude` entry.
    pub fn excludes(&self, relative: &str) -> bool {
        !is_discovered(relative, &self.read.config.exclude)
    }

    /// The project sources discovery finds now (`exclude` and the skipped
    /// directories applied).
    pub fn discover(&self) -> Vec<PathBuf> {
        discover_spec_files(&self.spec_root, &self.read.config.exclude)
    }

    /// Read and parse `discovered`, build their graph and resolve their
    /// imports: the one cold build every compile and session open
    /// starts from (ADR 0032).
    ///
    /// A `held` text (by source key) is read in place of its file: the
    /// editor's buffers, which the session layer holds; the core stays
    /// buffer-agnostic.
    pub(crate) fn build_sources(
        &self,
        discovered: &[PathBuf],
        held: &BTreeMap<String, &str>,
    ) -> SourceBuild {
        let (sources, files) = SourceCache::read_all(&self.spec_root, discovered, held);
        let graph = GraphBuild::of(files, self.graph_config());
        let imports = sources.import_diagnostics(&graph);
        SourceBuild {
            sources,
            graph,
            imports,
        }
    }
}

/// What a compile builds from the sources on disk (ADR 0032).
pub(crate) struct SourceBuild {
    pub sources: SourceCache,
    pub graph: GraphBuild,
    /// E025 for the unreadable sources, then the resolver's (E025, I004,
    /// W113, W027).
    pub imports: Vec<Diagnostic>,
}

/// E069: one way `specforge.json` is not used as written. An error: a
/// config that loads nothing must not pass `check`.
fn config_problem_diagnostic(problem: &ConfigProblem) -> Diagnostic {
    let message = if problem.loads_nothing() {
        format!("specforge.json can't be used: {problem}; no extension is loaded")
    } else {
        format!("specforge.json: {problem}; it is ignored")
    };
    Diagnostic::new(codes::E069, message).with_suggestion(format!(
        "fix specforge.json; `specforge explain {}` says what it must be",
        codes::E069
    ))
}

/// I002: no extension loaded, so the compile checks structure only (no
/// kind, field or rule checks). Reported after the errors that caused it,
/// if any: the config problems (E069) that left nothing to load, or the
/// load errors (E028).
fn structural_only_notice(
    found: bool,
    root: &Path,
    configured: &[String],
    problems: &[ConfigProblem],
) -> Diagnostic {
    let (message, suggestion) = if problems.iter().any(ConfigProblem::loads_nothing) {
        (
            "specforge.json could not be read — operating in structural-only mode".to_string(),
            format!("fix specforge.json ({} above)", codes::E069),
        )
    } else if !found && root.as_os_str().is_empty() {
        (
            "no project — operating in structural-only mode".to_string(),
            "create a project with: specforge init".to_string(),
        )
    } else if !found {
        // A loose root: no specforge.json at or above the path.
        (
            format!(
                "no specforge.json at or above {} — operating in structural-only mode",
                root.display()
            ),
            "create a project here with: specforge init".to_string(),
        )
    } else if configured.is_empty() {
        (
            "no extensions configured — operating in structural-only mode".to_string(),
            "enable kind-specific checks with: specforge add @specforge/software".to_string(),
        )
    } else {
        let which = match configured.len() {
            1 => "the configured extension did not load".to_string(),
            n => format!("none of the {n} configured extensions loaded"),
        };
        (
            format!("{which} — operating in structural-only mode"),
            "fix the extension load errors above (`specforge doctor` checks the setup)".to_string(),
        )
    };
    Diagnostic::new(codes::I002, message).with_suggestion(suggestion)
}
