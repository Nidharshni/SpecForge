//! `specforge.init`: create a project (`specforge_ops::init`).

use std::path::Path;

use crate::args::Arguments;
use crate::mutation::{
    Mutated, Mutation, MutationArgs, MutationEvent, MutationReply, NoDiagnostics, Written,
};

/// `specforge.init`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Project name (defaults to the directory name)
    name: Option<String>,
    /// Project version
    #[arg(default = specforge_ops::init::DEFAULT_VERSION.to_string())]
    version: String,
    /// Builtin extensions to enable (e.g. @specforge/software) and local .wasm files to install
    extensions: Vec<String>,
}

impl MutationArgs for Args {
    fn previews(&self) -> bool {
        false
    }
}

impl MutationReply for Reply {
    type After = NoDiagnostics;
}

/// `specforge.init`'s reply (`McpInitResult`): the init document `specforge
/// init --format json` prints (ADR 0052).
pub use specforge_ops::init::InitDocument as Reply;

pub(crate) fn call(
    path: &Path,
    runtime: &specforge_project::SharedRuntime,
    args: Args,
) -> Mutation<Reply> {
    use specforge_ops::init;

    // The directory the target names (as given: init creates it).
    let extensions = &args.extensions;

    // The scaffold `specforge init` writes. The call target refused a
    // directory inside the served project before this ran.
    let request = init::Request {
        dir: path,
        name: args.name.as_deref(),
        version: &args.version,
        extensions,
    };
    let outcome =
        init::plan(&request, runtime.as_ref()).and_then(|plan| init::apply(path, plan))?;
    let reply = outcome.document();
    // With no project served, the server serves the one it created (ADR
    // 0014 D5): `mutation::complete` does, once it wrote.
    let event = MutationEvent::ProjectInitialized {
        project_name: outcome.name.clone(),
        extension_count: outcome.extensions.len(),
        spec_file_path: init::STARTER_FILE.to_string(),
    };
    Ok(Mutated::new(
        reply,
        Written::files(outcome.writes).with_event(event),
    ))
}
