//! What a tool call produced, and the one place that turns it into a
//! `tools/call` reply.
//!
//! Handlers return a [`ToolOutcome`] (a mutation's handler, a
//! [`Mutation`](crate::mutation::Mutation) that [`crate::mutation::complete`]
//! turns into one); [`envelope`] alone builds `content`, `isError` and
//! `_meta`. What a mutation wrote crosses to the dispatcher typed (ADR 0022),
//! never read back from the reply.

use serde_json::{Value, json};
use specforge_common::{Diagnostic, Severity};
use specforge_graph::Graph;

use crate::args::Argument;
use crate::protocol::{JsonRpcError, JsonRpcResponse, error_codes};
use crate::target::{Call, ProjectTarget, TargetSpec, WithoutProject};
use crate::types::McpToolDescriptor;
use specforge_ops::{OpError, OpErrorKind};

/// The group a tool that is no mutation is listed in: the spec's
/// `McpToolGroup`, every `McpToolCategory` but `mutation`. A core tool
/// declares it on its effect; an extension tool's is the category it
/// declares when that names a group, else `Core`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolGroup {
    Core,
    Navigation,
    Management,
}

impl ToolGroup {
    /// The group `name` names (`core`, `navigation`, `management`); `None`
    /// for any other name, `mutation` included: only a core tool that writes
    /// its target's project files is a mutation.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "core" => Some(ToolGroup::Core),
            "navigation" => Some(ToolGroup::Navigation),
            "management" => Some(ToolGroup::Management),
            _ => None,
        }
    }
}

/// A tool's role as `tools/list` and `mcp_tool_invoked` name it: the spec's
/// `McpToolCategory`. Never declared: a mutation's is `Mutation`, any other
/// tool's its group ([`ToolSpec::category`]). Where a tool comes from is its
/// `source`, a separate field (ADR 0004 D4-b).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Core,
    Navigation,
    Mutation,
    Management,
}

impl Category {
    /// The category as `tools/list` spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Core => "core",
            Category::Navigation => "navigation",
            Category::Mutation => "mutation",
            Category::Management => "management",
        }
    }
}

impl From<ToolGroup> for Category {
    fn from(group: ToolGroup) -> Self {
        match group {
            ToolGroup::Core => Category::Core,
            ToolGroup::Navigation => Category::Navigation,
            ToolGroup::Management => Category::Management,
        }
    }
}

/// The `source` of every core tool; an extension tool's is the
/// extension's name.
pub const CORE_SOURCE: &str = "core";

/// How a tool that writes writes, as MCP's tool annotations say it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteHints {
    /// It may overwrite or remove what is there (`destructiveHint`); false:
    /// it only adds.
    pub destructive: bool,
    /// Calling it again with the same arguments changes nothing more
    /// (`idempotentHint`); a repeat it refuses, writing nothing, counts.
    pub idempotent: bool,
    /// It reaches beyond the project: a registry, a test runner
    /// (`openWorldHint`).
    pub open_world: bool,
}

impl WriteHints {
    /// `{readOnlyHint: false, destructiveHint, idempotentHint, openWorldHint}`.
    pub fn annotations(self) -> Value {
        json!({
            "readOnlyHint": false,
            "destructiveHint": self.destructive,
            "idempotentHint": self.idempotent,
            "openWorldHint": self.open_world,
        })
    }
}

/// The annotations of a tool that only reads and reaches nothing beyond the
/// project, `{readOnlyHint: true, openWorldHint: false}`: every core read,
/// and every extension tool (the host grants an extension no capability,
/// ADR 0037 D1).
pub fn read_only_annotations() -> Value {
    json!({ "readOnlyHint": true, "openWorldHint": false })
}

/// What a tool does to its environment, with the handler that does it: the
/// one declaration its category, annotations, call target and reply's
/// `files_written` derive from (ADR 0024, round-5 amendment).
#[derive(Clone, Copy)]
pub enum Effect {
    /// It only reads: `readOnlyHint`, listed in `group`.
    Reads { group: ToolGroup, handler: Handler },
    /// It writes output artifacts, not its target's project files (collect
    /// writes the recorded test report, render an export): listed in
    /// `group`, annotated with `hints`; its reply is all there is (ADR 0022).
    WritesOutput {
        group: ToolGroup,
        hints: WriteHints,
        handler: Handler,
    },
    /// It writes its target's project files: category `mutation`, annotated
    /// with `hints`; its handler says what it wrote, which the reply lists
    /// as `files_written` (ADR 0022).
    Mutates {
        hints: WriteHints,
        handler: MutationHandler,
    },
}

/// How a tool that is no mutation runs. The variant is what its handler is
/// given, and so its call target: whether it reads a project, and what a
/// call with nothing served gets. Built only by the table's `unscoped!`,
/// `view!` and `project!`, whose `run` reads the typed arguments and hands
/// the handler exactly its variant's input; no handler is given the call.
#[derive(Clone, Copy)]
pub enum Handler {
    /// Reads no project (explain). Its handler is
    /// `fn(Args) -> Answered<Reply>` (or `Answered<Text>`).
    Unscoped {
        arguments: fn() -> Vec<Argument>,
        reply: Option<fn() -> Value>,
        run: fn(&Call<'_>, Value) -> ToolOutcome,
    },
    /// Reads the project view of `target`'s project; with nothing served,
    /// the empty session's (a read that then names a file or an entity is
    /// refused as no project, ADR 0025). Its handler is
    /// `fn(ProjectView<'_>, Args) -> Answered<Reply>` (or `Answered<Text>`).
    View {
        target: ProjectTarget,
        arguments: fn() -> Vec<Argument>,
        reply: Option<fn() -> Value>,
        run: fn(&Call<'_>, Value) -> ToolOutcome,
    },
    /// Acts on `target`'s project on disk (its root, its runtime, its view):
    /// with nothing served and no project named, the call target refuses it
    /// as no project before it runs. Its handler is
    /// `fn(&ProjectRef<'_>, Args) -> Answered<Reply>` (or `Answered<Text>`).
    Project {
        target: ProjectTarget,
        arguments: fn() -> Vec<Argument>,
        reply: Option<fn() -> Value>,
        run: fn(&Call<'_>, Value) -> ToolOutcome,
    },
}

impl Handler {
    /// The call target this handler's input makes: `Unscoped`, a project
    /// read over the empty session when nothing is served (`View`), or one
    /// refused then (`Project`).
    pub fn target(&self) -> TargetSpec {
        match *self {
            Handler::Unscoped { .. } => TargetSpec::Unscoped,
            Handler::View { target, .. } => TargetSpec::Project {
                target,
                without: WithoutProject::EmptySession,
            },
            Handler::Project { target, .. } => TargetSpec::Project {
                target,
                without: WithoutProject::Refused,
            },
        }
    }

    /// The arguments it reads, in field order.
    pub fn arguments(&self) -> Vec<Argument> {
        match *self {
            Handler::Unscoped { arguments, .. }
            | Handler::View { arguments, .. }
            | Handler::Project { arguments, .. } => arguments(),
        }
    }

    /// The outputSchema its reply type derives: `None` for a handler
    /// answering text.
    pub fn reply(&self) -> Option<fn() -> Value> {
        match *self {
            Handler::Unscoped { reply, .. }
            | Handler::View { reply, .. }
            | Handler::Project { reply, .. } => reply,
        }
    }

    /// Run it on the resolved call.
    pub(crate) fn run(&self, call: &Call<'_>, arguments: Value) -> ToolOutcome {
        match *self {
            Handler::Unscoped { run, .. }
            | Handler::View { run, .. }
            | Handler::Project { run, .. } => run(call, arguments),
        }
    }
}

/// How a mutation runs: what its handler is given, and so its target. Built
/// only by the table's `mutation!` and `create!`.
#[derive(Clone, Copy)]
pub enum MutationHandler {
    /// Writes `target`'s project, refused as no project before it runs when
    /// nothing is served and the call names none. Its handler is
    /// `fn(&ProjectRef<'_>, Args) -> Mutation<Reply>`.
    Project {
        target: ProjectTarget,
        arguments: fn() -> Vec<Argument>,
        reply: fn() -> Value,
        run: MutationRun,
    },
    /// Creates the project its required `path` names (init); the target
    /// refuses a missing path and one inside the served project. Its handler
    /// is `fn(&Path, &SharedRuntime, Args) -> Mutation<Reply>`: the
    /// directory, and the runtime its extensions' declarations are read in
    /// (the host's, ADR 0028 D7).
    New {
        arguments: fn() -> Vec<Argument>,
        reply: fn() -> Value,
        run: MutationRun,
    },
}

/// How a mutation runs once its target resolved: given the call, the tool's
/// name and outputSchema and its arguments, its reply as sent and the events
/// to record ([`crate::mutation::complete`]). The events spell out `(String,
/// Value)`, not the crate-private `Event` alias, so the public
/// `MutationHandler` names no crate-private type.
pub type MutationRun =
    fn(&mut Call<'_>, &'static str, &Value, Value) -> (ToolOutcome, Vec<(String, Value)>);

impl MutationHandler {
    /// The call target this handler's input makes.
    pub fn target(&self) -> TargetSpec {
        match *self {
            MutationHandler::Project { target, .. } => TargetSpec::Project {
                target,
                without: WithoutProject::Refused,
            },
            MutationHandler::New { .. } => TargetSpec::NewProject,
        }
    }

    /// The arguments it reads, in field order.
    pub fn arguments(&self) -> Vec<Argument> {
        match *self {
            MutationHandler::Project { arguments, .. } | MutationHandler::New { arguments, .. } => {
                arguments()
            }
        }
    }

    /// The outputSchema its reply type derives, `files_written` included
    /// ([`crate::mutation::output_schema`]).
    pub fn reply(&self) -> fn() -> Value {
        match *self {
            MutationHandler::Project { reply, .. } | MutationHandler::New { reply, .. } => reply,
        }
    }

    /// Run it on the resolved call.
    pub(crate) fn run(
        &self,
        call: &mut Call<'_>,
        tool: &'static str,
        schema: &Value,
        arguments: Value,
    ) -> (ToolOutcome, Vec<(String, Value)>) {
        match *self {
            MutationHandler::Project { run, .. } | MutationHandler::New { run, .. } => {
                run(call, tool, schema, arguments)
            }
        }
    }
}

/// One core tool: everything the server lists, dispatches and reports about
/// it, from three fields.
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    /// What it does, and the handler that does it.
    pub effect: Effect,
}

impl ToolSpec {
    /// `Mutation` for a mutation, else its effect's group.
    pub fn category(&self) -> Category {
        match self.effect {
            Effect::Reads { group, .. } | Effect::WritesOutput { group, .. } => group.into(),
            Effect::Mutates { .. } => Category::Mutation,
        }
    }

    /// `read_only_annotations()` for `Reads`, else its hints' annotations.
    pub fn annotations(&self) -> Value {
        match self.effect {
            Effect::Reads { .. } => read_only_annotations(),
            Effect::WritesOutput { hints, .. } | Effect::Mutates { hints, .. } => {
                hints.annotations()
            }
        }
    }

    /// Whether it writes its target's project files (`Effect::Mutates`).
    pub fn is_mutation(&self) -> bool {
        matches!(self.effect, Effect::Mutates { .. })
    }

    /// Its call target: its handler's ([`Handler::target`],
    /// [`MutationHandler::target`]).
    pub fn target(&self) -> TargetSpec {
        match &self.effect {
            Effect::Reads { handler, .. } | Effect::WritesOutput { handler, .. } => {
                handler.target()
            }
            Effect::Mutates { handler, .. } => handler.target(),
        }
    }

    /// The outputSchema `tools/list` lists: the one its handler's reply type
    /// derives (a mutation's with `files_written`); none for a tool whose
    /// reply is text.
    pub fn output_schema(&self) -> Option<Value> {
        match &self.effect {
            Effect::Reads { handler, .. } | Effect::WritesOutput { handler, .. } => {
                handler.reply().map(|reply| reply())
            }
            Effect::Mutates { handler, .. } => Some(handler.reply()()),
        }
    }

    /// The input schema `tools/list` lists: the handler's arguments, then
    /// the target's, and no other property ([`crate::args::input_schema`]).
    pub fn input_schema(&self) -> Value {
        crate::args::input_schema(&self.arguments(), self.target())
    }

    /// The handler's declared arguments, in field order.
    pub fn arguments(&self) -> Vec<Argument> {
        match &self.effect {
            Effect::Reads { handler, .. } | Effect::WritesOutput { handler, .. } => {
                handler.arguments()
            }
            Effect::Mutates { handler, .. } => handler.arguments(),
        }
    }

    /// Every argument the call reads: the handler's, then the target's
    /// ([`TargetSpec::fields`]).
    pub fn reads(&self) -> Vec<&'static str> {
        self.arguments()
            .iter()
            .map(|argument| argument.name)
            .chain(self.target().fields().iter().copied())
            .collect()
    }

    /// The refusal of a call that sends a name neither the tool nor its
    /// target declares ([`crate::args::undeclared`]).
    pub fn undeclared(&self, arguments: &Value) -> Option<McpError> {
        crate::args::undeclared(arguments, &self.arguments(), self.target())
    }

    /// The tool as `tools/list` describes it.
    pub fn descriptor(&self) -> McpToolDescriptor {
        McpToolDescriptor {
            name: self.name.into(),
            description: self.description.into(),
            input_schema: self.input_schema(),
            output_schema: self.output_schema(),
            category: Some(self.category().as_str().into()),
            source: Some(CORE_SOURCE.into()),
            annotations: self.annotations(),
        }
    }
}

/// The spec's `McpErrorCode`: the small closed set agents branch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    InvalidInput,
    CompilationFailed,
    EntityNotFound,
    FileNotFound,
    ExtensionNotFound,
    PermissionDenied,
    Timeout,
    NotInitialized,
    SchemaMismatch,
    InternalError,
    Conflict,
    PreconditionFailed,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::InvalidInput => "invalid_input",
            ErrorCode::CompilationFailed => "compilation_failed",
            ErrorCode::EntityNotFound => "entity_not_found",
            ErrorCode::FileNotFound => "file_not_found",
            ErrorCode::ExtensionNotFound => "extension_not_found",
            ErrorCode::PermissionDenied => "permission_denied",
            ErrorCode::Timeout => "timeout",
            ErrorCode::NotInitialized => "not_initialized",
            ErrorCode::SchemaMismatch => "schema_mismatch",
            ErrorCode::InternalError => "internal_error",
            ErrorCode::Conflict => "conflict",
            ErrorCode::PreconditionFailed => "precondition_failed",
        }
    }

    /// The JSON-RPC error code a refusal is sent with where there is no
    /// `isError` result (`prompts/get`): invalid params for input the
    /// client can fix, invalid request before `initialize`, internal error
    /// for a failure on the server's side.
    pub fn rpc_code(self) -> i64 {
        match self {
            ErrorCode::InvalidInput
            | ErrorCode::EntityNotFound
            | ErrorCode::ExtensionNotFound
            | ErrorCode::Conflict => error_codes::INVALID_PARAMS,
            ErrorCode::NotInitialized => error_codes::INVALID_REQUEST,
            ErrorCode::CompilationFailed
            | ErrorCode::FileNotFound
            | ErrorCode::PermissionDenied
            | ErrorCode::Timeout
            | ErrorCode::SchemaMismatch
            | ErrorCode::InternalError
            | ErrorCode::PreconditionFailed => error_codes::INTERNAL_ERROR,
        }
    }

    /// The code a failure reported with diagnostic `code` carries: the
    /// kind operations give it ([`OpErrorKind::of_diagnostic`]).
    pub fn for_diagnostic(code: &str) -> Self {
        OpErrorKind::of_diagnostic(code).into()
    }
}

/// The code an operation's failure kind is reported as: total, one arm per
/// kind (ADR 0024 D7).
impl From<OpErrorKind> for ErrorCode {
    fn from(kind: OpErrorKind) -> Self {
        match kind {
            OpErrorKind::InvalidInput => ErrorCode::InvalidInput,
            OpErrorKind::EntityNotFound => ErrorCode::EntityNotFound,
            OpErrorKind::FileNotFound => ErrorCode::FileNotFound,
            OpErrorKind::ExtensionNotFound => ErrorCode::ExtensionNotFound,
            OpErrorKind::Conflict => ErrorCode::Conflict,
            OpErrorKind::SchemaMismatch => ErrorCode::SchemaMismatch,
            OpErrorKind::CompilationFailed => ErrorCode::CompilationFailed,
            OpErrorKind::PreconditionFailed => ErrorCode::PreconditionFailed,
            OpErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
            OpErrorKind::Timeout => ErrorCode::Timeout,
            OpErrorKind::Internal => ErrorCode::InternalError,
        }
    }
}

/// A failed tool call: the spec's `McpError`, the content of its `isError`
/// result. The diagnostic code, when there is one, is `diagnostic.code`,
/// never only message text.
#[derive(Debug, Clone)]
pub struct McpError {
    pub code: ErrorCode,
    pub message: String,
    pub tool: Option<String>,
    /// The prompt that refused: a `prompts/get` answered with an error.
    pub prompt: Option<String>,
    /// The URI of the resource whose read failed: a `resources/read`
    /// answered with an error (the URI read).
    pub uri: Option<String>,
    pub entity_id: Option<String>,
    /// The project file the failure is about (`file_not_found`): what a
    /// refusal that finds no project to look in names, rather than reading
    /// it back out of the message.
    pub file: Option<String>,
    pub argument: Option<String>,
    pub diagnostic: Option<Value>,
    pub data: Option<Value>,
    /// Other diagnostics the call reported on its way to failing; they
    /// ride in `_meta.diagnostics`, as a successful call's do.
    pub reported: Vec<Diagnostic>,
}

impl McpError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            tool: None,
            prompt: None,
            uri: None,
            entity_id: None,
            file: None,
            argument: None,
            diagnostic: None,
            data: None,
            reported: Vec::new(),
        }
    }

    /// A failure reported as `diagnostic`, its code mapped by
    /// [`ErrorCode::for_diagnostic`].
    pub fn from_diagnostic(diagnostic: &Diagnostic) -> Self {
        Self::new(
            ErrorCode::for_diagnostic(&diagnostic.code),
            diagnostic.message.clone(),
        )
        .with_diagnostic(diagnostic)
    }

    /// Invalid input: an argument the tool cannot use.
    pub fn invalid_input(argument: &str, message: impl Into<String>) -> Self {
        McpError::new(ErrorCode::InvalidInput, message).with_argument(argument)
    }

    pub fn with_entity(mut self, entity_id: impl Into<String>) -> Self {
        self.entity_id = Some(entity_id.into());
        self
    }

    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    pub fn with_argument(mut self, argument: impl Into<String>) -> Self {
        self.argument = Some(argument.into());
        self
    }

    pub fn with_diagnostic(mut self, diagnostic: &Diagnostic) -> Self {
        self.diagnostic = serde_json::to_value(specforge_common::diagnostics_json(
            std::slice::from_ref(diagnostic),
        ))
        .ok()
        .and_then(|mut all| all.get_mut(0).map(Value::take));
        self
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    /// The same error with `value` under `key` in its `data` object
    /// (created when it has none; a `data` that is no object is left as
    /// it is).
    pub fn with_data_field(mut self, key: &str, value: Value) -> Self {
        self.set_data_field(key, value);
        self
    }

    pub(crate) fn set_data_field(&mut self, key: &str, value: Value) {
        if let Value::Object(data) = self.data.get_or_insert_with(|| json!({})) {
            data.insert(key.to_string(), value);
        }
    }

    /// This refusal as the JSON-RPC error of a request that has no `isError`
    /// result (`prompts/get`, `resources/read`): -32602 when the client can
    /// fix it (it names an argument the client sent, or
    /// [`ErrorCode::rpc_code`] marks its code as input), else -32603; its
    /// data is this McpError (ADR 0004 D4-d, ADR 0024 D5).
    pub fn into_rpc_error(self) -> JsonRpcError {
        let code = if self.argument.is_some() {
            error_codes::INVALID_PARAMS
        } else {
            self.code.rpc_code()
        };
        JsonRpcError::new(code, self.message.clone()).with_data(self.to_json())
    }

    /// The error as its `isError` result carries it.
    pub fn to_json(&self) -> Value {
        let mut error = json!({ "code": self.code.as_str(), "message": self.message });
        for (key, value) in [
            ("tool", self.tool.clone().map(Value::from)),
            ("prompt", self.prompt.clone().map(Value::from)),
            ("uri", self.uri.clone().map(Value::from)),
            ("entity_id", self.entity_id.clone().map(Value::from)),
            ("file", self.file.clone().map(Value::from)),
            ("argument", self.argument.clone().map(Value::from)),
            ("diagnostic", self.diagnostic.clone()),
            ("data", self.data.clone()),
        ] {
            if let Some(value) = value {
                error[key] = value;
            }
        }
        error
    }
}

/// An operation's failure as an `McpError`: its kind picks the code. A
/// diagnostic code (`E027`) rides in `diagnostic`, with its suggestion; a
/// slug's suggestion and the operation's own data ride in `data`; the
/// entity the failure is about is `entity_id`.
impl From<OpError> for McpError {
    fn from(error: OpError) -> Self {
        let mut mcp_error = McpError::new(error.kind.into(), error.message.clone());
        let mut data = error.data.map_or_else(|| json!({}), |data| *data);
        if is_diagnostic_code(&error.code) {
            let mut diagnostic =
                Diagnostic::untyped(error.code.as_ref(), Severity::Error, error.message);
            if let Some(suggestion) = error.suggestion {
                diagnostic = diagnostic.with_suggestion(suggestion);
            }
            mcp_error = mcp_error.with_diagnostic(&diagnostic);
        } else if let Some(suggestion) = error.suggestion {
            data["suggestion"] = Value::from(suggestion);
        }
        if data.as_object().is_some_and(|d| !d.is_empty()) {
            mcp_error = mcp_error.with_data(data);
        }
        match error.entity {
            Some(entity) => mcp_error.with_entity(entity),
            None => mcp_error,
        }
    }
}

/// A question about `entity_id`, which no entity of `graph` declares:
/// `specforge_ops::navigate::not_found` (E003 in `diagnostic`, a
/// did-you-mean when an id is close), the one refusal tools and prompts
/// share with every operation.
pub fn entity_not_found(graph: &Graph, entity_id: &str) -> McpError {
    specforge_ops::navigate::not_found(graph, entity_id).into()
}

/// What a refusal of a file the project does not hold says before the file's
/// name ([`file_not_found`]).
pub(crate) const FILE_NOT_FOUND: &str = "File not found: ";

/// A question about `file`, which the project has no entity from and does
/// not hold under its spec root: `file_not_found` on argument `file`.
pub(crate) fn file_not_found(file: &str) -> McpError {
    McpError::new(ErrorCode::FileNotFound, format!("{FILE_NOT_FOUND}{file}"))
        .with_argument("file")
        .with_file(file)
}

/// Whether `code` is a diagnostic code (`E003`, `R004`, `R-RES-006`):
/// capitals, digits and dashes, not a slug such as `extension_not_found`.
pub fn is_diagnostic_code(code: &str) -> bool {
    code.starts_with(|c: char| c.is_ascii_uppercase())
        && code.contains(|c: char| c.is_ascii_digit())
        && code
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
}

/// A tool's result body.
#[derive(Debug, Clone)]
pub enum Payload {
    /// A structured result, sent as its JSON text.
    Json(Value),
    /// Plain text blocks, sent as they are: a message, a rendered document,
    /// a command's stdout and stderr.
    Text(Vec<String>),
}

/// What a tool call produced.
#[derive(Debug, Clone)]
pub enum ToolOutcome {
    /// The tool ran. `is_error` marks a failed run (an `isError` result);
    /// `diagnostics` ride in `_meta.diagnostics` and `meta`'s entries beside
    /// them in `_meta`.
    Done {
        payload: Payload,
        is_error: bool,
        diagnostics: Vec<Diagnostic>,
        meta: serde_json::Map<String, Value>,
    },
    /// The tool failed: an `isError` result carrying the `McpError` (ADR
    /// 0004 D4-a). The one way a tool reports a failure.
    Refused(Box<McpError>),
}

impl ToolOutcome {
    fn done(payload: Payload, is_error: bool) -> Self {
        ToolOutcome::Done {
            payload,
            is_error,
            diagnostics: Vec::new(),
            meta: serde_json::Map::new(),
        }
    }

    /// A successful structured result.
    pub fn ok(payload: Value) -> Self {
        Self::done(Payload::Json(payload), false)
    }

    /// A successful plain-text result.
    pub fn text(text: impl Into<String>) -> Self {
        Self::done(Payload::Text(vec![text.into()]), false)
    }

    /// A failed run whose output is a JSON object: a command's error
    /// object (ADR 0011).
    pub fn failed(payload: Value) -> Self {
        Self::done(Payload::Json(payload), true)
    }

    /// Plain-text blocks, failed or not: a command's output.
    pub fn texts(blocks: Vec<String>, is_error: bool) -> Self {
        Self::done(Payload::Text(blocks), is_error)
    }

    /// A failure with `code` and `message`.
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        McpError::new(code, message).into()
    }

    /// Invalid input: an argument the tool cannot use.
    pub fn invalid_input(argument: &str, message: impl Into<String>) -> Self {
        McpError::invalid_input(argument, message).into()
    }

    /// A tool that needs a project and has none to work on.
    pub fn no_project(message: impl Into<String>) -> Self {
        Self::error(ErrorCode::PreconditionFailed, message)
    }

    /// The same outcome with `extra` added to its `_meta.diagnostics`.
    pub fn with_diagnostics(mut self, extra: Vec<Diagnostic>) -> Self {
        match &mut self {
            ToolOutcome::Done { diagnostics, .. } => diagnostics.extend(extra),
            ToolOutcome::Refused(error) => error.reported.extend(extra),
        }
        self
    }

    /// The same outcome with `value` under `key` in its `_meta` (a
    /// `specforge/`-prefixed key: MCP reserves `mcp` and
    /// `modelcontextprotocol`). A failure has no `_meta` but its
    /// diagnostics, so it is left as it is.
    pub fn with_meta(mut self, key: impl Into<String>, value: Value) -> Self {
        if let ToolOutcome::Done { meta, .. } = &mut self {
            meta.insert(key.into(), value);
        }
        self
    }

    /// The same outcome, a failure naming `tool` unless it names one.
    pub fn from_tool(mut self, tool: &str) -> Self {
        if let ToolOutcome::Refused(error) = &mut self
            && error.tool.is_none()
        {
            error.tool = Some(tool.to_string());
        }
        self
    }

    /// Whether the tool ran without failing.
    pub fn succeeded(&self) -> bool {
        matches!(
            self,
            ToolOutcome::Done {
                is_error: false,
                ..
            }
        )
    }
}

impl From<McpError> for ToolOutcome {
    fn from(error: McpError) -> Self {
        ToolOutcome::Refused(Box::new(error))
    }
}

/// The `tools/call` reply for `outcome`: the only place that builds
/// `content`, `structuredContent`, `isError` and `_meta`. A failure is an
/// `isError` result whose text is its `McpError`. With `structured` (a
/// 2025-06-18 or later session), a JSON object payload is also sent as
/// `structuredContent`, beside the text block holding its JSON; except a
/// failure of a tool with an outputSchema (`typed`), which the schema does
/// not describe.
pub fn envelope(
    outcome: ToolOutcome,
    id: Option<Value>,
    structured: bool,
    typed: bool,
) -> JsonRpcResponse {
    let (payload, is_error, diagnostics, mut meta) = match outcome {
        ToolOutcome::Refused(error) => {
            let json = error.to_json();
            (
                Payload::Json(json),
                true,
                error.reported,
                serde_json::Map::new(),
            )
        }
        ToolOutcome::Done {
            payload,
            is_error,
            diagnostics,
            meta,
        } => (payload, is_error, diagnostics, meta),
    };
    let content: Vec<Value> = match &payload {
        Payload::Json(value) => vec![json!({ "type": "text", "text": value.to_string() })],
        Payload::Text(blocks) => blocks
            .iter()
            .map(|text| json!({ "type": "text", "text": text }))
            .collect(),
    };
    let mut result = json!({ "content": content, "isError": is_error });
    if let Payload::Json(object @ Value::Object(_)) = payload
        && structured
        && !(is_error && typed)
    {
        result["structuredContent"] = object;
    }
    if !diagnostics.is_empty() {
        meta.insert(
            "diagnostics".to_string(),
            serde_json::to_value(specforge_common::diagnostics_json(&diagnostics))
                .unwrap_or_default(),
        );
    }
    if !meta.is_empty() {
        result["_meta"] = Value::Object(meta);
    }
    JsonRpcResponse::success(id, result)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [ErrorCode; 12] = [
        ErrorCode::InvalidInput,
        ErrorCode::CompilationFailed,
        ErrorCode::EntityNotFound,
        ErrorCode::FileNotFound,
        ErrorCode::ExtensionNotFound,
        ErrorCode::PermissionDenied,
        ErrorCode::Timeout,
        ErrorCode::NotInitialized,
        ErrorCode::SchemaMismatch,
        ErrorCode::InternalError,
        ErrorCode::Conflict,
        ErrorCode::PreconditionFailed,
    ];

    #[test]
    fn every_operation_kind_has_an_error_code() {
        for (kind, code) in [
            (OpErrorKind::InvalidInput, ErrorCode::InvalidInput),
            (OpErrorKind::EntityNotFound, ErrorCode::EntityNotFound),
            (OpErrorKind::FileNotFound, ErrorCode::FileNotFound),
            (OpErrorKind::ExtensionNotFound, ErrorCode::ExtensionNotFound),
            (OpErrorKind::Conflict, ErrorCode::Conflict),
            (OpErrorKind::SchemaMismatch, ErrorCode::SchemaMismatch),
            (
                OpErrorKind::PreconditionFailed,
                ErrorCode::PreconditionFailed,
            ),
            (OpErrorKind::PermissionDenied, ErrorCode::PermissionDenied),
            (OpErrorKind::Timeout, ErrorCode::Timeout),
            (OpErrorKind::Internal, ErrorCode::InternalError),
        ] {
            assert_eq!(ErrorCode::from(kind), code, "{kind:?}");
            // One vocabulary: the name an operation reports a kind under is
            // the name MCP sends for it.
            assert_eq!(code.as_str(), kind.as_str(), "{kind:?}");
        }
    }

    #[test]
    fn an_operation_failure_is_an_mcp_error_by_its_kind() {
        let error: McpError = OpError::new(OpErrorKind::Conflict, "entity_exists", "taken")
            .with_entity("alpha")
            .with_suggestion("pick another")
            .into();
        let json = error.to_json();
        assert_eq!(json["code"], "conflict");
        assert_eq!(json["entity_id"], "alpha");
        assert_eq!(json["data"]["suggestion"], "pick another");
        assert!(json.get("diagnostic").is_none(), "{json}");

        let error: McpError =
            OpError::diagnostic(specforge_common::codes::E062, "the budget is too small")
                .with_suggestion("raise it")
                .into();
        let json = error.to_json();
        assert_eq!(json["code"], "invalid_input");
        assert_eq!(json["diagnostic"]["code"], "E062");
        assert_eq!(json["diagnostic"]["suggestion"], "raise it");
        assert_eq!(json["message"], "the budget is too small");
    }

    #[test]
    fn every_error_code_has_a_json_rpc_code() {
        for code in ALL {
            let rpc = code.rpc_code();
            let expected = match code.as_str() {
                "invalid_input" | "entity_not_found" | "extension_not_found" | "conflict" => {
                    error_codes::INVALID_PARAMS
                }
                "not_initialized" => error_codes::INVALID_REQUEST,
                _ => error_codes::INTERNAL_ERROR,
            };
            assert_eq!(rpc, expected, "{}", code.as_str());
        }
    }

    #[test]
    fn a_prompts_refusal_names_the_prompt() {
        let mut error = McpError::new(ErrorCode::InvalidInput, "no");
        assert!(error.to_json().get("prompt").is_none());
        error.prompt = Some("specforge://prompts/context".into());
        assert_eq!(error.to_json()["prompt"], "specforge://prompts/context");
        assert!(error.to_json().get("tool").is_none());
    }

    #[test]
    fn a_resources_refusal_names_the_uri() {
        let mut error = McpError::new(ErrorCode::InvalidInput, "no");
        assert!(error.to_json().get("uri").is_none());
        error.uri = Some("specforge://graph?depth=two".into());
        assert_eq!(error.to_json()["uri"], "specforge://graph?depth=two");
        assert!(error.to_json().get("resource").is_none());
        assert!(error.to_json().get("prompt").is_none());
    }

    #[test]
    fn a_refusal_without_is_error_is_invalid_params_when_the_client_can_fix_it() {
        let rpc = |error: McpError| error.into_rpc_error();
        // An argument it named, whatever its code.
        let named = McpError::new(ErrorCode::FileNotFound, "path not found").with_argument("path");
        let named = rpc(named);
        assert_eq!(named.code, error_codes::INVALID_PARAMS);
        assert_eq!(named.data.as_ref().unwrap()["argument"], "path");
        // A code that is input.
        assert_eq!(
            rpc(McpError::new(ErrorCode::EntityNotFound, "no")).code,
            error_codes::INVALID_PARAMS
        );
        // A failure on the server's side, and a missing project (no params
        // fix it).
        assert_eq!(
            rpc(McpError::new(ErrorCode::InternalError, "no")).code,
            error_codes::INTERNAL_ERROR
        );
        assert_eq!(
            rpc(McpError::new(ErrorCode::PreconditionFailed, "no")).code,
            error_codes::INTERNAL_ERROR
        );
    }

    #[test]
    fn an_unknown_entity_carries_its_e003() {
        let json = entity_not_found(&Graph::new(), "ghost").to_json();
        assert_eq!(json["code"], "entity_not_found");
        assert_eq!(json["entity_id"], "ghost");
        assert_eq!(json["diagnostic"]["code"], "E003");
        assert_eq!(
            json["message"],
            "unresolved entity 'ghost' — not found in graph"
        );
    }
}
