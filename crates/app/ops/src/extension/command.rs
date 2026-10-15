//! Extension commands: the CLI commands extensions declare in their
//! surfaces, each a `cmd__<id>` Wasm export.
//!
//! [`ExtensionCommand`] derives one command, once, for every surface: its
//! name on the command line (`specforge <short> <cli name>`), its MCP tool
//! (`specforge.<short>.<id>`, its input schema), its args' command-line
//! shapes, the args both surfaces send its export (normalized by the one
//! arg rule the SDK also runs, `specforge_protocol_types::command_args`),
//! and why the host refuses it. [`ExtensionCommands`] is the project's
//! commands as the registry build routes them (ADR 0097). The CLI and MCP both read them and run the command here:
//! the export receives the protocol's `CommandInput` (the args, the project
//! root, the compiled graph in the graph export's shape, the format the
//! caller asked for and the host's date) and answers with a
//! `CommandOutput` (`specforge_protocol_types`, through
//! `specforge_wasm::ExtensionCalls`). The host knows no command: which
//! exist, their args and what they print are the extension's (ADR 0008).
//! The host owns `--format` (ADR 0011), and normalizes the args (ADR 0017).
//! [`run`] runs a command over a project view: it normalizes the given args,
//! sends the project root (absolute, canonical), the evidence of the
//! recorded test report and today's date, and calls the export (ADR 0011,
//! "One operation runs a command").

use crate::OpError;
use crate::view::ProjectView;
use serde_json::{Map, Value, json};
use specforge_protocol_types::command_args::{self, ArgError, normalize_arg, option_name};
use specforge_protocol_types::{
    CommandArgDescriptor, CommandArgType, CommandDescriptor, CommandError, CommandEvidence,
    CommandInput, CommandOutput, EntityEvidence, RawGraph,
};
use specforge_registry::{CommandRoute, RegistryBuild};
use specforge_wasm::{CallError, CallFailure, ExtensionCalls, Operation};
use std::path::{Path, PathBuf};

/// The output a command is asked for: `human` (the CLI default) or `json`
/// (always, over MCP). The host's, not the command's: no command declares
/// an arg named `format` (ADR 0011).
pub use specforge_protocol_types::CommandFormat;

/// Why the host refuses a command: its arg declarations break the one arg
/// rule ([`command_args::refusal`]). A refused command is on neither
/// surface: the CLI refuses to run it (exit 2) and MCP serves no tool for
/// it (I017). Its `Display` is the reason both write.
pub use specforge_protocol_types::command_args::ArgRefusal as Refusal;

/// The options the host gives every extension command on the command line
/// (`--path`, `--format`, `--help`), which no declared arg may take: the
/// one list the SDK checks too.
pub use specforge_protocol_types::command_args::HOST_OPTIONS;

/// One command an extension declares, derived once for every surface.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionCommand {
    /// The declaring extension's name (`@specforge/product`).
    extension: String,
    /// The extension's short name (`product`): its declared `ext_short`,
    /// else its name's last segment.
    short: String,
    /// The command as declared.
    declaration: CommandDescriptor,
    /// Its args, in declaration order, as both surfaces present them.
    args: Vec<ExtensionArg>,
    refusal: Option<Refusal>,
}

/// One declared arg of a command, as both surfaces present it.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionArg {
    /// As declared: `sort_order`.
    pub name: String,
    pub value: ArgValue,
    pub shape: ArgShape,
    /// The declared default, typed (normalized once from `default_value`);
    /// `None` without one, or with one its type refuses (the command is
    /// then refused).
    pub default: Option<Value>,
    pub description: Option<String>,
}

/// What values an arg takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgValue {
    Text,
    Path,
    /// An integer, at least `minimum` when it has one (`0`: a count).
    Integer {
        minimum: Option<i64>,
    },
    /// `true` or `false`: `false` unless set.
    Flag,
    /// One of these values.
    OneOf(Vec<String>),
}

/// Where an arg is on the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgShape {
    /// A required arg that is not a flag: positional, its `index` among
    /// the positional args, in declaration order.
    Positional { index: usize },
    /// Any other arg that is not a flag: `--<long> <value>`.
    Option { long: String },
    /// Every flag, required or not: `--<long>`, set or not.
    Flag { long: String },
}

impl ExtensionArg {
    fn derive(declared: &CommandArgDescriptor, positional: &mut usize) -> Self {
        let value = match &declared.arg_type {
            CommandArgType::String => ArgValue::Text,
            CommandArgType::Path => ArgValue::Path,
            CommandArgType::Integer => ArgValue::Integer {
                minimum: declared.minimum,
            },
            CommandArgType::Bool => ArgValue::Flag,
            CommandArgType::Enum { values } => ArgValue::OneOf(values.clone()),
        };
        let long = option_name(&declared.name);
        let shape = match &value {
            ArgValue::Flag => ArgShape::Flag { long },
            _ if declared.required => {
                *positional += 1;
                ArgShape::Positional {
                    index: *positional - 1,
                }
            }
            _ => ArgShape::Option { long },
        };
        let default = match (&value, &declared.default_value) {
            (ArgValue::Flag, _) | (_, None) => None,
            (_, Some(default)) => normalize_arg(declared, &Value::String(default.clone())).ok(),
        };
        ExtensionArg {
            name: declared.name.clone(),
            value,
            shape,
            default,
            description: declared.description.clone(),
        }
    }

    /// The arg's property in its command tool's input schema: its JSON
    /// type, its values, its minimum, its default and its description.
    fn schema(&self) -> Value {
        let mut property = match &self.value {
            ArgValue::Text | ArgValue::Path => json!({"type": "string"}),
            ArgValue::Integer { minimum } => {
                let mut property = json!({"type": "integer"});
                if let Some(minimum) = minimum {
                    property["minimum"] = json!(minimum);
                }
                property
            }
            ArgValue::Flag => json!({"type": "boolean", "default": false}),
            ArgValue::OneOf(values) => json!({"type": "string", "enum": values}),
        };
        if let Some(default) = &self.default {
            property["default"] = default.clone();
        }
        if let Some(description) = &self.description {
            property["description"] = json!(description);
        }
        property
    }

    /// Whether the caller must give it: a required arg that is not a flag.
    pub fn is_required(&self) -> bool {
        matches!(self.shape, ArgShape::Positional { .. })
    }
}

impl ExtensionCommand {
    /// The command `route` routes, derived for every surface.
    pub fn of(route: &CommandRoute) -> Self {
        Self::new(route.extension(), route.short(), route.declaration())
    }

    /// `declaration`, a command of `extension` (short name `short`),
    /// derived for every surface.
    pub fn new(extension: &str, short: &str, declaration: &CommandDescriptor) -> Self {
        let mut positional = 0;
        let args = declaration
            .args
            .iter()
            .map(|arg| ExtensionArg::derive(arg, &mut positional))
            .collect();
        ExtensionCommand {
            extension: extension.to_string(),
            short: short.to_string(),
            declaration: declaration.clone(),
            args,
            refusal: command_args::refusal(&declaration.args),
        }
    }

    /// The declaring extension's name.
    pub fn extension(&self) -> &str {
        &self.extension
    }

    /// The declaring extension's short name, which routes its commands.
    pub fn short(&self) -> &str {
        &self.short
    }

    /// The command's declared id (`milestone_completion`).
    pub fn id(&self) -> &str {
        &self.declaration.id
    }

    /// The Wasm export that runs it.
    pub fn export(&self) -> &str {
        &self.declaration.export
    }

    /// The one-line title help lists it under.
    pub fn title(&self) -> &str {
        &self.declaration.title
    }

    /// What it answers: its long help and its tool's description.
    pub fn description(&self) -> &str {
        &self.declaration.description
    }

    /// The command as declared.
    pub fn declaration(&self) -> &CommandDescriptor {
        &self.declaration
    }

    /// Its name on the command line: its id, `_` spelled `-`
    /// (`milestone_completion` is `milestone-completion`).
    pub fn cli_name(&self) -> String {
        option_name(&self.declaration.id)
    }

    /// Its MCP tool's name: `specforge.{short}.{id}`.
    pub fn tool_name(&self) -> String {
        format!("specforge.{}.{}", self.short, self.declaration.id)
    }

    /// Its args, in declaration order.
    pub fn args(&self) -> &[ExtensionArg] {
        &self.args
    }

    /// Why the host refuses it, if it does: then it runs on no surface.
    pub fn refusal(&self) -> Option<&Refusal> {
        self.refusal.as_ref()
    }

    /// The JSON Schema of its MCP tool's arguments: each arg's type, its
    /// values, its minimum, its default and its description; `required`
    /// lists the required args that are not flags (a flag is `false` unless
    /// set); no undeclared property is accepted.
    pub fn input_schema(&self) -> Value {
        let properties: Map<String, Value> = self
            .args
            .iter()
            .map(|arg| (arg.name.clone(), arg.schema()))
            .collect();
        let mut schema = json!({
            "type": "object",
            "properties": properties,
            "additionalProperties": false,
        });
        let required: Vec<&str> = self
            .args
            .iter()
            .filter(|arg| arg.is_required())
            .map(|arg| arg.name.as_str())
            .collect();
        if !required.is_empty() {
            schema["required"] = json!(required);
        }
        schema
    }

    /// The args both surfaces send its export for the arguments `given`:
    /// [`command_args::normalize_args`] over its declaration (a declared
    /// default for an absent arg, `false` for an unset flag, each value its
    /// declared type; an undeclared argument, a value of another type or a
    /// missing required arg refused).
    pub fn normalize(&self, given: &Map<String, Value>) -> Result<Map<String, Value>, ArgError> {
        command_args::normalize_args(&self.declaration.args, given)
    }
}

/// The project's extension commands, as the registry build routes them
/// (`RegistryBuild::commands`, ADR 0097): each once, under `(short name,
/// CLI name)`, the first in load order (then declaration order) winning; a
/// later command under a taken pair is [`shadowed`](Self::shadowed) and
/// runs on no surface. It derives each route and decides nothing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtensionCommands {
    commands: Vec<ExtensionCommand>,
    /// Each shadowed command with the index, in `commands`, of the one
    /// routed under its name.
    shadowed: Vec<(ExtensionCommand, usize)>,
}

impl ExtensionCommands {
    /// The commands `build` routes, in its order.
    pub fn build(build: &RegistryBuild) -> Self {
        let routes = build.commands();
        ExtensionCommands {
            commands: routes.routed().iter().map(ExtensionCommand::of).collect(),
            shadowed: routes
                .shadowed()
                .iter()
                .map(|s| (ExtensionCommand::of(s.route()), s.by()))
                .collect(),
        }
    }

    /// Every routed command, refused ones included, in load order.
    pub fn all(&self) -> &[ExtensionCommand] {
        &self.commands
    }

    /// The routed commands of the extensions whose short name is `short`.
    pub fn of<'a>(&'a self, short: &'a str) -> impl Iterator<Item = &'a ExtensionCommand> {
        self.commands.iter().filter(move |c| c.short == short)
    }

    /// The short names that route commands, each once, in load order.
    pub fn shorts(&self) -> Vec<&str> {
        let mut shorts: Vec<&str> = Vec::new();
        for command in &self.commands {
            if !shorts.contains(&command.short.as_str()) {
                shorts.push(&command.short);
            }
        }
        shorts
    }

    /// The commands that run on no surface, each with the command routed
    /// under its name: same short name, same CLI name.
    pub fn shadowed(&self) -> impl Iterator<Item = (&ExtensionCommand, &ExtensionCommand)> + '_ {
        self.shadowed
            .iter()
            .map(|(shadowed, by)| (shadowed, &self.commands[*by]))
    }
}

/// What the view's recorded test report proves, as a command's input
/// carries it: `none` without a report, `unreadable` (with why) when the
/// report is there and cannot be used, else every entity that counts toward
/// coverage (its standing, ADR 0004 D2-b) scored by the one coverage rule
/// (`ProjectView::coverage`, the numbers `specforge stats` reports).
fn evidence(view: &ProjectView<'_>) -> CommandEvidence {
    let recorded = match view.recorded() {
        Ok(recorded) => recorded,
        Err(error) => {
            return CommandEvidence::Unreadable {
                reason: error.to_string(),
            };
        }
    };
    if recorded.report.is_none() {
        return CommandEvidence::None;
    }
    let entities = view
        .entities()
        .iter()
        .filter(|(_, standing)| standing.counts())
        .map(|(record, _)| {
            let verdict = recorded.coverage.verdict(&record.id);
            let evidence = EntityEvidence {
                obligations: verdict.map_or(0, |v| v.obligations),
                proven: verdict.map_or(0, |v| v.proven),
                failing: verdict.map_or(0, |v| v.failing),
            };
            (record.id.clone(), evidence)
        })
        .collect();
    CommandEvidence::Recorded { entities }
}

/// Why a command did not answer with its output: what each surface renders
/// (the CLI on stderr with its exit code, MCP as the tool's error).
#[derive(Debug)]
pub enum RunError {
    /// The given args break the one arg rule (`INVALID_INPUT`, ADR 0011 B):
    /// an undeclared argument, a value of another type, a missing required
    /// arg. The export was not called.
    Args(ArgError),
    /// The view has no project on disk to run the command over
    /// (`no_project`, [`ProjectView::project_root`]).
    NoProject(OpError),
    /// The export did not answer a `CommandOutput`: it trapped, the guest
    /// does not route it, its answer is not one, or its extension is not
    /// loaded in the runtime (E028, ADR 0013 D4).
    Call(CallError),
}

impl RunError {
    /// The error object a command writes for this failure (ADR 0011 B):
    /// `{code, message, suggestion?}`. `INVALID_INPUT` for args, the E028
    /// diagnostic's code, message and suggestion for a call, `no_project`
    /// with its hint for a rootless view.
    pub fn to_command_error(&self) -> CommandError {
        match self {
            RunError::Args(error) => error.to_command_error(),
            RunError::NoProject(error) => CommandError {
                suggestion: error.suggestion.clone(),
                ..CommandError::new(error.code.to_string(), error.message.clone())
            },
            RunError::Call(error) => {
                let diagnostic = error.diagnostic();
                CommandError {
                    suggestion: diagnostic.suggestion,
                    ..CommandError::new(diagnostic.code, diagnostic.message)
                }
            }
        }
    }
}

/// Run `command` over the project `view` describes, with the arguments the
/// caller `given` (by declared name; a flag `true`/`false`, an integer a
/// number or its text, the CLI's parsed command line or an MCP tool's
/// arguments), asked for `format`, in the view's runtime. In order:
///
/// 1. the args are normalized by the one arg rule both surfaces and the SDK
///    share ([`ExtensionCommand::normalize`]): declared defaults applied, an
///    unset flag `false`, each value its declared type; a refusal is
///    [`RunError::Args`] and nothing else runs;
/// 2. the project root is the view's (`no_project` without one), sent
///    absolute and canonical (symlinks resolved; as given if that fails);
/// 3. the evidence is what the view's recorded test report proves (none
///    without a report, unreadable with the reason, else each entity that
///    counts toward coverage, scored by the coverage rule `specforge stats`
///    reports; ADR 0039 D3);
/// 4. the date is the host's, UTC, `YYYY-MM-DD` (ADR 0011 C);
/// 5. the view's graph is rendered as the graph export
///    (`specforge_emitter::emit`, at the project's versioned schema version)
///    and the `cmd__` export called with that `CommandInput`.
///
/// A command the host refuses ([`ExtensionCommand::refusal`]) is not routed
/// on either surface; `run` does not check it again (ADR 0011, "One
/// operation runs a command", O5).
pub fn run(
    view: &ProjectView<'_>,
    command: &ExtensionCommand,
    given: &Map<String, Value>,
    format: CommandFormat,
) -> Result<CommandOutput, RunError> {
    run_on(view, command, given, format, &today())
}

/// The view's graph as the plain graph export, naming the version of the
/// project's schema.
fn graph_export(view: &ProjectView<'_>) -> String {
    let versioned = view.versioned_schema();
    specforge_emitter::emit(
        view.graph(),
        &specforge_emitter::EmitOptions {
            schema: versioned.schema(),
            ..specforge_emitter::EmitOptions::default()
        },
    )
    .expect("a graph export cannot fail")
}

/// [`run`], on the date `today` (unit tests pin it here; `run` passes the
/// clock's).
fn run_on(
    view: &ProjectView<'_>,
    command: &ExtensionCommand,
    given: &Map<String, Value>,
    format: CommandFormat,
    today: &str,
) -> Result<CommandOutput, RunError> {
    let args = command.normalize(given).map_err(RunError::Args)?;
    let root = view.project_root().map_err(RunError::NoProject)?;
    let input = CommandInput {
        args,
        cwd: canonical(root).display().to_string(),
        format,
        today: today.to_string(),
        graph: RawGraph::new(graph_export(view)).expect("the graph export is one JSON value"),
        evidence: evidence(view),
    };
    let Some(runtime) = view.runtime() else {
        return Err(RunError::Call(CallError::new(
            Operation::Command,
            command.extension(),
            command.export(),
            CallFailure::NotLoaded,
        )));
    };
    ExtensionCalls::new(runtime.as_ref())
        .run_command(command.extension(), command.export(), &input)
        .map_err(RunError::Call)
}

/// Today's date in UTC, `YYYY-MM-DD`: the one clock every command reads.
fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

/// `root` absolute and canonical, else as given.
fn canonical(root: &Path) -> PathBuf {
    std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}

#[cfg(test)]
mod tests;
