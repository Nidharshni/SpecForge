use super::{
    NoProject, Reach, TargetArgument, TargetError, TargetSpec, WithoutProject, no_project,
};
use crate::containment;
use crate::events::EventSink;
use crate::lifecycle::Revision;
use crate::state::McpState;
use crate::tool::{ErrorCode, McpError};
use serde_json::Value;
use specforge_common::{Diagnostic, ProjectRoot};
use specforge_graph::Graph;
use specforge_ops::analysis::collect::CollectorMachine;
use specforge_ops::asking::Asking;
use specforge_ops::solver::Solver;
use specforge_ops::view::ProjectView;
use specforge_ops::{Caller, Containment, Outside};
use specforge_project::{ProjectSession, SharedRuntime};
use std::path::{Path, PathBuf};

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

/// What MCP lends a call (ADR 0124): `containment` to reach, and nobody to ask. The server never prompts
/// (ADR 0110 D5): an agent can't be asked, so a publisher key change is refused rather than re-pinned.
pub(crate) fn lent(containment: &Containment) -> Caller<'_> {
    Caller::new(containment, Asking::Nobody)
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
    /// The host's collector machine (the server's): what `specforge.collect` runs commands on.
    pub machine: &'a dyn CollectorMachine,
    /// The project view, built once for the call: rooted at the project
    /// root, reporting what the server reports for the project.
    view: ProjectView<'a>,
    /// What a file the call reads or writes must land inside: the project's root
    /// (ADR 0092 D2, ADR 0112 D4).
    containment: Containment,
    /// Whether it is the served project (not one a `path` named).
    served: bool,
}

impl<'a> ProjectRef<'a> {
    /// What the call lends the operation: the project's root (ADR 0112 D4), nobody to ask. A tool
    /// hands it to the operation's request and builds none of its own.
    pub fn caller(&self) -> Caller<'_> {
        lent(&self.containment)
    }

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
    #[expect(
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
                    .contained()
                    .also_reporting(self.state.surfaces().diagnostics());
                let (Some(root), Some(runtime)) = (
                    session.project().environment().project_root(),
                    view.runtime(),
                ) else {
                    return Err(unreachable_no_project());
                };
                Ok(ProjectRef {
                    root,
                    runtime,
                    solver: &*self.state.solver,
                    machine: &*self.state.collector_machine,
                    containment: Containment::within(root.dir()),
                    view,
                    served: true,
                })
            }
            CallTarget::Other(session) => {
                // `McpState::open` opens every project from disk, with a
                // runtime (see above).
                let view = ProjectView::of(session.project()).contained();
                let (Some(root), Some(runtime)) = (
                    session.project().environment().project_root(),
                    view.runtime(),
                ) else {
                    return Err(unreachable_no_project());
                };
                Ok(ProjectRef {
                    root,
                    runtime,
                    solver: &*self.state.solver,
                    machine: &*self.state.collector_machine,
                    containment: Containment::within(root.dir()),
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
                .contained()
                .also_reporting(self.state.surfaces().diagnostics()),
        }
    }

    /// The directory the paths this request names must resolve inside.
    pub(crate) fn bound(&self) -> Containment {
        containment::bound(self.state)
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
            dir = containment::bound(state)
                .resolve(&dir)
                .map_err(TargetError::Outside)?
                .spelled;
            if let Some(root) = state.project_root() {
                let served = Containment::within(root);
                if served.resolve(&dir).is_ok() {
                    let root = containment::landed(root);
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
    let bound = containment::bound(state);
    let named = bound
        .resolve(Path::new(path))
        .map_err(TargetError::Outside)?
        .spelled;
    let root = project_at(&named)?;
    if let Err(outside) = bound.resolve(root.canonical()) {
        // The project the name lands on is outside the bound: named as the call spelled it.
        let outside = Outside::new(&named, outside.landed().to_path_buf(), outside.root());
        return Err(TargetError::Outside(outside));
    }
    let served_root = state.project_root().map(containment::landed);
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
