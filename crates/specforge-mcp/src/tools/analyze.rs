use crate::args::Arguments;
use crate::reply::Answered;
use crate::target::ProjectRef;
use crate::target::{AnyUnlessCached, Project};
use crate::tool::McpError;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};
use specforge_ops::OpError;
use specforge_ops::analyze::{AnalyzeError, AnalyzeOptions, ReportSource, analyze};

/// `specforge.analyze`'s reply: the document `specforge analyze --format json`
/// prints (`McpAnalyzeResult`).
type Reply = specforge_ops::analyze::AnalyzeDocument;

/// `specforge.analyze`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Analysis pass to run: all, coverage, contracts, or a pass an extension declares (`<extension>:<pass>`)
    #[arg(default = specforge_ops::analyze::EVERY_PASS.to_string())]
    pass: String,
    /// Promote warnings to errors
    strict: bool,
    /// Path to a specforge-report.json for proof-level verdicts
    test_results: Option<String>,
    /// Proof-coverage minimum, in percent (0 to 100): below it the run fails (E048); it needs test results and the coverage pass
    min: Option<f64>,
}

/// `specforge.analyze` — run the analysis passes (coverage, contracts) plus
/// extension-owned compiler passes over the call's project and return
/// structured findings. Extension passes execute in the runtime the
/// project's environment was loaded in (ADR 0015, "The runtime travels with
/// the environment"): the served session's, or the session another project
/// was opened as for this call. With no project served and no `path`, there is nothing to
/// analyze: a no-project refusal (plan 01 D7).
pub(crate) struct Tool;

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.analyze";
    const DESCRIPTION: &'static str = "Run analysis passes (coverage: proof obligations and discharge funnel; contracts: clause symmetry) over the compiled project";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Core,
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpAnalyzeResult");
    const OPEN_VALUES: &'static [&'static str] = &["$.passes[].summary"];
    type Args = Args;
    type Input = Project<AnyUnlessCached>;
    type Returns = Answered<Reply>;

    fn call(project: ProjectRef<'_>, args: Args) -> Answered<Reply> {
        let view = project.view();
        // Without `test_results`, use what `specforge collect` last recorded at
        // the project root, as the CLI does.
        let options = AnalyzeOptions {
            pass: args.pass,
            strict: args.strict,
            report: match args.test_results {
                // A relative path names a file under the call's project, as the
                // paths of `specforge.format` do; an absolute one is itself.
                Some(named) => ReportSource::File(project.root.dir().join(named)),
                None => ReportSource::Recorded,
            },
            min: args.min,
            prove: None,
        };
        match analyze(&view, &options) {
            Ok(outcome) => Ok(outcome.document().into()),
            Err(e) => {
                // The argument an unknown pass names is this surface's spelling.
                let unknown_pass = matches!(e, AnalyzeError::UnknownPass { .. });
                let error = McpError::from(OpError::from(e));
                Err(Box::new(match unknown_pass {
                    true => error.with_argument("pass"),
                    false => error,
                }))
            }
        }
    }
}
