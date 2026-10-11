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
use crate::surface_table::{ToolEntry, ToolKind};
use crate::target::{Call, TargetSpec};
use crate::tool::{Category, Effect, ErrorCode, McpError, ToolOutcome, ToolSpec, envelope};
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

/// A failed extension call as a failed tool result: the diagnostic the
/// runtime reported, in `diagnostic`.
fn extension_error(diag: &specforge_common::Diagnostic) -> ToolOutcome {
    McpError::from_diagnostic(diag).into()
}

/// An auto-promoted command's output as a tool result. The command was
/// asked for json (ADR 0011): a JSON object on stdout, and nothing on
/// stderr, is the result's structured payload; a failure that wrote one JSON
/// object on stderr, and nothing on stdout, is an `isError` result carrying
/// it. Otherwise its stdout, then its stderr when it wrote any; a nonzero
/// exit code fails the call.
fn output_result(output: specforge_protocol_types::CommandOutput) -> ToolOutcome {
    let object = |text: &str| match serde_json::from_str::<Value>(text) {
        Ok(object @ Value::Object(_)) => Some(object),
        _ => None,
    };
    let failed = output.exit_code != 0;
    if !failed
        && output.stderr.is_empty()
        && let Some(payload) = object(&output.stdout)
    {
        return ToolOutcome::ok(payload);
    }
    if failed
        && output.stdout.is_empty()
        && let Some(error) = object(&output.stderr)
    {
        return ToolOutcome::failed(error);
    }
    let mut blocks = vec![output.stdout];
    if !output.stderr.is_empty() {
        blocks.push(output.stderr);
    }
    ToolOutcome::texts(blocks, failed)
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
                let (outcome, dispatched) = extension_tool(call, entry, arguments);
                Ran {
                    outcome,
                    events: dispatched
                        .map(|(name, params)| (name.to_string(), params))
                        .into_iter()
                        .collect(),
                }
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

/// A dispatch event: its name and payload.
type Dispatched = Option<(&'static str, Value)>;

/// An extension tool, found in the extension surface table, run by one of
/// its two adapters over the `WasmRuntime` seam the call's project was
/// compiled in: an explicit tool's `mcp__` export, or a command's `cmd__`
/// export. An explicit tool's arguments its declared schema refuses are
/// refused first, the project resolved after; a command's arguments are
/// normalized by its run, over the resolved project; the dispatch event is
/// the one to record when the export returned (whatever the result: a schema
/// mismatch is a dispatched tool that failed).
fn extension_tool(
    call: &mut Call<'_>,
    entry: &ToolEntry,
    arguments: Value,
) -> (ToolOutcome, Dispatched) {
    match &entry.kind {
        ToolKind::McpTool {
            export,
            input_schema,
            output_schema,
        } => {
            if let Err(refused) = check_input(input_schema, &arguments) {
                return (refused, None);
            }
            let project = match call.project() {
                Ok(project) => project,
                Err(refused) => return (refused.into(), None),
            };
            mcp_tool_adapter(
                project.runtime.as_ref(),
                entry,
                export,
                output_schema.as_ref(),
                &arguments,
            )
        }
        ToolKind::Command(command) => {
            let project = match call.project() {
                Ok(project) => project,
                Err(refused) => return (refused.into(), None),
            };
            // The run `specforge <ext> <command>` makes, over the call's
            // project, always asked for json: the tool has no format
            // argument (ADR 0011 A).
            let given = arguments.as_object().cloned().unwrap_or_default();
            let started = std::time::Instant::now();
            let outcome = specforge_ops::command::run(
                &project.view(),
                command,
                &given,
                specforge_ops::command::CommandFormat::Json,
            );
            command_result(command, outcome, started)
        }
    }
}

/// An explicit tool's arguments checked against the input schema it
/// declares, before its module runs: `invalid_input` naming each
/// violation (its schema is opaque JSON to the host, ADR 0004 D4-a).
fn check_input(schema: &Value, arguments: &Value) -> Result<(), ToolOutcome> {
    let violations = specforge_common::shape::violations(schema, arguments);
    if violations.is_empty() {
        return Ok(());
    }
    Err(McpError::new(
        ErrorCode::InvalidInput,
        format!(
            "the arguments do not match the tool's input schema: {}",
            violations.join("; ")
        ),
    )
    .with_data(json!({ "violations": violations }))
    .into())
}

/// How long since `started`, in whole milliseconds.
fn elapsed_ms(started: std::time::Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// An explicit tool: its `mcp__` export called with the arguments, its
/// output checked against the output schema it declares.
fn mcp_tool_adapter(
    runtime: &dyn specforge_wasm::runtime::WasmRuntime,
    entry: &ToolEntry,
    export: &str,
    output_schema: Option<&Value>,
    arguments: &Value,
) -> (ToolOutcome, Dispatched) {
    let started = std::time::Instant::now();
    let result = match specforge_wasm::ExtensionCalls::new(runtime).call_mcp_tool(
        &entry.extension,
        export,
        arguments,
    ) {
        Ok(value) => match output_schema {
            // An output the tool's own schema refuses is never served as
            // its structured result.
            Some(schema) => {
                let violations = specforge_common::shape::violations(schema, &value);
                if violations.is_empty() {
                    ToolOutcome::ok(value)
                } else {
                    McpError::new(
                        ErrorCode::SchemaMismatch,
                        format!(
                            "extension tool '{}' returned output that does not match its output schema: {}",
                            entry.name,
                            violations.join("; ")
                        ),
                    )
                    .with_data(json!({ "violations": violations }))
                    .into()
                }
            }
            None => ToolOutcome::ok(value),
        },
        // A failed call is the tool's error, and no dispatch is recorded.
        Err(error) => return (extension_error(&error.diagnostic()), None),
    };
    // A tool whose export returned is a dispatched tool; it succeeded when
    // its output is the tool's result.
    let event = json!({
        "extensionName": entry.extension,
        "toolName": entry.name,
        "durationMs": elapsed_ms(started),
        "success": result.succeeded(),
    });
    (result, Some(("surface_mcp_tool_dispatched", event)))
}

/// A command's run as a tool result, and its dispatch event when its export
/// returned (whatever its exit code). Args the rule refuses are the
/// command's own `INVALID_INPUT` object, the one the CLI writes; an export
/// that did not answer is a structured MCP error carrying its E028 (ADR 0013
/// D4); no dispatch is recorded for either.
fn command_result(
    command: &specforge_ops::command::ExtensionCommand,
    outcome: Result<specforge_protocol_types::CommandOutput, specforge_ops::command::RunError>,
    started: std::time::Instant,
) -> (ToolOutcome, Dispatched) {
    use specforge_ops::command::RunError;
    match outcome {
        Ok(output) => {
            let event = json!({
                "extensionName": command.extension(),
                "commandId": command.id(),
                "exitCode": output.exit_code,
                "durationMs": elapsed_ms(started),
            });
            (
                output_result(output),
                Some(("surface_command_dispatched", event)),
            )
        }
        Err(RunError::Args(refused)) => (ToolOutcome::failed(refused.to_json()), None),
        Err(RunError::Call(error)) => (extension_error(&error.diagnostic()), None),
        Err(RunError::NoProject(error)) => (McpError::from(error).into(), None),
    }
}
