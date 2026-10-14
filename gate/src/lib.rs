//! The repository's check list: every check CI, `cargo gate` and the pre-commit hook run, each written once
//! with the run it joins and the paths that trigger it. `cargo gate list` prints it.

pub mod blob_inputs;
mod change_set;
mod checks;
mod doc_links;
pub mod judge;
pub mod layers;
mod repo;
mod root_layout;
mod run;
mod steps;

pub use change_set::{ChangeSet, Pattern};
pub use checks::{CHECKS, Check, Reach, Selection, Trigger, render_list};
pub use repo::{Dependency, DependencyKind, Member, OUTSIDE_WORKSPACE, PROJECTS, Project, Repo};
pub use run::{CARGO_AUDIT, Ctx, Mode, Outcome, Report, TREE_SITTER, Tool, baseline, run};
