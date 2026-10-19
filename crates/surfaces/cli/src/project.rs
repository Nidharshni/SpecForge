//! The project a command acts on (ADR 0053): `--path`, any path inside it.
//! Every command that reads a project flattens [`ProjectArg`]; the extension
//! command line takes the same option ([`ProjectArg::arg`]) and finds it
//! before parsing ([`ProjectArg::scan`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use specforge_common::ProjectRoot;
use specforge_component::ComponentRuntime;
use specforge_ops::OpError;
use specforge_project::{CompiledProject, SharedRuntime};

/// The help of `--path`.
const PATH_HELP: &str = "A path inside the project (the nearest directory at or above it holding specforge.json; none: the directory itself, with the default configuration)";

/// `--path <PATH>`: the project a command acts on, named by any path inside
/// it, `.` by default. Flatten it (`#[command(flatten)] project: ProjectArg`)
/// into every command that reads a project.
#[derive(clap::Args, Debug, Clone)]
pub(crate) struct ProjectArg {
    #[arg(long, default_value = ".", help = PATH_HELP)]
    path: PathBuf,
}

impl ProjectArg {
    /// The option's id and long name. The extension protocol reserves it
    /// (`specforge_protocol_types::command_args::HOST_OPTIONS`).
    pub(crate) const NAME: &'static str = "path";

    /// The option as clap declares it: the one declaration, the derive's,
    /// for a command line built at run time (an extension command's).
    pub(crate) fn arg() -> clap::Arg {
        use clap::Args as _;
        Self::augment_args(clap::Command::new("project"))
            .get_arguments()
            .find(|argument| argument.get_id() == Self::NAME)
            .cloned()
            .expect("ProjectArg declares --path")
    }

    /// The option among raw `args` (`--path <dir>` or `--path=<dir>`), else
    /// `.`: an extension command's project, needed to build the command line
    /// that parses `args`.
    pub(crate) fn scan(args: &[String]) -> Self {
        Self::named(".").overridden_by(args)
    }

    /// The project `args` name with `--path` (`--path <dir>` or `--path=<dir>`), else this one: a command that
    /// parses its own options after it has read the project (`new`'s field flags; an extension command's).
    pub(crate) fn overridden_by(&self, args: &[String]) -> Self {
        let flag = format!("--{}", Self::NAME);
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            if *arg == flag {
                if let Some(value) = args.next() {
                    return Self::named(value);
                }
            } else if let Some(value) = arg.strip_prefix(&format!("{flag}=")) {
                return Self::named(value);
            }
        }
        self.clone()
    }

    fn named(value: &str) -> Self {
        Self {
            path: PathBuf::from(value),
        }
    }

    /// The path as given, not resolved: `publish`'s default extension and
    /// `format --stdin`'s directory.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// The project the path is in; `file_not_found` when it does not exist.
    pub(crate) fn root(&self) -> Result<ProjectRoot, OpError> {
        ProjectRoot::of(&self.path).map_err(OpError::from)
    }

    /// The project, compiled with its extensions loaded into the component
    /// runtime (with the per-user compile cache): the one compile every
    /// command holds to build a project view
    /// (`specforge_ops::view::ProjectView::of`). The compiled project's
    /// environment holds that runtime, which every operation over its view
    /// calls extensions in.
    pub(crate) fn compile(&self) -> Result<CompiledProject, OpError> {
        Ok(compile_at(&self.root()?))
    }

    /// [`Self::compile`] with the extensions in `runtime` (`check`'s tests give theirs).
    pub(crate) fn compile_in(&self, runtime: SharedRuntime) -> Result<CompiledProject, OpError> {
        Ok(CompiledProject::compile(&self.root()?, Some(runtime)))
    }

    /// The project at `path` (tests that hold a path already).
    #[cfg(test)]
    pub(crate) fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

/// [`ProjectArg::compile`] for a root already resolved.
pub(crate) fn compile_at(root: &ProjectRoot) -> CompiledProject {
    CompiledProject::compile(root, Some(Arc::new(ComponentRuntime::with_user_cache())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn scan_finds_the_option_before_parsing() {
        assert_eq!(
            ProjectArg::scan(&args(&["features"])).path(),
            Path::new(".")
        );
        assert_eq!(
            ProjectArg::scan(&args(&["features", "--path", "/p", "--limit", "1"])).path(),
            Path::new("/p")
        );
        assert_eq!(
            ProjectArg::scan(&args(&["--path=/q", "features"])).path(),
            Path::new("/q")
        );
        assert_eq!(
            ProjectArg::scan(&args(&["features", "--path"])).path(),
            Path::new(".")
        );
        assert_eq!(
            ProjectArg::named("/a").overridden_by(&args(&["x"])).path(),
            Path::new("/a")
        );
        assert_eq!(
            ProjectArg::named("/a")
                .overridden_by(&args(&["x", "--path", "/b"]))
                .path(),
            Path::new("/b")
        );
    }
}
