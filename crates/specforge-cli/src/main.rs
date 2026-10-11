mod add;
mod analyze;
mod check;
mod collect;

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
mod terminal;
mod trace;
mod trust;
mod update;
mod watch;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use outcome::Exit;
use std::path::{Path, PathBuf};
use terminal::Terminal;

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

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new SpecForge project
    Init(init::Args),
    /// Validate .spec files and report diagnostics
    Check(check::Args),
    /// Export spec graph to stdout in various formats
    Export(export::Args),
    /// Output the Graph Protocol schema
    Schema(export::SchemaArgs),
    /// Render the logical data model (entity kinds, fields, relationships)
    Model(model::Args),
    /// Render the extension architecture (dependencies, enhancements, contributions)
    Outline(outline::Args),
    /// Query the graph at multiple resolutions
    Query(query::Args),
    /// Show traceability chain for an entity, or for every entity
    Trace(trace::Args),
    /// Format .spec files
    Format(format::Args),
    /// Show project statistics
    Stats(stats::Args),
    /// Where to start reading the graph: starting points, hubs and unconnected entities
    Explore(explore::Args),
    /// Coverage gaps around an entity, or of the whole project
    Review(review::Args),
    /// What to look for in code to write a kind's entities
    InferGuide(infer_guide::Args),
    /// Install an extension
    Add(add::Args),
    /// Remove an installed extension
    Remove(remove::Args),
    /// List installed extensions
    Extensions(extensions::Args),
    /// Publish an extension to the registry that serves its name: its binary,
    /// and the declaration read from it as the package's manifest
    Publish(publish::Args),
    /// Search for extensions in registries
    Search(search::Args),
    /// Run static analysis passes over the compiled project
    Analyze(analyze::Args),
    /// Watch a project and rebuild incrementally on changes
    Watch(watch::Args),
    /// Scaffold a new extension project
    New(new::Args),
    /// Update installed extensions to latest compatible versions
    Update(update::Args),
    /// Authenticate with a registry
    Login(login::Args),
    /// Remove registry credentials
    Logout(login::LogoutArgs),
    /// List configured providers
    Providers(providers::Args),
    /// Run the project's test runners and record which entities their tests prove
    Collect(collect::Args),
    /// Run health checks on installed extensions
    Doctor(doctor::Args),
    /// Start MCP server (JSON-RPC over stdio)
    Mcp {
        /// Where to find the project to serve: the nearest directory at or above it holding
        /// specforge.json or specforge.spec (none: nothing is served until a call names a
        /// project or init creates one)
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Migrate .spec files between format versions
    Migrate(migrate::Args),
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
    InferStatus(infer_status::Args),
    /// Extension authoring commands
    Extension {
        #[command(subcommand)]
        action: extension_authoring::Action,
    },
    /// An extension's command: `specforge <extension> <command>`, e.g.
    /// `specforge product features` (the project's extensions declare them)
    #[command(external_subcommand)]
    External(Vec<String>),
}

fn main() {
    let cli = Cli::parse();
    let terminal = Terminal::process();

    let exit_code = match cli.command {
        // An extension command's exit code is its own (ADR 0011).
        Commands::External(argv) => {
            let builtins: Vec<String> = Cli::command()
                .get_subcommands()
                .map(|c| c.get_name().to_string())
                .collect();
            extension_command::run(&argv, &builtins)
        }
        command => run(command, &terminal).code(),
    };
    terminal.flush();
    std::process::exit(exit_code);
}

/// Run one core command at `terminal`.
fn run(command: Commands, terminal: &Terminal) -> Exit {
    match command {
        Commands::Init(args) => args.run(terminal),
        Commands::Check(args) => args.run(terminal),
        Commands::Export(args) => args.run(terminal),
        Commands::Schema(args) => args.run(terminal),
        Commands::Model(args) => args.run(terminal),
        Commands::Outline(args) => args.run(terminal),
        Commands::Query(args) => args.run(terminal),
        Commands::Trace(args) => args.run(terminal),
        Commands::Format(args) => args.run(terminal),
        Commands::Stats(args) => args.run(terminal),
        Commands::Explore(args) => args.run(terminal),
        Commands::Review(args) => args.run(terminal),
        Commands::InferGuide(args) => args.run(terminal),
        Commands::Add(args) => args.run(terminal),
        Commands::Analyze(args) => args.run(terminal),
        Commands::Watch(args) => args.run(terminal),
        Commands::New(args) => args.run(terminal),
        Commands::Remove(args) => args.run(terminal),
        Commands::Extensions(args) => args.run(terminal),
        Commands::Publish(args) => args.run(terminal),
        Commands::Search(args) => args.run(terminal),
        Commands::Update(args) => args.run(terminal),
        Commands::Login(args) => args.run(terminal),
        Commands::Logout(args) => args.run(terminal),
        Commands::Providers(args) => args.run(terminal),
        Commands::Collect(args) => args.run(terminal),
        Commands::Doctor(args) => args.run(terminal),
        Commands::Mcp { path } => mcp::run(&path),
        Commands::Completions { shell } => {
            // The built-ins, and the commands the extensions of the project
            // in the current directory contribute (`specforge product ...`).
            let mut cmd =
                extension_command::with_extension_commands(Cli::command(), Path::new("."));
            clap_complete::generate(shell, &mut cmd, "specforge", &mut std::io::stdout());
            Exit::Passed
        }
        Commands::Explain { code } => explain::run(&code, terminal),
        Commands::Migrate(args) => args.run(terminal),
        Commands::InferStatus(args) => args.run(terminal),
        Commands::External(_) => unreachable!("main runs an extension command itself"),
        Commands::Extension { action } => action.run(terminal),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
