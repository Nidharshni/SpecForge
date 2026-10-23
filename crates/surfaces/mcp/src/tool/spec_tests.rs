use super::*;
use crate::args::Arguments;
use crate::lifecycle::Revision;
use crate::mutation::{Mutated, NoDiagnostics, Written};
use crate::mutation::{Mutation, MutationArgs, MutationReply};
use crate::reply::Answer;
use crate::reply::Answered;
use crate::state::McpState;
use crate::target::{Call, TargetSpec};
use crate::target::{CallTarget, Unscoped};
use crate::testing::EventLog;
use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_ops::{Writes, Wrote};

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

    fn call(_: (), args: MutationProbeArgs) -> Mutation<MutationProbeReply> {
        Ok(Mutated::new(
            Answer::new(MutationProbeReply { ok: true }),
            Written::of(Wrote::of(args.dry_run, Writes::none())),
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
    let outcome = tool.refused(
        McpError::new(ErrorCode::InvalidInput, "no"),
        &json!({}),
        &mut log,
    );
    let ToolOutcome::Refused(error) = outcome else {
        panic!("a refused call is a failure");
    };
    assert_eq!(error.data, Some(json!({"files_written": []})));
    assert_eq!(log.names(), vec!["mcp_mutation_completed"]);
    assert_eq!(log.named("mcp_mutation_completed")[0]["success"], false);

    // A refusal of a call whose arguments read and ask for a preview is one: nothing listed, nothing recorded.
    for (arguments, previews) in [
        (json!({"dry_run": true}), true),
        (json!({"dry_run": "nope"}), false),
        (json!({"dry_run": true, "bogus": 1}), false),
    ] {
        let mut log = EventLog::new();
        let refusal = McpError::new(ErrorCode::PreconditionFailed, "no project is served");
        let ToolOutcome::Refused(error) = tool.refused(refusal, &arguments, &mut log) else {
            panic!("a refused call is a failure");
        };
        assert_eq!(
            error.data.is_none(),
            previews,
            "{arguments}: {:?}",
            error.data
        );
        assert_eq!(log.is_empty(), previews, "{arguments}");
    }

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
