//! Surface contributions: the CLI commands, MCP tools and MCP resources an
//! extension contributes, each declared together with the function that
//! answers it.
//!
//! One declaration gives both halves of a surface: the `surfaces` describe
//! payload the host loads (ids, args, exports) and the routing of the
//! export the host then calls (`cmd__<prefix>_<id>`, `mcp__<name>`) to the
//! declared handler, so a declared command cannot lack its code, nor code
//! answer an export nothing declares. A command's handler reads its args
//! through [`CommandCall`], which only reads args the command declares, with
//! the type it declares them, after the SDK has checked the caller's values
//! against the declaration:
//!
//! ```
//! # use specforge_extension_sdk::prelude::*;
//! # let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/widgets", "1.0.0"));
//! c.command_prefix("acme");
//! c.command("widgets", |cmd| {
//!     cmd.title("List widgets")
//!         .description("Every widget, by id")
//!         .arg("limit", |a| {
//!             a.count().description("Return at most this many");
//!         })
//!         .handler(|call| {
//!             let limit = call.count("limit").unwrap_or(100);
//!             let ids: Vec<&str> = call
//!                 .graph()
//!                 .nodes_of_kind("widget")
//!                 .take(limit)
//!                 .map(|n| n.id.as_str())
//!                 .collect();
//!             call.render(&serde_json::json!({ "widgets": ids }), |out| {
//!                 out.push_str(&ids.join("\n"));
//!             })
//!         });
//! });
//! # let out = c.call_command("cmd__acme_widgets", &CommandInput::default()).unwrap();
//! # assert_eq!(out.exit_code, 0);
//! ```
//!
//! `testing::call_every_command` (feature `testing`) runs every declared command once
//! with every arg set, so a test catches a handler reading an arg its
//! command does not declare.

use crate::{
    CommandError, CommandEvidence, CommandFormat, CommandGraph, CommandInput, CommandOutput,
    McpResourceContent, McpResourceRequest,
};
use serde_json::Value;
use specforge_protocol_types::command_args::{self, normalize_args};
use specforge_protocol_types::{
    CommandArgDescriptor, CommandArgType, CommandDescriptor, McpResourceDescriptor,
    McpToolDescriptor, SurfaceDescriptor,
};

/// The error code of a command called with an arg it cannot use (ADR 0011):
/// the host's own ([`command_args::INVALID_INPUT`]).
pub use specforge_protocol_types::command_args::INVALID_INPUT;

/// The exit code of an `INVALID_INPUT` answer, the code clap gives the usage
/// errors it catches itself (ADR 0011).
pub const INVALID_INPUT_EXIT: i32 = 2;

type CommandHandler = Box<dyn Fn(&CommandCall<'_>) -> CommandOutput>;
type ToolHandler = Box<dyn Fn(&Value) -> Result<Value, String>>;
type ResourceHandler = Box<dyn Fn(&str) -> Result<String, String>>;

/// Every surface an extension declared, with its handlers.
#[derive(Default)]
pub(crate) struct Surfaces {
    prefix: Option<String>,
    commands: Vec<Command>,
    tools: Vec<Tool>,
    resources: Vec<Resource>,
    /// The describing and routing code, set by the first declaration. It is
    /// only reachable through a declaration, so a guest that declares no
    /// surface does not link it (the arg checks, the command input's and the
    /// descriptors' serde code).
    machinery: Option<Machinery>,
}

/// The wire answer of an export: `None` when nothing declared answers it.
type ExportAnswer = Option<Result<Vec<u8>, String>>;

#[derive(Clone, Copy)]
struct Machinery {
    describe: fn(&Surfaces) -> SurfaceDescriptor,
    dispatch: fn(&Surfaces, &str, &[u8]) -> ExportAnswer,
    call_command: fn(&Surfaces, &str, &CommandInput) -> Option<CommandOutput>,
}

const MACHINERY: Machinery = Machinery {
    describe: Surfaces::describe_declared,
    dispatch: Surfaces::dispatch_declared,
    call_command: Surfaces::call_declared_command,
};

struct Command {
    /// The command as declared, its args included.
    descriptor: CommandDescriptor,
    handler: CommandHandler,
}

struct Tool {
    descriptor: McpToolDescriptor,
    handler: ToolHandler,
}

struct Resource {
    descriptor: McpResourceDescriptor,
    handler: ResourceHandler,
}

/// `name` as an export suffix: every character that is not ASCII
/// alphanumeric or `_` becomes `_` (`probe.tool` is `probe_tool`).
fn export_suffix(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

impl Surfaces {
    /// Panics when a declared surface already answers `export`: two names
    /// that differ only in characters an export spells `_` (`a.b`, `a-b`,
    /// `a_b`), or a tool and a resource of one name.
    fn assert_free(&self, export: &str, what: &str) {
        let taken = self
            .commands
            .iter()
            .map(|c| (&c.descriptor.export, &c.descriptor.id))
            .chain(
                self.tools
                    .iter()
                    .map(|t| (&t.descriptor.export, &t.descriptor.name)),
            )
            .chain(
                self.resources
                    .iter()
                    .map(|r| (&r.descriptor.export, &r.descriptor.name)),
            )
            .find(|(e, _)| *e == export);
        if let Some((_, other)) = taken {
            panic!("{what}'s export {export} is already '{other}''s");
        }
    }

    /// Every declared surface's export, with the surface's name.
    pub(crate) fn exports(&self) -> impl Iterator<Item = (&str, &str)> {
        self.commands
            .iter()
            .map(|c| (c.descriptor.export.as_str(), c.descriptor.id.as_str()))
            .chain(
                self.tools
                    .iter()
                    .map(|t| (t.descriptor.export.as_str(), t.descriptor.name.as_str())),
            )
            .chain(
                self.resources
                    .iter()
                    .map(|r| (r.descriptor.export.as_str(), r.descriptor.name.as_str())),
            )
    }

    /// The name of the declared surface whose export is `export`.
    pub(crate) fn owner(&self, export: &str) -> Option<&str> {
        self.exports()
            .find(|(e, _)| *e == export)
            .map(|(_, name)| name)
    }

    /// Every declared command's descriptor, in declaration order.
    pub(crate) fn command_descriptors(&self) -> impl Iterator<Item = &CommandDescriptor> {
        self.commands.iter().map(|c| &c.descriptor)
    }

    pub(crate) fn set_prefix(&mut self, prefix: &str) {
        assert!(
            self.commands.is_empty(),
            "command_prefix(\"{prefix}\") must come before the commands it names"
        );
        self.prefix = Some(export_suffix(prefix));
    }

    // Each `add_*` (and `CommandBuilder::arg`) is generic over its closure
    // only to call it: the rest is one non-generic `start_*`/`finish_*`
    // pair, so a guest declaring many surfaces holds one copy of it.
    pub(crate) fn add_command(&mut self, id: &str, f: impl FnOnce(&mut CommandBuilder)) {
        let mut b = self.start_command(id);
        f(&mut b);
        self.finish_command(b);
    }

    fn start_command(&mut self, id: &str) -> CommandBuilder {
        self.machinery = Some(MACHINERY);
        assert!(
            !self.commands.iter().any(|c| c.descriptor.id == id),
            "command '{id}' is declared twice"
        );
        let export = match &self.prefix {
            Some(prefix) => format!("cmd__{prefix}_{}", export_suffix(id)),
            None => format!("cmd__{}", export_suffix(id)),
        };
        CommandBuilder {
            descriptor: CommandDescriptor {
                id: id.to_string(),
                title: String::new(),
                description: String::new(),
                category: None,
                export,
                args: Vec::new(),
            },
            handler: None,
        }
    }

    fn finish_command(&mut self, b: CommandBuilder) {
        let id = &b.descriptor.id;
        let Some(handler) = b.handler else {
            panic!("command '{id}' declares no handler");
        };
        self.assert_free(&b.descriptor.export, &format!("command '{id}'"));
        self.commands.push(Command {
            descriptor: b.descriptor,
            handler,
        });
    }

    pub(crate) fn add_tool(&mut self, name: &str, f: impl FnOnce(&mut McpToolBuilder)) {
        let mut b = self.start_tool(name);
        f(&mut b);
        self.finish_tool(b);
    }

    fn start_tool(&mut self, name: &str) -> McpToolBuilder {
        self.machinery = Some(MACHINERY);
        McpToolBuilder {
            descriptor: McpToolDescriptor {
                name: name.to_string(),
                description: String::new(),
                category: None,
                export: format!("mcp__{}", export_suffix(name)),
                input_schema: serde_json::json!({"type": "object"}),
                output_schema: None,
            },
            handler: None,
        }
    }

    fn finish_tool(&mut self, b: McpToolBuilder) {
        let name = &b.descriptor.name;
        let Some(handler) = b.handler else {
            panic!("MCP tool '{name}' declares no handler");
        };
        self.assert_free(&b.descriptor.export, &format!("MCP tool '{name}'"));
        self.tools.push(Tool {
            descriptor: b.descriptor,
            handler,
        });
    }

    pub(crate) fn add_resource(&mut self, name: &str, f: impl FnOnce(&mut McpResourceBuilder)) {
        let mut b = self.start_resource(name);
        f(&mut b);
        self.finish_resource(b);
    }

    fn start_resource(&mut self, name: &str) -> McpResourceBuilder {
        self.machinery = Some(MACHINERY);
        McpResourceBuilder {
            descriptor: McpResourceDescriptor {
                uri_template: String::new(),
                name: name.to_string(),
                description: None,
                export: format!("mcp__{}", export_suffix(name)),
                mime_type: "application/json".to_string(),
            },
            handler: None,
        }
    }

    fn finish_resource(&mut self, b: McpResourceBuilder) {
        let name = &b.descriptor.name;
        let Some(handler) = b.handler else {
            panic!("MCP resource '{name}' declares no handler");
        };
        self.assert_free(&b.descriptor.export, &format!("MCP resource '{name}'"));
        self.resources.push(Resource {
            descriptor: b.descriptor,
            handler,
        });
    }

    /// Every declared surface, in declaration order (empty when nothing is
    /// declared): the declaration's `surfaces`.
    pub(crate) fn descriptor(&self) -> SurfaceDescriptor {
        match self.machinery {
            Some(m) => (m.describe)(self),
            None => SurfaceDescriptor::default(),
        }
    }

    /// The command whose export is `export`, run on `input`; `None` when no
    /// command has that export.
    pub(crate) fn call_command(&self, export: &str, input: &CommandInput) -> Option<CommandOutput> {
        (self.machinery?.call_command)(self, export, input)
    }

    /// The wire answer of the surface export `export`; `None` when no
    /// declared surface has it.
    pub(crate) fn dispatch(&self, export: &str, input: &[u8]) -> ExportAnswer {
        (self.machinery?.dispatch)(self, export, input)
    }

    fn describe_declared(&self) -> SurfaceDescriptor {
        SurfaceDescriptor {
            commands: self.commands.iter().map(|c| c.descriptor.clone()).collect(),
            mcp_tools: self.tools.iter().map(|t| t.descriptor.clone()).collect(),
            mcp_resources: self
                .resources
                .iter()
                .map(|r| r.descriptor.clone())
                .collect(),
        }
    }

    fn call_declared_command(&self, export: &str, input: &CommandInput) -> Option<CommandOutput> {
        let command = self
            .commands
            .iter()
            .find(|c| c.descriptor.export == export)?;
        Some(match CommandCall::new(command, input) {
            Ok(call) => (command.handler)(&call),
            Err(error) => CommandOutput::error(input.format, &error, INVALID_INPUT_EXIT),
        })
    }

    fn dispatch_declared(&self, export: &str, input: &[u8]) -> ExportAnswer {
        if self.commands.iter().any(|c| c.descriptor.export == export) {
            let input: CommandInput = match serde_json::from_slice(input) {
                Ok(input) => input,
                Err(e) => return Some(Err(format!("invalid command input: {e}"))),
            };
            return self
                .call_declared_command(export, &input)
                .map(|output| Ok(output.to_bytes()));
        }
        if let Some(tool) = self.tools.iter().find(|t| t.descriptor.export == export) {
            let input: Value = match serde_json::from_slice(input) {
                Ok(input) => input,
                Err(e) => return Some(Err(format!("invalid tool input: {e}"))),
            };
            return Some((tool.handler)(&input).map(|out| out.to_string().into_bytes()));
        }
        let resource = self
            .resources
            .iter()
            .find(|r| r.descriptor.export == export)?;
        let request: McpResourceRequest = match serde_json::from_slice(input) {
            Ok(request) => request,
            Err(e) => return Some(Err(format!("invalid resource input: {e}"))),
        };
        Some((resource.handler)(&request.uri).map(|content| {
            let answer = McpResourceContent {
                content,
                mime_type: resource.descriptor.mime_type.clone(),
            };
            serde_json::to_vec(&answer).expect("resource content serialization cannot fail")
        }))
    }
}

/// Builder for one command ([`crate::ContributionsBuilder::command`]).
pub struct CommandBuilder {
    /// The command as declared so far, its args included.
    descriptor: CommandDescriptor,
    handler: Option<CommandHandler>,
}

impl CommandBuilder {
    /// The one-line title help lists the command under.
    pub fn title(&mut self, title: &str) -> &mut Self {
        self.descriptor.title = title.to_string();
        self
    }

    /// What the command answers: its help text and its MCP tool's description.
    pub fn description(&mut self, description: &str) -> &mut Self {
        self.descriptor.description = description.to_string();
        self
    }

    pub fn category(&mut self, category: &str) -> &mut Self {
        self.descriptor.category = Some(category.to_string());
        self
    }

    /// Declare the arg `name` (a string unless `f` says otherwise). On the
    /// command line a required arg is positional, in declaration order; any
    /// other, and every flag, is `--<name with _ as ->`.
    ///
    /// Panics on what the host refuses ([`command_args::refusal`]): an arg
    /// taking an option the host owns (`path`, `format`, `help`), two args
    /// spelling one option (`all_kinds`, `all-kinds`), a required arg with
    /// a default, a flag with a default (a flag is `false` unless set), a
    /// default its type refuses; and on a required flag or an empty
    /// `one_of`, which would mean nothing.
    pub fn arg(&mut self, name: &str, f: impl FnOnce(&mut ArgBuilder)) -> &mut Self {
        let mut b = ArgBuilder(CommandArgDescriptor {
            name: name.to_string(),
            ..Default::default()
        });
        f(&mut b);
        self.finish_arg(b)
    }

    fn finish_arg(&mut self, b: ArgBuilder) -> &mut Self {
        let id = &self.descriptor.id;
        let arg = b.0;
        let name = &arg.name;
        if let CommandArgType::Enum { values } = &arg.arg_type {
            assert!(
                !values.is_empty(),
                "command '{id}' declares arg '{name}' one of no value"
            );
        }
        assert!(
            !(arg.arg_type == CommandArgType::Bool && arg.required),
            "command '{id}' declares the flag '{name}' required: a flag is false unless set"
        );
        self.descriptor.args.push(arg);
        if let Some(refused) = command_args::refusal(&self.descriptor.args) {
            panic!("command '{id}' is refused by the host: {refused}");
        }
        self
    }

    /// The function that answers the command.
    pub fn handler(
        &mut self,
        handler: impl Fn(&CommandCall<'_>) -> CommandOutput + 'static,
    ) -> &mut Self {
        self.handler = Some(Box::new(handler));
        self
    }
}

/// Builder for one command arg ([`CommandBuilder::arg`]).
pub struct ArgBuilder(CommandArgDescriptor);

impl ArgBuilder {
    /// A string (the default); read with [`CommandCall::str`].
    pub fn string(&mut self) -> &mut Self {
        self.set(CommandArgType::String)
    }

    /// A file system path, passed as a string; read with [`CommandCall::str`].
    pub fn path(&mut self) -> &mut Self {
        self.set(CommandArgType::Path)
    }

    /// A boolean `--flag`; read with [`CommandCall::flag`].
    pub fn flag(&mut self) -> &mut Self {
        self.set(CommandArgType::Bool)
    }

    /// An integer; read with [`CommandCall::integer`].
    pub fn integer(&mut self) -> &mut Self {
        self.set(CommandArgType::Integer)
    }

    /// A non-negative integer (an integer of `minimum` 0 on the wire); read
    /// with [`CommandCall::count`]. A negative value is `INVALID_INPUT`.
    pub fn count(&mut self) -> &mut Self {
        self.set(CommandArgType::Integer);
        self.0.minimum = Some(0);
        self
    }

    /// One of `values`; read with [`CommandCall::str`]. Any other value is
    /// `INVALID_INPUT` (the CLI refuses it first).
    pub fn one_of(&mut self, values: &[&str]) -> &mut Self {
        self.set(CommandArgType::Enum {
            values: values.iter().map(|v| v.to_string()).collect(),
        })
    }

    /// The caller must set it: a positional arg on the command line.
    pub fn required(&mut self) -> &mut Self {
        self.0.required = true;
        self
    }

    /// The value the arg has when the caller leaves it out, on every
    /// surface: the host applies it on the command line and over MCP, and
    /// the SDK to a call that lacks it ([`command_args::normalize_args`]).
    /// Not for a `required` arg or a flag.
    pub fn default_value(&mut self, value: &str) -> &mut Self {
        self.0.default_value = Some(value.to_string());
        self
    }

    /// The arg's help text and its MCP tool property's description.
    pub fn description(&mut self, description: &str) -> &mut Self {
        self.0.description = Some(description.to_string());
        self
    }

    fn set(&mut self, arg_type: CommandArgType) -> &mut Self {
        self.0.arg_type = arg_type;
        self.0.minimum = None;
        self
    }
}

/// Builder for one MCP tool ([`crate::ContributionsBuilder::mcp_tool`]).
pub struct McpToolBuilder {
    descriptor: McpToolDescriptor,
    handler: Option<ToolHandler>,
}

impl McpToolBuilder {
    pub fn description(&mut self, description: &str) -> &mut Self {
        self.descriptor.description = description.to_string();
        self
    }
    pub fn category(&mut self, category: &str) -> &mut Self {
        self.descriptor.category = Some(category.to_string());
        self
    }
    /// The JSON Schema of the tool's arguments (default `{"type": "object"}`).
    pub fn input_schema(&mut self, schema: Value) -> &mut Self {
        self.descriptor.input_schema = schema;
        self
    }
    pub fn output_schema(&mut self, schema: Value) -> &mut Self {
        self.descriptor.output_schema = Some(schema);
        self
    }
    /// The function that answers the tool: its arguments in, its JSON
    /// result out; an `Err` fails the call.
    pub fn handler(
        &mut self,
        handler: impl Fn(&Value) -> Result<Value, String> + 'static,
    ) -> &mut Self {
        self.handler = Some(Box::new(handler));
        self
    }
}

/// Builder for one MCP resource ([`crate::ContributionsBuilder::mcp_resource`]).
pub struct McpResourceBuilder {
    descriptor: McpResourceDescriptor,
    handler: Option<ResourceHandler>,
}

impl McpResourceBuilder {
    /// The URIs the resource answers (`specforge://ext/acme/{id}`).
    pub fn uri_template(&mut self, template: &str) -> &mut Self {
        self.descriptor.uri_template = template.to_string();
        self
    }
    pub fn description(&mut self, description: &str) -> &mut Self {
        self.descriptor.description = Some(description.to_string());
        self
    }
    /// The content's MIME type (default `application/json`), which every
    /// read reports.
    pub fn mime_type(&mut self, mime_type: &str) -> &mut Self {
        self.descriptor.mime_type = mime_type.to_string();
        self
    }
    /// The function that reads the resource: the URI read in, its content
    /// out; an `Err` fails the read.
    pub fn handler(
        &mut self,
        handler: impl Fn(&str) -> Result<String, String> + 'static,
    ) -> &mut Self {
        self.handler = Some(Box::new(handler));
        self
    }
}

/// One call of a command, as its handler sees it: the args the caller set,
/// checked against the command's declaration, and the rest of the
/// [`CommandInput`].
///
/// The arg accessors read only args the command declares, as the type it
/// declares them: reading an undeclared arg, or a declared one as another
/// type, is a bug in the extension and panics (the host reports the trap
/// as E028), so a test calling the command finds it.
pub struct CommandCall<'a> {
    command: &'a Command,
    input: &'a CommandInput,
    /// The declared args the caller set, normalized to their declared type
    /// (a string, an integer or a boolean).
    values: serde_json::Map<String, Value>,
}

impl<'a> CommandCall<'a> {
    /// `input`'s args normalized by the host's rule
    /// ([`command_args::normalize_args`]): an absent arg takes its declared
    /// default and an unset flag is `false`; a required arg missing, a value
    /// of another type, or an arg the command does not declare is
    /// `INVALID_INPUT`.
    fn new(command: &'a Command, input: &'a CommandInput) -> Result<Self, CommandError> {
        let values = normalize_args(&command.descriptor.args, &input.args)
            .map_err(|error| error.to_command_error())?;
        Ok(CommandCall {
            command,
            input,
            values,
        })
    }

    /// The id of the command called.
    pub fn id(&self) -> &str {
        &self.command.descriptor.id
    }

    /// The compiled project's graph.
    pub fn graph(&self) -> &'a CommandGraph {
        &self.input.graph
    }

    /// The format the caller asked for.
    pub fn format(&self) -> CommandFormat {
        self.input.format
    }

    /// Whether the caller asked for `json`.
    pub fn is_json(&self) -> bool {
        self.input.is_json()
    }

    /// The host's date, `YYYY-MM-DD`, UTC; empty when the host passed none.
    pub fn today(&self) -> &'a str {
        &self.input.today
    }

    /// The project root, absolute and canonical (the host resolves symlinks).
    pub fn cwd(&self) -> &'a str {
        &self.input.cwd
    }

    /// What the project's recorded test report proves, per entity that
    /// counts toward coverage, as the host scores it; `none` without a
    /// report (or from a host older than the field).
    pub fn evidence(&self) -> &'a CommandEvidence {
        &self.input.evidence
    }

    /// Whether the declared arg `name` has a value: the caller's, or its
    /// declared default (a flag always has one, `false` unless set).
    pub fn is_set(&self, name: &str) -> bool {
        self.declared(name);
        self.values.contains_key(name)
    }

    /// The string, path or enum arg `name`, when set.
    pub fn str(&self, name: &str) -> Option<&str> {
        let arg = self.declared(name);
        self.expect(
            arg,
            matches!(
                arg.arg_type,
                CommandArgType::String | CommandArgType::Path | CommandArgType::Enum { .. }
            ),
            "a string",
        );
        self.values.get(name).and_then(Value::as_str)
    }

    /// The count arg `name` ([`ArgBuilder::count`]), when set; a count
    /// beyond `usize` (on a 32-bit guest) is `usize::MAX`.
    pub fn count(&self, name: &str) -> Option<usize> {
        let arg = self.declared(name);
        self.expect(arg, is_count(arg), "a count");
        self.values
            .get(name)
            .and_then(Value::as_u64)
            .map(|n| usize::try_from(n).unwrap_or(usize::MAX))
    }

    /// The integer (or count) arg `name`, when set.
    pub fn integer(&self, name: &str) -> Option<i64> {
        let arg = self.declared(name);
        self.expect(arg, arg.arg_type == CommandArgType::Integer, "an integer");
        self.values.get(name).and_then(Value::as_i64)
    }

    /// The flag `name`; `false` when unset.
    pub fn flag(&self, name: &str) -> bool {
        let arg = self.declared(name);
        self.expect(arg, arg.arg_type == CommandArgType::Bool, "a flag");
        self.values
            .get(name)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// `payload` as pretty JSON (one root object, then a newline) when the
    /// caller asked for `json`, else what `human` writes.
    pub fn render<T: serde::Serialize>(
        &self,
        payload: &T,
        human: impl FnOnce(&mut String),
    ) -> CommandOutput {
        let mut out = String::new();
        if self.is_json() {
            out = serde_json::to_string_pretty(payload).unwrap_or_default();
            out.push('\n');
        } else {
            human(&mut out);
        }
        CommandOutput::ok(out)
    }

    /// `error` on stderr in the format the caller asked for, exit
    /// `exit_code` ([`CommandOutput::error`]).
    pub fn fail(&self, error: &CommandError, exit_code: i32) -> CommandOutput {
        CommandOutput::error(self.format(), error, exit_code)
    }

    fn declared(&self, name: &str) -> &'a CommandArgDescriptor {
        let command = self.command;
        command
            .descriptor
            .args
            .iter()
            .find(|a| a.name == name)
            .unwrap_or_else(|| {
                panic!(
                    "command '{}' reads arg '{name}', which it does not declare",
                    command.descriptor.id
                )
            })
    }

    fn expect(&self, arg: &CommandArgDescriptor, ok: bool, what: &str) {
        assert!(
            ok,
            "command '{}' reads arg '{}' as {what}, but declares it {:?}{}",
            self.command.descriptor.id,
            arg.name,
            arg.arg_type,
            if is_count(arg) { " (a count)" } else { "" }
        );
    }
}

/// Whether `arg` is a count ([`ArgBuilder::count`]): an integer that is
/// never negative.
fn is_count(arg: &CommandArgDescriptor) -> bool {
    arg.arg_type == CommandArgType::Integer && arg.minimum.is_some_and(|minimum| minimum >= 0)
}

#[cfg(test)]
mod tests {
    use crate::{CommandFormat, CommandInput, CommandOutput, ContributionsBuilder, ExtensionMeta};
    use serde_json::json;

    fn builder() -> ContributionsBuilder {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/widgets", "1.0.0"));
        c.command_prefix("acme");
        c.command("widgets", |cmd| {
            cmd.title("List widgets")
                .description("Every widget")
                .category("query")
                .arg("widget", |a| {
                    a.required().description("The widget id");
                })
                .arg("limit", |a| {
                    a.count();
                })
                .arg("shift", |a| {
                    a.integer();
                })
                .arg("all", |a| {
                    a.flag();
                })
                .arg("color", |a| {
                    a.one_of(&["red", "blue"]);
                })
                .handler(|call| {
                    let out = json!({
                        "widget": call.str("widget"),
                        "limit": call.count("limit"),
                        "shift": call.integer("shift"),
                        "all": call.flag("all"),
                        "color": call.str("color"),
                        "nodes": call.graph().nodes().len(),
                        "today": call.today(),
                    });
                    call.render(&out, |s| s.push_str("human\n"))
                });
        });
        c
    }

    fn call(c: &ContributionsBuilder, args: serde_json::Value) -> CommandOutput {
        let input = CommandInput {
            args: args.as_object().unwrap().clone(),
            format: CommandFormat::Json,
            today: "2026-10-04".into(),
            ..Default::default()
        };
        c.call_command("cmd__acme_widgets", &input).unwrap()
    }

    #[test]
    fn a_declared_command_is_described_with_its_args_in_order() {
        let c = builder();
        let described: serde_json::Value =
            serde_json::from_str(&c.describe_response_json("surfaces").unwrap()).unwrap();
        assert_eq!(
            described["items"],
            json!([{"commands": [{
                "id": "widgets", "title": "List widgets", "description": "Every widget",
                "category": "query", "export": "cmd__acme_widgets",
                "args": [
                    {"name": "widget", "arg_type": "string", "required": true,
                     "description": "The widget id"},
                    {"name": "limit", "arg_type": "integer", "required": false, "minimum": 0},
                    {"name": "shift", "arg_type": "integer", "required": false},
                    {"name": "all", "arg_type": "bool", "required": false},
                    {"name": "color", "arg_type": {"enum": {"values": ["red", "blue"]}},
                     "required": false}
                ]
            }]}])
        );
    }

    #[test]
    fn no_surfaces_describe_as_none() {
        let c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
        let described: serde_json::Value =
            serde_json::from_str(&c.describe_response_json("surfaces").unwrap()).unwrap();
        assert_eq!(described, json!({"category": "surfaces", "items": []}));
    }

    #[test]
    fn the_handler_reads_the_declared_args_typed() {
        let out = call(
            &builder(),
            json!({"widget": "w1", "limit": "3", "shift": -2, "all": "true", "color": "red"}),
        );
        assert_eq!(out.exit_code, 0, "{out:?}");
        let out: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(
            out,
            json!({"widget": "w1", "limit": 3, "shift": -2, "all": true, "color": "red",
                   "nodes": 0, "today": "2026-10-04"})
        );
    }

    #[test]
    fn a_value_the_declaration_refuses_is_invalid_input() {
        let c = builder();
        for (args, says) in [
            (json!({}), "missing required arg 'widget'"),
            (
                json!({"widget": "w", "limit": -1}),
                "limit must be a non-negative integer, got -1",
            ),
            (
                json!({"widget": "w", "shift": "x"}),
                "shift must be an integer, got 'x'",
            ),
            (
                json!({"widget": "w", "undeclared": 1}),
                "unknown argument 'undeclared'",
            ),
            (
                json!({"widget": "w", "all": 1}),
                "all must be true or false, got 1",
            ),
            (
                json!({"widget": "w", "color": "green"}),
                "color must be one of red, blue, got 'green'",
            ),
            (json!({"widget": 5}), "widget must be a string, got 5"),
        ] {
            let out = call(&c, args.clone());
            assert_eq!((out.exit_code, out.stdout.as_str()), (2, ""), "{args}");
            let error: serde_json::Value = serde_json::from_str(&out.stderr).unwrap();
            assert_eq!(error["code"], "INVALID_INPUT", "{args}");
            assert_eq!(error["message"], says, "{args}");
        }
    }

    #[test]
    fn the_export_routes_to_the_handler_and_nothing_else() {
        let c = builder();
        let input = json!({"args": {"widget": "w1"}, "format": "json"}).to_string();
        let out: CommandOutput = serde_json::from_slice(
            &c.dispatch_export("cmd__acme_widgets", input.as_bytes())
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(out.exit_code, 0);
        assert!(
            c.dispatch_export("cmd__acme_nope", input.as_bytes())
                .is_none()
        );
        assert!(
            c.dispatch_export("cmd__acme_widgets", b"not json")
                .unwrap()
                .is_err()
        );
    }

    #[test]
    #[should_panic(expected = "command 'w' reads arg 'nope', which it does not declare")]
    fn reading_an_undeclared_arg_panics() {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
        c.command("w", |cmd| {
            cmd.handler(|call| CommandOutput::ok(call.str("nope").unwrap_or_default()));
        });
        c.call_command("cmd__w", &CommandInput::default());
    }

    #[test]
    #[should_panic(expected = "reads arg 'n' as a count, but declares it String")]
    fn reading_an_arg_as_another_type_panics() {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
        c.command("w", |cmd| {
            cmd.arg("n", |_| {})
                .handler(|call| CommandOutput::ok(format!("{:?}", call.count("n"))));
        });
        c.call_command("cmd__w", &CommandInput::default());
    }

    #[test]
    #[should_panic(expected = "is refused by the host: its arg 'format' takes the host's --format")]
    fn an_arg_the_host_owns_is_refused() {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
        c.command("w", |cmd| {
            cmd.arg("format", |_| {});
        });
    }

    #[test]
    #[should_panic(expected = "is refused by the host: it declares the arg 'all-kinds' twice")]
    fn two_args_spelling_one_option_are_refused() {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
        c.command("w", |cmd| {
            cmd.arg("all_kinds", |a| {
                a.flag();
            })
            .arg("all-kinds", |a| {
                a.flag();
            });
        });
    }

    #[test]
    #[should_panic(expected = "command 'w' declares no handler")]
    fn a_command_without_a_handler_is_refused() {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
        c.command("w", |cmd| {
            cmd.title("W");
        });
    }

    #[test]
    fn an_absent_arg_takes_its_default_on_every_surface() {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
        c.command("w", |cmd| {
            cmd.arg("limit", |a| {
                a.count().default_value("7");
            })
            .arg("order", |a| {
                a.one_of(&["asc", "desc"]).default_value("desc");
            })
            .handler(|call| {
                CommandOutput::ok(format!(
                    "{:?} {:?} {}",
                    call.count("limit"),
                    call.str("order"),
                    call.is_set("order")
                ))
            });
        });
        let out = c.call_command("cmd__w", &CommandInput::default()).unwrap();
        assert_eq!(out.stdout, "Some(7) Some(\"desc\") true");
        let input = CommandInput {
            args: json!({"limit": 2, "order": "asc"})
                .as_object()
                .unwrap()
                .clone(),
            ..Default::default()
        };
        let out = c.call_command("cmd__w", &input).unwrap();
        assert_eq!(out.stdout, "Some(2) Some(\"asc\") true");
    }

    #[test]
    fn a_declaration_that_contradicts_itself_is_refused() {
        type Declare = fn(&mut crate::ArgBuilder);
        let cases: [(Declare, &str); 5] = [
            (
                |a| {
                    a.required().default_value("x");
                },
                "both required and with a default",
            ),
            (
                |a| {
                    a.flag().required();
                },
                "a flag is false unless set",
            ),
            (
                |a| {
                    a.flag().default_value("true");
                },
                "a flag is false unless set",
            ),
            (
                |a| {
                    a.count().default_value("-1");
                },
                "a default its type refuses: n must be a non-negative integer",
            ),
            (
                |a| {
                    a.one_of(&["a"]).default_value("b");
                },
                "a default its type refuses: n must be one of a, got 'b'",
            ),
        ];
        for (declare, says) in cases {
            let panic = std::panic::catch_unwind(|| {
                let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
                c.command("w", |cmd| {
                    cmd.arg("n", declare).handler(|_| CommandOutput::ok(""));
                });
            })
            .expect_err(says);
            let message = panic.downcast_ref::<String>().cloned().unwrap_or_default();
            assert!(message.contains(says), "{message}");
        }
    }

    #[test]
    fn two_surfaces_cannot_share_an_export() {
        type Declare = fn(&mut ContributionsBuilder);
        let cases: [(Declare, &str); 3] = [
            (
                |c| {
                    c.command("a_b", |cmd| {
                        cmd.handler(|_| CommandOutput::ok(""));
                    });
                    c.command("a-b", |cmd| {
                        cmd.handler(|_| CommandOutput::ok(""));
                    });
                },
                "command 'a-b''s export cmd__a_b is already 'a_b''s",
            ),
            (
                |c| {
                    c.mcp_tool("acme.doc", |t| {
                        t.handler(|_| Ok(json!({})));
                    });
                    c.mcp_resource("acme-doc", |r| {
                        r.handler(|_| Ok(String::new()));
                    });
                },
                "MCP resource 'acme-doc''s export mcp__acme_doc is already 'acme.doc''s",
            ),
            (
                |c| {
                    c.command_prefix("acme");
                    c.command("x", |cmd| {
                        cmd.handler(|_| CommandOutput::ok(""));
                    });
                    c.command("x", |cmd| {
                        cmd.handler(|_| CommandOutput::ok(""));
                    });
                },
                "command 'x' is declared twice",
            ),
        ];
        for (declare, says) in cases {
            let panic = std::panic::catch_unwind(|| {
                let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
                declare(&mut c);
            })
            .expect_err(says);
            let message = panic.downcast_ref::<String>().cloned().unwrap_or_default();
            assert!(message.contains(says), "{message}");
        }
    }

    #[test]
    fn calling_every_command_sets_every_arg_with_its_type() {
        let outputs = crate::testing::call_every_command(
            &builder(),
            &Default::default(),
            "2026-10-04",
            |command, arg| format!("{command}.{arg}"),
        );
        assert_eq!(outputs.len(), 1);
        let (id, out) = &outputs[0];
        assert_eq!((id.as_str(), out.exit_code), ("widgets", 0), "{out:?}");
        let out: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(
            out,
            json!({"widget": "widgets.widget", "limit": 1, "shift": 1, "all": true,
                   "color": "red", "nodes": 0, "today": "2026-10-04"})
        );
    }

    #[test]
    #[should_panic(expected = "command 'w' reads arg 'typo', which it does not declare")]
    fn calling_every_command_finds_an_undeclared_read() {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
        c.command("w", |cmd| {
            cmd.arg("n", |a| {
                a.flag();
            })
            .handler(|call| {
                // Only reached with the flag set: a call without args misses it.
                if call.flag("n") {
                    let _ = call.str("typo");
                }
                CommandOutput::ok("")
            });
        });
        crate::testing::call_every_command(&c, &Default::default(), "", |_, _| String::new());
    }

    #[test]
    fn tools_and_resources_are_described_and_routed() {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
        c.mcp_tool("acme.echo", |t| {
            t.description("Echo")
                .handler(|input| Ok(json!({"echo": input})));
        });
        c.mcp_resource("acme-doc", |r| {
            r.uri_template("specforge://ext/acme/{id}")
                .mime_type("text/plain")
                .handler(|uri| Ok(format!("read {uri}")));
        });
        let described: serde_json::Value =
            serde_json::from_str(&c.describe_response_json("surfaces").unwrap()).unwrap();
        assert_eq!(
            described["items"],
            json!([{
                "commands": [],
                "mcp_tools": [{"name": "acme.echo", "description": "Echo",
                    "export": "mcp__acme_echo", "input_schema": {"type": "object"}}],
                "mcp_resources": [{"uri_template": "specforge://ext/acme/{id}",
                    "name": "acme-doc", "export": "mcp__acme_doc", "mime_type": "text/plain"}]
            }])
        );
        let tool = c
            .dispatch_export("mcp__acme_echo", br#"{"a":1}"#)
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&tool).unwrap(),
            json!({"echo": {"a": 1}})
        );
        let read = c
            .dispatch_export("mcp__acme_doc", br#"{"uri":"specforge://ext/acme/d1"}"#)
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&read).unwrap(),
            json!({"content": "read specforge://ext/acme/d1", "mime_type": "text/plain"})
        );
    }
}
