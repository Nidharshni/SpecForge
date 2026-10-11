mod add_extension;
mod analyze;
mod collect;
pub(crate) mod coverage;
mod doctor;
mod explain;
mod export;
mod extensions;
mod find_definition;
mod find_implementation;
mod find_references;
pub(crate) mod find_spec_for_source;
mod format;
mod infer_gaps;
mod infer_progress;
mod infer_session;
mod init;
mod inspect;
pub(crate) mod list;
mod migrate;
mod model;
mod outline;
mod outline_extensions;
mod providers;
mod query;
mod remove_extension;
mod rename;
mod render;
mod schema;
mod search;
mod stats;
mod suggest_fixes;
mod table;
pub(crate) mod trace;
mod validate;

use serde_json::{Value, json};

use crate::lifecycle::Revision;
use crate::mutation;
use crate::protocol::JsonRpcResponse;
use crate::state::McpState;
use crate::surface_call::{Event, Found, Invocation, Ran, Surface};
use crate::surface_table::ToolEntry;
use crate::target::{Call, TargetSpec};
use crate::tool::{Category, Effect, McpError, ToolOutcome, ToolSpec, envelope};
use specforge_ops::view::ProjectView;
pub use table::CORE_TOOLS;

/// Where the `.spec` files of the project `view` reads are keyed from: its
/// spec root, when it has a root (the empty session has none, so no file is
/// a project's, ADR 0025).
pub(crate) fn spec_root<'v>(view: &ProjectView<'v>) -> Option<&'v std::path::Path> {
    view.root().map(|_| view.env().spec_root.as_path())
}

/// The navigator over `view` (`specforge_ops::navigate`), each file's text
/// read from disk under its spec root (with no root, no file is read). The
/// navigation tools render its answers as JSON and nothing else (ADR 0016).
pub(crate) fn navigator<'v>(
    view: ProjectView<'v>,
) -> specforge_ops::navigate::Navigator<'v, impl Fn(&str) -> Option<String> + 'v> {
    let spec_root = spec_root(&view);
    specforge_ops::navigate::Navigator::new(view, move |file| {
        std::fs::read_to_string(spec_root?.join(file)).ok()
    })
}

/// The core tool named `name`.
pub fn core_tool(name: &str) -> Option<&'static ToolSpec> {
    CORE_TOOLS.iter().find(|t| t.name == name)
}

/// The name of the core tool `name` as `tools/list` lists it: the one way
/// a prompt or the server's instructions name a tool. A name no core tool
/// has is a SpecForge bug; the prompts' tests render every text that names
/// one, so it fails there first.
pub(crate) fn core_tool_name(name: &'static str) -> &'static str {
    core_tool(name)
        .map(|tool| tool.name)
        .unwrap_or_else(|| panic!("SpecForge bug: no core tool is named {name}"))
}

/// `tools/call`: the core tool table, then the extension surface table (ADR
/// 0017 D7). The request pipeline's tools adapter
/// ([`crate::surface_call`]).
pub(crate) struct Tools;

impl Surface for Tools {
    const NAMED_BY: &'static str = "name";
    const TAKES_ARGUMENTS: bool = true;
    const EXTENDED: bool = true;
    // MCP spec (tools/call): an unrecognized tool is an Invalid params
    // protocol error, as its "Unknown tool" example shows.
    const UNKNOWN: &'static str = "Unknown tool";

    type Core = &'static ToolSpec;
    type Extension = ToolEntry;
    type Outcome = ToolOutcome;

    fn core(name: &str) -> Option<&'static ToolSpec> {
        core_tool(name)
    }

    fn extension(state: &McpState, name: &str) -> Option<ToolEntry> {
        state.surfaces().tool(name).cloned()
    }

    fn target(found: &Found<&'static ToolSpec, ToolEntry>) -> TargetSpec {
        match found {
            Found::Core(spec) => spec.target(),
            Found::Extension(_) => TargetSpec::SERVED_PROJECT,
        }
    }

    fn invoked(
        found: &Found<&'static ToolSpec, ToolEntry>,
        invocation: &Invocation,
    ) -> Option<Event> {
        // The category it is listed with: no second lookup.
        let category = match found {
            Found::Core(spec) => spec.category().as_str(),
            Found::Extension(entry) => Category::from(entry.group).as_str(),
        };
        let mut event = json!({
            "toolName": invocation.name,
            "category": category,
            "params": invocation.arguments.to_string(),
        });
        if let Some(entity_id) = invocation
            .arguments
            .get("entity_id")
            .and_then(Value::as_str)
        {
            event["entityId"] = Value::from(entity_id);
        }
        Some(("mcp_tool_invoked".to_string(), event))
    }

    fn run(
        call: &mut Call<'_>,
        found: &Found<&'static ToolSpec, ToolEntry>,
        invocation: &Invocation,
    ) -> Ran<ToolOutcome> {
        // A name the tool and its target do not declare is refused, before
        // anything is read (a refused mutation is a failed one).
        if let Found::Core(spec) = found
            && let Some(error) = spec.undeclared(&invocation.arguments)
        {
            return Self::refused(found, error);
        }
        let arguments = invocation.arguments.clone();
        match found {
            // A mutation's handler and `mutation::complete` do the rest: the
            // refresh, the reply sent once and checked against its
            // outputSchema, the events (ADR 0022).
            Found::Core(ToolSpec {
                effect: Effect::Mutates { handler, .. },
                name,
                ..
            }) => {
                let schema = handler.reply()();
                let (outcome, events) = handler.run(call, name, &schema, arguments);
                Ran { outcome, events }
            }
            Found::Core(
                spec @ ToolSpec {
                    effect: Effect::Reads { handler, .. } | Effect::WritesOutput { handler, .. },
                    ..
                },
            ) => Ran::of(conforming(spec, handler.run(call, arguments))),
            Found::Extension(entry) => {
                crate::extension_dispatch::run_tool(call, entry, &invocation.arguments)
            }
        }
    }

    fn refused(found: &Found<&'static ToolSpec, ToolEntry>, error: McpError) -> Ran<ToolOutcome> {
        match found {
            // A refused mutation is a failed one: it wrote nothing, and says so.
            Found::Core(spec) if spec.is_mutation() => {
                let (outcome, events) = mutation::refused(spec.name, error);
                Ran { outcome, events }
            }
            _ => Ran::of(error.into()),
        }
    }

    fn refusal_mut(outcome: &mut ToolOutcome) -> Option<&mut McpError> {
        match outcome {
            ToolOutcome::Refused(error) => Some(error),
            ToolOutcome::Done { .. } => None,
        }
    }

    fn completed(
        _: &Found<&'static ToolSpec, ToolEntry>,
        _: &Invocation,
        _: &ToolOutcome,
    ) -> Option<Event> {
        None
    }

    fn envelope(
        revision: Revision,
        found: &Found<&'static ToolSpec, ToolEntry>,
        invocation: &Invocation,
        outcome: ToolOutcome,
        id: Option<Value>,
    ) -> JsonRpcResponse {
        // A tool with an outputSchema: a core one, or an extension's that
        // declares one.
        let typed = match found {
            Found::Core(spec) => spec.output_schema().is_some(),
            Found::Extension(entry) => entry.output_schema().is_some(),
        };
        envelope(
            outcome.from_tool(&invocation.name),
            id,
            revision.sends_structured_content(),
            typed,
        )
    }
}

/// `outcome` when it conforms to `spec`'s outputSchema (a tool with none is
/// not checked), else the `schema_mismatch` failure ([`crate::reply::conforming`]).
fn conforming(spec: &ToolSpec, outcome: ToolOutcome) -> ToolOutcome {
    match spec.output_schema() {
        Some(schema) => crate::reply::conforming(spec.name, &schema, outcome),
        None => outcome,
    }
}
