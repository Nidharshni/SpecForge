//! `specforge.collect`: record which entities the tests prove (`specforge_ops::analysis::collect`).

use crate::target::{AnyProject, Project};
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup, WriteHints};

use crate::args::Arguments;
use crate::refusal::ArgumentOf;
use crate::reply::Answered;
use crate::target::ProjectRef;
use specforge_ops::analysis::collect::CollectField;

/// `specforge.collect`'s reply: the document `specforge collect --format
/// json` prints (`McpCollectResult`).
type Reply = specforge_ops::analysis::collect::CollectDocument;

/// `specforge.collect`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Collector name (e.g. cargo-test); detected from project files if omitted
    runner: Option<String>,
    /// Run the test command first; it must have been approved with `specforge collect` in a terminal (otherwise the existing report is parsed)
    run: bool,
}

pub(crate) struct Tool;

impl ArgumentOf<CollectField> for Tool {
    fn argument(field: CollectField) -> Option<&'static str> {
        match field {
            CollectField::Runner => Some("runner"),
        }
    }
}

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.collect";
    const DESCRIPTION: &'static str = "Record which entities the project's tests prove, from the test runner's report (runs the runner only with run: true and prior approval)";
    const EFFECT: Effect = Effect::WritesOutput {
        group: ToolGroup::Management,
        hints: WriteHints {
            destructive: true,
            idempotent: false,
            open_world: true,
        },
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpCollectResult");
    type Args = Args;
    type Input = Project<AnyProject>;
    type Returns = Answered<Reply>;

    fn call(project: ProjectRef<'_>, args: Args) -> Answered<Reply> {
        use specforge_ops::analysis::collect::{self, Mode, Request, RunnerOutput};
        use specforge_ops::asking::Asking;

        let runner = args.runner.as_deref();
        let run = args.run;

        // Tests map to the entities on disk now: the target brought the served
        // project up to date, or compiled the project `path` names for this
        // call, in the runtime its environment was loaded in.
        let request = Request {
            runner,
            mode: if run {
                // The server owns stdio: the runner's output is discarded.
                Mode::Run(RunnerOutput::Discard)
            } else {
                Mode::NoRun
            },
            // The server never prompts: a command runs only if the user already
            // approved it for this project with `specforge collect` in a terminal.
            asking: Asking::Nobody,
            announce: &mut |_, _| {},
        };
        collect::collect(&project.view(), request)
            .map(|outcome| outcome.document().into())
            .map_err(|refusal| Box::new(Self::refused(refusal)))
    }
}
