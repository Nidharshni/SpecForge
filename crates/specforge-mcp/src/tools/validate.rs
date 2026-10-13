use crate::args::Arguments;
use crate::reply::{Answer, Answered, Text};
use crate::target::ProjectRef;
use crate::target::{AnyUnlessCached, Project};
use crate::tool::McpError;
use crate::tool::{CoreTool, Effect, RefusalArgument, ToolGroup};
use specforge_ops::check::{CheckError, CheckOptions, check, parse_lint_profiles, parse_severity};

/// `specforge.validate`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Only report diagnostics of this severity, after strict promotion (case-insensitive). The verdict in _meta["specforge/check"] still counts everything reported
    #[arg(names = specforge_ops::check::SEVERITY_NAMES)]
    severity_filter: Option<String>,
    /// Promote warnings to errors, before severity_filter applies
    strict: bool,
    /// Extra lint profiles, as `specforge check --lint` takes (inferred: I200/I202 from specforge-infer.json; pedantic is the default and adds nothing)
    #[arg(names = specforge_project::LINT_PROFILE_NAMES)]
    lint: Vec<String>,
}

/// The `_meta` key of validate's verdict: `{ok, errors, warnings, infos,
/// shown}` over everything reported, whatever `severity_filter` shows.
pub const VERDICT_META: &str = "specforge/check";

/// `specforge.validate`: the check operation over what `specforge check`
/// reports for the call's project (the target brought the served project
/// up to date with disk unless `use_cached`, or compiled the project
/// `path` names for this call). Finding errors is a successful call (ADR
/// 0004 D4-a); whether the check passed is the `_meta` verdict. The tool
/// never records the build cache.
pub(crate) struct Tool;

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.validate";
    const DESCRIPTION: &'static str = "Recompile and validate the spec project";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Core,
    };
    type Args = Args;
    type Input = Project<AnyUnlessCached>;
    type Returns = Answered<Text>;
    const REFUSAL_ARGUMENTS: &'static [RefusalArgument] = &[
        RefusalArgument {
            code: specforge_ops::check::UNKNOWN_SEVERITY,
            argument: "severity_filter",
        },
        RefusalArgument {
            code: specforge_ops::check::UNKNOWN_LINT_PROFILE,
            argument: "lint",
        },
    ];

    fn call(project: ProjectRef<'_>, args: Args) -> Answered<Text> {
        let severity = match args.severity_filter.as_deref().map(parse_severity) {
            None => None,
            Some(Ok(severity)) => Some(severity),
            Some(Err(error)) => return Err(refused(error)),
        };
        let lint_profiles = match parse_lint_profiles(&args.lint) {
            Ok(profiles) => profiles,
            Err(error) => return Err(refused(error)),
        };
        let options = CheckOptions {
            strict: args.strict,
            lint_profiles,
            severity,
            record_cache: false,
        };
        let outcome = match check(&project.view(), &options) {
            Ok(outcome) => outcome,
            Err(error) => return Err(refused(error)),
        };
        let shown: Vec<specforge_common::Diagnostic> =
            outcome.shown().into_iter().cloned().collect();
        Ok(
            Answer::new(Text(specforge_common::serialize_diagnostics(&shown)))
                .with_meta(VERDICT_META, outcome.verdict_json()),
        )
    }
}

/// Why validate could not run: an argument it cannot use (`invalid_input`,
/// named by [`Tool::REFUSAL_ARGUMENTS`], with the closest valid name), or no
/// project root.
fn refused(error: CheckError) -> Box<McpError> {
    Box::new(McpError::from(specforge_ops::OpError::from(error)))
}
