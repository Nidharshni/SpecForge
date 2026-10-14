//! The project one MCP call acts on (CONTEXT.md "Call target", ADR 0014).
//!
//! A call's optional `path` and its entry's [`TargetSpec`] (reach) are
//! resolved into one [`CallTarget`] before the handler runs: the served
//! session, brought up to date with disk; another project, opened as a session for this call
//! only; or the directory `init` creates. Handlers read their project as a
//! [`ProjectRef`] and cannot tell which adapter answered it; they never
//! resolve a root or reload anything themselves.

use serde_json::Value;

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

mod call;
mod error;
mod input;
#[cfg(test)]
mod tests;

pub use call::{Call, CallTarget, ProjectRef, Serving, resolve};
pub(crate) use error::refuse_without_project;
pub use error::{TargetError, no_project};
pub use input::{
    AnyProject, Input, NewProject, NewProjectDir, Project, Reaches, Served, Unscoped, View,
};
