use super::*;
use crate::terminal::Facts;
use specforge_protocol_types::{CommandArgDescriptor, CommandArgType, CommandDescriptor};
use specforge_test_macros::test as specforge_test;
use specforge_wasm::{CallError, CallFailure, Operation};
use std::collections::BTreeSet;

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
    assert_eq!(
        sub.get_one::<std::path::PathBuf>(ProjectArg::NAME),
        Some(&std::path::PathBuf::from("."))
    );
    let args = command.normalize(&arg_values(&command, sub)).unwrap();
    Ok((args, format_value(sub)))
}

fn parse_with_format(argv: &[&str]) -> Result<(Map<String, Value>, CommandFormat), clap::Error> {
    sent_by(&contribution(), argv)
}

fn parse(argv: &[&str]) -> Result<Map<String, Value>, clap::Error> {
    parse_with_format(argv).map(|(args, _)| args)
}

/// `c`, a command of `@acme/x` (short name `x`).
fn declared(c: CommandDescriptor) -> ExtensionCommand {
    ExtensionCommand::new("@acme/x", "x", &c)
}

/// `a` with `description`.
fn described(mut a: CommandArgDescriptor, description: &str) -> CommandArgDescriptor {
    a.description = Some(description.into());
    a
}

/// `a` with `minimum`.
fn at_least(mut a: CommandArgDescriptor, minimum: i64) -> CommandArgDescriptor {
    a.minimum = Some(minimum);
    a
}

/// Commands whose declarations take every path through the host's
/// rendering. `show` declares an arg of every shape and value
/// ([`exercised`]), with and without a description, a minimum and a
/// default, and has a long description. `plain` has neither args nor a
/// description. `refused` declares an arg named `format`, which the host
/// refuses, so it is listed nowhere.
fn every_shape() -> Vec<CommandDescriptor> {
    let enum_of = |values: &[&str]| CommandArgType::Enum {
        values: values.iter().map(|v| v.to_string()).collect(),
    };
    let command = |id: &str, title: &str, description: &str, args| CommandDescriptor {
        id: id.into(),
        title: title.into(),
        description: description.into(),
        category: None,
        export: format!("cmd__x_{id}"),
        args,
    };
    vec![
        command(
            "show",
            "Show one thing",
            "Show one thing in full: its fields, then its links.",
            vec![
                // Required, not a flag: positional, in declaration order.
                described(
                    arg("target", CommandArgType::String, true, None),
                    "The target id",
                ),
                arg("file", CommandArgType::Path, true, None),
                described(
                    arg("side", enum_of(&["left", "right"]), true, None),
                    "Which side",
                ),
                described(
                    at_least(arg("count", CommandArgType::Integer, true, None), 1),
                    "How many",
                ),
                // Optional, not a flag: `--option <VALUE>`.
                described(
                    arg("label", CommandArgType::String, false, None),
                    "A label to show",
                ),
                arg("out", CommandArgType::Path, false, None),
                described(
                    arg("status", enum_of(&["open", "closed"]), false, Some("open")),
                    "Only things with this status",
                ),
                described(
                    at_least(arg("limit", CommandArgType::Integer, false, Some("10")), 0),
                    "Return at most this many",
                ),
                arg("shift", CommandArgType::Integer, false, None),
                // Every flag, required or not: `--flag`.
                described(
                    arg("details", CommandArgType::Bool, false, None),
                    "Show every field",
                ),
                arg("strict", CommandArgType::Bool, true, None),
            ],
        ),
        command("plain", "List plain things", "", vec![]),
        command(
            "refused",
            "Refused",
            "Declares the host's own option.",
            vec![arg("format", CommandArgType::String, false, None)],
        ),
    ]
}

/// Every (shape, value) pair the args of `commands` take. The matches are
/// exhaustive on purpose: a new `ArgShape` or `ArgValue` variant fails to
/// compile here until `every_shape` declares an arg of it.
fn exercised(commands: &[ExtensionCommand]) -> BTreeSet<(&'static str, &'static str)> {
    commands
        .iter()
        .flat_map(ExtensionCommand::args)
        .map(|arg| {
            let shape = match arg.shape {
                ArgShape::Positional { .. } => "positional",
                ArgShape::Option { .. } => "option",
                ArgShape::Flag { .. } => "flag",
            };
            let value = match arg.value {
                ArgValue::Text => "text",
                ArgValue::Path => "path",
                ArgValue::Integer { minimum: Some(_) } => "integer_at_least",
                ArgValue::Integer { minimum: None } => "integer",
                ArgValue::Flag => "flag",
                ArgValue::OneOf(_) => "one_of",
            };
            (shape, value)
        })
        .collect()
}

/// What `specforge x <argv>` prints for a `--help` among `argv`: clap's
/// help, rendered as `run` writes it.
fn help(commands: &[&ExtensionCommand], argv: &[&str]) -> String {
    let error = command_line("x", commands)
        .try_get_matches_from(std::iter::once("specforge x").chain(argv.iter().copied()))
        .expect_err("--help ends the parse");
    assert!(is_display(&error), "{argv:?}: {error}");
    error.render().to_string()
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "the command line shows each declared arg by its shape, with its values, default and description, beside the host's --path and --format"
)]
fn the_command_line_help_is_derived_from_the_declaration() {
    let commands: Vec<ExtensionCommand> = every_shape().into_iter().map(declared).collect();
    assert_eq!(
        exercised(&commands),
        BTreeSet::from([
            ("positional", "text"),
            ("positional", "path"),
            ("positional", "one_of"),
            ("positional", "integer_at_least"),
            ("option", "text"),
            ("option", "path"),
            ("option", "one_of"),
            ("option", "integer_at_least"),
            ("option", "integer"),
            ("flag", "flag"),
        ])
    );
    assert!(commands[2].refusal().is_some(), "`refused` must be refused");
    let commands: Vec<&ExtensionCommand> = commands.iter().collect();
    let pinned: Vec<String> = [&["--help"][..], &["show", "--help"], &["plain", "--help"]]
        .iter()
        .map(|argv| {
            format!(
                "$ specforge x {}\n{}",
                argv.join(" "),
                help(&commands, argv)
            )
        })
        .collect();
    insta::assert_snapshot!("command_line_help", pinned.join("\n"));
}

#[specforge_test(
    behavior = "locate_project_root",
    verify = "every command that reads a project takes the one --path option, and none takes a project path as a positional argument"
)]
fn the_host_path_option_is_the_project_arg() {
    let command = ExtensionCommand::new("@acme/x", "x", &contribution());
    let line = command_line("x", &[&command]);
    let sub = line.find_subcommand(command.cli_name()).unwrap();
    let path = sub
        .get_arguments()
        .find(|a| a.get_id() == ProjectArg::NAME)
        .unwrap();
    let project = ProjectArg::arg();
    assert!(!path.is_positional());
    assert_eq!(path.get_long(), project.get_long());
    assert_eq!(
        path.get_help().map(ToString::to_string),
        project.get_help().map(ToString::to_string)
    );
    assert_eq!(path.get_default_values(), project.get_default_values());
    assert!(specforge_protocol_types::command_args::HOST_OPTIONS.contains(&ProjectArg::NAME));
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
/// `crates/surfaces/mcp/tests/surface_wiring.rs` sends over MCP
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
