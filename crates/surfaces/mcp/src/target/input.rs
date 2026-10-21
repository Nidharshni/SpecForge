use super::{Call, ProjectRef, Reach, TargetError, TargetSpec, WithoutProject};
use crate::tool::McpError;
use specforge_ops::view::ProjectView;
use specforge_ops::{Caller, Containment};
use specforge_project::SharedRuntime;
use std::marker::PhantomData;
use std::path::Path;

/// What a core tool's handler is given before its arguments, and so its
/// call target (ADR 0024 A2: the handler's input is the target; ADR 0049).
/// Five implementations: [`Unscoped`], [`View`], [`OutputView`], [`Project`], [`NewProject`].
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

/// Given the served project's view (with nothing served, the empty session's, as [`View`]) and what
/// the call lends beside it: a view tool that writes an output file inside the project (render).
/// `TargetSpec::Project { Served, EmptySession }`.
pub struct OutputView;

/// What [`OutputView`] hands its handler.
pub struct ViewToWrite<'c> {
    pub view: ProjectView<'c>,
}

impl ViewToWrite<'_> {
    /// What the call lends the operation: the call's one containment, the view's (ADR 0092 D7,
    /// ADR 0183), nobody to ask.
    pub fn caller(&self) -> Caller<'_> {
        super::call::lent(self.view.containment())
    }
}

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
    /// Where a local file the call installs from must lie: the request's bound.
    pub(crate) containment: &'c Containment,
}

impl NewProjectDir<'_> {
    /// What the call lends init: the call's containment (ADR 0092 D7, ADR 0183), nobody to ask.
    pub fn caller(&self) -> Caller<'_> {
        super::call::lent(self.containment)
    }
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

impl Input for OutputView {
    type Given<'c> = ViewToWrite<'c>;
    const TARGET: TargetSpec = TargetSpec::Project {
        reach: Reach::Served,
        without: WithoutProject::EmptySession,
    };

    fn given<'c>(call: &'c Call<'_>) -> Result<ViewToWrite<'c>, Box<McpError>> {
        Ok(ViewToWrite { view: call.view() })
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
                containment: call.containment(),
            }),
            // The call target refused a call without a path.
            None => Err(Box::new(McpError::from(TargetError::PathRequired))),
        }
    }
}
