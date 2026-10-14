//! The `specforge` binary: each command's arguments, the terminal it writes to and asks from, and the exit code
//! it ends with. A core command turns its arguments into a request to `specforge-ops` and presents the outcome
//! (ADR 0029); `specforge mcp` serves `specforge-mcp` over stdio. Extension commands are read from the loaded
//! extensions' declarations.

#![deny(clippy::print_stdout, clippy::print_stderr)]

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
mod project;
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
use terminal::{Stream, Terminal};

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
    Mcp(mcp::Args),
    /// Migrate .spec files between format versions
    Migrate(migrate::Args),
    /// Generate shell completions: the built-in commands, and the commands
    /// of the extensions the project at --path enables
    Completions {
        /// Target shell
        #[arg(value_enum)]
        shell: Shell,

        #[command(flatten)]
        project: project::ProjectArg,
    },
    /// Explain a diagnostic code
    Explain(explain::Args),
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
            extension_command::run(&argv, &builtins, &terminal)
        }
        command => run(command, &terminal).code(),
    };
    terminal.flush();
    std::process::exit(exit_code);
}

/// `specforge completions`: the built-ins, and the commands the extensions
/// of the project at `--path` contribute (`specforge product ...`). A path
/// that does not exist is refused like on every command.
fn completions(shell: Shell, project: &project::ProjectArg, terminal: &Terminal) -> Exit {
    let root = match project.root() {
        Ok(root) => root,
        Err(error) => {
            return outcome::Refusal::of(outcome::OutputFormat::Human).report(terminal, &error);
        }
    };
    let mut cmd = extension_command::with_extension_commands(Cli::command(), &root);
    clap_complete::generate(
        shell,
        &mut cmd,
        "specforge",
        &mut terminal.writer(Stream::Out),
    );
    Exit::Passed
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
        Commands::Mcp(args) => args.run(terminal),
        Commands::Completions { shell, project } => completions(shell, &project, terminal),
        Commands::Explain(args) => args.run(terminal),
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

    /// The commands that read no project: they take no `--path`.
    const NO_PROJECT: [&str; 8] = [
        "init",
        "new",
        "explain",
        "mcp",
        "extension",
        "extension init",
        "extension build",
        "extension validate",
    ];

    /// Every command of the tree under `command` (not the generated `help`
    /// nor the external-subcommand placeholder), by its path from the root.
    fn commands<'a>(
        command: &'a clap::Command,
        at: &str,
        found: &mut Vec<(String, &'a clap::Command)>,
    ) {
        for sub in command.get_subcommands() {
            if sub.get_name() == "help" || sub.is_allow_external_subcommands_set() {
                continue;
            }
            let name = if at.is_empty() {
                sub.get_name().to_string()
            } else {
                format!("{at} {}", sub.get_name())
            };
            found.push((name.clone(), sub));
            commands(sub, &name, found);
        }
    }

    #[specforge_test_macros::test(
        behavior = "locate_project_root",
        verify = "every command that reads a project takes the one --path option, and none takes a project path as a positional argument"
    )]
    fn every_command_that_reads_a_project_takes_the_project_arg() {
        let mut root = Cli::command();
        root.build();
        let mut found = Vec::new();
        commands(&root, "", &mut found);
        for name in NO_PROJECT {
            assert!(
                found.iter().any(|(command, _)| command == name),
                "NO_PROJECT names `{name}`, which is no command"
            );
        }
        let project = crate::project::ProjectArg::arg();
        let shown = |value: Option<&clap::builder::StyledStr>| value.map(ToString::to_string);
        for (name, command) in &found {
            let paths: Vec<&clap::Arg> = command
                .get_arguments()
                .filter(|argument| {
                    argument.get_id() == crate::project::ProjectArg::NAME
                        || argument.get_long() == Some(crate::project::ProjectArg::NAME)
                })
                .collect();
            if NO_PROJECT.contains(&name.as_str()) {
                assert!(
                    paths.is_empty(),
                    "`specforge {name}` reads no project but takes --path"
                );
                continue;
            }
            // A command with subcommands only dispatches; its leaves read.
            if command.has_subcommands() {
                assert!(
                    paths.is_empty(),
                    "`specforge {name}` dispatches but takes --path"
                );
                continue;
            }
            let [path] = paths.as_slice() else {
                panic!(
                    "`specforge {name}` reads a project and takes no --path (flatten ProjectArg, or list it in NO_PROJECT)"
                );
            };
            assert!(
                !path.is_positional(),
                "`specforge {name}` takes its project positionally"
            );
            assert_eq!(path.get_long(), project.get_long(), "`specforge {name}`");
            assert_eq!(
                shown(path.get_help()),
                shown(project.get_help()),
                "`specforge {name}`"
            );
            assert_eq!(
                path.get_default_values(),
                project.get_default_values(),
                "`specforge {name}`"
            );
        }
    }

    /// Every argument of `command` and of each subcommand under it.
    fn all_arguments(command: &clap::Command, found: &mut Vec<(String, clap::Arg)>) {
        for argument in command.get_arguments() {
            found.push((command.get_name().to_string(), argument.clone()));
        }
        for sub in command.get_subcommands() {
            all_arguments(sub, found);
        }
    }

    #[specforge_test_macros::test(
        behavior = "report_command_outcome",
        verify = "every core command with a JSON form takes --format human|json, and none takes --json"
    )]
    fn every_json_output_is_asked_for_with_format() {
        let mut root = Cli::command();
        root.build();
        let mut arguments = Vec::new();
        all_arguments(&root, &mut arguments);
        for (command, argument) in &arguments {
            assert_ne!(
                argument.get_long(),
                Some("json"),
                "`specforge {command}` takes --json"
            );
        }
        for name in ["analyze", "watch", "format"] {
            let command = root.find_subcommand(name).expect("a core command");
            let format = command
                .get_arguments()
                .find(|argument| argument.get_id() == "format")
                .unwrap_or_else(|| panic!("`specforge {name}` takes --format"));
            let values: Vec<String> = format
                .get_possible_values()
                .iter()
                .map(|value| value.get_name().to_string())
                .collect();
            assert_eq!(values, ["human", "json"], "`specforge {name} --format`");
            let defaults: Vec<_> = format.get_default_values().iter().collect();
            assert_eq!(defaults, ["human"], "`specforge {name} --format` default");
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
        assert_eq!(
            defaults,
            [specforge_ops::read::query::DEFAULT_DEPTH.to_string()]
        );
    }
}
