use crate::notes::Notes;
use crate::outcome::{Exit, OutputFormat, Refusal};
use crate::terminal::{Terminal, out};
use specforge_ops::read::plan::{AgentPlan, PlanOutcome};
use specforge_ops::read::trace::{Ask, TraceChain, TraceLink, TraceOutcome};
use specforge_ops::view::ProjectView;

// `specforge trace`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Entity ID to trace (omit to trace every entity)
    entity: Option<String>,

    /// An agent plan file (an AgentPlan's JSON) to check against the graph instead of tracing
    #[arg(long, value_name = "FILE", value_parser = plan_file, conflicts_with = "entity")]
    plan: Option<AgentPlan>,

    #[command(flatten)]
    project: crate::project::ProjectArg,

    /// Output format: json or human
    #[arg(long, default_value = "json")]
    format: OutputFormat,
}

/// The AgentPlan the file at `path` holds: clap's refusal (exit 2) when it
/// cannot be read or is not one.
fn plan_file(path: &str) -> Result<AgentPlan, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    AgentPlan::parse(&text).map_err(|error| error.message("plan"))
}

impl Args {
    /// `specforge trace [entity]`: the trace operation over the project
    /// compiled at `path`: one entity's chain, or every entity's when none
    /// is named. Expected edges come from the loaded extensions'
    /// registries; the ones a chain lacks are reported as missing. With
    /// `--plan`, the plan check of an agent plan file instead (exit 1 when
    /// the plan fails).
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let project = match self.project.compile() {
            Ok(project) => project,
            Err(error) => return Refusal::of(self.format).report(terminal, &error),
        };
        let view = ProjectView::of(&project);
        // clap refuses an entity with --plan first; the reading is the operation's all the same.
        let target = match Ask::of(self.entity.as_deref(), self.plan.as_ref()) {
            Err(refusal) => {
                return Refusal::of(self.format).report(terminal, &refusal.into_error());
            }
            Ok(Ask::Plan(plan)) => {
                let outcome = specforge_ops::read::plan::check(&view, plan);
                match self.format {
                    OutputFormat::Human => out!(terminal, "{}", plan_text(&outcome)),
                    OutputFormat::Json => terminal.json(&outcome),
                }
                return Exit::of_verdict(outcome.ok);
            }
            Ok(Ask::Trace(target)) => target,
        };
        let traced = match specforge_ops::read::trace::trace(&view, target) {
            Ok(traced) => traced,
            Err(refusal) => return Refusal::of(self.format).report(terminal, &refusal),
        };

        let outcome = Notes::new(terminal, self.format).noticed(traced);
        match self.format {
            OutputFormat::Human => out!(terminal, "{}", trace_text(&outcome)),
            OutputFormat::Json => terminal.json(&outcome),
        }
        Exit::Passed
    }
}

/// The plan check as `specforge trace --plan --format human` prints it.
fn plan_text(outcome: &PlanOutcome) -> String {
    let mut text = String::new();
    if outcome.resolved_entries.is_empty() {
        text.push_str("resolved: none\n");
    } else {
        text.push_str(&format!(
            "resolved: {}\n",
            outcome.resolved_entries.join(", ")
        ));
    }
    if outcome.gaps.is_empty() {
        text.push_str("gaps: none\n");
    } else {
        text.push_str(&format!("gaps ({}):\n", outcome.gaps.len()));
        for gap in &outcome.gaps {
            text.push_str(&format!("  - {}: {}\n", gap.kind.as_str(), gap.context));
        }
    }
    text.push_str(if outcome.ok {
        "plan check: passed\n"
    } else {
        "plan check: failed\n"
    });
    text
}

/// The chains as terminal text, one block per chain, separated by a blank line.
fn trace_text(outcome: &TraceOutcome) -> String {
    outcome
        .chains
        .iter()
        .map(chain_text)
        .collect::<Vec<_>>()
        .join("\n")
}

/// A chain as terminal text: the entity, then its upstream, downstream and missing links, one per line. Missing
/// links are marked `MISSING`.
fn chain_text(chain: &TraceChain) -> String {
    let mut out = format!("{} [{}]\n", chain.entity_id, chain.entity_kind);
    let section = |out: &mut String, name: &str, links: &[TraceLink], arrow: fn(&str) -> String| {
        out.push_str(&format!("  {name}:\n"));
        if links.is_empty() {
            out.push_str("    (none)\n");
        }
        for link in links {
            out.push_str(&format!(
                "    {} {} [{}] (depth {})\n",
                arrow(&link.edge_label),
                link.entity_id,
                link.entity_kind,
                link.depth
            ));
        }
    };
    section(&mut out, "upstream", &chain.upstream, |l| format!("<-{l}-"));
    section(&mut out, "downstream", &chain.downstream, |l| {
        format!("-{l}->")
    });
    if !chain.missing.is_empty() {
        out.push_str("  missing:\n");
        for link in &chain.missing {
            out.push_str(&format!(
                "    MISSING {} -{}-> [{}]{} (depth {})\n",
                link.from,
                link.edge_label,
                link.expected_kind,
                if link.required { " required" } else { "" },
                link.depth
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_ops::read::plan::{PlanGap, PlanGapKind};
    use specforge_ops::read::trace::{MissingLink, TraceLinkStatus};

    fn plan(ok: bool, resolved: &[&str], gaps: Vec<PlanGap>) -> PlanOutcome {
        PlanOutcome {
            ok,
            resolved_entries: resolved.iter().map(ToString::to_string).collect(),
            gaps,
        }
    }

    fn gap(kind: PlanGapKind, context: &str) -> PlanGap {
        PlanGap {
            source: String::new(),
            target: String::new(),
            kind,
            context: context.into(),
        }
    }

    #[test]
    fn plan_text_lists_resolved_entries_gaps_and_the_verdict() {
        let failed = plan(
            false,
            &["b", "c"],
            vec![
                gap(
                    PlanGapKind::UnresolvedEntity,
                    "E003: unresolved entity 'ghost' in plan — not found in graph",
                ),
                gap(
                    PlanGapKind::Ordering,
                    "'b' depends on 'c' (via depends_on), but 'c' appears later in the plan",
                ),
            ],
        );
        assert_eq!(
            plan_text(&failed),
            "resolved: b, c\n\
             gaps (2):\n  \
             - unresolved_entity: E003: unresolved entity 'ghost' in plan — not found in graph\n  \
             - ordering: 'b' depends on 'c' (via depends_on), but 'c' appears later in the plan\n\
             plan check: failed\n"
        );
        assert_eq!(
            plan_text(&plan(true, &["c", "b"], vec![])),
            "resolved: c, b\ngaps: none\nplan check: passed\n"
        );
        assert!(
            plan_text(&plan(
                false,
                &[],
                vec![gap(PlanGapKind::MissingPlanEntry, "x")]
            ))
            .starts_with("resolved: none\ngaps (1):")
        );
    }

    #[test]
    fn chain_text_marks_missing_links() {
        let chain = TraceChain {
            entity_id: "b".into(),
            entity_kind: "behavior".into(),
            upstream: vec![],
            downstream: vec![TraceLink {
                entity_id: "a".into(),
                entity_kind: "feature".into(),
                edge_label: "features".into(),
                depth: 1,
                status: TraceLinkStatus::Resolved,
            }],
            missing: vec![MissingLink {
                from: "b".into(),
                from_kind: "behavior".into(),
                edge_label: "invariants".into(),
                edge_type: None,
                expected_kind: "invariant".into(),
                depth: 1,
                required: false,
                status: TraceLinkStatus::Missing,
            }],
        };
        assert_eq!(
            chain_text(&chain),
            concat!(
                "b [behavior]\n",
                "  upstream:\n",
                "    (none)\n",
                "  downstream:\n",
                "    -features-> a [feature] (depth 1)\n",
                "  missing:\n",
                "    MISSING b -invariants-> [invariant] (depth 1)\n",
            )
        );
    }
}
