//! The project a command's path is in (ADR 0053): every core command and
//! extension command opens its project here.

use std::path::Path;
use std::sync::Arc;

use specforge_common::ProjectRoot;
use specforge_component::ComponentRuntime;
use specforge_ops::OpError;
use specforge_project::{CompiledProject, SharedRuntime};

/// The project `path` is in; `file_not_found` when it does not exist.
pub(crate) fn root(path: &Path) -> Result<ProjectRoot, OpError> {
    ProjectRoot::of(path).map_err(OpError::from)
}

/// The project `path` is in, compiled with its extensions loaded into the
/// component runtime (with the per-user compile cache): the one compile
/// every command holds to build a project view
/// (`specforge_ops::view::ProjectView::of`). The compiled project's
/// environment holds that runtime, which every operation over its view
/// calls extensions in.
pub(crate) fn compile(path: &Path) -> Result<CompiledProject, OpError> {
    Ok(compile_at(&root(path)?))
}

/// [`compile`] with the extensions in `runtime` (`check`'s tests give theirs).
pub(crate) fn compile_in(path: &Path, runtime: SharedRuntime) -> Result<CompiledProject, OpError> {
    Ok(CompiledProject::compile(&root(path)?, Some(runtime)))
}

/// [`compile`] for a root already resolved.
pub(crate) fn compile_at(root: &ProjectRoot) -> CompiledProject {
    CompiledProject::compile(root, Some(Arc::new(ComponentRuntime::with_user_cache())))
}
