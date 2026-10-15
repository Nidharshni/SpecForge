//! Extension dispatch: a `tools/call` or `resources/read` the extension
//! surface table answers, run by one of three adapters, each one call over
//! the `WasmRuntime` seam the call's project was compiled in (ADR 0017 D8,
//! ADR 0013): an explicit tool's `mcp__` export ([`mcp_tool_adapter`]), a
//! command's `cmd__` export ([`command_adapter`]) and a resource's `mcp__`
//! export ([`resource_adapter`]). The dispatch event is recorded through the
//! call's event sink when the export returned, whatever it answered.

use serde_json::{Value, json};
use specforge_ops::extension::command::{CommandFormat, ExtensionCommand, RunError};
use specforge_protocol_types::CommandOutput;
use specforge_wasm::runtime::WasmRuntime;

use crate::events::McpEvent;
use crate::reply::Source;
use crate::resource::{ReadOutcome, ResourceText};
use crate::surface_table::{ResourceEntry, ToolEntry, ToolKind};
use crate::target::{Call, ProjectRef};
use crate::tool::{ErrorCode, McpError, ToolOutcome};

/// The extension tool `entry`, called with `arguments` over the call's
/// project: an explicit tool's arguments its declared schema refuses are
/// refused first, the project resolved after; a command's are normalized by
/// its run.
pub(crate) fn run_tool(call: &mut Call<'_>, entry: &ToolEntry, arguments: &Value) -> ToolOutcome {
    let (outcome, dispatched) = match &entry.kind {
        ToolKind::McpTool {
            export,
            input_schema,
            output_schema,
        } => {
            if let Err(refused) = check_input(input_schema, arguments) {
                return refused;
            }
            match call.project() {
                Ok(project) => mcp_tool_adapter(
                    project.runtime.as_ref(),
                    entry,
                    export,
                    output_schema.as_ref(),
                    arguments,
                ),
                Err(refused) => (refused.into(), None),
            }
        }
        ToolKind::Command(command) => match call.project() {
            Ok(project) => command_adapter(&project, command, arguments),
            Err(refused) => (refused.into(), None),
        },
    };
    if let Some(event) = dispatched {
        call.events().record(event);
    }
    outcome
}

/// The extension resource `entry`, read at `uri` over the call's project. A
/// failed read is the read's error (E028 in `diagnostic`), and no dispatch
/// is recorded.
pub(crate) fn read_resource(call: &mut Call<'_>, entry: &ResourceEntry, uri: &str) -> ReadOutcome {
    // The project the resource reads, in the runtime it was compiled in.
    let (outcome, dispatched) = match call.project() {
        Ok(project) => resource_adapter(project.runtime.as_ref(), entry, uri),
        Err(refused) => (Err(Box::new(refused)), None),
    };
    if let Some(event) = dispatched {
        call.events().record(event);
    }
    outcome
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

/// An explicit tool: its `mcp__` export called with the arguments, its
/// output checked against the output schema it declares. The tool's
/// outcome, and its dispatch event when its export returned.
fn mcp_tool_adapter(
    runtime: &dyn WasmRuntime,
    entry: &ToolEntry,
    export: &str,
    output_schema: Option<&Value>,
    arguments: &Value,
) -> (ToolOutcome, Option<McpEvent>) {
    let started = std::time::Instant::now();
    let result = match specforge_wasm::ExtensionCalls::new(runtime).call_mcp_tool(
        &entry.extension,
        export,
        arguments,
    ) {
        Ok(value) => match output_schema {
            // An output the tool's own schema refuses is never served as
            // its structured result.
            Some(schema) => crate::reply::conforming(
                &entry.name,
                Source::Extension(&entry.extension),
                schema,
                ToolOutcome::ok(value),
            ),
            None => ToolOutcome::ok(value),
        },
        // A failed call is the tool's error, and no dispatch is recorded.
        Err(error) => return (failed_call(&error.diagnostic()).into(), None),
    };
    // A tool whose export returned is a dispatched tool; it succeeded when
    // its output is the tool's result.
    let event = McpEvent::SurfaceMcpToolDispatched {
        extension_name: entry.extension.clone(),
        tool_name: entry.name.clone(),
        duration_ms: elapsed_ms(started),
        success: result.succeeded(),
    };
    (result, Some(event))
}

/// A command's run as a tool result, and its dispatch event when its export
/// returned (whatever its exit code). The run is `specforge <ext> <command>`
/// over the call's project, always asked for json: the tool has no format
/// argument (ADR 0011 A). Args the rule refuses are the command's own
/// `INVALID_INPUT` object, the one the CLI writes; an export that did not
/// answer is a structured MCP error carrying its E028 (ADR 0013 D4); no
/// dispatch is recorded for either.
fn command_adapter(
    project: &ProjectRef<'_>,
    command: &ExtensionCommand,
    arguments: &Value,
) -> (ToolOutcome, Option<McpEvent>) {
    let given = arguments.as_object().cloned().unwrap_or_default();
    let started = std::time::Instant::now();
    match specforge_ops::extension::command::run(
        &project.view(),
        command,
        &given,
        CommandFormat::Json,
    ) {
        Ok(output) => {
            let event = McpEvent::SurfaceCommandDispatched {
                extension_name: command.extension().to_string(),
                command_id: command.id().to_string(),
                exit_code: output.exit_code,
                duration_ms: elapsed_ms(started),
            };
            (output_result(output), Some(event))
        }
        Err(RunError::Args(refused)) => (ToolOutcome::failed(refused.to_json()), None),
        Err(RunError::Call(error)) => (failed_call(&error.diagnostic()).into(), None),
        Err(RunError::NoProject(error)) => (McpError::from(error).into(), None),
    }
}

/// An extension resource read through its `mcp__` export. A failed read (a
/// trap, an answer that is not its content and MIME type, an export the
/// guest does not route) is a server-side fault of the extension: an
/// internal error (-32603) whose `data` is the `McpError` carrying the E028
/// diagnostic (D6, `mcp_structured_error_responses`).
fn resource_adapter(
    runtime: &dyn WasmRuntime,
    entry: &ResourceEntry,
    uri: &str,
) -> (ReadOutcome, Option<McpEvent>) {
    let started = std::time::Instant::now();
    match specforge_wasm::ExtensionCalls::new(runtime).read_mcp_resource(
        &entry.extension,
        &entry.export,
        uri,
    ) {
        // A read whose export answered its content is a dispatched
        // resource; a failed call is the read's error, and no dispatch is
        // recorded.
        Ok(read) => {
            let event = McpEvent::SurfaceMcpResourceDispatched {
                extension_name: entry.extension.clone(),
                uri_template: entry.uri_template.clone(),
                mime_type: read.mime_type.clone(),
                duration_ms: elapsed_ms(started),
            };
            let text = ResourceText {
                text: read.content,
                mime_type: read.mime_type,
            };
            (Ok(text), Some(event))
        }
        Err(error) => (Err(Box::new(failed_call(&error.diagnostic()))), None),
    }
}

/// An auto-promoted command's output as a tool result. The command was
/// asked for json (ADR 0011): a JSON object on stdout, and nothing on
/// stderr, is the result's structured payload; a failure that wrote one JSON
/// object on stderr, and nothing on stdout, is an `isError` result carrying
/// it. Otherwise its stdout, then its stderr when it wrote any; a nonzero
/// exit code fails the call.
fn output_result(output: CommandOutput) -> ToolOutcome {
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

/// A failed extension call as an MCP error: the diagnostic the runtime
/// reported, in `diagnostic`.
fn failed_call(diagnostic: &specforge_common::Diagnostic) -> McpError {
    McpError::new(ErrorCode::InternalError, diagnostic.message.clone()).with_diagnostic(diagnostic)
}

/// How long since `started`, in whole milliseconds.
fn elapsed_ms(started: std::time::Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}
