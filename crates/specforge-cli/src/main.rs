mod add;
mod analyze;
mod check;
mod collect;
mod color;
mod doctor;
mod explain;
mod explore;
mod export;
mod extension_authoring;
mod extension_command;
mod extensions;
mod format;
mod infer_guide;
mod infer_status;
mod init;
mod login;
mod mcp;
mod migrate;
mod model;
mod new;
mod options;
mod outcome;
mod outline;
mod pipeline;
mod providers;
mod publish;
mod query;
mod remove;
mod review;
mod search;
mod stats;
mod trace;
mod trust;
mod update;
mod watch;

use clap::builder::{PossibleValuesParser, TypedValueParser};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use outcome::Exit;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "specforge",
    version,
    about = "SpecForge compiler",
    after_help = "Extensions add commands: specforge <extension> <command>, e.g. `specforge product features`.\n`specforge <extension> --help` lists an extension's commands."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Human-readable vs machine-readable output for registry-style commands.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum OutputFormat {
    Human,
    Json,
}

impl OutputFormat {
    /// Report diagnostics that don't stop the command (the registry
    /// configuration's E067/W140/I003): `severity[CODE]: message` on
    /// stderr in either format, so JSON stdout stays one document.
    fn eprint_diagnostics(self, diagnostics: &[specforge_common::Diagnostic]) {
        for diagnostic in diagnostics {
            eprintln!("{}", specforge_common::render_plain(diagnostic));
        }
    }

    /// How a publisher key change is decided for this output: `--yes`
    /// accepts it; JSON output can't ask anyone, so it refuses; a terminal
    /// is asked.
    fn trust(self, assume_yes: bool) -> specforge_ops::extension::Trust {
        use specforge_ops::extension::Trust;
        match (assume_yes, self) {
            (true, _) => Trust::AssumeYes,
            (false, OutputFormat::Json) => Trust::Refuse,
            (false, OutputFormat::Human) => Trust::Prompt,
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new SpecForge project
    Init {
        /// Project name (defaults to directory name)
        #[arg(long)]
        name: Option<String>,

        /// Project version
        #[arg(long, default_value = specforge_ops::init::DEFAULT_VERSION)]
        version: String,

        /// Extensions to install
        #[arg(long)]
        extensions: Vec<String>,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
    /// Validate .spec files and report diagnostics
    Check {
        /// Path to the spec root directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Promote warnings to errors
        #[arg(long)]
        strict: bool,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,

        /// Extra lint profiles: inferred (I200/I202 from specforge-infer.json);
        /// pedantic is the default and adds nothing
        #[arg(
            long,
            value_delimiter = ',',
            value_parser = PossibleValuesParser::new(specforge_project::LINT_PROFILE_NAMES)
                .try_map(|name| name.parse::<specforge_project::LintProfile>())
        )]
        lint: Vec<specforge_project::LintProfile>,

        /// Show only diagnostics of this severity (after --strict); the exit
        /// code and --cache still judge everything reported
        #[arg(
            long,
            ignore_case = true,
            value_parser = PossibleValuesParser::new(specforge_ops::check::SEVERITY_NAMES)
                .try_map(|name| specforge_ops::check::parse_severity(&name))
        )]
        severity: Option<specforge_common::Severity>,

        /// Record each entity's lifecycle state (e.g. a feature's status) in
        /// specforge-cache.json when the check passes (the build cache history
        /// rules compare against)
        #[arg(long)]
        cache: bool,
    },
    /// Export spec graph to stdout in various formats
    Export {
        /// Path to the spec root directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Output format
        #[arg(
            long,
            value_parser = options::choice(&specforge_ops::export::FORMAT),
            default_value = specforge_ops::export::FORMAT.default_name()
        )]
        format: specforge_ops::export::Format,

        /// Scope export to subgraph reachable from this entity ID
        #[arg(long)]
        scope: Option<String>,

        /// With --scope, how many hops from the scoped entity to include
        #[arg(long, requires = "scope")]
        depth: Option<usize>,

        /// Keep only entities of these kinds (comma-separated); the scoped
        /// entity always stays
        #[arg(long, value_delimiter = ',')]
        kinds: Vec<String>,

        /// Suppress schema embedding in `graph` exports (keeps format_version 1.0)
        #[arg(long)]
        no_schema: bool,

        /// Embed the schema in `context` and `brief` exports, and in a
        /// `graph` export under --max-tokens. They leave it out by default:
        /// an agent reading the export needs the entities, and the schema is
        /// most of the bytes. Under --max-tokens an embedded schema counts
        /// toward the budget, and a budget it doesn't fit in fails (E062).
        #[arg(long, conflicts_with = "no_schema")]
        with_schema: bool,

        /// Request a specific schema version for the export
        #[arg(long)]
        schema_version: Option<String>,

        /// Token budget for the export: keeps the most central entities that
        /// fit. The schema is left out unless --with-schema is given, and
        /// then counts toward the budget. The export lists the dropped
        /// entities under `token_budget`; below one entity it is the envelope
        /// with no entities, and below even that it fails (E062). Ignored by
        /// `dot`.
        #[arg(long)]
        max_tokens: Option<usize>,
    },
    /// Output the Graph Protocol schema
    Schema {
        /// Path to the spec root directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// That entity kind and the edge types that can start or end at it
        #[arg(long, conflicts_with = "publish")]
        kind: Option<String>,

        /// Leave the edge types out
        #[arg(long, conflicts_with = "publish")]
        no_edges: bool,

        /// Add the validation rules the loaded extensions declare
        #[arg(long, conflicts_with = "publish")]
        validation_rules: bool,

        /// Publish as standalone JSON Schema (draft 2020-12)
        #[arg(long)]
        publish: bool,

        /// Export format the published schema describes
        #[arg(
            long,
            requires = "publish",
            value_parser = options::choice(&specforge_ops::export::AGENT_FORMAT),
            default_value = specforge_ops::export::AGENT_FORMAT.default_name()
        )]
        format: specforge_ops::export::Format,
    },
    /// Render the logical data model (entity kinds, fields, relationships)
    Model {
        /// Path to the spec root directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Output format
        #[arg(
            long,
            value_parser = options::choice(&specforge_ops::model::MODEL_FORMAT),
            default_value = specforge_ops::model::MODEL_FORMAT.default_name()
        )]
        format: specforge_ops::model::ModelFormat,

        /// How to group entities
        #[arg(
            long,
            value_parser = options::choice(&specforge_ops::model::GROUP_BY),
            default_value = specforge_ops::model::GROUP_BY.default_name()
        )]
        group_by: specforge_ops::model::GroupBy,

        /// Field detail level
        #[arg(
            long,
            value_parser = options::choice(&specforge_ops::model::MODEL_FIELDS),
            default_value = specforge_ops::model::MODEL_FIELDS.default_name()
        )]
        fields: specforge_ops::model::FieldLevel,

        /// Filter to a single extension
        #[arg(long)]
        extension: Option<String>,

        /// Filter to specific entity kinds (comma-separated)
        #[arg(long, value_delimiter = ',')]
        kinds: Vec<String>,

        /// Root entity kind for depth-scoped output
        #[arg(long)]
        root: Option<String>,

        /// Maximum depth from root kind (requires --root)
        #[arg(long, requires = "root")]
        depth: Option<usize>,
    },
    /// Render the extension architecture (dependencies, enhancements, contributions)
    Outline {
        /// Path to the spec root directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Output format
        #[arg(
            long,
            value_parser = options::choice(&specforge_ops::model::OUTLINE_FORMAT),
            default_value = specforge_ops::model::OUTLINE_FORMAT.default_name()
        )]
        format: specforge_ops::model::OutlineFormat,

        /// Detail level
        #[arg(
            long,
            value_parser = options::choice(&specforge_ops::model::OUTLINE_FIELDS),
            default_value = specforge_ops::model::OUTLINE_FIELDS.default_name()
        )]
        fields: specforge_ops::model::OutlineDetail,

        /// Dependency visibility
        #[arg(
            long,
            value_parser = options::choice(&specforge_ops::model::DEPS),
            default_value = specforge_ops::model::DEPS.default_name()
        )]
        deps: specforge_ops::model::DependencyDepth,
    },
    /// Query the graph at multiple resolutions
    Query {
        /// Entity ID to query
        entity: String,

        /// Path to the spec root directory
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Number of hops from the entity (0 = entity only)
        #[arg(long, default_value_t = specforge_ops::query::DEFAULT_DEPTH)]
        depth: usize,

        /// Filter results to specific entity kinds (can be repeated)
        #[arg(long)]
        kind: Vec<String>,

        /// Output detail level
        #[arg(
            long,
            value_parser = options::choice(&specforge_ops::export::AGENT_FORMAT),
            default_value = specforge_ops::export::AGENT_FORMAT.default_name()
        )]
        format: specforge_ops::export::Format,

        /// Give every entity its coverage status
        #[arg(long)]
        include_coverage: bool,
    },
    /// Show traceability chain for an entity, or for every entity
    Trace {
        /// Entity ID to trace (omit to trace every entity)
        entity: Option<String>,

        /// Path to the spec root directory
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Output format: json or human
        #[arg(long, default_value = "json")]
        format: OutputFormat,
    },
    /// Format .spec files
    Format {
        /// Explicit file or directory paths to format
        #[arg()]
        paths: Vec<String>,

        /// Path to the spec root directory
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Check formatting without modifying files (exit 1 if unformatted)
        #[arg(long)]
        check: bool,

        /// Show unified diff of formatting changes
        #[arg(long)]
        diff: bool,

        /// Read from stdin, write to stdout
        #[arg(long)]
        stdin: bool,
    },
    /// Show project statistics
    Stats {
        /// Path to the spec root directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
    /// Where to start reading the graph: starting points, hubs and unconnected entities
    Explore {
        /// Entity ID to explore from (omit to explore the whole project)
        entity: Option<String>,

        /// Path to the spec root directory
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Only entities of this kind
        #[arg(long)]
        kind: Option<String>,

        /// Hops from the entity (unbounded when omitted)
        #[arg(long)]
        depth: Option<usize>,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
    /// Coverage gaps around an entity, or of the whole project
    Review {
        /// Entity ID to review (omit to review the whole project)
        entity: Option<String>,

        /// Path to the spec root directory
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Hops around the entity
        #[arg(long, default_value_t = specforge_ops::review::DEFAULT_DEPTH)]
        depth: usize,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
    /// What to look for in code to write a kind's entities
    InferGuide {
        /// Entity kind (omit for every declared kind)
        kind: Option<String>,

        /// Path to the spec root directory
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
    /// Install an extension
    Add {
        /// Extension specifier (e.g., @scope/name@1.0.0 or ./path)
        specifier: String,

        /// Path to the project root
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,

        /// Accept unsigned packages (publisher verification skipped)
        #[arg(long, default_value_t = false)]
        allow_unsigned: bool,

        /// Accept key changes non-interactively (for CI)
        #[arg(long, default_value_t = false)]
        yes: bool,
    },
    /// Remove an installed extension
    Remove {
        /// Extension name
        name: String,

        /// Path to the project root
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Force removal even if other extensions depend on it
        #[arg(long)]
        force: bool,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
    /// List installed extensions
    Extensions {
        /// Path to the project root
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
    /// Publish an extension to the registry that serves its name: its binary,
    /// and the declaration read from it as the package's manifest
    Publish {
        /// The extension to publish: a .wasm component, or the extension's
        /// crate directory (its target/wasm32-wasip2/release component).
        /// Defaults to the --path directory
        extension: Option<PathBuf>,

        /// The project whose specforge.json configures the registries
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
    /// Search for extensions in registries
    Search {
        /// Search query
        query: String,

        /// Only extensions that declare this category of contribution
        #[arg(long, value_parser = options::choice(&specforge_ops::registry::CONTRIBUTES))]
        contributes: Option<specforge_protocol_types::DeclaredCategory>,

        /// Path to the project root (for registry config)
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
    /// Run static analysis passes over the compiled project
    Analyze {
        /// Analysis pass to run: all (default), coverage, contracts, or a pass
        /// an extension declares (`<extension>:<pass>`)
        pass: Option<String>,

        /// Project directory (defaults to current directory)
        #[arg(long)]
        path: Option<String>,

        /// Machine-readable output
        #[arg(long, default_value_t = false)]
        json: bool,

        /// Fail (exit 1) on warnings as well as errors
        #[arg(long, default_value_t = false)]
        strict: bool,

        /// Test-results report for proof-level verdicts (default: the project's
        /// specforge-report.json, written by `specforge collect`)
        #[arg(long)]
        test_results: Option<String>,

        /// Verify declared bounds and claims with an SMT solver (z3)
        #[arg(long, default_value_t = false)]
        prove: bool,

        /// Fail with a non-zero exit when proof coverage falls below this
        /// percentage (needs test results)
        #[arg(long)]
        min: Option<f64>,
    },
    /// Watch a project and rebuild incrementally on changes
    Watch {
        /// Project directory (defaults to current directory)
        #[arg(long)]
        path: Option<String>,

        /// Emit one JSON object per rebuild cycle
        #[arg(long, default_value_t = false)]
        json: bool,

        /// Check every incremental rebuild against a cold rebuild (slower)
        #[arg(long, default_value_t = false)]
        verify_incremental: bool,
    },
    /// Scaffold a new extension project
    New {
        /// Extension name (e.g. @you/my-ext)
        name: String,

        /// Scaffold an extension project (SDK-authored wasm)
        #[arg(long, default_value_t = false)]
        extension: bool,

        /// Directory to scaffold into
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
    /// Update installed extensions to latest compatible versions
    Update {
        /// Extension name (updates all if omitted)
        name: Option<String>,

        /// Path to the project root
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,

        /// Allow new major versions (otherwise each extension stays within
        /// its locked version's caret range)
        #[arg(long, default_value_t = false)]
        major: bool,

        /// Accept unsigned packages (publisher verification skipped)
        #[arg(long, default_value_t = false)]
        allow_unsigned: bool,

        /// Accept key changes non-interactively (for CI)
        #[arg(long, default_value_t = false)]
        yes: bool,
    },
    /// Authenticate with a registry
    Login {
        /// Registry alias (defaults to the default registry)
        #[arg(long)]
        registry: Option<String>,

        /// Authentication token, kept as a secret (OS keyring, else a 0600 file).
        /// Exactly one of --token, --token-env, --token-file
        #[arg(long)]
        token: Option<String>,

        /// Environment variable that holds the token; the variable's name is kept, not the token
        #[arg(long)]
        token_env: Option<String>,

        /// File that holds the token; the file's path is kept, not the token
        #[arg(long)]
        token_file: Option<PathBuf>,

        /// Path to the project root (for registry config)
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
    /// Remove registry credentials
    Logout {
        /// Registry alias (defaults to the default registry)
        #[arg(long)]
        registry: Option<String>,

        /// The project whose specforge.json names the default registry (when
        /// --registry is not given)
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
    /// List configured providers
    Providers {
        /// Path to the project root
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
    /// Run the project's test runners and record which entities their tests prove
    Collect {
        /// Path to the project root
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Collector to use (e.g. cargo-test). Detected from project files if omitted.
        #[arg(long, alias = "collector", value_name = "NAME")]
        runner: Option<String>,

        /// Parse the runner's existing report instead of running it
        #[arg(long)]
        no_run: bool,

        /// Report files to parse instead of running the runner (implies --no-run)
        #[arg(long = "report", value_name = "FILE")]
        reports: Vec<PathBuf>,

        /// Run the declared test command without asking (for CI)
        #[arg(long)]
        yes: bool,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
    /// Run health checks on installed extensions
    Doctor {
        /// Path to the project root
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
    /// Start MCP server (JSON-RPC over stdio)
    Mcp {
        /// Where to find the project to serve: the nearest directory at or above it holding
        /// specforge.json or specforge.spec (none: nothing is served until a call names a
        /// project or init creates one)
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Migrate .spec files between format versions
    Migrate {
        /// Path to the spec root directory
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Show unified diff without modifying files
        #[arg(long)]
        dry_run: bool,

        /// Skip creating .spec.bak backup files
        #[arg(long)]
        no_backup: bool,

        /// Restore files from .spec.bak backups
        #[arg(long)]
        rollback: bool,

        /// Target format version (defaults to current)
        #[arg(long)]
        target_version: Option<String>,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
    /// Generate shell completions: the built-in commands, and the commands
    /// of the extensions the project in the current directory enables
    Completions {
        /// Target shell
        #[arg(value_enum)]
        shell: Shell,
    },
    /// Explain a diagnostic code
    Explain {
        /// Diagnostic code (e.g., E001, W010)
        code: String,
    },
    /// Show inference progress (analyzed files, gaps, stale entries)
    InferStatus {
        /// Path to the project root
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,

        /// Show unanalyzed files grouped by directory
        #[arg(long)]
        gaps: bool,

        /// Show stale and deleted files
        #[arg(long)]
        stale: bool,

        /// Show detailed gap analysis (pub Rust items without spec entities)
        #[arg(long)]
        gaps_detail: bool,
    },
    /// Extension authoring commands
    Extension {
        #[command(subcommand)]
        action: ExtensionAction,
    },
    /// An extension's command: `specforge <extension> <command>`, e.g.
    /// `specforge product features` (the project's extensions declare them)
    #[command(external_subcommand)]
    External(Vec<String>),
}

#[derive(Subcommand)]
enum ExtensionAction {
    /// Scaffold an extension crate written with the SDK
    Init {
        /// Extension name
        #[arg(long)]
        name: Option<String>,

        /// Path for the new extension
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
    /// Build the extension's component (cargo build --release --target
    /// wasm32-wasip2)
    Build {
        /// Path to the extension project
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
    /// Load the built component and report its declaration's diagnostics
    Validate {
        /// Path to the extension project
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Output format: human or json
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
}

fn main() {
    let cli = Cli::parse();

    let exit_code = match cli.command {
        // An extension command's exit code is its own (ADR 0011).
        Commands::External(argv) => {
            let builtins: Vec<String> = Cli::command()
                .get_subcommands()
                .map(|c| c.get_name().to_string())
                .collect();
            extension_command::run(&argv, &builtins)
        }
        command => run(command).code(),
    };
    std::process::exit(exit_code);
}

/// Run one core command.
fn run(command: Commands) -> Exit {
    match command {
        Commands::Init {
            name,
            version,
            extensions,
            format,
        } => {
            let path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            init::run(&path, name.as_deref(), &version, &extensions, format)
        }
        Commands::Check {
            path,
            strict,
            format,
            lint,
            severity,
            cache,
        } => check::run(&path, strict, format, &lint, severity, cache),
        Commands::Export {
            path,
            format,
            scope,
            depth,
            kinds,
            no_schema,
            with_schema,
            schema_version,
            max_tokens,
        } => export::run(
            &path,
            &specforge_ops::export::Request {
                format: Some(format),
                scope: scope.as_deref(),
                depth,
                kinds: kinds.iter().map(String::as_str).collect(),
                max_tokens,
                // context, brief and a budgeted graph export leave the schema
                // out unless asked for it (the policy lives in specforge-ops).
                schema: match (no_schema, with_schema) {
                    (true, _) => specforge_ops::export::Schema::Without,
                    (_, true) => specforge_ops::export::Schema::With,
                    _ => specforge_ops::export::Schema::Default,
                },
                schema_version: schema_version.as_deref(),
            },
        ),
        Commands::Schema {
            path,
            kind,
            no_edges,
            validation_rules,
            publish,
            format,
        } => export::run_schema(
            &path,
            &specforge_ops::schema::SchemaRequest {
                kind: kind.as_deref(),
                edges: !no_edges,
                validation_rules,
            },
            publish.then_some(format),
        ),
        Commands::Model {
            path,
            format,
            group_by,
            fields,
            extension,
            kinds,
            root,
            depth,
        } => model::run(
            &path,
            &specforge_ops::model::ModelOptions {
                format,
                group_by,
                fields,
                extension,
                kinds,
                root: root.map(|kind| specforge_ops::model::ModelRoot { kind, depth }),
            },
        ),
        Commands::Outline {
            path,
            format,
            fields,
            deps,
        } => outline::run(
            &path,
            &specforge_ops::model::OutlineOptions {
                format,
                detail: fields,
                deps,
            },
        ),
        Commands::Query {
            entity,
            path,
            depth,
            kind,
            format,
            include_coverage,
        } => query::run(
            &path,
            &specforge_ops::query::QueryRequest {
                entity_id: &entity,
                depth: Some(depth),
                kinds: kind.iter().map(String::as_str).collect(),
                format: Some(format),
                include_coverage,
            },
        ),
        Commands::Trace {
            entity,
            path,
            format,
        } => trace::run(&path, entity.as_deref(), format),
        Commands::Format {
            paths,
            path,
            check,
            diff,
            stdin,
        } => format::run(&path, check, diff, stdin, &paths),
        Commands::Stats { path, format } => stats::run(&path, format),
        Commands::Explore {
            entity,
            path,
            kind,
            depth,
            format,
        } => explore::run(
            &path,
            &specforge_ops::explore::ExplorationRequest {
                entity_id: entity.as_deref(),
                kind: kind.as_deref(),
                depth,
            },
            format,
        ),
        Commands::Review {
            entity,
            path,
            depth,
            format,
        } => review::run(
            &path,
            &specforge_ops::review::ReviewRequest {
                entity_id: entity.as_deref(),
                depth,
            },
            format,
        ),
        Commands::InferGuide { kind, path, format } => {
            infer_guide::run(&path, kind.as_deref(), format)
        }
        Commands::Add {
            specifier,
            path,
            format,
            allow_unsigned,
            yes,
        } => add::run(&specifier, &path, format, allow_unsigned, format.trust(yes)),
        Commands::Analyze {
            pass,
            path,
            json,
            strict,
            test_results,
            min,
            prove,
        } => {
            let project = path
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            analyze::run(
                &project,
                pass,
                json,
                strict,
                test_results.as_deref().map(Path::new),
                min,
                prove,
            )
        }
        Commands::Watch {
            path,
            json,
            verify_incremental,
        } => {
            let project = path
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            watch::run(&project, json, verify_incremental)
        }
        Commands::New {
            name,
            extension,
            path,
            format,
        } => new::run(&name, extension, &path, format),
        Commands::Remove {
            name,
            path,
            force,
            format,
        } => remove::run(&name, &path, force, format),
        Commands::Extensions { path, format } => extensions::run(&path, format),
        Commands::Publish {
            extension,
            path,
            format,
        } => publish::run(extension.as_deref().unwrap_or(&path), &path, format),
        Commands::Search {
            query,
            contributes,
            path,
            format,
        } => search::run(&query, contributes, &path, format),
        Commands::Update {
            name,
            path,
            format,
            major,
            allow_unsigned,
            yes,
        } => update::run(
            name.as_deref(),
            &path,
            format,
            major,
            allow_unsigned,
            format.trust(yes),
        ),
        Commands::Login {
            registry,
            token,
            token_env,
            token_file,
            path,
            format,
        } => login::run(
            registry.as_deref(),
            &login::TokenSource {
                token: token.as_deref(),
                token_env: token_env.as_deref(),
                token_file: token_file.as_deref(),
            },
            &path,
            format,
        ),
        Commands::Logout {
            registry,
            path,
            format,
        } => login::run_logout(registry.as_deref(), &path, format),
        Commands::Providers { path, format } => providers::run(&path, format),
        Commands::Collect {
            path,
            runner,
            no_run,
            reports,
            yes,
            format,
        } => collect::run(
            &path,
            &collect::Options {
                runner: runner.as_deref(),
                no_run,
                reports: &reports,
                yes,
            },
            format,
        ),
        Commands::Doctor { path, format } => doctor::run(&path, format),
        Commands::Mcp { path } => mcp::run(&path),
        Commands::Completions { shell } => {
            // The built-ins, and the commands the extensions of the project
            // in the current directory contribute (`specforge product ...`).
            let mut cmd =
                extension_command::with_extension_commands(Cli::command(), Path::new("."));
            clap_complete::generate(shell, &mut cmd, "specforge", &mut std::io::stdout());
            Exit::Passed
        }
        Commands::Explain { code } => explain::run(&code),
        Commands::Migrate {
            path,
            dry_run,
            no_backup,
            rollback,
            target_version,
            format,
        } => migrate::run(
            &path,
            dry_run,
            no_backup,
            rollback,
            target_version.as_deref(),
            format,
        ),
        Commands::InferStatus {
            path,
            format,
            gaps,
            stale,
            gaps_detail,
        } => infer_status::run(&path, format, gaps, stale, gaps_detail),
        Commands::External(_) => unreachable!("main runs an extension command itself"),
        Commands::Extension { action } => match action {
            ExtensionAction::Init { name, path, format } => {
                extension_authoring::run_init(&path, name.as_deref(), format)
            }
            ExtensionAction::Build { path, format } => {
                extension_authoring::run_build(&path, format)
            }
            ExtensionAction::Validate { path, format } => {
                extension_authoring::run_validate(&path, format)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_is_yes_then_what_the_output_can_ask() {
        use specforge_ops::extension::Trust;
        assert_eq!(OutputFormat::Human.trust(true), Trust::AssumeYes);
        assert_eq!(OutputFormat::Json.trust(true), Trust::AssumeYes);
        assert_eq!(OutputFormat::Json.trust(false), Trust::Refuse);
        assert_eq!(OutputFormat::Human.trust(false), Trust::Prompt);
    }

    /// The plain long help of `command` and of each subcommand under it,
    /// by the snapshot name `help_root` or `help_<path joined by _>`.
    fn pin_help(command: &clap::Command, path: &[String]) {
        let name = if path.is_empty() {
            "help_root".to_string()
        } else {
            format!("help_{}", path.join("_"))
        };
        insta::assert_snapshot!(name, command.clone().render_long_help().to_string());
        for sub in command.get_subcommands() {
            // The generated `help` command and the external-subcommand
            // placeholder have no help of their own to pin.
            if sub.get_name() == "help" || sub.is_allow_external_subcommands_set() {
                continue;
            }
            let mut below = path.to_vec();
            below.push(sub.get_name().to_string());
            pin_help(sub, &below);
        }
    }

    /// Every core command's `--help` is pinned, so moving the arguments out
    /// of this file cannot change a byte of it (plan 11).
    #[test]
    fn every_core_command_help_is_pinned() {
        let mut root = Cli::command();
        root.build();
        pin_help(&root, &[]);
    }

    /// Today `analyze` and `watch` take `--json`, not `--format` (T6 flips
    /// this).
    #[test]
    fn analyze_and_watch_take_json_today() {
        use clap::error::ErrorKind;
        for command in ["analyze", "watch"] {
            assert!(Cli::try_parse_from(["specforge", command, "--json"]).is_ok());
            let refused = Cli::try_parse_from(["specforge", command, "--format", "json"])
                .err()
                .expect("--format is refused");
            assert_eq!(refused.kind(), ErrorKind::UnknownArgument);
        }
    }

    /// `specforge query --depth` defaults to the query operation's constant,
    /// the one `specforge.query`'s schema advertises (plan 08 T8).
    #[test]
    fn the_query_depth_default_is_the_operations() {
        let command = Cli::command();
        let query = command.find_subcommand("query").expect("a query command");
        let depth = query
            .get_arguments()
            .find(|argument| argument.get_id() == "depth")
            .expect("query takes --depth");
        let defaults: Vec<String> = depth
            .get_default_values()
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        assert_eq!(defaults, [specforge_ops::query::DEFAULT_DEPTH.to_string()]);
    }
}
