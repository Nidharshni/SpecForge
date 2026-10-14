use crate::target::Project;
use crate::tool::{CoreTool, Effect, ReplySpec, WriteHints};
use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_ops::read::infer::{
    self, EndStatus, Recorded, SessionAction, SessionField, SessionOutcome, SessionStatus,
    SessionStep,
};

use crate::args::Arguments;
use crate::mutation::{Mutated, Mutation, MutationArgs, MutationReply, NoDiagnostics, Written};
use crate::refusal::ArgumentOf;
use crate::target::ProjectRef;
use crate::tool::{ErrorCode, McpError};

/// `specforge.infer_session`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Session action to perform
    #[arg(choice = infer::SESSION_ACTION)]
    action: String,
    /// Agent identifier (for start)
    agent: Option<String>,
    /// Source directories to scan (for start)
    source_roots: Option<Vec<String>>,
    /// Relative path to analyzed file (for mark_analyzed)
    source_file: Option<String>,
    /// Entity IDs produced from the file (for mark_analyzed)
    entities_produced: Vec<String>,
    /// Session ID to end (for end)
    session_id: Option<String>,
    /// Final status (for end)
    #[arg(choice = infer::END_STATUS)]
    status: EndStatus,
}

impl MutationArgs for Args {
    fn previews(&self) -> bool {
        false
    }
}

impl MutationReply for Reply {
    type After = NoDiagnostics;
}

/// `specforge.infer_session`'s reply: one step's
/// outcome. The three steps share their keys, so it is one struct.
#[derive(Debug, Serialize, Shape)]
pub struct Reply {
    /// `active` (started), `recorded` (a file marked) or the status the
    /// session ended with.
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    entities_produced: Option<Vec<String>>,
}

/// `specforge.infer_session`: one step of an inference session
/// (`specforge_ops::read::infer::session`), which writes `specforge-infer.json`.
pub(crate) struct Tool;

impl ArgumentOf<SessionField> for Tool {
    fn argument(field: SessionField) -> Option<&'static str> {
        match field {
            SessionField::SourceRoots => Some("source_roots"),
            SessionField::SourceFile => Some("source_file"),
            SessionField::SessionId => Some("session_id"),
        }
    }
}

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.infer_session";
    const DESCRIPTION: &'static str =
        "Manage inference sessions: start a new session, mark files as analyzed, or end a session";
    const EFFECT: Effect = Effect::Mutates {
        hints: WriteHints {
            destructive: true,
            idempotent: false,
            open_world: false,
        },
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpInferSessionResult");
    type Args = Args;
    type Input = Project;
    type Returns = Mutation<Reply>;

    fn call(project: ProjectRef<'_>, args: Args) -> Mutation<Reply> {
        let step = step(&args)?;
        let SessionOutcome { recorded, writes } =
            infer::session(&project.view(), step).map_err(Self::failed)?;
        let written = match &recorded {
            Recorded::Marked { entities, .. } => {
                Written::files(writes).with_entities(entities.clone())
            }
            _ => Written::files(writes),
        };
        Ok(Mutated::new(reply(&recorded), written))
    }
}

/// The step `args` asks for, or the refusal of the arguments.
fn step(args: &Args) -> Result<SessionStep<'_>, Box<McpError>> {
    let action = infer::SESSION_ACTION
        .parse(&args.action)
        .map_err(|error| Box::new(McpError::from(error).with_argument("action")))?;
    Ok(match action {
        SessionAction::Start => SessionStep::Start {
            agent: args.agent.as_deref(),
            source_roots: args.source_roots.as_deref(),
        },
        SessionAction::MarkAnalyzed => SessionStep::MarkAnalyzed {
            source_file: required(&args.source_file, "source_file")?,
            entities: &args.entities_produced,
        },
        SessionAction::End => SessionStep::End {
            session_id: required(&args.session_id, "session_id")?,
            status: args.status,
        },
    })
}

/// The argument `value` holds, or the refusal that it is missing.
fn required<'a>(value: &'a Option<String>, name: &str) -> Result<&'a str, Box<McpError>> {
    value.as_deref().ok_or_else(|| {
        let message = format!("Missing required parameter: {name}");
        Box::new(McpError::new(ErrorCode::InvalidInput, message).with_argument(name))
    })
}

/// What a recorded step replies.
fn reply(recorded: &Recorded) -> Reply {
    match recorded {
        Recorded::Started { session_id } => Reply {
            status: SessionStatus::Active.name().to_string(),
            session_id: Some(session_id.to_string()),
            source_file: None,
            entities_produced: None,
        },
        Recorded::Marked {
            source_file,
            entities,
        } => Reply {
            status: "recorded".to_string(),
            session_id: None,
            source_file: Some(source_file.to_string()),
            entities_produced: Some(entities.clone()),
        },
        Recorded::Ended { session_id, status } => Reply {
            status: status.name().to_string(),
            session_id: Some(session_id.to_string()),
            source_file: None,
            entities_produced: None,
        },
    }
}
