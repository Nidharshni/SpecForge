//! The compiled project (architecture plan 01, ADR 0004 D1-d).
//!
//! Every surface (`specforge check` and the other CLI commands, watch, the
//! LSP, MCP) used to assemble "config, extensions, registries, resolve,
//! graph, checks" on its own, and each copy drifted. This crate owns that
//! assembly:
//! - an [`Environment`] is everything derived from `specforge.json` and the
//!   loaded extensions before any `.spec` file is read;
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
pub mod snapshot;
mod sources;
mod verdicts;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sources::SourceCache;

use snapshot::EntitySnapshot;
use specforge_common::{
    ConfigProblem, ConfigRead, Diagnostic, ProjectConfig, codes, discover_spec_files,
    is_discovered, read_project_config,
};
use specforge_graph::{Graph, GraphBuild, GraphConfig};
use specforge_installed::{Builtins, Installed};
use specforge_parser::SpecFile;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::{
    RegistryBuild, build_registries,
    rules::{CustomVerdicts, NoVerdicts},
};
use specforge_resolver::resolve_imports;
use specforge_wasm::WasmRuntime;
use verdicts::WasmVerdicts;

pub use buffers::Buffer;
pub use build_cache::{BUILD_CACHE_FILE, BUILD_CACHE_FORMAT, BuildCache, CachedStatus};
pub use compiled::CompiledProject;
pub use inputs::{Changes, InputRole, SessionInputs, UpdateKind, WatchRoot, Watched, source_key};
pub use policy::{DiagnosticPolicy, LINT_PROFILE_NAMES, LintProfile, UnknownLintProfile};
pub use providers::Providers;
pub use session::{CheckMode, OpeningProject, ProjectSession, RuntimeSource, SourceChange, Update};
pub use specforge_graph::{
    EdgeChange, GraphDelta, ModifiedNodeChange, NodeChange, compute_graph_delta,
};
pub use specforge_installed::EnabledExtension;

/// The runtime a project's extensions run in (every [`WasmRuntime`] is
/// `Send + Sync`).
pub type SharedRuntime = Arc<dyn WasmRuntime>;

/// The builtin extensions this host embeds.
pub fn builtins() -> Builtins<'static> {
    Builtins(specforge_component::builtins::BUILTIN_EXTENSIONS)
}

/// Everything derived from `specforge.json` and the loaded extensions,
/// before any `.spec` file is read.
pub struct Environment {
    /// The project root (where `specforge.json` lives).
    pub root: PathBuf,
    pub config: ProjectConfig,
    /// How `specforge.json` is not used as written, in file order: each is
    /// reported as E069, first. With one that
    /// [`loads_nothing`](ConfigProblem::loads_nothing), the compile loaded
    /// no extension.
    pub config_problems: Vec<ConfigProblem>,
    /// `specforge.json` exists at the root. `false` for [`Self::empty`],
    /// [`Self::from_declarations`] and [`Self::with_registries`] (no file
    /// was read).
    pub config_found: bool,
    /// The project's installed extensions: what `specforge.lock` held when
    /// the environment was read (absent, read, or unreadable with its
    /// problem), once per environment, which every operation over the
    /// project reads instead of the disk. A changed lock reloads the
    /// environment ([`SessionInputs`]).
    pub installed: Installed,
    /// What each `specforge.json` `extensions` entry enables, in order, as
    /// the runtime loaded it (a `.wasm` file entry by the name its
    /// component declares).
    pub enabled: Vec<EnabledExtension>,
    /// Where `.spec` files are discovered: `spec_root` from the config,
    /// relative to the project root, or the project root itself.
    pub spec_root: PathBuf,
    /// The registries, rules, passes and graph inputs built from the loaded
    /// declarations.
    pub registries: RegistryBuild,
    /// The `providers` specforge.json configures, registered once against
    /// the loaded declarations (ADR 0004 D3-c): with any scheme registered,
    /// a ref with another scheme is I005.
    pub providers: Providers,
    /// Extension loading diagnostics: one E069 per config problem, the
    /// runtime's load failures (E028/E033) in load order, then the
    /// declarations' unknown keys (W138).
    pub load_diagnostics: Vec<Diagnostic>,
    /// After the registry build: I002 when no extension loaded.
    pub setup_diagnostics: Vec<Diagnostic>,
    /// The runtime the extensions were loaded in: every extension call an
    /// operation makes over this project runs there, a custom rule's
    /// verdict and a check pass included. None: the environment was loaded
    /// without one, so no extension loaded ([`Self::empty`],
    /// [`Self::from_declarations`] and [`Self::with_registries`] have none).
    pub runtime: Option<SharedRuntime>,
}

impl Environment {
    /// No project: the default config, no spec root, no extension.
    pub fn empty() -> Self {
        Environment {
            root: PathBuf::new(),
            config: ProjectConfig::default(),
            config_problems: Vec::new(),
            config_found: false,
            installed: Installed::none(),
            enabled: Vec::new(),
            spec_root: PathBuf::new(),
            registries: RegistryBuild::default(),
            providers: Providers::default(),
            load_diagnostics: Vec::new(),
            setup_diagnostics: Vec::new(),
            runtime: None,
        }
    }

    /// An environment of `declarations` (in entry order; the registry build
    /// puts them in load order) and no project: the default config, no spec
    /// root, the registry build of exactly these declarations.
    pub fn from_declarations(declarations: Vec<ExtensionDeclaration>) -> Self {
        Environment {
            registries: build_registries(declarations),
            ..Environment::empty()
        }
    }

    /// An environment of exactly `registries` and no project: the default
    /// config, nothing enabled, no spec root (tests, a graph built in
    /// memory).
    pub fn with_registries(registries: RegistryBuild) -> Self {
        Environment {
            registries,
            ..Environment::empty()
        }
    }

    /// Read the project's config and load its extensions into `runtime`
    /// (none without one), which the environment then holds, then build the
    /// registries from them.
    pub fn load(root: &Path, runtime: Option<SharedRuntime>) -> Self {
        Self::from_read(root, read_project_config(root), runtime)
    }

    /// The environment of the config `read` (the one read of
    /// `specforge.json`), its extensions loaded into `runtime`.
    pub fn from_read(root: &Path, read: ConfigRead, runtime: Option<SharedRuntime>) -> Self {
        let config = read.config;
        let mut load_diagnostics: Vec<Diagnostic> = read
            .problems
            .iter()
            .map(config_problem_diagnostic)
            .collect();
        // The lock is read once, here; the extensions load through the
        // production policy into whatever runtime this is given.
        let installed = Installed::at(root);
        let (enabled, declarations) = match runtime.as_deref() {
            Some(runtime) => {
                let loaded = installed.load(&config.extensions, &builtins(), runtime);
                load_diagnostics.extend(loaded.diagnostics);
                (loaded.enabled, loaded.declarations)
            }
            None => (
                config
                    .extensions
                    .iter()
                    .map(|entry| EnabledExtension::unloaded(entry))
                    .collect(),
                Vec::new(),
            ),
        };
        let mut registries = build_registries(declarations);
        // A custom rule's wasm_function is resolved against its extension
        // now, so a name it does not export is reported once (W112).
        if let Some(runtime) = runtime.as_deref() {
            let probes = registries
                .rules
                .probe(&verdicts::WasmVerdicts::probe_only(runtime));
            registries.registry_diagnostics.extend(probes);
        }
        let providers = Providers::register(config.raw.as_ref(), registries.declarations());
        let mut setup_diagnostics = Vec::new();
        if registries.declarations().is_empty() {
            setup_diagnostics.push(structural_only_notice(&config.extensions, &read.problems));
        }
        let spec_root = config.spec_root_in(root);
        Environment {
            root: root.to_path_buf(),
            config,
            config_problems: read.problems,
            config_found: read.found,
            installed,
            enabled,
            spec_root,
            registries,
            providers,
            load_diagnostics,
            setup_diagnostics,
            runtime,
        }
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

    /// The diagnostics reported before any source, in this order: why
    /// `specforge.json` is not used as written (E069), the runtime's load
    /// failures (E028/E033), unknown declaration keys
    /// (W138), the declarations' own (E030, W021, E027, W145), provider
    /// registration (W118/E057), I002, then the registry build's.
    pub fn diagnostics(&self) -> impl Iterator<Item = &Diagnostic> {
        self.load_diagnostics
            .iter()
            .chain(&self.registries.declaration_diagnostics)
            .chain(self.providers.diagnostics())
            .chain(&self.setup_diagnostics)
            .chain(&self.registries.registry_diagnostics)
    }

    /// Surface registration conflicts (E039), which `check` reports last.
    pub fn surface_diagnostics(&self) -> &[Diagnostic] {
        &self.registries.surface_diagnostics
    }

    /// Whether a file (its path relative to the spec root) is left out of
    /// the project, as discovery leaves it out: not a `.spec` file, under a
    /// skipped directory (`target`, `node_modules`, ...) or matched by an
    /// `exclude` entry.
    pub fn excludes(&self, relative: &str) -> bool {
        !is_discovered(relative, &self.config.exclude)
    }

    /// The project sources discovery finds now (`exclude` and the skipped
    /// directories applied).
    pub fn discover(&self) -> Vec<PathBuf> {
        discover_spec_files(&self.spec_root, &self.config.exclude)
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
        let imports = self.import_diagnostics(&sources, &graph);
        SourceBuild {
            sources,
            graph,
            imports,
        }
    }

    /// The import diagnostics of what `sources` and `graph` hold: E025 for
    /// each source that could not be read, then the resolver's (E025,
    /// I004, W113, W027). The same function after a cold read and after
    /// every update.
    pub(crate) fn import_diagnostics(
        &self,
        sources: &SourceCache,
        graph: &GraphBuild,
    ) -> Vec<Diagnostic> {
        let files: Vec<(&str, &SpecFile)> = graph.files().collect();
        sources
            .unreadable()
            .cloned()
            .chain(resolve_imports(&self.spec_root, &files, &|path: &Path| {
                path.is_file()
            }))
            .collect()
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
fn structural_only_notice(configured: &[String], problems: &[ConfigProblem]) -> Diagnostic {
    let (message, suggestion) = if problems.iter().any(ConfigProblem::loads_nothing) {
        (
            "specforge.json could not be read — operating in structural-only mode".to_string(),
            format!("fix specforge.json ({} above)", codes::E069),
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
