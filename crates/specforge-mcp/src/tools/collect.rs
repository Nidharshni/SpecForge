//! `specforge.collect`: record which entities the tests prove (`specforge_ops::collect`).

use crate::target::{AnyProject, Project};
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup, WriteHints};
use specforge_common::codes;

use crate::args::Arguments;
use crate::refusal::ArgumentOf;
use crate::reply::Answered;
use crate::target::ProjectRef;
use specforge_ops::collect::CollectField;

/// `specforge.collect`'s reply: the document `specforge collect --format
/// json` prints (`McpCollectResult`).
type Reply = specforge_ops::collect::CollectDocument;

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
        use specforge_ops::collect::{self, Consent, Mode, Request, RunnerOutput};

        let runner = args.runner.as_deref().filter(|r| *r != "auto");
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
            consent: Consent::Approved,
            announce: &mut |_, _| {},
        };
        match collect::collect(&project.view(), request) {
            Ok(outcome) => Ok(outcome.document().into()),
            Err(mut refusal) => {
                if refusal.error.is(codes::E059) {
                    refusal.error.message = format!(
                        "the test command isn't approved for this project; run `specforge collect` \
                     in a terminal once to approve it ({})",
                        refusal.error.message
                    );
                }
                Err(Box::new(Self::refused(refusal)))
            }
        }
    }
}
