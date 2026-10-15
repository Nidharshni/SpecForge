//! `specforge <extension> <command> [args]`: the commands extensions
//! contribute (ADR 0008).
//!
//! Any first argument that is not a built-in command names an extension by
//! its short name (`product` for `@specforge/product`); `ext:command` is the
//! same as `ext command`. The project is the one at `--path` (default `.`),
//! and the output the one `--format` asks for (`human`, the default, or
//! `json`): options every extension command has, the host's, which no
//! declared arg may take (ADR 0011). Its extensions declare the commands,
//! and `specforge_ops::extension::command::ExtensionCommand` derives each one's command
//! line (a required arg is positional, in declaration order; any other is a
//! `--option`; every flag is a `--flag`, set or not) and the args its export
//! receives, by the one arg rule MCP also sends them by (ADR 0017). This
//! module renders that derivation as clap's command line, routes the
//! command over the project's environment, compiles the project from that
//! environment, and runs the command through
//! `specforge_ops::extension::command::run`; the export's stdout and stderr are printed
//! as they are, and its exit code is the CLI's.

use crate::outcome::Exit;
use crate::project::ProjectArg;
use crate::terminal::{Stream, Terminal, err, errln, out};
use clap::builder::PossibleValuesParser;
use clap::parser::ValueSource;
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::{Map, Value};
use specforge_common::ProjectRoot;
use specforge_ops::extension::command::{
    ArgShape, ArgValue, CommandFormat, ExtensionArg, ExtensionCommand, ExtensionCommands, RunError,
};
use specforge_ops::view::ProjectView;
use specforge_project::{CompiledProject, Environment};
use specforge_protocol_types::command_args::{ArgError, normalize_arg};
use specforge_protocol_types::{CommandError, CommandOutput};
use std::sync::Arc;

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

    let root = match ProjectArg::scan(&rest).root() {
        Ok(root) => root,
        Err(error) => {
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
        for (shadowed, _) in routed
            .shadowed()
            .filter(|(_, by)| std::ptr::eq(*by, command))
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
    let outcome =
        specforge_ops::extension::command::run(&ProjectView::of(&project), command, &given, format);
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
        // The derive numbered `--path` 0 in its own command; here it comes
        // after the declared options, as the host's own.
        let after_declared = sub.get_arguments().filter(|a| !a.is_positional()).count();
        sub = sub
            .arg(ProjectArg::arg().display_order(after_declared))
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
mod tests;
