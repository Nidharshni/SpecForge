//! The project one MCP call acts on (CONTEXT.md "Call target", ADR 0014).
//!
//! A call's optional `path` and its entry's [`TargetSpec`] (reach) are
//! resolved into one [`CallTarget`] before the handler runs: the served
//! session, brought up to date with disk; another project, opened as a session for this call
//! only; or the directory `init` creates. Handlers read their project as a
//! [`ProjectRef`] and cannot tell which adapter answered it; they never
//! resolve a root or reload anything themselves.

use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use serde_json::Value;
use specforge_common::{Diagnostic, ProjectRoot};
use specforge_graph::Graph;
use specforge_ops::solver::Solver;
use specforge_ops::view::ProjectView;
use specforge_project::{ProjectSession, SharedRuntime};

use crate::containment::{self, Containment, Outside};
use crate::events::EventSink;
use crate::lifecycle::Revision;
use crate::state::McpState;
use crate::tool::{ErrorCode, McpError};

/// Which project a call that reads one may act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    /// The served project only: no listed `path` (its own root is accepted).
    Served,
    /// The served project or, by `path`, another one opened for this call.
    AnyProject,
}

impl Reach {
    /// Whether an entry of this reach names its project by a `path`
    /// argument: the one predicate for the input schema and for what a
    /// no-project refusal tells the client to pass.
    pub(crate) fn takes_path(self) -> bool {
        self == Reach::AnyProject
    }
}

/// Whether a call may change the project the server serves: decided by the
/// revision the request is made under (ADR 0077 D1). The one place that
/// decision is made; [`resolve`] reads it, and the call carries it to
/// `init`'s completion and to the no-project refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Serving {
    /// A handshake revision (2025-03-26 to 2025-11-25): the session is the
    /// client's, so while nothing is served a call's `path` naming a project
    /// is adopted and the directory `init` created is served (ADR 0014 D5).
    Adopts,
    /// The stateless revision (MCP 2026-07-28): nothing a request names is
    /// served. A `path` is opened for its call only, `init` serves nothing,
    /// and what is served is the project at or above the server's root as
    /// it is on disk ([`McpState::follow_root`]).
    Fixed,
}

impl Serving {
    /// The policy of a request made under `revision`.
    pub fn of(revision: Revision) -> Self {
        if revision.is_stateless() {
            Serving::Fixed
        } else {
            Serving::Adopts
        }
    }
}

/// What a call with nothing served and no project named is told it can do:
/// its entry's reach (whether a `path` would fix it) and whether a call may
/// serve a project. The no-project refusal and the empty session carry it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoProject {
    pub reach: Reach,
    pub serving: Serving,
}

/// What a call with nothing served, and no project named, gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WithoutProject {
    /// Its handler reads the empty session (ADR 0014 D7, ADR 0025 D1).
    EmptySession,
    /// The target refuses it as no project, before its arguments are read.
    Refused,
}

/// How a tool, resource or prompt reaches its project: derived from what its
/// handler is given, never declared beside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetSpec {
    /// Reads no project (explain): no `path`.
    Unscoped,
    /// Reads or acts on a project.
    Project {
        reach: Reach,
        without: WithoutProject,
    },
    /// `path` names the directory to create a project in (init): required,
    /// used as given, never inside the served project.
    NewProject,
}

impl TargetSpec {
    /// The served project, brought up to date, read over the empty session
    /// when nothing is served: every core prompt and resource.
    pub const SERVED_VIEW: TargetSpec = TargetSpec::Project {
        reach: Reach::Served,
        without: WithoutProject::EmptySession,
    };

    /// The served project, brought up to date, refused when nothing is
    /// served: every extension tool and resource.
    pub const SERVED_PROJECT: TargetSpec = TargetSpec::Project {
        reach: Reach::Served,
        without: WithoutProject::Refused,
    };

    /// Whether a call answers with nothing served (`Unscoped`, or a project
    /// target read over the empty session).
    pub fn answers_without_project(self) -> bool {
        match self {
            TargetSpec::Unscoped => true,
            TargetSpec::Project { without, .. } => without == WithoutProject::EmptySession,
            TargetSpec::NewProject => false,
        }
    }

    /// Whether the call names its project by a `path` argument
    /// (`AnyProject`, `NewProject`): the input schema and the no-project
    /// hint.
    pub fn takes_path(self) -> bool {
        match self {
            TargetSpec::Unscoped => false,
            TargetSpec::Project { reach, .. } => reach.takes_path(),
            TargetSpec::NewProject => true,
        }
    }

    /// The input-schema properties the target reads from a call: `path` for
    /// a reach that names a project by it (`AnyProject`: "Project root path
    /// (the served project if omitted)"; `NewProject`: "Directory for the
    /// new project, outside the current one"). A tool's listed schema is its own properties
    /// plus these ([`ToolSpec::input_schema`]); its handler never reads
    /// them.
    pub fn properties(self) -> serde_json::Map<String, Value> {
        let mut properties = serde_json::Map::new();
        if self.takes_path() {
            let description = match self {
                TargetSpec::NewProject => "Directory for the new project, outside the current one",
                _ => "Project root path (the served project if omitted)",
            };
            properties.insert(
                TargetArgument::Path.name().into(),
                serde_json::json!({ "type": "string", "description": description }),
            );
        }
        properties
    }

    /// The arguments a call cannot be made without: `["path"]` for
    /// `NewProject`, refused by [`resolve`] before the handler runs.
    pub fn required(self) -> &'static [TargetArgument] {
        match self {
            TargetSpec::NewProject => &[TargetArgument::Path],
            _ => &[],
        }
    }

    /// Every name the target reads from a call, listed or not: `path` for
    /// every target but `Unscoped` (a `Served` entry accepts its own
    /// project's root and refuses another's,
    /// [`TargetError::OtherProjectRefused`]). [`Self::fields`] stays the
    /// listed ones.
    pub fn accepted(self) -> &'static [TargetArgument] {
        if self == TargetSpec::Unscoped {
            &[]
        } else {
            &[TargetArgument::Path]
        }
    }

    /// The names [`Self::properties`] declares, for the schema drift test.
    pub fn fields(self) -> &'static [TargetArgument] {
        if self.takes_path() {
            &[TargetArgument::Path]
        } else {
            &[]
        }
    }
}

/// The project a call acts on, resolved before its handler runs.
pub enum CallTarget {
    /// The served session (in a handshake session, adopted if nothing was served).
    Served,
    /// Another project, opened as a session for this call; the server keeps
    /// serving its own.
    Other(Box<ProjectSession>),
    /// The directory `init` creates a project in, and whether the call may
    /// serve it once created.
    New { dir: PathBuf, serving: Serving },
    /// The tool reads no project.
    Unscoped,
    /// Nothing is served and the call names no project, for a target read
    /// over the empty session (a `Refused` one never resolves to it). The
    /// reach of the entry it was resolved for, and whether the call may
    /// serve a project, say what would fix a read that names something
    /// ([`no_project`]).
    EmptySession(NoProject),
}

/// What every handler reads, from either adapter: the served session or the
/// session opened for the call.
pub struct ProjectRef<'a> {
    /// The project root (where `specforge.json` lives), as the session was opened at it.
    pub root: &'a ProjectRoot,
    /// The runtime its extensions were loaded in, read from the view (its
    /// environment's): every project a call reaches has one (the host's or
    /// the project's own, as `McpState::open` gives it), so an extension
    /// call never finds none (ADR 0017). For MCP's own extension adapters,
    /// which are not operations over the view.
    pub runtime: &'a SharedRuntime,
    /// The host's SMT solver (the server's): what doctor checks.
    pub solver: &'a dyn Solver,
    /// The project view, built once for the call: rooted at the project
    /// root, reporting what the server reports for the project.
    view: ProjectView<'a>,
    /// Whether it is the served project (not one a `path` named).
    served: bool,
}

impl<'a> ProjectRef<'a> {
    /// Whether it is the served project (not one a `path` named): the server
    /// serves what a call installs into it from the next call on.
    pub fn is_served(&self) -> bool {
        self.served
    }

    /// The project's graph, the view's.
    pub fn graph(&self) -> &'a Graph {
        self.view.graph()
    }

    /// Where its `.spec` files are keyed from: spans are relative to it.
    pub fn spec_root(&self) -> &'a Path {
        self.view.env().spec_root()
    }

    /// What every operation over this project reads, rooted at the project
    /// root: the one way MCP builds a project view. Its recorded test
    /// report and coverage are memoized by its owner (the served session,
    /// or the session opened for this call); it reports what `specforge
    /// check` reports, then, for the served project, the contributions of
    /// its extensions MCP does not serve under their names (I017).
    pub fn view(&self) -> ProjectView<'a> {
        self.view
    }

    /// Everything the server reports for this project
    /// ([`ProjectView::reported`]).
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.view.reported()
    }
}

/// An argument the call target reads from a call beside the entry's own
/// (ADR 0033 D3): spelled once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetArgument {
    Path,
}

impl TargetArgument {
    /// Its name in a call's arguments and in the input schema.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Path => "path",
        }
    }
}

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

/// One call: the state and its target.
pub struct Call<'s> {
    state: &'s mut McpState,
    target: CallTarget,
}

impl<'s> Call<'s> {
    /// A call on `target`, resolved by [`resolve`].
    pub(crate) fn new(state: &'s mut McpState, target: CallTarget) -> Self {
        Call { state, target }
    }

    /// The server's event sink: what a handler records goes here (ADR 0057).
    pub(crate) fn events(&mut self) -> &mut dyn EventSink {
        &mut *self.state.events
    }

    /// The session the call reads: the served one, or the one opened for
    /// another project. `None` for a target that is no project.
    fn session(&self) -> Option<&ProjectSession> {
        match &self.target {
            CallTarget::Served => Some(self.state.session()),
            CallTarget::Other(session) => Some(session.as_ref()),
            CallTarget::New { .. } | CallTarget::Unscoped | CallTarget::EmptySession(_) => None,
        }
    }

    /// The project the call acts on. The empty session is
    /// `precondition_failed` (a target that refuses then never resolves to
    /// it), and a target that is no project is an internal error: tools
    /// return it with `?` through `From<McpError> for ToolOutcome`, prompts
    /// as their refusal.
    #[allow(
        clippy::result_large_err,
        reason = "the refusal is the McpError tools and prompts return as is (ADR 0014)"
    )]
    pub(crate) fn project(&self) -> Result<ProjectRef<'_>, McpError> {
        match &self.target {
            CallTarget::Served => {
                let session = self.state.session();
                // `McpState::serve` opens every served project from disk, with
                // a runtime: a session without either is no project (it is
                // unreachable, and a server answers rather than panics).
                let view = ProjectView::of(session.project())
                    .also_reporting(self.state.surfaces().diagnostics());
                let (Some(root), Some(runtime)) =
                    (session.project().project_root(), view.runtime())
                else {
                    return Err(unreachable_no_project());
                };
                Ok(ProjectRef {
                    root,
                    runtime,
                    solver: &*self.state.solver,
                    view,
                    served: true,
                })
            }
            CallTarget::Other(session) => {
                // `McpState::open` opens every project from disk, with a
                // runtime (see above).
                let view = ProjectView::of(session.project());
                let (Some(root), Some(runtime)) =
                    (session.project().project_root(), view.runtime())
                else {
                    return Err(unreachable_no_project());
                };
                Ok(ProjectRef {
                    root,
                    runtime,
                    solver: &*self.state.solver,
                    view,
                    served: false,
                })
            }
            CallTarget::EmptySession(refusal) => Err(no_project(*refusal)),
            // Only a project handler calls this, and its target never
            // resolves to these.
            CallTarget::New { .. } | CallTarget::Unscoped => Err(McpError::new(
                ErrorCode::InternalError,
                "this call has no project",
            )),
        }
    }

    pub(crate) fn target(&self) -> &CallTarget {
        &self.target
    }

    /// The runtime the call's operation reads an extension's declaration in:
    /// its project's (the served session's, or the one another project's
    /// session was opened with); with none, the host's (`McpState::extension_runtime`),
    /// else one of its own with the per-user compile cache. `init` and
    /// `add_extension` pass it to their operation (ADR 0028 D7).
    pub(crate) fn runtime(&self) -> SharedRuntime {
        self.session()
            .and_then(|session| session.runtime().cloned())
            .or_else(|| self.state.extension_runtime.clone())
            .unwrap_or_else(|| {
                std::sync::Arc::new(specforge_component::ComponentRuntime::with_user_cache())
            })
    }

    /// The project view of what the call reads: its project's
    /// ([`ProjectRef::view`]), else, with no project, the empty session's
    /// graph without a root: no recorded report, no schema cache; it
    /// reports what the server reports for the served session
    /// ([`McpState::diagnostics`]). For a handler given the view.
    pub(crate) fn view(&self) -> ProjectView<'_> {
        match self.project() {
            Ok(project) => project.view(),
            Err(_) => ProjectView::of(self.state.session().project())
                .also_reporting(self.state.surfaces().diagnostics()),
        }
    }

    /// The directory `init` creates its project in.
    pub(crate) fn new_project_dir(&self) -> Option<&Path> {
        match &self.target {
            CallTarget::New { dir, .. } => Some(dir),
            _ => None,
        }
    }

    /// The root the files a mutation wrote are named from: the directory
    /// `init` created, else the root of the project the call acts on
    /// ([`crate::mutation::complete`]).
    pub(crate) fn written_root(&self) -> Option<&Path> {
        match &self.target {
            CallTarget::New { dir, .. } => Some(dir),
            CallTarget::Served | CallTarget::Other(_) => self.session()?.project().root(),
            CallTarget::Unscoped | CallTarget::EmptySession(_) => None,
        }
    }

    /// The call wrote its target's files: bring the target up to date now
    /// and return what `specforge check` reports for it. Called only by
    /// [`crate::mutation::complete`] (ADR 0022). The served project is
    /// brought up to date with disk; another project's session is brought up
    /// to date the same way (the server keeps serving its own); the directory `init` created is
    /// served when nothing is, in a handshake session (ADR 0014 D5).
    pub(crate) fn bring_up_to_date(&mut self) -> Vec<Diagnostic> {
        match &mut self.target {
            CallTarget::Served => {
                self.state.ensure_fresh();
                self.state.diagnostics()
            }
            // Exactly what the call wrote is applied, as for the served
            // project; nobody subscribes to another project's changes.
            CallTarget::Other(session) => {
                if let Some(update) = session.ensure_fresh() {
                    crate::state::assert_converged(&update, "another project");
                }
                session.project().diagnostics()
            }
            CallTarget::New { dir, serving } => {
                // A handshake session serves the project init created when
                // none is served (ADR 0014 D5); a stateless request serves
                // nothing it names (ADR 0077 D2): a later request finds it
                // only at the server's root.
                if *serving == Serving::Adopts
                    && self.state.project_root().is_none()
                    && let Ok(root) = ProjectRoot::of(dir)
                {
                    self.state.serve(&root.canonicalized());
                }
                Vec::new()
            }
            CallTarget::Unscoped | CallTarget::EmptySession(_) => Vec::new(),
        }
    }
}

/// Resolve the call's `path` (from `arguments`) under `spec` for a request
/// made under `revision`, then bring a served target up to date with disk.
///
/// A `path` names the project it is in (`ProjectRoot::of`, canonical: the
/// nearest directory at or above it holding `specforge.json`, else the
/// directory itself); one that does not exist is `file_not_found`. A path to a project while
/// nothing is served is adopted when the revision lets the call serve one
/// ([`Serving::Adopts`]): the server serves it, and the call acts on it as
/// the served project. A path naming the served project (or a directory
/// inside it) is the served project; any other, a directory that holds no
/// project included, and any path under [`Serving::Fixed`], is opened as a
/// session for this call only and never served.
/// `init`'s path is used as given (it creates it), may not lie inside the
/// served project, and is served once created only under `Adopts`.
pub fn resolve(
    state: &mut McpState,
    revision: Revision,
    spec: TargetSpec,
    arguments: &Value,
) -> Result<CallTarget, TargetError> {
    // The target's own argument, read by its type as every argument is
    // (ADR 0033 D4): a `path` that is not a string is refused.
    let serving = Serving::of(revision);
    let (reach, without) = match spec {
        TargetSpec::Unscoped => return Ok(CallTarget::Unscoped),
        TargetSpec::NewProject => {
            let Some(path) = argument::<String>(arguments, TargetArgument::Path)? else {
                return Err(TargetError::PathRequired);
            };
            let mut dir = PathBuf::from(path);
            // Inside the request's bound, read against it (ADR 0092 D2, D4):
            // with a project served that is the project, and inside it is
            // refused below, so init creates the first project of a session.
            if let Some(bound) = Containment::of(state) {
                bound.resolve(&dir).map_err(TargetError::Outside)?;
                dir = bound.spelled(&dir);
            }
            if let Some(root) = state.project_root() {
                let (inside, root) = (containment::resolved(&dir), containment::resolved(root));
                if inside.starts_with(&root) {
                    return Err(TargetError::InsideServed { dir, served: root });
                }
            }
            return Ok(CallTarget::New { dir, serving });
        }
        TargetSpec::Project { reach, without } => (reach, without),
    };
    let path = argument::<String>(arguments, TargetArgument::Path)?;
    let path = path.as_deref();
    let no_project = NoProject { reach, serving };
    let served = |state: &mut McpState| {
        if state.project_root().is_none() {
            // Nothing is served and the call names no project: the target
            // says what that call gets.
            return match without {
                WithoutProject::EmptySession => Ok(CallTarget::EmptySession(no_project)),
                WithoutProject::Refused => Err(TargetError::NoProject(no_project)),
            };
        }
        state.ensure_fresh();
        Ok(CallTarget::Served)
    };
    let Some(path) = path else {
        return served(state);
    };
    // The project it names lies inside the request's bound, and so does the
    // path itself, which is read against it (ADR 0092 D2, D3).
    let bound = Containment::of(state);
    let named = match &bound {
        Some(bound) => {
            bound
                .resolve(Path::new(path))
                .map_err(TargetError::Outside)?;
            bound.spelled(Path::new(path))
        }
        None => PathBuf::from(path),
    };
    let root = project_at(&named)?;
    if let Some(bound) = &bound
        && !bound.holds(root.canonical())
    {
        let landed = root.canonical().to_path_buf();
        return Err(TargetError::Outside(bound.outside(&named, landed)));
    }
    let served_root = state.project_root().map(containment::resolved);
    if served_root.is_none()
        && serving == Serving::Adopts
        && reach != Reach::Served
        && root.is_configured()
    {
        // A handshake session serves the project its call names while none
        // is served (ADR 0014 D5), as it is on disk now. A directory that
        // is no project is never served; it is opened for this call below.
        state.serve(&root);
        return Ok(CallTarget::Served);
    }
    if served_root.as_deref() == Some(root.canonical()) {
        return served(state);
    }
    if reach == Reach::Served {
        return Err(TargetError::OtherProjectRefused);
    }
    Ok(CallTarget::Other(Box::new(state.open(&root))))
}

/// The refusal of a served session or a session without a root or runtime:
/// unreachable, since every session is opened from disk with both.
fn unreachable_no_project() -> McpError {
    McpError::new(
        ErrorCode::InternalError,
        "the call's project has no root or runtime",
    )
}

/// The target argument `name` of the call, read as `T`: none when absent
/// or `null`.
fn argument<T: crate::args::Arg>(
    arguments: &Value,
    name: TargetArgument,
) -> Result<Option<T>, TargetError> {
    match arguments.get(name.name()) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            T::read(name.name(), value)
                .map(Some)
                .map_err(|message| TargetError::InvalidArgument {
                    argument: name,
                    message,
                })
        }
    }
}

/// The project `path` names ([`ProjectRoot::of`]), spelled canonically as
/// MCP serves and compares it (ADR 0014 D6).
fn project_at(path: &Path) -> Result<ProjectRoot, TargetError> {
    ProjectRoot::of(path)
        .map(ProjectRoot::canonicalized)
        .map_err(|e| TargetError::PathNotFound(e.path))
}

/// What a core tool's handler is given before its arguments, and so its
/// call target (ADR 0024 A2: the handler's input is the target; ADR 0049).
/// Four implementations: [`Unscoped`], [`View`], [`Project`], [`NewProject`].
pub trait Input {
    /// The value handed to the handler, borrowing the call.
    type Given<'c>;
    /// The call target this input makes.
    const TARGET: TargetSpec;
    /// The value, from the resolved call. A target that refuses a call with
    /// no project never resolves to one, so a refusal here is unreachable.
    fn given<'c>(call: &'c Call<'_>) -> Result<Self::Given<'c>, Box<McpError>>;
}

/// How a handler given a project reaches it: one of two marker types.
pub trait Reaches {
    const REACH: Reach;
}

/// The served project, brought up to date.
pub struct Served;
/// The served project or the one `path` names, brought up to date.
pub struct AnyProject;

impl Reaches for Served {
    const REACH: Reach = Reach::Served;
}
impl Reaches for AnyProject {
    const REACH: Reach = Reach::AnyProject;
}

/// Given nothing: reads no project (explain). `TargetSpec::Unscoped`.
pub struct Unscoped;

/// Given the project view of the project `R` reaches; with nothing served,
/// the empty session's (ADR 0025).
/// `TargetSpec::Project { R::REACH, EmptySession }`.
pub struct View<R = Served>(PhantomData<R>);

/// Given the project `R` reaches, on disk (its root, its runtime, its view):
/// with nothing served and no project named, refused before it runs.
/// `TargetSpec::Project { R::REACH, Refused }`.
pub struct Project<R = Served>(PhantomData<R>);

/// Given the directory init creates and the runtime its extensions'
/// declarations are read in (ADR 0028 D7). `TargetSpec::NewProject`.
pub struct NewProject;

/// What [`NewProject`] hands its handler.
pub struct NewProjectDir<'c> {
    /// The directory, as the call gave it (init creates it).
    pub dir: &'c Path,
    /// The runtime its extensions' declarations are read in: the call's.
    pub runtime: SharedRuntime,
}

impl Input for Unscoped {
    type Given<'c> = ();
    const TARGET: TargetSpec = TargetSpec::Unscoped;

    fn given(_: &Call<'_>) -> Result<(), Box<McpError>> {
        Ok(())
    }
}

impl<R: Reaches> Input for View<R> {
    type Given<'c> = ProjectView<'c>;
    const TARGET: TargetSpec = TargetSpec::Project {
        reach: R::REACH,
        without: WithoutProject::EmptySession,
    };

    fn given<'c>(call: &'c Call<'_>) -> Result<ProjectView<'c>, Box<McpError>> {
        Ok(call.view())
    }
}

impl<R: Reaches> Input for Project<R> {
    type Given<'c> = ProjectRef<'c>;
    const TARGET: TargetSpec = TargetSpec::Project {
        reach: R::REACH,
        without: WithoutProject::Refused,
    };

    fn given<'c>(call: &'c Call<'_>) -> Result<ProjectRef<'c>, Box<McpError>> {
        call.project().map_err(Box::new)
    }
}

impl Input for NewProject {
    type Given<'c> = NewProjectDir<'c>;
    const TARGET: TargetSpec = TargetSpec::NewProject;

    fn given<'c>(call: &'c Call<'_>) -> Result<NewProjectDir<'c>, Box<McpError>> {
        match call.new_project_dir() {
            Some(dir) => Ok(NewProjectDir {
                dir,
                runtime: call.runtime(),
            }),
            // The call target refused a call without a path.
            None => Err(Box::new(McpError::from(TargetError::PathRequired))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[specforge_test_macros::test(
        behavior = "list_mcp_tools",
        verify = "a tool's path is declared once, by its target"
    )]
    fn each_input_makes_its_target() {
        let project = |reach, without| TargetSpec::Project { reach, without };
        assert_eq!(Unscoped::TARGET, TargetSpec::Unscoped);
        assert_eq!(NewProject::TARGET, TargetSpec::NewProject);
        assert_eq!(
            View::<Served>::TARGET,
            project(Reach::Served, WithoutProject::EmptySession)
        );
        assert_eq!(
            View::<AnyProject>::TARGET,
            project(Reach::AnyProject, WithoutProject::EmptySession)
        );
        assert_eq!(
            Project::<Served>::TARGET,
            project(Reach::Served, WithoutProject::Refused)
        );
        assert_eq!(
            Project::<AnyProject>::TARGET,
            project(Reach::AnyProject, WithoutProject::Refused)
        );
    }

    #[specforge_test_macros::test(
        behavior = "list_mcp_tools",
        verify = "a tool's path is declared once, by its target"
    )]
    fn target_arguments_follow_reach() {
        let names =
            |spec: TargetSpec| -> Vec<String> { spec.properties().keys().cloned().collect() };
        let mut specs = vec![TargetSpec::Unscoped, TargetSpec::NewProject];
        for reach in [Reach::Served, Reach::AnyProject] {
            for without in [WithoutProject::EmptySession, WithoutProject::Refused] {
                specs.push(TargetSpec::Project { reach, without });
            }
        }
        for spec in specs {
            let takes_path = match spec {
                TargetSpec::Unscoped => false,
                TargetSpec::NewProject => true,
                TargetSpec::Project { reach, .. } => reach == Reach::AnyProject,
            };
            let mut declared = names(spec);
            declared.sort();
            let mut fields: Vec<String> = spec
                .fields()
                .iter()
                .map(|argument| argument.name().to_string())
                .collect();
            fields.sort();
            assert_eq!(declared, fields, "{spec:?}");
            assert_eq!(
                declared.iter().any(|n| n == TargetArgument::Path.name()),
                takes_path,
                "{spec:?}"
            );
            assert!(
                declared.iter().all(|n| n != "use_cached"),
                "{spec:?} lists use_cached"
            );
            assert_eq!(
                spec.required(),
                if spec == TargetSpec::NewProject {
                    &[TargetArgument::Path][..]
                } else {
                    &[][..]
                },
                "{spec:?}"
            );
            assert_eq!(spec.takes_path(), takes_path, "{spec:?}");
        }
        // `path` reads differently where the call creates its project.
        let described = |spec: TargetSpec| {
            spec.properties()[TargetArgument::Path.name()]["description"]
                .as_str()
                .map(str::to_string)
        };
        assert_eq!(
            described(TargetSpec::NewProject).as_deref(),
            Some("Directory for the new project, outside the current one")
        );
        assert_eq!(
            described(TargetSpec::Project {
                reach: Reach::AnyProject,
                without: WithoutProject::Refused,
            })
            .as_deref(),
            Some("Project root path (the served project if omitted)")
        );
    }

    #[specforge_test_macros::test(
        invariant = "mcp_structured_error_responses",
        verify = "a no-project refusal names path only for an entry that takes one"
    )]
    fn no_project_names_path_only_where_the_entry_takes_one() {
        let adopts = |reach| NoProject {
            reach,
            serving: Serving::Adopts,
        };
        let error = no_project(adopts(Reach::AnyProject));
        assert_eq!(error.code, ErrorCode::PreconditionFailed);
        assert_eq!(error.argument.as_deref(), Some("path"));
        assert!(error.message.contains("pass {\"path\": ...}"));
        // The client can fix it: -32602 where there is no `isError`.
        assert_eq!(error.into_rpc_error().code, -32602);

        let error = no_project(adopts(Reach::Served));
        assert_eq!(error.code, ErrorCode::PreconditionFailed);
        assert_eq!(error.argument, None);
        assert!(!error.message.contains("pass {"), "{}", error.message);
        assert!(error.message.contains("specforge mcp <root>"));
        assert!(error.message.contains("call a tool that takes a `path`"));
        // No params fix it: a server-side -32603.
        assert_eq!(error.into_rpc_error().code, -32603);

        // A stateless call may serve nothing: its hint names no call that would.
        let error = no_project(NoProject {
            reach: Reach::Served,
            serving: Serving::Fixed,
        });
        assert_eq!(error.argument, None);
        assert!(
            error.message.contains("create one at its root"),
            "{}",
            error.message
        );
        assert!(!error.message.contains("call a tool"), "{}", error.message);

        // A read that named something refuses the same way, its hint the
        // entry's own.
        let target = CallTarget::EmptySession(adopts(Reach::Served));
        let refused = without_project(
            &target,
            crate::tool::entity_not_found(&specforge_graph::Graph::new(), "alpha"),
        );
        assert_eq!(refused.code, ErrorCode::PreconditionFailed);
        assert_eq!(refused.argument, None);
        assert_eq!(refused.entity_id.as_deref(), Some("alpha"));
        assert!(!refused.message.contains("pass {"), "{}", refused.message);
        let target = CallTarget::EmptySession(adopts(Reach::AnyProject));
        let refused = without_project(
            &target,
            crate::tool::entity_not_found(&specforge_graph::Graph::new(), "alpha"),
        );
        assert_eq!(refused.argument.as_deref(), Some("path"));
    }

    #[specforge_test_macros::test(
        invariant = "mcp_structured_error_responses",
        verify = "a path that does not exist is a file_not_found error on path"
    )]
    fn each_target_refusal_names_its_argument() {
        let adopts = |reach| NoProject {
            reach,
            serving: Serving::Adopts,
        };
        let path = Some("path");
        for (error, argument) in [
            (TargetError::PathNotFound(PathBuf::from("x")), path),
            (TargetError::OtherProjectRefused, path),
            (TargetError::PathRequired, path),
            (
                TargetError::InsideServed {
                    dir: PathBuf::from("a"),
                    served: PathBuf::from("b"),
                },
                path,
            ),
            (
                TargetError::InvalidArgument {
                    argument: TargetArgument::Path,
                    message: "not a string".into(),
                },
                path,
            ),
            // `no_project` names `path` only where the entry takes one.
            (TargetError::NoProject(adopts(Reach::Served)), None),
            (TargetError::NoProject(adopts(Reach::AnyProject)), path),
        ] {
            let refused = McpError::from(error.clone());
            assert_eq!(refused.argument.as_deref(), argument, "{error:?}");
        }
        assert_eq!(
            TargetError::NoProject(NoProject {
                reach: Reach::AnyProject,
                serving: Serving::Adopts
            })
            .argument(),
            None
        );
    }

    #[test]
    fn a_target_that_needs_a_project_refuses_when_none_is_served() {
        let mut state = McpState::new();
        let revision = Revision::Negotiated(crate::lifecycle::LATEST_PROTOCOL_VERSION);
        let mut refused = |spec| resolve(&mut state, revision, spec, &serde_json::json!({}));

        assert!(matches!(
            refused(TargetSpec::SERVED_PROJECT),
            Err(TargetError::NoProject(NoProject {
                reach: Reach::Served,
                serving: Serving::Adopts
            }))
        ));
        let any = TargetSpec::Project {
            reach: Reach::AnyProject,
            without: WithoutProject::Refused,
        };
        let error = match refused(any) {
            Err(error) => error,
            Ok(_) => panic!("a project target resolved with nothing served"),
        };
        assert_eq!(
            error,
            TargetError::NoProject(NoProject {
                reach: Reach::AnyProject,
                serving: Serving::Adopts
            })
        );
        assert_eq!(McpError::from(error).argument.as_deref(), Some("path"));
        assert!(matches!(
            refused(TargetSpec::SERVED_VIEW),
            Ok(CallTarget::EmptySession(NoProject {
                reach: Reach::Served,
                serving: Serving::Adopts
            }))
        ));
        assert!(matches!(
            refused(TargetSpec::Unscoped),
            Ok(CallTarget::Unscoped)
        ));
    }

    #[test]
    fn serving_follows_the_revision() {
        assert_eq!(
            Serving::of(Revision::Stateless("2026-07-28")),
            Serving::Fixed
        );
        for version in crate::lifecycle::PROTOCOL_VERSIONS {
            assert_eq!(
                Serving::of(Revision::Negotiated(version)),
                Serving::Adopts,
                "{version}"
            );
        }
    }

    #[test]
    fn without_project_names_the_file_from_the_refusal() {
        let target = CallTarget::EmptySession(NoProject {
            reach: Reach::Served,
            serving: Serving::Adopts,
        });

        // The file is what the refusal says it is about, not what its
        // message happens to start with.
        let mut refusal = crate::tool::file_not_found("spec/a.spec");
        refusal.message = "the project holds no such file".to_string();
        let refused = without_project(&target, refusal);
        assert_eq!(refused.code, ErrorCode::PreconditionFailed);
        assert!(
            refused
                .message
                .contains("'spec/a.spec' is no project's file"),
            "{}",
            refused.message
        );

        // A refusal that names no file keeps its own message.
        let unnamed = McpError::new(ErrorCode::FileNotFound, "gone for another reason");
        let refused = without_project(&target, unnamed);
        assert!(
            refused.message.contains("gone for another reason"),
            "{}",
            refused.message
        );
    }
}
