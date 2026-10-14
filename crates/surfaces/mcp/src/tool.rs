//! What a tool call produced, and the one place that turns it into a
//! `tools/call` reply.
//!
//! Handlers return a [`ToolOutcome`] (a mutation's handler, a
//! [`Mutation`](crate::mutation::Mutation) that [`crate::mutation::complete`]
//! turns into one); [`envelope`] alone builds `content`, `isError` and
//! `_meta`. What a mutation wrote crosses to the dispatcher typed (ADR 0022),
//! never read back from the reply.

use serde::Serialize;
use serde_json::{Value, json};
use specforge_common::Diagnostic;
use specforge_common::shape::Shape;

use crate::protocol::JsonRpcResponse;
use crate::registry::ToolAnnotations;

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

mod core_tool;
mod error;
#[cfg(test)]
mod spec_tests;
#[cfg(test)]
mod tests;

pub use core_tool::{CoreTool, Handled, Previews, ToolSpec, spec};
pub(crate) use error::file_not_found;
pub use error::{ErrorCode, McpError, entity_not_found, is_diagnostic_code};
