//! `specforge <extension> <command> [args]`: the commands extensions
//! contribute (ADR 0008).
//!
//! Any first argument that is not a built-in command names an extension by
//! its short name (`product` for `@specforge/product`); `ext:command` is the
//! same as `ext command`. The project is the one at `--path` (default `.`),
//! and the output the one `--format` asks for (`human`, the default, or
//! `json`): options every extension command has, the host's, which no
//! declared arg may take (ADR 0011). Its extensions declare the commands,
//! and `specforge_ops::command::ExtensionCommand` derives each one's command
//! line (a required arg is positional, in declaration order; any other is a
//! `--option`; every flag is a `--flag`, set or not) and the args its export
//! receives, by the one arg rule MCP also sends them by (ADR 0017). This
//! module renders that derivation as clap's command line, routes the
//! command over the project's environment, compiles the project from that
//! environment, and runs the command through
//! `specforge_ops::command::run`; the export's stdout and stderr are printed
//! as they are, and its exit code is the CLI's.

use crate::outcome::Exit;
use crate::terminal::{Stream, Terminal, err, errln, out};
use clap::builder::PossibleValuesParser;
use clap::parser::ValueSource;
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::{Map, Value};
use specforge_common::ProjectRoot;
use specforge_ops::OpError;
use specforge_ops::command::{
    ArgShape, ArgValue, CommandFormat, ExtensionArg, ExtensionCommand, ExtensionCommands, RunError,
};
use specforge_ops::view::ProjectView;
use specforge_project::{CompiledProject, Environment};
use specforge_protocol_types::command_args::{ArgError, normalize_arg};
use specforge_protocol_types::{CommandError, CommandOutput};
use std::path::PathBuf;
use std::sync::Arc;

/// The host's own option on every extension command: where the project is.
const PATH: &str = "path";

/// The host's own option on every extension command: the output asked for.
const FORMAT: &str = "format";

/// Run the extension command `argv` names (`argv[0]` is the extension).
/// `builtins` are the CLI's own commands, suggested for a name no extension
/// has.
///
/// Only the project's environment (its config and its extensions' declared
/// surfaces) is loaded to route the command; the project is compiled from it
/// (its sources read, its graph built and checked) only once a declared
/// command is matched.
pub fn run(argv: &[String], builtins: &[String], terminal: &Terminal) -> i32 {
    let Some((first, rest)) = argv.split_first() else {
        return INVALID_INPUT_EXIT;
    };
    // `product:features` is `product features`.
    let (ext, mut rest) = match first.split_once(':') {
        Some((ext, command)) => {
            let mut rest = rest.to_vec();
            rest.insert(0, command.to_string());
            (ext.to_string(), rest)
        }
        None => (first.clone(), rest.to_vec()),
    };

    let root = match ProjectRoot::of(&project_path(&rest)) {
        Ok(root) => root,
        Err(error) => {
            let error = OpError::from(error);
            if asks_for_json(&rest) {
                let line = serde_json::json!({"code": error.code, "message": error.message});
                errln!(terminal, "{line}");
            } else {
                errln!(terminal, "error[{}]: {}", error.code, error.message);
            }
            return Exit::Failed.code();
        }
    };
    let env = Environment::load(
        &root,
        Some(Arc::new(
            specforge_component::ComponentRuntime::with_user_cache(),
        )),
    );
    let routed = ExtensionCommands::build(env.registries());
    let commands: Vec<&ExtensionCommand> = routed.of(&ext).collect();
    if commands.is_empty() {
        errln!(
            terminal,
            "error: unrecognized subcommand '{ext}': no built-in command, and no extension of the project at {} with that name contributes commands",
            root.dir().display()
        );
        let mut names: Vec<&str> = builtins.iter().map(String::as_str).collect();
        names.extend(routed.shorts());
        if let Some(close) = specforge_common::find_close_match(&ext, names) {
            errln!(terminal, "\n  tip: a similar subcommand exists: '{close}'");
        }
        errln!(terminal, "\nFor more information, try 'specforge --help'.");
        return INVALID_INPUT_EXIT;
    }
    let requested = rest.first().and_then(|requested| {
        commands
            .iter()
            .copied()
            .find(|c| c.cli_name() == *requested)
    });
    if let Some(command) = requested {
        if let Some(why) = command.refusal() {
            errln!(
                terminal,
                "error: {}'s command '{}' cannot run on the command line: {why}",
                command.extension(),
                command.cli_name()
            );
            return INVALID_INPUT_EXIT;
        }
        // A later extension's command of this name is not routed (D13):
        // say which, so its author can tell why it never runs.
        for shadowed in routed
            .shadowed()
            .iter()
            .filter(|s| s.short() == ext && s.cli_name() == command.cli_name())
        {
            errln!(
                terminal,
                "note: {}'s command '{}' is not routed: {}'s command of that name came first",
                shadowed.extension(),
                shadowed.cli_name(),
                command.extension()
            );
        }
    }

    // Known before parsing, so a usage error is written in the format asked
    // for wherever `--format` is on the command line, or if parsing stops
    // before reaching it.
    let json = asks_for_json(&rest);
    rest.insert(0, format!("specforge {ext}"));
    let matches = match command_line(&ext, &commands).try_get_matches_from(rest) {
        Ok(matches) => matches,
        Err(e) if json && !is_display(&e) => {
            errln!(terminal, "{}", usage_error(&e, requested));
            return INVALID_INPUT_EXIT;
        }
        Err(e) => {
            // Clap's own error, coloured as the stream it goes to is.
            let (stream, color) = if e.use_stderr() {
                (Stream::Err, terminal.facts().color_err)
            } else {
                (Stream::Out, terminal.facts().color_out)
            };
            let rendered = e.render();
            if color {
                terminal.write(stream, format_args!("{}", rendered.ansi()));
            } else {
                terminal.write(stream, format_args!("{rendered}"));
            }
            return e.exit_code();
        }
    };
    let Some((name, matches)) = matches.subcommand() else {
        return INVALID_INPUT_EXIT;
    };
    let Some(command) = commands.iter().copied().find(|c| c.cli_name() == name) else {
        return INVALID_INPUT_EXIT;
    };

    let format = format_value(matches);
    // What the command line set, as clap typed it; the operation normalizes
    // it by the rule MCP's arguments go through.
    let given = arg_values(command, matches);
    // The project, compiled from the environment that routed the command:
    // its sources read, its graph built and checked (ADR 0011, "One
    // operation runs a command", O3).
    let project = CompiledProject::of(env);
    let outcome = specforge_ops::command::run(&ProjectView::of(&project), command, &given, format);
    ended(outcome, format, terminal)
}

/// What a command's run writes, and the exit code: the output's stdout and
/// stderr as the command printed them and its exit code; a failure written
/// to stderr in the format asked for ([`written`]), exit 2 for args the rule
/// refuses (as clap's usage errors, ADR 0011 B), else 1.
fn ended(
    outcome: Result<CommandOutput, RunError>,
    format: CommandFormat,
    terminal: &Terminal,
) -> i32 {
    match outcome {
        Ok(output) => {
            out!(terminal, "{}", output.stdout);
            err!(terminal, "{}", output.stderr);
            output.exit_code
        }
        Err(error) => {
            err!(terminal, "{}", written(&error, format));
            match error {
                RunError::Args(_) => Exit::Unjudged.code(),
                RunError::NoProject(_) | RunError::Call(_) => Exit::Failed.code(),
            }
        }
    }
}

/// A failed run as the CLI writes it: under `json` the command's error
/// object ([`RunError::to_command_error`]) on one line; under `human` the
/// arg rule's `error: <message>`, the E028 diagnostic's plain rendering, or
/// `error[no_project]: <message>`.
fn written(error: &RunError, format: CommandFormat) -> String {
    match format {
        CommandFormat::Json => {
            let mut line = serde_json::to_string(&error.to_command_error())
                .expect("command error serialization cannot fail");
            line.push('\n');
            line
        }
        CommandFormat::Human => match error {
            RunError::Args(error) => format!("error: {error}\n"),
            RunError::Call(error) => {
                format!("{}\n", specforge_common::render_plain(&error.diagnostic()))
            }
            RunError::NoProject(error) => format!("error[{}]: {}\n", error.code, error.message),
        },
    }
}

/// `cli` with a subcommand per extension of the project at `root` that
/// contributes commands, as `specforge <ext>` routes them: what shell
/// completions are generated from. An extension whose short name is a
/// built-in command's is left out (the built-in wins), and so is any
/// command the host refuses.
pub fn with_extension_commands(mut cli: Command, root: &ProjectRoot) -> Command {
    let env = Environment::load(
        root,
        Some(Arc::new(
            specforge_component::ComponentRuntime::with_user_cache(),
        )),
    );
    let routed = ExtensionCommands::build(env.registries());
    for ext in routed.shorts() {
        if cli.find_subcommand(ext).is_none() {
            let commands: Vec<&ExtensionCommand> = routed.of(ext).collect();
            let about = format!("{ext} extension commands");
            cli = cli.subcommand(command_line(ext, &commands).about(about));
        }
    }
    cli
}

/// The exit code of a usage error clap catches, `INVALID_INPUT`'s: the
/// one commands give the usage errors they catch themselves (ADR 0011).
const INVALID_INPUT_EXIT: i32 = crate::outcome::Exit::Unjudged.code();

/// Whether `args` ask for `--format json` (`--format json` or
/// `--format=json`), read before the command line is parsed.
fn asks_for_json(args: &[String]) -> bool {
    let flag = format!("--{FORMAT}");
    let json = CommandFormat::Json.as_str();
    let mut args = args.iter().take_while(|a| *a != "--");
    while let Some(arg) = args.next() {
        if *arg == flag {
            if args.next().is_some_and(|v| v == json) {
                return true;
            }
        } else if arg.strip_prefix(&format!("{flag}=")) == Some(json) {
            return true;
        }
    }
    false
}

/// Whether `e` is clap's help or version rather than a usage error.
fn is_display(e: &clap::Error) -> bool {
    use clap::error::ErrorKind;
    matches!(
        e.kind(),
        ErrorKind::DisplayHelp
            | ErrorKind::DisplayVersion
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    )
}

/// A usage error clap caught on an extension's command line (`command`'s,
/// when the command line named one), as the error object commands write
/// under `json` (`{code, message, suggestion?}`): `INVALID_INPUT`, with the
/// message and suggestion of the one arg rule
/// (`specforge_protocol_types::command_args`), the ones MCP answers for the
/// same mistake; an unknown option is named as typed, with clap's
/// suggestion.
fn usage_error(e: &clap::Error, command: Option<&ExtensionCommand>) -> Value {
    use clap::error::{ContextKind, ContextValue, ErrorKind};
    let text = |kind| match e.get(kind) {
        Some(ContextValue::String(s)) => Some(s.clone()),
        Some(ContextValue::Strings(s)) => s.first().cloned(),
        _ => None,
    };
    // The declared arg clap names, and the value it refused.
    let declared = text(ContextKind::InvalidArg).and_then(|shown| {
        let name = declared_name(&shown);
        command?
            .declaration()
            .args
            .iter()
            .find(|arg| arg.name == name)
    });
    let refused = match (e.kind(), declared) {
        (ErrorKind::InvalidValue | ErrorKind::ValueValidation, Some(declared)) => {
            let value = Value::String(text(ContextKind::InvalidValue).unwrap_or_default());
            normalize_arg(declared, &value).err()
        }
        (ErrorKind::MissingRequiredArgument, _) => {
            let names = match e.get(ContextKind::InvalidArg) {
                Some(ContextValue::Strings(args)) => {
                    args.iter().map(|a| declared_name(a)).collect()
                }
                _ => Vec::new(),
            };
            Some(ArgError::Missing { names })
        }
        (ErrorKind::UnknownArgument, _) => Some(ArgError::Unknown {
            name: text(ContextKind::InvalidArg).unwrap_or_default(),
            suggestion: text(ContextKind::SuggestedArg),
        }),
        _ => None,
    };
    if let Some(refused) = refused {
        return refused.to_json();
    }
    let (message, suggestion) = match e.kind() {
        ErrorKind::InvalidSubcommand => (
            format!(
                "unknown command '{}'",
                text(ContextKind::InvalidSubcommand).unwrap_or_default()
            ),
            text(ContextKind::SuggestedSubcommand),
        ),
        _ => {
            let rendered = e.render().to_string();
            let first = rendered.lines().next().unwrap_or_default();
            (
                first.strip_prefix("error: ").unwrap_or(first).to_string(),
                None,
            )
        }
    };
    serde_json::to_value(CommandError {
        suggestion,
        ..CommandError::new(
            specforge_protocol_types::command_args::INVALID_INPUT,
            message,
        )
    })
    .expect("a command error serializes to a JSON object")
}

/// The declared name of an arg as clap shows it: `<MILESTONE>` is
/// `milestone`, `--sort-order <SORT_ORDER>` is `sort_order`, `--details`
/// is `details` (a value name is the declared name upper-cased).
fn declared_name(shown: &str) -> String {
    match shown.split_once('<') {
        Some((_, rest)) => rest.split('>').next().unwrap_or_default().to_lowercase(),
        None => shown
            .trim_start_matches('-')
            .split(['=', ' '])
            .next()
            .unwrap_or_default()
            .replace('-', "_"),
    }
}

/// `--path <dir>` or `--path=<dir>` among `args`, else `.`: the project the
/// extensions are loaded from, needed before the command line can be parsed.
fn project_path(args: &[String]) -> PathBuf {
    let flag = format!("--{PATH}");
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if *arg == flag {
            if let Some(value) = args.next() {
                return PathBuf::from(value);
            }
        } else if let Some(value) = arg.strip_prefix(&format!("{flag}=")) {
            return PathBuf::from(value);
        }
    }
    PathBuf::from(".")
}

/// `specforge <ext>`'s command line: one subcommand per command the host
/// does not refuse.
fn command_line(ext: &str, commands: &[&ExtensionCommand]) -> Command {
    let mut cli = Command::new(ext.to_string())
        .bin_name(format!("specforge {ext}"))
        .subcommand_required(true)
        .arg_required_else_help(true);
    for command in commands.iter().filter(|c| c.refusal().is_none()) {
        let mut sub = Command::new(command.cli_name()).about(command.title().to_string());
        if !command.description().is_empty() {
            sub = sub.long_about(command.description().to_string());
        }
        for arg in command.args() {
            sub = sub.arg(declared_arg(arg));
        }
        sub = sub
            .arg(
                Arg::new(PATH)
                    .long(PATH)
                    .value_name("PATH")
                    .default_value(".")
                    .help(crate::project::PATH_HELP),
            )
            .arg(
                Arg::new(FORMAT)
                    .long(FORMAT)
                    .value_name("FORMAT")
                    .value_parser(PossibleValuesParser::new(
                        CommandFormat::ALL.map(CommandFormat::as_str),
                    ))
                    .default_value(CommandFormat::Human.as_str())
                    .help("Output format"),
            );
        cli = cli.subcommand(sub);
    }
    cli
}

/// An arg as clap takes it, from its derived shape: a positional, a
/// `--option <VALUE>` or a `--flag`; an integer at least its minimum (a
/// negative one given as a value, not taken for an option), one of its
/// values; its default shown and filled.
fn declared_arg(declared: &ExtensionArg) -> Arg {
    let mut arg = Arg::new(declared.name.clone());
    arg = match &declared.shape {
        ArgShape::Positional { .. } => arg.value_name(declared.name.to_uppercase()).required(true),
        ArgShape::Option { long } => arg
            .long(long.clone())
            .value_name(declared.name.to_uppercase()),
        ArgShape::Flag { long } => arg.long(long.clone()).action(ArgAction::SetTrue),
    };
    if let Some(help) = &declared.description {
        arg = arg.help(help.clone());
    }
    arg = match &declared.value {
        ArgValue::Integer { minimum } => {
            let integer = clap::value_parser!(i64);
            let arg = arg.allow_negative_numbers(true);
            match minimum {
                Some(minimum) => arg.value_parser(integer.range(*minimum..)),
                None => arg.value_parser(integer),
            }
        }
        ArgValue::OneOf(values) => arg.value_parser(PossibleValuesParser::new(values)),
        ArgValue::Text | ArgValue::Path | ArgValue::Flag => arg,
    };
    if let Some(default) = &declared.default {
        let shown = match default {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        arg = arg.default_value(shown);
    }
    arg
}

/// The host's `--format` the command line set (or defaulted).
fn format_value(matches: &ArgMatches) -> CommandFormat {
    matches
        .get_one::<String>(FORMAT)
        .and_then(|f| CommandFormat::parse(f))
        .unwrap_or_default()
}

/// The args the user set on the command line, typed as clap parsed them;
/// never a default (the rule fills those) nor the host's own options.
fn arg_values(command: &ExtensionCommand, matches: &ArgMatches) -> Map<String, Value> {
    let mut args = Map::new();
    for arg in command.args() {
        if matches.value_source(&arg.name) != Some(ValueSource::CommandLine) {
            continue;
        }
        let value = match arg.value {
            ArgValue::Flag => Some(Value::Bool(matches.get_flag(&arg.name))),
            ArgValue::Integer { .. } => matches.get_one::<i64>(&arg.name).map(|n| Value::from(*n)),
            _ => matches
                .get_one::<String>(&arg.name)
                .map(|s| Value::from(s.as_str())),
        };
        if let Some(value) = value {
            args.insert(arg.name.clone(), value);
        }
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::Facts;
    use specforge_protocol_types::{CommandArgDescriptor, CommandArgType, CommandDescriptor};
    use specforge_test_macros::test as specforge_test;
    use specforge_wasm::{CallError, CallFailure, Operation};

    fn arg(
        name: &str,
        arg_type: CommandArgType,
        required: bool,
        default: Option<&str>,
    ) -> CommandArgDescriptor {
        CommandArgDescriptor {
            name: name.into(),
            arg_type,
            required,
            default_value: default.map(str::to_string),
            description: None,
            minimum: None,
        }
    }

    fn contribution() -> CommandDescriptor {
        CommandDescriptor {
            id: "milestone_completion".into(),
            title: "Show progress".into(),
            description: String::new(),
            category: None,
            export: "cmd__x_milestone_completion".into(),
            args: vec![
                arg("milestone", CommandArgType::String, true, None),
                arg("limit", CommandArgType::Integer, false, None),
                arg("all_kinds", CommandArgType::Bool, false, None),
                arg(
                    "order",
                    CommandArgType::Enum {
                        values: vec!["asc".into(), "desc".into()],
                    },
                    false,
                    Some("asc"),
                ),
            ],
        }
    }

    /// The args `specforge x <argv>` sends the export of `c`, and the
    /// format asked for.
    fn sent_by(
        c: &CommandDescriptor,
        argv: &[&str],
    ) -> Result<(Map<String, Value>, CommandFormat), clap::Error> {
        let command = ExtensionCommand::new("@acme/x", "x", c);
        let matches = command_line("x", &[&command])
            .try_get_matches_from(std::iter::once("specforge x").chain(argv.iter().copied()))?;
        let (name, sub) = matches.subcommand().unwrap();
        assert_eq!(name, command.cli_name());
        assert_eq!(sub.get_one::<String>(PATH).map(String::as_str), Some("."));
        let args = command.normalize(&arg_values(&command, sub)).unwrap();
        Ok((args, format_value(sub)))
    }

    fn parse_with_format(
        argv: &[&str],
    ) -> Result<(Map<String, Value>, CommandFormat), clap::Error> {
        sent_by(&contribution(), argv)
    }

    fn parse(argv: &[&str]) -> Result<Map<String, Value>, clap::Error> {
        parse_with_format(argv).map(|(args, _)| args)
    }

    #[specforge_test(
        behavior = "locate_project_root",
        verify = "every command's project path argument says it is a path inside the project"
    )]
    fn the_host_path_option_has_the_one_help() {
        let command = ExtensionCommand::new("@acme/x", "x", &contribution());
        let line = command_line("x", &[&command]);
        let sub = line.find_subcommand(command.cli_name()).unwrap();
        let path = sub.get_arguments().find(|a| a.get_id() == PATH).unwrap();
        assert_eq!(
            path.get_help().map(ToString::to_string).as_deref(),
            Some(crate::project::PATH_HELP)
        );
    }

    #[test]
    fn declared_args_become_typed_values() {
        let args = parse(&["milestone-completion", "m1", "--limit", "3", "--all-kinds"]).unwrap();
        assert_eq!(
            Value::Object(args),
            serde_json::json!({"milestone": "m1", "limit": 3, "all_kinds": true, "order": "asc"})
        );
    }

    #[specforge_test(
        behavior = "surface_format_conventions",
        verify = "default format is human"
    )]
    fn every_command_takes_the_hosts_format_human_by_default() {
        let (args, format) = parse_with_format(&["milestone-completion", "m1"]).unwrap();
        assert_eq!(format, CommandFormat::Human);
        assert!(!args.contains_key(FORMAT), "the format is not an arg");
        let (args, format) =
            parse_with_format(&["milestone-completion", "m1", "--format", "json"]).unwrap();
        assert_eq!(format, CommandFormat::Json);
        assert!(!args.contains_key(FORMAT));
        // There are no other formats.
        for other in ["table", "brief", "xml"] {
            assert!(parse(&["milestone-completion", "m1", "--format", other]).is_err());
        }
    }

    #[test]
    fn the_command_line_refuses_what_the_declaration_does() {
        // A required arg is positional and required; an enum takes its values.
        assert!(parse(&["milestone-completion"]).is_err());
        assert!(parse(&["milestone-completion", "m1", "--order", "sideways"]).is_err());
        assert!(parse(&["milestone-completion", "m1", "--limit", "many"]).is_err());
    }

    #[test]
    fn a_required_bool_is_a_flag_and_a_repeated_flag_is_refused() {
        let mut c = contribution();
        c.args.push(arg("strict", CommandArgType::Bool, true, None));
        let parse = |argv: &[&str]| {
            let argv: Vec<&str> = std::iter::once("milestone-completion")
                .chain(argv.iter().copied())
                .collect();
            sent_by(&c, &argv).map(|(args, _)| args)
        };
        assert_eq!(parse(&["m1", "--strict"]).unwrap()["strict"], true);
        assert_eq!(parse(&["m1"]).unwrap()["strict"], false);
        assert!(parse(&["m1", "--limit", "1", "--limit", "2"]).is_err());
    }

    #[test]
    fn a_count_below_its_minimum_is_refused_and_a_negative_integer_is_a_value() {
        let mut c = contribution();
        c.args[1].minimum = Some(0);
        c.args
            .push(arg("shift", CommandArgType::Integer, false, None));
        let parse = |argv: &[&str]| {
            let argv: Vec<&str> = ["milestone-completion", "m1"]
                .into_iter()
                .chain(argv.iter().copied())
                .collect();
            sent_by(&c, &argv).map(|(args, _)| args)
        };
        assert_eq!(parse(&["--shift", "-2"]).unwrap()["shift"], -2);
        assert_eq!(parse(&["--limit", "0"]).unwrap()["limit"], 0);
        let refused = parse(&["--limit", "-1"]).unwrap_err();
        let command = ExtensionCommand::new("@acme/x", "x", &c);
        assert_eq!(
            usage_error(&refused, Some(&command)),
            serde_json::json!({"code": "INVALID_INPUT",
                "message": "limit must be a non-negative integer, got -1"})
        );
        let refused = parse(&["--limit", "abc"]).unwrap_err();
        assert_eq!(
            usage_error(&refused, Some(&command))["message"],
            "limit must be a non-negative integer, got 'abc'"
        );
    }

    #[specforge_test(
        behavior = "dispatch_surface_command",
        verify = "under --format json a command whose export trapped prints one JSON error object"
    )]
    fn a_trapped_command_reports_in_the_format_asked_for() {
        let trap = CallError::new(
            Operation::Command,
            "@acme/x",
            "cmd__x",
            CallFailure::Trapped {
                kind: "call_failed".into(),
                message: "unreachable: the command panicked".into(),
            },
        );
        let trap = RunError::Call(trap);
        let json = written(&trap, CommandFormat::Json);
        let error: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            error,
            serde_json::json!({"code": "E028",
                "message": "command cmd__x() of '@acme/x' trapped: call_failed: unreachable: the command panicked",
                "suggestion": "report the failure to the author of '@acme/x', or check it is installed and up to date"})
        );
        assert!(json.ends_with('\n') && json.lines().count() == 1, "{json}");
        let human = written(&trap, CommandFormat::Human);
        assert!(
            human.starts_with("error[E028]: command cmd__x() of '@acme/x' trapped"),
            "{human}"
        );
    }

    #[test]
    fn a_refused_arg_ends_with_invalid_input_and_exit_2() {
        let refused = || {
            Err(RunError::Args(
                specforge_protocol_types::command_args::ArgError::Missing {
                    names: vec!["milestone".into()],
                },
            ))
        };
        let (terminal, capture) = Terminal::captured("", Facts::PIPED);
        let code = ended(refused(), CommandFormat::Json, &terminal);
        assert_eq!(code, 2);
        assert!(capture.stdout().is_empty());
        assert_eq!(
            capture.stderr(),
            "{\"code\":\"INVALID_INPUT\",\"message\":\"missing required arg 'milestone'\"}\n"
        );

        let (terminal, capture) = Terminal::captured("", Facts::PIPED);
        let code = ended(refused(), CommandFormat::Human, &terminal);
        assert_eq!(code, 2);
        assert_eq!(
            capture.stderr(),
            "error: missing required arg 'milestone'\n"
        );
    }

    #[specforge_test(
        behavior = "dispatch_surface_command",
        verify = "a command whose output is not a CommandOutput is an ExtensionError, not exit 0 with the raw bytes"
    )]
    fn a_command_answering_no_command_output_exits_1_with_e028() {
        let c = contribution();
        for format in CommandFormat::ALL {
            let answered = CallError::new(
                Operation::Command,
                "@acme/x",
                &c.export,
                CallFailure::Malformed {
                    expected: "CommandOutput",
                    reason: "expected value at line 1 column 1".into(),
                },
            );
            let (terminal, capture) = Terminal::captured("", Facts::PIPED);
            let code = ended(Err(RunError::Call(answered)), format, &terminal);
            assert_eq!(code, 1, "{format:?}");
            assert!(capture.stdout().is_empty(), "the raw bytes are not printed");
            let stderr = capture.stderr();
            let message = "command cmd__x_milestone_completion() of '@acme/x' answered output \
                           that is not a CommandOutput: ";
            match format {
                CommandFormat::Json => {
                    let error: Value = serde_json::from_str(&stderr).unwrap();
                    assert_eq!(error["code"], "E028");
                    assert!(
                        error["message"].as_str().unwrap().starts_with(message),
                        "{error}"
                    );
                }
                CommandFormat::Human => assert!(
                    stderr.starts_with(&format!("error[E028]: {message}")),
                    "{stderr}"
                ),
            }
        }
    }

    /// The args `specforge x <argv>` sends the export of `declared`, a
    /// `CommandDescriptor` as JSON.
    fn sent(declared: Value, argv: &[&str]) -> Value {
        let c: CommandDescriptor = serde_json::from_value(declared).unwrap();
        Value::Object(sent_by(&c, argv).unwrap().0)
    }

    /// What the command line sends for the declarations
    /// `crates/specforge-mcp/tests/surface_wiring.rs` sends over MCP
    /// (`ordered_command`, `strict_command`): `ExtensionCommand::normalize`
    /// of what was typed, as MCP sends `normalize` of its arguments.
    #[specforge_test(
        behavior = "dispatch_surface_command",
        verify = "the CLI and MCP send a command's export the same args for the same input, its declared defaults applied by the host"
    )]
    fn the_cli_sends_the_args_the_derivation_normalizes() {
        let ordered = serde_json::json!({"id": "ordered", "title": "Ordered",
            "description": "List in order", "export": "cmd__ordered",
            "args": [{"name": "order", "arg_type": {"enum": {"values": ["asc", "desc"]}},
                      "default_value": "desc"},
                     {"name": "all", "arg_type": "bool"}]});
        assert_eq!(
            sent(ordered.clone(), &["ordered"]),
            serde_json::json!({"order": "desc", "all": false})
        );
        // The same map MCP sends for no arguments.
        let derived = ExtensionCommand::new(
            "@test/cmds",
            "x",
            &serde_json::from_value(ordered.clone()).unwrap(),
        );
        assert_eq!(
            sent(ordered.clone(), &["ordered"]),
            Value::Object(derived.normalize(&Map::new()).unwrap())
        );
        assert_eq!(
            sent(ordered, &["ordered", "--order", "asc", "--all"]),
            serde_json::json!({"order": "asc", "all": true})
        );
        let strict = serde_json::json!({"id": "strict", "title": "Strict",
            "description": "Check strictly", "export": "cmd__strict",
            "args": [{"name": "strict", "arg_type": "bool", "required": true}]});
        assert_eq!(
            sent(strict.clone(), &["strict"]),
            serde_json::json!({"strict": false})
        );
        assert_eq!(
            sent(strict, &["strict", "--strict"]),
            serde_json::json!({"strict": true})
        );
    }

    #[test]
    fn the_project_path_is_found_before_parsing() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(project_path(&args(&["features"])), PathBuf::from("."));
        assert_eq!(
            project_path(&args(&["features", "--path", "/p", "--limit", "1"])),
            PathBuf::from("/p")
        );
        assert_eq!(
            project_path(&args(&["--path=/q", "features"])),
            PathBuf::from("/q")
        );
    }
}
