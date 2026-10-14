//! `specforge.init`: create a project (`specforge_ops::init`).

use crate::refusal::ArgumentOf;
use crate::target::TargetArgument;
use crate::target::{NewProject, NewProjectDir};
use crate::tool::{CoreTool, Effect, ReplySpec, WriteHints};
use specforge_ops::init::InitField;

use crate::args::Arguments;
use crate::events::ProjectInitialized;
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

/// `specforge.init`'s reply: the init document `specforge
/// init --format json` prints (ADR 0052).
type Reply = specforge_ops::init::InitDocument;

pub(crate) struct Tool;

impl ArgumentOf<InitField> for Tool {
    fn argument(field: InitField) -> Option<&'static str> {
        match field {
            InitField::Dir => Some(TargetArgument::Path.name()),
            InitField::Name => Some("name"),
            InitField::Extensions => Some("extensions"),
        }
    }
}

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.init";
    const DESCRIPTION: &'static str = "Initialize a new SpecForge project";
    const EFFECT: Effect = Effect::Mutates {
        hints: WriteHints {
            destructive: false,
            idempotent: true,
            open_world: false,
        },
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpInitResult");
    type Args = Args;
    type Input = NewProject;
    type Returns = Mutation<Reply>;

    fn call(new: NewProjectDir<'_>, args: Args) -> Mutation<Reply> {
        let (path, runtime) = (new.dir, &new.runtime);
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
        let outcome = init::init(&request, runtime.as_ref()).map_err(Self::failed)?;
        let reply = outcome.document();
        // In a handshake session with no project served, the server serves
        // the one it created (ADR 0014 D5): `mutation::complete` does, once
        // it wrote; a stateless request serves nothing it names.
        let event = MutationEvent::ProjectInitialized(ProjectInitialized {
            project_name: outcome.name.clone(),
            extension_count: outcome.extensions.len(),
            spec_file_path: init::STARTER_FILE.to_string(),
        });
        Ok(Mutated::new(
            reply,
            Written::files(outcome.writes).with_event(event),
        ))
    }
}
