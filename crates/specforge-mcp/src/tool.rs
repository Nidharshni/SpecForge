//! What a tool call produced, and the one place that turns it into a
//! `tools/call` reply.
//!
//! Handlers return a [`ToolOutcome`] (a mutation's handler, a
//! [`Mutation`](crate::mutation::Mutation) that [`crate::mutation::complete`]
//! turns into one); [`envelope`] alone builds `content`, `isError` and
//! `_meta`. What a mutation wrote crosses to the dispatcher typed (ADR 0022),
//! never read back from the reply.

use std::marker::PhantomData;

use serde::Serialize;
use serde_json::{Value, json};
use specforge_common::shape::Shape;
use specforge_common::{Diagnostic, DiagnosticDocument, Severity};
use specforge_graph::Graph;

use crate::args::{Argument, Arguments};
use crate::events::EventSink;
use crate::lifecycle::Revision;
use crate::mutation::{Mutation, MutationArgs, MutationReply};
use crate::protocol::{JsonRpcError, JsonRpcResponse, error_codes};
use crate::registry::{ToolAnnotations, ToolDescriptor};
use crate::reply::{Answered, Object, Source, Text};
use crate::target::{Call, Input, TargetSpec};
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "lowercase")]
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
    pub fn annotations(self) -> ToolAnnotations {
        ToolAnnotations {
            read_only_hint: false,
            destructive_hint: Some(self.destructive),
            idempotent_hint: Some(self.idempotent),
            open_world_hint: self.open_world,
        }
    }
}

/// The annotations of a tool that only reads and reaches nothing beyond the
/// project, `{readOnlyHint: true, openWorldHint: false}`: every core read,
/// and every extension tool (the host grants an extension no capability,
/// ADR 0037 D1).
pub fn read_only_annotations() -> ToolAnnotations {
    ToolAnnotations {
        read_only_hint: true,
        destructive_hint: None,
        idempotent_hint: None,
        open_world_hint: false,
    }
}

/// What a tool does to its environment: the one declaration its category,
/// annotations and a mutation's `files_written` derive from (ADR 0024 A1,
/// ADR 0049). A core tool states it as [`CoreTool::EFFECT`]; that it agrees
/// with what the tool's handler answers is checked when the table is built
/// ([`spec`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// It only reads: `readOnlyHint`, listed in `group`.
    Reads { group: ToolGroup },
    /// It writes output artifacts, not its target's project files (collect
    /// writes the recorded test report, render an export): listed in
    /// `group`, annotated with `hints`; its reply is all there is (ADR 0022).
    WritesOutput { group: ToolGroup, hints: WriteHints },
    /// It writes its target's project files: category `mutation`, annotated
    /// with `hints`; its handler answers a `Mutation`, whose files the reply
    /// lists as `files_written` (ADR 0022).
    Mutates { hints: WriteHints },
}

impl Effect {
    /// Whether it writes its target's project files.
    pub const fn is_mutation(self) -> bool {
        matches!(self, Effect::Mutates { .. })
    }

    /// `Mutation` for a mutation, else its group.
    pub fn category(self) -> Category {
        match self {
            Effect::Reads { group } | Effect::WritesOutput { group, .. } => group.into(),
            Effect::Mutates { .. } => Category::Mutation,
        }
    }

    /// `read_only_annotations()` for `Reads`, else its hints' annotations.
    pub fn annotations(self) -> ToolAnnotations {
        match self {
            Effect::Reads { .. } => read_only_annotations(),
            Effect::WritesOutput { hints, .. } | Effect::Mutates { hints } => hints.annotations(),
        }
    }
}

/// Where `spec/types` states a core tool's reply (ADR 0048 D8): what the
/// reflection test compares the derived output schema with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplySpec {
    /// The `Mcp*` type of `spec/types/mcp.spec` the reply is.
    Type(&'static str),
    /// A document another spec states (the agent export, ADR 0007; the
    /// GraphProtocolSchema). The text says which.
    Document(&'static str),
    /// A document in text: no outputSchema, no spec type.
    Text,
}

impl ReplySpec {
    /// Whether the reply is text.
    pub const fn is_text(self) -> bool {
        matches!(self, ReplySpec::Text)
    }
}

/// One core MCP tool, stated once in its own module under `tools/`
/// (CONTEXT.md "Tool spec"). Implemented on the module's unit struct
/// `Tool`; listed by [`crate::tools::CORE_TOOLS`] as [`spec`]`(&x::Tool)`.
/// Everything the server lists, dispatches, checks and reports about the
/// tool derives from it ([`ToolSpec`]).
pub trait CoreTool: Sync + 'static {
    /// Its name as `tools/list` lists it and every prompt names it.
    const NAME: &'static str;
    /// Its `tools/list` description.
    const DESCRIPTION: &'static str;
    /// What it does to its environment.
    const EFFECT: Effect;
    /// Where the spec states its reply; a text reply states none.
    const SPEC: ReplySpec = ReplySpec::Text;
    /// The paths of its outputSchema (`$.nodes[].fields`) whose content an
    /// extension defines: open values no core type can close (ADR 0048 D1).
    /// The reflection test holds the schema to exactly these.
    const OPEN_VALUES: &'static [&'static str] = &[];
    /// The typed arguments it reads (ADR 0033), as its reply kind reads
    /// them before the handler runs: a mutation's arguments say whether the
    /// call only previews (`MutationArgs`); a read's say nothing.
    type Args: Arguments + Previews<Self::Returns>;
    /// What its handler is given before its arguments, and so its call
    /// target (ADR 0024 A2): `Unscoped`, `View`, `Project` or `NewProject`
    /// ([`crate::target`]).
    type Input: Input;
    /// What its handler returns, and so its reply kind: `Answered<Reply>`,
    /// `Answered<Text>` or `Mutation<Reply>` ([`Handled`]).
    type Returns: Handled;
    /// The handler: its input, then its typed arguments.
    fn call(input: <Self::Input as Input>::Given<'_>, args: Self::Args) -> Self::Returns;
}

/// A tool's arguments as its reply kind reads them before the handler takes
/// them: whether the call only previews (ADR 0022). Two blanket
/// implementations, never written by hand: a read never previews, and a
/// mutation's arguments say ([`MutationArgs::previews`]). A mutation whose
/// `Args` are no `MutationArgs`, or whose `Reply` is no `MutationReply`,
/// does not build.
pub trait Previews<H> {
    fn previews(&self) -> bool;
}

impl<A: Arguments, R> Previews<Answered<R>> for A {
    fn previews(&self) -> bool {
        false
    }
}

impl<A: MutationArgs, R: MutationReply> Previews<Mutation<R>> for A {
    fn previews(&self) -> bool {
        MutationArgs::previews(self)
    }
}

/// What a core tool's handler returns: its reply kind. Three
/// implementations: `Answered<Text>`, `Answered<R>` (`R: Object +
/// Serialize`, ADR 0048) and `Mutation<R>` (`R: MutationReply`, ADR 0022).
pub trait Handled: Sized {
    /// Whether the handler is a mutation's: it wrote its target's project
    /// files, which the pipeline refreshes and reports (ADR 0022). The tool's
    /// effect must be `Mutates` exactly then.
    const MUTATES: bool;
    /// Whether the tool lists an outputSchema; read without building it.
    const TYPED: bool;
    /// The outputSchema `tools/list` lists and every structured reply is
    /// checked against: `None` for text. The one place a core tool's output
    /// schema is built.
    fn output_schema() -> Option<&'static Value>;
    /// A call refused before its handler ran (an undeclared name, arguments
    /// that do not read, a call target that refused): the refusal as it is
    /// for a read; for a mutation a failed mutation that wrote nothing
    /// ([`crate::mutation::refused`], recorded into `events`).
    fn refused(error: Box<McpError>, tool: &'static str, events: &mut dyn EventSink)
    -> ToolOutcome;
    /// The rest of a call whose handler ran: a read's reply checked against
    /// its schema; a mutation completed ([`crate::mutation::complete`]: the
    /// target brought up to date, the reply sent once and checked, its events
    /// recorded through the call), `preview` read from its arguments
    /// beforehand.
    fn finish(self, call: &mut Call<'_>, tool: &'static str, preview: bool) -> ToolOutcome;
}

impl Handled for Answered<Text> {
    const MUTATES: bool = false;
    const TYPED: bool = false;

    fn output_schema() -> Option<&'static Value> {
        None
    }

    fn refused(error: Box<McpError>, _: &'static str, _: &mut dyn EventSink) -> ToolOutcome {
        ToolOutcome::Refused(error)
    }

    fn finish(self, _: &mut Call<'_>, _: &'static str, _: bool) -> ToolOutcome {
        crate::reply::text(self)
    }
}

impl<R: Object + Serialize + 'static> Handled for Answered<R> {
    const MUTATES: bool = false;
    const TYPED: bool = true;

    fn output_schema() -> Option<&'static Value> {
        Some(crate::reply::output_schema::<R>())
    }

    fn refused(error: Box<McpError>, _: &'static str, _: &mut dyn EventSink) -> ToolOutcome {
        ToolOutcome::Refused(error)
    }

    fn finish(self, _: &mut Call<'_>, tool: &'static str, _: bool) -> ToolOutcome {
        let schema = Self::output_schema().expect("a typed reply has an outputSchema");
        crate::reply::conforming(tool, Source::Core, schema, crate::reply::structured(self))
    }
}

impl<R: MutationReply> Handled for Mutation<R> {
    const MUTATES: bool = true;
    const TYPED: bool = true;

    /// `WrittenReply<R, R::After>`'s schema ([`crate::mutation::output_schema`]).
    fn output_schema() -> Option<&'static Value> {
        Some(crate::mutation::output_schema::<R>())
    }

    fn refused(
        error: Box<McpError>,
        tool: &'static str,
        events: &mut dyn EventSink,
    ) -> ToolOutcome {
        crate::mutation::refused(tool, error, events)
    }

    fn finish(self, call: &mut Call<'_>, tool: &'static str, preview: bool) -> ToolOutcome {
        // `complete` owns the order: refresh, send once, check against the
        // schema, record the events.
        let schema = Self::output_schema().expect("a mutation has an outputSchema");
        crate::mutation::complete::<R>(call, tool, schema, preview, self)
    }
}

/// A core tool as the table holds it: everything the server lists,
/// dispatches, checks and reports about it. Implemented once, for every
/// [`CoreTool`]; listings, the tools adapter, events and the reflection
/// tests read it.
pub trait ToolSpec: Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn effect(&self) -> Effect;
    /// Where the spec states its reply.
    fn spec_reply(&self) -> ReplySpec;
    /// The open values of its outputSchema ([`CoreTool::OPEN_VALUES`]).
    fn open_values(&self) -> &'static [&'static str];
    /// Its declared arguments, in field order.
    fn arguments(&self) -> Vec<Argument>;
    /// Its call target: its input's ([`Input::TARGET`]).
    fn target(&self) -> TargetSpec;
    /// Its outputSchema ([`Handled::output_schema`]); `None` for text.
    fn output_schema(&self) -> Option<&'static Value>;
    /// Whether it lists an outputSchema, without building it.
    fn is_typed(&self) -> bool;
    /// Run it on its resolved call: a name neither it nor its target
    /// declares is refused first, then its arguments are read, its input
    /// given and its reply finished. The request pipeline's; a test cannot
    /// build a `Call`.
    fn run(&self, call: &mut Call<'_>, arguments: &Value) -> ToolOutcome;
    /// The outcome of a call refused before it ran; a refused mutation
    /// records into `events`.
    fn refused(&self, error: McpError, events: &mut dyn EventSink) -> ToolOutcome;

    /// `Mutation` for a mutation, else its effect's group.
    fn category(&self) -> Category {
        self.effect().category()
    }

    /// `readOnlyHint` for a read, else its hints'.
    fn annotations(&self) -> ToolAnnotations {
        self.effect().annotations()
    }

    /// Whether it writes its target's project files.
    fn is_mutation(&self) -> bool {
        self.effect().is_mutation()
    }

    /// The input schema `tools/list` lists: its arguments, then its target's,
    /// and no other property ([`crate::args::input_schema`]).
    fn input_schema(&self) -> Value {
        crate::args::input_schema(&self.arguments(), self.target())
    }

    /// Every argument the call reads: its own, then its target's
    /// ([`TargetSpec::fields`]).
    fn reads(&self) -> Vec<&'static str> {
        self.arguments()
            .iter()
            .map(|argument| argument.name)
            .chain(self.target().fields().iter().copied())
            .collect()
    }

    /// The refusal of a call that sends a name neither the tool nor its
    /// target declares ([`crate::args::undeclared`]).
    fn undeclared(&self, arguments: &Value) -> Option<McpError> {
        crate::args::undeclared(arguments, &self.arguments(), self.target())
    }

    /// The tool as `tools/list` describes it under `revision`: it lists its
    /// outputSchema from 2025-06-18, the revision that came with
    /// structured content.
    fn descriptor(&self, revision: Revision) -> ToolDescriptor<'static> {
        ToolDescriptor {
            name: self.name().into(),
            description: self.description().into(),
            input_schema: self.input_schema(),
            output_schema: self
                .output_schema()
                .filter(|_| revision.sends_structured_content()),
            category: self.category(),
            source: CORE_SOURCE.into(),
            annotations: self.annotations(),
        }
    }
}

/// `tool` as the table holds it, its declarations checked when the table is
/// built (at compile time, `cargo check` included): its effect is `Mutates`
/// exactly when its handler answers a `Mutation`, and it names a spec type
/// exactly when its reply is typed.
pub const fn spec<T: CoreTool>(tool: &'static T) -> &'static dyn ToolSpec {
    let () = Agrees::<T>::CHECKED;
    tool
}

struct Agrees<T>(PhantomData<T>);

impl<T: CoreTool> Agrees<T> {
    const CHECKED: () = {
        assert!(
            T::EFFECT.is_mutation() == <T::Returns as Handled>::MUTATES,
            "a core tool's effect is Mutates exactly when its handler answers a Mutation"
        );
        assert!(
            T::SPEC.is_text() != <T::Returns as Handled>::TYPED,
            "a core tool names a spec type exactly when it answers a typed reply"
        );
    };
}

impl<T: CoreTool> ToolSpec for T {
    fn name(&self) -> &'static str {
        T::NAME
    }

    fn description(&self) -> &'static str {
        T::DESCRIPTION
    }

    fn effect(&self) -> Effect {
        T::EFFECT
    }

    fn spec_reply(&self) -> ReplySpec {
        T::SPEC
    }

    fn open_values(&self) -> &'static [&'static str] {
        T::OPEN_VALUES
    }

    fn arguments(&self) -> Vec<Argument> {
        <T::Args as Arguments>::declared()
    }

    fn target(&self) -> TargetSpec {
        <T::Input as Input>::TARGET
    }

    fn output_schema(&self) -> Option<&'static Value> {
        <T::Returns as Handled>::output_schema()
    }

    fn is_typed(&self) -> bool {
        <T::Returns as Handled>::TYPED
    }

    fn run(&self, call: &mut Call<'_>, arguments: &Value) -> ToolOutcome {
        // A name the tool and its target do not declare is refused before
        // anything is read (a refused mutation is a failed one).
        if let Some(error) = self.undeclared(arguments) {
            return self.refused(error, call.events());
        }
        let args = match crate::args::read::<T::Args>(arguments) {
            Ok(args) => args,
            // Arguments that do not read cannot say they asked for a preview.
            Err(refused) => {
                return <T::Returns as Handled>::refused(refused, T::NAME, call.events());
            }
        };
        // Read before the handler takes the arguments (ADR 0022).
        let preview = <T::Args as Previews<T::Returns>>::previews(&args);
        // The input borrows the call; what the handler returns does not.
        let returned = <T::Input as Input>::given(call).map(|input| T::call(input, args));
        match returned {
            Ok(returned) => returned.finish(call, T::NAME, preview),
            // The call target refused a call with no project before this
            // runs; unreachable, answered rather than panicked.
            Err(refused) => <T::Returns as Handled>::refused(refused, T::NAME, call.events()),
        }
    }

    fn refused(&self, error: McpError, events: &mut dyn EventSink) -> ToolOutcome {
        <T::Returns as Handled>::refused(Box::new(error), T::NAME, events)
    }
}

/// The spec's `McpErrorCode`: the small closed set agents branch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "snake_case")]
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

/// A failed call or read: the spec's `McpError`. It is the text (and, for an
/// untyped tool, the structuredContent) of an `isError` tool result, and the
/// `data` of a prompt's or a resource read's JSON-RPC error; serialized from
/// this one definition everywhere. The diagnostic code, when there is one,
/// is `diagnostic.code`, never only message text.
#[derive(Debug, Clone, Serialize, Shape)]
pub struct McpError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// The prompt that refused: a `prompts/get` answered with an error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// The URI of the resource whose read failed: a `resources/read`
    /// answered with an error (the URI read).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    /// The project file the failure is about (`file_not_found`): what a
    /// refusal that finds no project to look in names, rather than reading
    /// it back out of the message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub argument: Option<String>,
    /// The diagnostic behind the failure, as every surface presents one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<DiagnosticDocument>,
    /// More about the failure: the open value `McpError` reserves.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    /// Other diagnostics the call reported on its way to failing; they
    /// ride in `_meta.diagnostics`, as a successful call's do.
    #[serde(skip)]
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
        self.diagnostic = Some(DiagnosticDocument(diagnostic.clone()));
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
        let data = serde_json::to_value(&self).expect("an McpError serializes");
        JsonRpcError::new(code, self.message).with_data(data)
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
            let json = serde_json::to_value(&*error).expect("an McpError serializes");
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

    #[specforge_test_macros::test(
        behavior = "state_mcp_messages_as_spec_types",
        verify = "an McpError is serialized from its type, the isError content and a JSON-RPC error's data alike"
    )]
    fn an_error_serializes_as_its_type() {
        let mut error = McpError::new(ErrorCode::InvalidInput, "no")
            .with_argument("scope")
            .with_diagnostic(&Diagnostic::untyped("E003", Severity::Error, "gone"));
        error.prompt = Some("specforge://prompts/context".into());
        // As a JSON value (its keys sorted, as every JSON-RPC document the
        // server builds from a value).
        let text = serde_json::to_value(&error).unwrap().to_string();

        // The isError result's text is the type, byte for byte.
        let reply = envelope(
            ToolOutcome::Refused(Box::new(error.clone())),
            None,
            false,
            false,
        );
        let content = reply.result.as_ref().unwrap()["content"][0]["text"].clone();
        assert_eq!(content.as_str(), Some(text.as_str()));

        // And so is a JSON-RPC error's data.
        let rpc = error.into_rpc_error();
        assert_eq!(rpc.data.unwrap().to_string(), text);
    }

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
        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(json["code"], "conflict");
        assert_eq!(json["entity_id"], "alpha");
        assert_eq!(json["data"]["suggestion"], "pick another");
        assert!(json.get("diagnostic").is_none(), "{json}");

        let error: McpError = OpError::coded(
            specforge_ops::OpErrorKind::InvalidInput,
            specforge_common::codes::E062,
            "the budget is too small",
        )
        .with_suggestion("raise it")
        .into();
        let json = serde_json::to_value(&error).unwrap();
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
        assert!(
            serde_json::to_value(&error)
                .unwrap()
                .get("prompt")
                .is_none()
        );
        error.prompt = Some("specforge://prompts/context".into());
        assert_eq!(
            serde_json::to_value(&error).unwrap()["prompt"],
            "specforge://prompts/context"
        );
        assert!(serde_json::to_value(&error).unwrap().get("tool").is_none());
    }

    #[test]
    fn a_resources_refusal_names_the_uri() {
        let mut error = McpError::new(ErrorCode::InvalidInput, "no");
        assert!(serde_json::to_value(&error).unwrap().get("uri").is_none());
        error.uri = Some("specforge://graph?depth=two".into());
        assert_eq!(
            serde_json::to_value(&error).unwrap()["uri"],
            "specforge://graph?depth=two"
        );
        assert!(
            serde_json::to_value(&error)
                .unwrap()
                .get("resource")
                .is_none()
        );
        assert!(
            serde_json::to_value(&error)
                .unwrap()
                .get("prompt")
                .is_none()
        );
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
        let json = serde_json::to_value(entity_not_found(&Graph::new(), "ghost")).unwrap();
        assert_eq!(json["code"], "entity_not_found");
        assert_eq!(json["entity_id"], "ghost");
        assert_eq!(json["diagnostic"]["code"], "E003");
        assert_eq!(
            json["message"],
            "unresolved entity 'ghost' — not found in graph"
        );
    }
}

#[cfg(test)]
mod spec_tests {
    use super::*;
    use crate::events::testing::EventLog;
    use crate::mutation::{Mutated, NoDiagnostics, Written};
    use crate::reply::Answer;
    use crate::state::McpState;
    use crate::target::{CallTarget, Unscoped};
    use specforge_common::shape::Shape;

    #[derive(Debug, Serialize, Shape)]
    struct Probe {
        name: String,
    }

    #[derive(Debug, Arguments)]
    struct ProbeArgs {
        /// A name
        name: String,
    }

    /// A read probe: unscoped, a typed reply; its handler panics when it
    /// runs on "boom", so a call that never reaches it is shown.
    struct ReadProbe;

    impl CoreTool for ReadProbe {
        const NAME: &'static str = "specforge.probe";
        const DESCRIPTION: &'static str = "A probe";
        const EFFECT: Effect = Effect::Reads {
            group: ToolGroup::Navigation,
        };
        const SPEC: ReplySpec = ReplySpec::Type("McpProbe");
        type Args = ProbeArgs;
        type Input = Unscoped;
        type Returns = Answered<Probe>;

        fn call(_: (), args: ProbeArgs) -> Answered<Probe> {
            assert_ne!(args.name, "boom", "the handler ran");
            Ok(Answer::new(Probe { name: args.name }))
        }
    }

    #[derive(Debug, Arguments)]
    struct MutationProbeArgs {
        /// Only preview
        dry_run: bool,
    }

    impl MutationArgs for MutationProbeArgs {
        fn previews(&self) -> bool {
            self.dry_run
        }
    }

    #[derive(Debug, Serialize, Shape)]
    struct MutationProbeReply {
        ok: bool,
    }

    impl MutationReply for MutationProbeReply {
        type After = NoDiagnostics;
    }

    struct MutationProbe;

    impl CoreTool for MutationProbe {
        const NAME: &'static str = "specforge.probe_mutation";
        const DESCRIPTION: &'static str = "A mutation probe";
        const EFFECT: Effect = Effect::Mutates {
            hints: WriteHints {
                destructive: false,
                idempotent: true,
                open_world: false,
            },
        };
        const SPEC: ReplySpec = ReplySpec::Type("McpProbeMutation");
        type Args = MutationProbeArgs;
        type Input = Unscoped;
        type Returns = Mutation<MutationProbeReply>;

        fn call(_: (), _: MutationProbeArgs) -> Mutation<MutationProbeReply> {
            Ok(Mutated::new(
                Answer::new(MutationProbeReply { ok: true }),
                Written::nothing(),
            ))
        }
    }

    #[specforge_test_macros::test(
        behavior = "list_mcp_tools",
        verify = "a core tool's spec is derived from its one module: its name, description, effect, arguments, target and reply"
    )]
    fn a_core_tool_spec_is_derived_from_its_module() {
        let tool = spec(&ReadProbe);
        assert_eq!(tool.name(), "specforge.probe");
        assert_eq!(tool.description(), "A probe");
        assert_eq!(tool.category(), Category::Navigation);
        assert_eq!(tool.annotations(), read_only_annotations());
        assert_eq!(tool.target(), TargetSpec::Unscoped);
        assert!(!tool.is_mutation());
        assert!(tool.is_typed());
        assert_eq!(tool.spec_reply(), ReplySpec::Type("McpProbe"));
        assert_eq!(tool.reads(), vec!["name"]);
        let input = tool.input_schema();
        assert_eq!(input["properties"]["name"]["type"], "string", "{input}");
        let output = tool.output_schema().expect("a typed reply lists a schema");
        assert_eq!(output["properties"]["name"]["type"], "string", "{output}");
        let descriptor = tool.descriptor(Revision::Negotiated(
            crate::lifecycle::LATEST_PROTOCOL_VERSION,
        ));
        assert_eq!(descriptor.category, Category::Navigation);
        assert_eq!(descriptor.source, CORE_SOURCE);
    }

    #[specforge_test_macros::test(
        behavior = "list_mcp_tools",
        verify = "a core tool's spec is derived from its one module: its name, description, effect, arguments, target and reply"
    )]
    fn a_core_tool_runs_its_handler_on_its_input() {
        let tool = spec(&ReadProbe);
        let log = EventLog::new();
        let mut state = McpState::new();
        state.events = Box::new(log.clone());
        let mut call = Call::new(&mut state, CallTarget::Unscoped);

        let outcome = tool.run(&mut call, &json!({"name": "a"}));
        let ToolOutcome::Done {
            payload: Payload::Json(reply),
            ..
        } = outcome
        else {
            panic!("the probe answers a structured reply");
        };
        assert_eq!(reply, json!({"name": "a"}));
        assert!(log.is_empty());

        // Arguments that do not read never reach the handler.
        let outcome = tool.run(&mut call, &json!({"name": 3}));
        let ToolOutcome::Refused(error) = outcome else {
            panic!("a name that is no string is refused");
        };
        assert_eq!(error.code, ErrorCode::InvalidInput);
        assert_eq!(error.argument.as_deref(), Some("name"));

        let outcome = tool.run(&mut call, &json!({"name": "a", "bogus": 1}));
        let ToolOutcome::Refused(error) = outcome else {
            panic!("an undeclared name is refused");
        };
        assert_eq!(error.message, "unknown argument 'bogus'");
    }

    #[specforge_test_macros::test(
        behavior = "list_mcp_tools",
        verify = "a core tool's spec is derived from its one module: its name, description, effect, arguments, target and reply"
    )]
    fn a_refused_mutation_reports_a_failed_mutation() {
        let tool = spec(&MutationProbe);
        assert!(tool.is_mutation());
        assert_eq!(tool.category(), Category::Mutation);

        let mut log = EventLog::new();
        let outcome = tool.refused(McpError::new(ErrorCode::InvalidInput, "no"), &mut log);
        let ToolOutcome::Refused(error) = outcome else {
            panic!("a refused call is a failure");
        };
        assert_eq!(error.data, Some(json!({"files_written": []})));
        assert_eq!(log.names(), vec!["mcp_mutation_completed"]);
        assert_eq!(log.named("mcp_mutation_completed")[0]["success"], false);

        // A preview reaches `complete` as one: no event, no `files_written`.
        let log = EventLog::new();
        let mut state = McpState::new();
        state.events = Box::new(log.clone());
        let mut call = Call::new(&mut state, CallTarget::Unscoped);
        let outcome = tool.run(&mut call, &json!({"dry_run": true}));
        let ToolOutcome::Done {
            payload: Payload::Json(reply),
            ..
        } = outcome
        else {
            panic!("a preview answers its reply");
        };
        assert_eq!(reply, json!({"ok": true}));
        assert!(log.is_empty());
    }
}
