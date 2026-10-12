//! What a prompt rendered, and the one place that turns it into a
//! `prompts/get` reply: the prompt-side twin of [`crate::tool`].
//!
//! Each core prompt is one module implementing [`CorePrompt`] (ADR 0049 D1,
//! applied to prompts by ADR 0058): its name, description, the spec type its
//! payload is, its typed arguments and its typed payload are stated there
//! once. [`PromptSpec`] is what the listing, the prompts adapter and the
//! reflection tests read of it. [`prompt_envelope`] alone builds the
//! `messages` of a rendered prompt and the error of a refused one (ADR
//! 0004 D4-d).

use serde::Serialize;
use serde_json::{Value, json};
use specforge_common::shape::{self, Object, Shape};

use crate::args::{Argument, Arguments};
use crate::protocol::JsonRpcResponse;
use crate::registry::{PromptArgument, PromptDescriptor};
use crate::target::TargetSpec;
use crate::tool::McpError;
use specforge_ops::view::ProjectView;

/// One core prompt, stated once in its own module under `prompts/`: its
/// name, description, the spec type its payload is, its typed arguments and
/// its typed payload. Its listing, its reading, its rendering and the
/// reflection tests derive from it.
pub trait CorePrompt: Sync + 'static {
    /// Its name as `prompts/list` lists it (`specforge://prompts/<name>`).
    const NAME: &'static str;
    const DESCRIPTION: &'static str;
    /// The `Mcp*` type of `spec/types/mcp.spec` its payload is.
    const SPEC: &'static str;
    /// Its payload's open values, as `CoreTool::OPEN_VALUES`.
    const OPEN_VALUES: &'static [&'static str] = &[];
    /// What it reads from the request's `arguments` (ADR 0033).
    type Args: Arguments;
    /// What it renders: the second user message's JSON.
    type Payload: Serialize + Shape + Object + 'static;
    /// The instruction and the payload over `view` (the served project's,
    /// brought up to date, or the empty session's), or the refusal.
    fn render(
        view: ProjectView<'_>,
        args: Self::Args,
    ) -> Result<Prompted<Self::Payload>, Box<McpError>>;
}

/// What a prompt rendered, typed.
pub struct Prompted<P> {
    pub instruction: String,
    pub payload: P,
}

/// What a `prompts/get` produced, erased: the instruction and the payload
/// as JSON text in its type's field order.
#[derive(Debug, Clone)]
pub struct Rendered {
    pub instruction: String,
    pub payload: String,
}

/// What a `prompts/get` produced. A refusal is the spec's `McpError`, boxed
/// as a tool's refusal is.
pub type PromptOutcome = Result<Rendered, Box<McpError>>;

/// What the listing, the prompts adapter and the reflection tests read of a
/// core prompt; implemented once for every [`CorePrompt`].
pub trait PromptSpec: Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    /// The `Mcp*` type its payload is.
    fn spec_payload(&self) -> &'static str;
    /// The open values of its payload ([`CorePrompt::OPEN_VALUES`]).
    fn open_values(&self) -> &'static [&'static str];
    /// Its payload's JSON Schema, built once per process.
    fn payload_schema(&self) -> &'static Value;
    /// Its arguments ([`Arguments::declared`] of its `Args`): the listing
    /// (name, description, required) and the names a request may send.
    fn arguments(&self) -> Vec<Argument>;
    /// Read `arguments` into its `Args` (refusing what does not read) and
    /// render over `view`: the served project's, brought up to date, or the
    /// empty session's when nothing is served.
    fn get(&self, view: ProjectView<'_>, arguments: &Value) -> PromptOutcome;

    /// How it reaches its project: every core prompt reads the served
    /// project's view ([`TargetSpec::SERVED_VIEW`]).
    fn target(&self) -> TargetSpec {
        TargetSpec::SERVED_VIEW
    }

    /// The refusal of a request that sends a name neither the prompt nor
    /// its target declares ([`crate::args::undeclared`]).
    fn undeclared(&self, arguments: &Value) -> Option<McpError> {
        crate::args::undeclared(arguments, &self.arguments(), self.target())
    }

    /// The prompt as `prompts/list` describes it.
    fn descriptor(&self) -> PromptDescriptor {
        PromptDescriptor {
            name: self.name().into(),
            description: self.description().into(),
            arguments: self
                .arguments()
                .into_iter()
                .map(|argument| PromptArgument {
                    name: argument.name.into(),
                    description: argument.description.into(),
                    required: argument.required,
                })
                .collect(),
        }
    }
}

impl<T: CorePrompt> PromptSpec for T {
    fn name(&self) -> &'static str {
        T::NAME
    }

    fn description(&self) -> &'static str {
        T::DESCRIPTION
    }

    fn spec_payload(&self) -> &'static str {
        T::SPEC
    }

    fn open_values(&self) -> &'static [&'static str] {
        T::OPEN_VALUES
    }

    fn payload_schema(&self) -> &'static Value {
        shape::document::<T::Payload>()
    }

    fn arguments(&self) -> Vec<Argument> {
        <T::Args as Arguments>::declared()
    }

    fn get(&self, view: ProjectView<'_>, arguments: &Value) -> PromptOutcome {
        let args = crate::args::read::<T::Args>(arguments)?;
        let Prompted {
            instruction,
            payload,
        } = T::render(view, args)?;
        Ok(Rendered {
            instruction,
            payload: serde_json::to_string(&payload).expect("a payload serializes"),
        })
    }
}

/// The `prompts/get` reply for `outcome`: the only place that builds a
/// prompt's `messages` or a prompt's error. A rendered prompt is its
/// description and two user messages: the instruction, then the payload
/// as JSON text. MCP prompts have no `isError`,
/// so a refusal is a JSON-RPC error under the one code rule
/// ([`McpError::into_rpc_error`]), its `McpError`, naming the prompt, as the
/// error's `data`.
pub fn prompt_envelope(
    outcome: PromptOutcome,
    spec: &dyn PromptSpec,
    id: Option<Value>,
) -> JsonRpcResponse {
    match outcome {
        // Both user messages (C9-14): graph data, user-authored text
        // included, never poses as the model's own earlier turn.
        Ok(Rendered {
            instruction,
            payload,
        }) => JsonRpcResponse::success(
            id,
            json!({
                "description": spec.description(),
                "messages": [
                    { "role": "user", "content": { "type": "text", "text": instruction } },
                    { "role": "user", "content": { "type": "text", "text": payload } },
                ],
            }),
        ),
        Err(mut error) => {
            error.prompt.get_or_insert_with(|| spec.name().to_string());
            JsonRpcResponse::from_error(id, error.into_rpc_error())
        }
    }
}
