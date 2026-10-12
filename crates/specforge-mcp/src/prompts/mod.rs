//! The core prompts: the `prompts/get` adapter of the request pipeline
//! ([`crate::surface_call`]) over the Prompt spec table ([`CORE_PROMPTS`]).

mod context;
mod explore;
mod infer;
mod review;
mod table;
mod trace;

use serde_json::Value;

use crate::events::{EventSink, McpEvent};
use crate::lifecycle::Revision;
use crate::prompt::{PromptOutcome, PromptSpec, prompt_envelope};
use crate::protocol::JsonRpcResponse;
use crate::state::McpState;
use crate::surface_call::{Found, Invocation, Surface};
use crate::target::{Call, TargetSpec};
use crate::tool::McpError;
use crate::types::McpPromptDescriptor;
pub use table::CORE_PROMPTS;

/// The core prompt named `name`.
pub fn core_prompt(name: &str) -> Option<&'static PromptSpec> {
    CORE_PROMPTS.iter().find(|p| p.name == name)
}

/// What `prompts/list` lists and `initialize` reports: every core prompt,
/// in table order.
pub fn descriptors() -> Vec<McpPromptDescriptor> {
    CORE_PROMPTS.iter().map(PromptSpec::descriptor).collect()
}

/// `prompts/get`: the core prompt table (no extension declares a prompt).
/// The request pipeline's prompts adapter ([`crate::surface_call`]).
pub(crate) struct Prompts;

impl Surface for Prompts {
    const NAMED_BY: &'static str = "name";
    const TAKES_ARGUMENTS: bool = true;
    const EXTENDED: bool = false;
    const UNKNOWN: &'static str = "Unknown prompt";

    type Core = &'static PromptSpec;
    type Extension = std::convert::Infallible;
    type Outcome = PromptOutcome;

    fn core(name: &str) -> Option<&'static PromptSpec> {
        core_prompt(name)
    }

    fn extension(_: &McpState, _: &str) -> Option<std::convert::Infallible> {
        None
    }

    fn target(found: &Found<&'static PromptSpec, std::convert::Infallible>) -> TargetSpec {
        found.core_entry().target()
    }

    fn invoked(
        found: &Found<&'static PromptSpec, std::convert::Infallible>,
        invocation: &Invocation,
    ) -> Option<McpEvent> {
        let spec = found.core_entry();
        let argument = |name: &str| {
            invocation
                .arguments
                .get(name)
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        Some(McpEvent::McpPromptInvoked {
            prompt_name: spec.name.to_string(),
            entity_id: argument("entity_id"),
            kind: argument("kind"),
        })
    }

    fn run(
        call: &mut Call<'_>,
        found: &Found<&'static PromptSpec, std::convert::Infallible>,
        invocation: &Invocation,
    ) -> PromptOutcome {
        let spec = found.core_entry();
        if let Some(error) = spec.undeclared(&invocation.arguments) {
            return Err(Box::new(error));
        }
        (spec.render)(call.view(), invocation.arguments.clone())
    }

    fn refused(
        _: &Found<&'static PromptSpec, std::convert::Infallible>,
        error: McpError,
        _: &mut dyn EventSink,
    ) -> PromptOutcome {
        Err(Box::new(error))
    }

    fn refusal_mut(outcome: &mut PromptOutcome) -> Option<&mut McpError> {
        outcome.as_mut().err().map(|refusal| &mut **refusal)
    }

    fn completed(
        _: &Found<&'static PromptSpec, std::convert::Infallible>,
        _: &Invocation,
        _: &PromptOutcome,
    ) -> Option<McpEvent> {
        None
    }

    fn envelope(
        _: Revision,
        found: &Found<&'static PromptSpec, std::convert::Infallible>,
        _: &Invocation,
        outcome: PromptOutcome,
        id: Option<Value>,
    ) -> JsonRpcResponse {
        let spec = found.core_entry();
        prompt_envelope(outcome, spec, id)
    }
}
