//! `specforge.doctor`: the health report (`specforge_ops::doctor`).

use crate::args::NoArgs;
use crate::reply::Answered;
use crate::target::ProjectRef;
use crate::target::{Project, ServedUnlessCached};
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};

/// `specforge.doctor`'s reply: the document `specforge doctor --format json`
/// prints, without the user's credential health (`McpDoctorReport`).
type Reply = specforge_ops::doctor::DoctorDocument;

pub(crate) struct Tool;

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.doctor";
    const DESCRIPTION: &'static str = "Run health checks";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Management,
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpDoctorReport");
    const OPEN_VALUES: &'static [&'static str] = &["$.enhancements"];
    type Args = NoArgs;
    type Input = Project<ServedUnlessCached>;
    type Returns = Answered<Reply>;

    fn call(project: ProjectRef<'_>, _args: NoArgs) -> Answered<Reply> {
        // The target brought the project up to date with disk unless the
        // caller opted into the last compile (`use_cached`, ADR 0004 D3-d).
        // The same report `specforge doctor` prints (`DoctorReport::document`), as
        // the spec's McpDoctorReport. Credential health is the user's, not the
        // project's: only the CLI reports it.
        let report = specforge_ops::doctor::diagnose(&project.view(), project.solver);
        Ok(report.document().into())
    }
}
