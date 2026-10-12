//! One MCP request that invokes something by name — `tools/call`,
//! `resources/read`, `prompts/get` — through one pipeline (ADR 0024): read
//! the request, find what it names (the core table, then, with the served
//! project brought up to date, the extension surface table), record the
//! invocation, resolve the call target, run the handler (which records its
//! own events), record the completion, and answer with the kind's envelope;
//! every record goes through the server's event sink (ADR 0057). Each request kind is one
//! [`Surface`] adapter; no other code builds the reply of these three
//! methods; it also owns the one freshness decision of the extension
//! surface table (a lookup, a listing, a subscription). The router guards
//! initialization.

use serde_json::Value;

use crate::events::{EventSink, McpEvent};
use crate::lifecycle::Revision;
use crate::protocol::{JsonRpcError, JsonRpcResponse, error_codes};
use crate::state::McpState;
use crate::target::{self, Call, TargetSpec};
use crate::tool::McpError;

/// What one request invokes: a tool's or prompt's name with its `arguments`
/// object, or the URI a resource read names (with `{}`).
pub(crate) struct Invocation {
    pub name: String,
    pub arguments: Value,
}

impl Invocation {
    /// The invocation `params` carry for request kind `S`. A request that
    /// fails its method's own schema is malformed, -32602 (ADR 0004 D4-a):
    /// no `S::NAMED_BY` string ("Missing required parameter: name"), or
    /// `arguments` that is neither absent, null nor an object ("Invalid
    /// params: arguments must be an object").
    pub fn read<S: Surface>(params: &Value) -> Result<Self, JsonRpcError> {
        let Some(name) = params.get(S::NAMED_BY).and_then(Value::as_str) else {
            return Err(JsonRpcError::new(
                error_codes::INVALID_PARAMS,
                format!("Missing required parameter: {}", S::NAMED_BY),
            ));
        };
        let arguments = if S::TAKES_ARGUMENTS {
            match params.get("arguments") {
                None | Some(Value::Null) => Value::Object(Default::default()),
                Some(object @ Value::Object(_)) => object.clone(),
                Some(_) => {
                    return Err(JsonRpcError::new(
                        error_codes::INVALID_PARAMS,
                        "Invalid params: arguments must be an object",
                    ));
                }
            }
        } else {
            Value::Object(Default::default())
        };
        Ok(Invocation {
            name: name.to_string(),
            arguments,
        })
    }
}

/// The entry a request names: a core table's, or the extension surface
/// table's (ADR 0017 D7).
pub(crate) enum Found<C, E> {
    Core(C),
    Extension(E),
}

impl<C: Copy> Found<C, std::convert::Infallible> {
    /// The core entry, for a kind no extension contributes to.
    pub fn core_entry(&self) -> C {
        match self {
            Found::Core(core) => *core,
            Found::Extension(never) => match *never {},
        }
    }
}

/// One request kind. Three adapters: [`crate::tools::Tools`],
/// [`crate::resources::Resources`], [`crate::prompts::Prompts`].
pub(crate) trait Surface {
    /// The parameter naming what is invoked: `"name"` or `"uri"`.
    const NAMED_BY: &'static str;
    /// Whether the request carries an `arguments` object.
    const TAKES_ARGUMENTS: bool;
    /// Whether extensions contribute entries of this kind (tools,
    /// resources; no extension declares a prompt).
    const EXTENDED: bool;
    /// How an unknown name is refused: "Unknown tool", "Unknown resource
    /// URI", "Unknown prompt" (-32602, before anything is recorded).
    const UNKNOWN: &'static str;

    type Core: Copy;
    type Extension;
    type Outcome;

    /// The core entry `name` names.
    fn core(name: &str) -> Option<Self::Core>;
    /// The served project's extension entry `name` names, asked after the
    /// project was brought up to date.
    fn extension(state: &McpState, name: &str) -> Option<Self::Extension>;
    /// How the entry reaches its project.
    fn target(found: &Found<Self::Core, Self::Extension>) -> TargetSpec;
    /// Recorded once the entry is found, before its target resolves
    /// (`mcp_tool_invoked`, `mcp_prompt_invoked`); none for a read.
    fn invoked(
        found: &Found<Self::Core, Self::Extension>,
        invocation: &Invocation,
    ) -> Option<McpEvent>;
    /// Run the entry over its resolved call; it records its own events
    /// through the call ([`Call::events`]).
    fn run(
        call: &mut Call<'_>,
        found: &Found<Self::Core, Self::Extension>,
        invocation: &Invocation,
    ) -> Self::Outcome;
    /// What a refusal of the call's target is; a refused mutation still
    /// records into `events`.
    fn refused(
        found: &Found<Self::Core, Self::Extension>,
        error: McpError,
        events: &mut dyn EventSink,
    ) -> Self::Outcome;
    /// How an unknown name is refused (-32602 for a tool or a prompt; a
    /// resource that does not exist is not found in the revision of the
    /// request, ADR 0024 D4).
    fn unknown(_revision: Revision, name: &str) -> JsonRpcError {
        JsonRpcError::new(
            error_codes::INVALID_PARAMS,
            format!("{}: {name}", Self::UNKNOWN),
        )
    }
    /// The refusal an outcome is, if it is one: the pipeline makes it the
    /// no-project refusal when its target is none ([`target::without_project`],
    /// ADR 0025).
    fn refusal_mut(outcome: &mut Self::Outcome) -> Option<&mut McpError>;
    /// Recorded after the handler's events (`mcp_resource_read`).
    fn completed(
        found: &Found<Self::Core, Self::Extension>,
        invocation: &Invocation,
        outcome: &Self::Outcome,
    ) -> Option<McpEvent>;
    /// The reply: the only place this kind builds its result or its error,
    /// a failure naming the entry (`tool`, `prompt`, `uri`).
    fn envelope(
        revision: Revision,
        found: &Found<Self::Core, Self::Extension>,
        invocation: &Invocation,
        outcome: Self::Outcome,
        id: Option<Value>,
    ) -> JsonRpcResponse;
}

/// Serve one `tools/call`, `resources/read` or `prompts/get`.
pub(crate) fn serve<S: Surface>(
    state: &mut McpState,
    revision: Revision,
    params: Value,
    id: Option<Value>,
) -> JsonRpcResponse {
    let invocation = match Invocation::read::<S>(&params) {
        Ok(invocation) => invocation,
        Err(error) => return JsonRpcResponse::from_error(id, error),
    };
    let Some(found) = find::<S>(state, &invocation.name) else {
        return JsonRpcResponse::from_error(id, S::unknown(revision, &invocation.name));
    };
    if let Some(event) = S::invoked(&found, &invocation) {
        state.events.record(event);
    }
    // The project the call acts on, resolved (and brought up to date)
    // before the handler runs: handlers never pick a root or reload.
    let outcome = match target::resolve(state, S::target(&found), &invocation.arguments) {
        Ok(target) => {
            let mut call = Call::new(state, target);
            // The handler records its own events through `call.events()`.
            let mut outcome = S::run(&mut call, &found, &invocation);
            // With nothing served, a refusal that names a file or an
            // entity is the no-project refusal, whichever kind of request
            // it is.
            if let Some(refusal) = S::refusal_mut(&mut outcome) {
                target::refuse_without_project(call.target(), refusal);
            }
            outcome
        }
        Err(refused) => S::refused(&found, McpError::from(refused), &mut *state.events),
    };
    if let Some(event) = S::completed(&found, &invocation, &outcome) {
        state.events.record(event);
    }
    S::envelope(revision, &found, &invocation, outcome, id)
}

/// The entry `name` names: a core one; else, for a kind extensions
/// contribute, the extension surface table's, the served project brought up
/// to date first, so an extension enabled on disk since the last request is
/// found (ADR 0014 D12, ADR 0024 D2). The lookup `resources/subscribe` and
/// `subscriptions/listen` make too, so a resource is "served" by one rule.
pub(crate) fn find<S: Surface>(
    state: &mut McpState,
    name: &str,
) -> Option<Found<S::Core, S::Extension>> {
    if let Some(core) = S::core(name) {
        return Some(Found::Core(core));
    }
    if !S::EXTENDED {
        return None;
    }
    bring_surface_up_to_date(state);
    S::extension(state, name).map(Found::Extension)
}

/// Run a listing (`tools/list`, `resources/list`, `resources/templates/list`,
/// `prompts/list`) of kind `S`: for a kind extensions contribute to, the
/// served project is brought up to date first, so the listing names exactly
/// what a call dispatches under that name (ADR 0024 D2).
pub(crate) fn listed<S: Surface, R>(
    state: &mut McpState,
    list: impl FnOnce(&mut McpState) -> R,
) -> R {
    if S::EXTENDED {
        bring_surface_up_to_date(state);
    }
    list(state)
}

/// The one freshness decision of the extension surface table: a served
/// project is brought up to date with disk before the table is read, and
/// nothing is loaded when nothing is served. Every read of the table (a
/// lookup, a listing, a subscription) goes through [`find`] or [`listed`].
fn bring_surface_up_to_date(state: &mut McpState) {
    if state.project_root().is_some() {
        state.ensure_fresh();
    }
}
