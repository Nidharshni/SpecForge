mod add_extension;
pub(crate) mod analyze;
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
pub(crate) mod infer_progress;
pub(crate) mod infer_session;
mod init;
pub(crate) mod inspect;
pub(crate) mod list;
mod migrate;
mod model;
mod outline;
mod outline_extensions;
mod providers;
pub(crate) mod query;
mod remove_extension;
mod rename;
mod render;
pub(crate) mod schema;
pub(crate) mod search;
mod stats;
mod suggest_fixes;
pub(crate) mod trace;
pub(crate) mod validate;

use serde_json::{Value, json};

use crate::lifecycle::Revision;
use crate::protocol::JsonRpcResponse;
use crate::state::McpState;
use crate::surface_call::{Event, Found, Invocation, Ran, Surface};
use crate::surface_table::ToolEntry;
use crate::target::{Call, TargetSpec};
use crate::tool::{Category, McpError, ToolOutcome, ToolSpec, envelope, spec};
use specforge_ops::view::ProjectView;

/// The core tools, in listing order (ADR 0017 D8): each its own module
/// implementing [`CoreTool`](crate::tool::CoreTool), its declarations
/// checked here when the table is built ([`spec`]).
pub static CORE_TOOLS: &[&dyn ToolSpec] = &[
    spec(&query::Tool),
    spec(&validate::Tool),
    spec(&analyze::Tool),
    spec(&export::Tool),
    spec(&trace::Tool),
    spec(&search::Tool),
    spec(&explain::Tool),
    spec(&schema::Tool),
    spec(&model::Tool),
    spec(&outline_extensions::Tool),
    spec(&coverage::Tool),
    spec(&stats::Tool),
    spec(&list::Tool),
    spec(&inspect::Tool),
    spec(&find_definition::Tool),
    spec(&find_references::Tool),
    spec(&outline::Tool),
    spec(&suggest_fixes::Tool),
    spec(&format::Tool),
    spec(&rename::Tool),
    spec(&init::Tool),
    spec(&add_extension::Tool),
    spec(&remove_extension::Tool),
    spec(&migrate::Tool),
    spec(&extensions::Tool),
    spec(&providers::Tool),
    spec(&doctor::Tool),
    spec(&collect::Tool),
    spec(&render::Tool),
    spec(&infer_progress::Tool),
    spec(&infer_gaps::Tool),
    spec(&infer_session::Tool),
    spec(&find_implementation::Tool),
    spec(&find_spec_for_source::Tool),
];

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
pub fn core_tool(name: &str) -> Option<&'static dyn ToolSpec> {
    CORE_TOOLS.iter().copied().find(|tool| tool.name() == name)
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

    type Core = &'static dyn ToolSpec;
    type Extension = ToolEntry;
    type Outcome = ToolOutcome;

    fn core(name: &str) -> Option<&'static dyn ToolSpec> {
        core_tool(name)
    }

    fn extension(state: &McpState, name: &str) -> Option<ToolEntry> {
        state.surfaces().tool(name).cloned()
    }

    fn target(found: &Found<&'static dyn ToolSpec, ToolEntry>) -> TargetSpec {
        match found {
            Found::Core(spec) => spec.target(),
            Found::Extension(_) => TargetSpec::SERVED_PROJECT,
        }
    }

    fn invoked(
        found: &Found<&'static dyn ToolSpec, ToolEntry>,
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
        found: &Found<&'static dyn ToolSpec, ToolEntry>,
        invocation: &Invocation,
    ) -> Ran<ToolOutcome> {
        match found {
            Found::Core(tool) => tool.run(call, &invocation.arguments),
            Found::Extension(entry) => {
                crate::extension_dispatch::run_tool(call, entry, &invocation.arguments)
            }
        }
    }

    fn refused(
        found: &Found<&'static dyn ToolSpec, ToolEntry>,
        error: McpError,
    ) -> Ran<ToolOutcome> {
        match found {
            // A refused mutation is a failed one: it wrote nothing, and says so.
            Found::Core(tool) => tool.refused(error),
            Found::Extension(_) => Ran::of(error.into()),
        }
    }

    fn refusal_mut(outcome: &mut ToolOutcome) -> Option<&mut McpError> {
        match outcome {
            ToolOutcome::Refused(error) => Some(error),
            ToolOutcome::Done { .. } => None,
        }
    }

    fn completed(
        _: &Found<&'static dyn ToolSpec, ToolEntry>,
        _: &Invocation,
        _: &ToolOutcome,
    ) -> Option<Event> {
        None
    }

    fn envelope(
        revision: Revision,
        found: &Found<&'static dyn ToolSpec, ToolEntry>,
        invocation: &Invocation,
        outcome: ToolOutcome,
        id: Option<Value>,
    ) -> JsonRpcResponse {
        // A tool with an outputSchema: a core one, or an extension's that
        // declares one.
        let typed = match found {
            Found::Core(spec) => spec.is_typed(),
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
