use super::{CallTarget, NoProject, Serving, TargetArgument};
use crate::containment::Outside;
use crate::tool::{ErrorCode, McpError};
use std::path::PathBuf;

/// Why a call's target could not be resolved: the call fails with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetError {
    /// `path` names nothing on disk: `file_not_found` on argument `path`.
    PathNotFound(PathBuf),
    /// A tool that serves only the served project was given another
    /// project's `path`: `invalid_input` on argument `path` (the dispatcher
    /// names the tool).
    OtherProjectRefused,
    /// `init` was asked to create a project inside the served one:
    /// `conflict`.
    InsideServed { dir: PathBuf, served: PathBuf },
    /// `path` resolves outside the served project (while none is served,
    /// the server's root): `invalid_input` on argument `path` (ADR 0092).
    Outside(Outside),
    /// `init` was called without the `path` it creates: `invalid_input`
    /// "Missing required parameter: path" on argument `path`.
    PathRequired,
    /// `path` is not a string: `invalid_input` on that argument, as every
    /// argument of the wrong type is refused.
    InvalidArgument {
        argument: TargetArgument,
        message: String,
    },
    /// Nothing is served and the call names no project, for a target that
    /// refuses then: `no_project(no_project)`.
    NoProject(NoProject),
}

impl TargetError {
    /// The target argument the refusal is about: `path` for every variant
    /// but `InvalidArgument` (its own) and `NoProject` (whose refusal names
    /// `path` only where the entry takes one, [`no_project`]).
    pub fn argument(&self) -> Option<TargetArgument> {
        match self {
            TargetError::PathNotFound(_)
            | TargetError::OtherProjectRefused
            | TargetError::InsideServed { .. }
            | TargetError::Outside(_)
            | TargetError::PathRequired => Some(TargetArgument::Path),
            TargetError::InvalidArgument { argument, .. } => Some(*argument),
            TargetError::NoProject(_) => None,
        }
    }
}

impl From<TargetError> for McpError {
    fn from(error: TargetError) -> Self {
        let argument = error.argument();
        let refused = match error {
            TargetError::PathNotFound(path) => McpError::new(
                ErrorCode::FileNotFound,
                format!("path not found: {}", path.display()),
            ),
            TargetError::OtherProjectRefused => McpError::new(
                ErrorCode::InvalidInput,
                "this tool acts on the project the server serves; path cannot name another project",
            ),
            TargetError::Outside(outside) => outside.refusal(TargetArgument::Path.name()),
            TargetError::PathRequired => {
                McpError::new(ErrorCode::InvalidInput, "Missing required parameter: path")
            }
            TargetError::InvalidArgument { message, .. } => {
                McpError::new(ErrorCode::InvalidInput, message)
            }
            TargetError::NoProject(refusal) => no_project(refusal),
            TargetError::InsideServed { dir, served } => McpError::new(
                ErrorCode::Conflict,
                format!(
                    "{} is inside the current project at {}",
                    dir.display(),
                    served.display()
                ),
            ),
        };
        match argument {
            Some(argument) => refused.with_argument(argument.name()),
            None => refused,
        }
    }
}

/// What a client can do about a missing project, ending every no-project
/// refusal: an entry that takes a `path` can pass one (opened for the call,
/// or adopted in a handshake session); one that serves only the served
/// project says to start the server in a project, and, where a call may
/// serve one, to call a tool that takes a `path` first.
fn serve_a_project(no_project: NoProject) -> &'static str {
    match (no_project.reach.takes_path(), no_project.serving) {
        (true, _) => "pass {\"path\": ...} or start the server in a project",
        (false, Serving::Adopts) => {
            "start the server in a project (`specforge mcp <root>`), or call a tool that takes a `path` first (ADR 0014 D5)"
        }
        (false, Serving::Fixed) => {
            "start the server in a project (`specforge mcp <root>`), or create one at its root"
        }
    }
}

/// The refusal of a handler that needs a project when none is served. It
/// names `path` only where the entry takes one: the client can fix it, so a
/// surface without `isError` answers it -32602; for any other entry no
/// params fix it, and it is a -32603 (ADR 0024 D5). Its hint never names a
/// call that would serve a project unless the call may
/// ([`Serving::Adopts`]).
pub fn no_project(no_project: NoProject) -> McpError {
    let error = McpError::new(
        ErrorCode::PreconditionFailed,
        format!("no project is served: {}", serve_a_project(no_project)),
    );
    if no_project.reach.takes_path() {
        error.with_argument(TargetArgument::Path.name())
    } else {
        error
    }
}

/// A refusal that names something of the project (a file, an entity), as
/// the call's target makes it: with nothing served ([`CallTarget::EmptySession`])
/// a file or an entity is not "not found", there is no project to look in,
/// so the refusal is [`no_project`] (`precondition_failed`), its message
/// leading with what was asked, `entity_id` kept and `argument` dropped. Any
/// other refusal, and every refusal of a call that has a project, is
/// returned as it is. The one place that rule is applied: the dispatchers of
/// tools, prompts and resources call it on what their handler refused with
/// (ADR 0025).
pub(crate) fn without_project(target: &CallTarget, error: McpError) -> McpError {
    let CallTarget::EmptySession(no_project) = *target else {
        return error;
    };
    let asked = match error.code {
        ErrorCode::FileNotFound => match &error.file {
            Some(file) => format!("'{file}' is no project's file"),
            None => error.message.clone(),
        },
        ErrorCode::EntityNotFound => {
            let entity = error.entity_id.as_deref().unwrap_or("that entity");
            format!("entity '{entity}' is in no project")
        }
        _ => return error,
    };
    let mut refused = McpError::new(
        ErrorCode::PreconditionFailed,
        format!(
            "no project is served, so {asked}: {}",
            serve_a_project(no_project)
        ),
    );
    if no_project.reach.takes_path() {
        refused.argument = Some(TargetArgument::Path.name().to_string());
    }
    refused.entity_id = error.entity_id;
    refused.tool = error.tool;
    refused.prompt = error.prompt;
    refused.uri = error.uri;
    refused.reported = error.reported;
    refused
}

/// [`without_project`], in place.
pub(crate) fn refuse_without_project(target: &CallTarget, error: &mut McpError) {
    let refused = std::mem::replace(
        error,
        McpError::new(ErrorCode::InternalError, String::new()),
    );
    *error = without_project(target, refused);
}
