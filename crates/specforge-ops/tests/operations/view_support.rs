//! Projects for the read operations' tests: a graph parsed from source,
//! registries declaring what made-up extensions declare, and a root
//! directory to record a test report in.

use std::path::Path;
use std::sync::Arc;

use specforge_common::{ConfigRead, Diagnostic};
use specforge_extension_sdk::PeerDependency;
use specforge_extension_sdk::prelude::*;
use specforge_graph::Graph;
use specforge_ops::plan::{PlanGapKind, PlanOutcome};
use specforge_ops::stats::Stats;
use specforge_ops::trace::{Target, TraceChain};
use specforge_ops::view::ProjectView;
use specforge_project::coverage::RecordedCoverage;
use specforge_project::{Environment, ExtensionLoad, Loaded, SharedRuntime};
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::{RegistryBuild, build_registries};
use specforge_wasm::testing::InProcessRuntime;
use tempfile::TempDir;

/// A project of five kinds of connectedness (plan 04): `hub` is referenced
/// by `linked`; `lonely` and `alone` reference nothing and nothing references
/// them; `dangling`'s only reference does not resolve; `selfish` references
/// only itself.
pub const R1_SOURCE: &str = "\
feature hub \"Hub\" {
}

behavior linked \"Linked\" {
  features [hub]
  verify unit \"linked works\"
}

behavior lonely \"Lonely\" {
  verify unit \"lonely works\"
}

behavior dangling \"Dangling\" {
  needs [nowhere]
  verify unit \"dangling works\"
}

feature selfish \"Selfish\" {
  depends_on [selfish]
}

feature alone \"Alone\" {
}
";

/// Registries whose `obligated` kinds are testable and must declare
/// obligations (W004 targets each), and whose `free` kinds are testable
/// with no such rule (governance kinds).
pub fn registries(obligated: &[&str], free: &[&str]) -> RegistryBuild {
    registries_with(obligated, free, |_| {})
}

/// [`registries`], with `extra` adding more to `@t/soft`, the extension
/// that declares the kinds.
pub fn registries_with(
    obligated: &[&str],
    free: &[&str],
    extra: impl FnOnce(&mut ContributionsBuilder),
) -> RegistryBuild {
    let mut soft = ContributionsBuilder::new(ExtensionMeta::new("@t/soft", "1.0.0"));
    for name in obligated.iter().chain(free) {
        soft.kind(name, |k| {
            k.keyword(name).testable(true).supports_verify(true);
        });
    }
    extra(&mut soft);
    // The testing extension owns W004: one rule per obligated kind.
    let mut testing = ContributionsBuilder::new(ExtensionMeta::new("@specforge/testing", "1.0.0"));
    testing.meta.peer_dependencies = vec![PeerDependency {
        name: "@t/soft".into(),
        version: "^1".into(),
        optional: false,
    }];
    for name in obligated {
        testing.rule("W004", |r| {
            r.severity(ValidationSeverity::Warning)
                .message_template("{kind} '{id}' is testable but declares no verify obligations")
                .check(CheckKind::NoVerifyStatements)
                .target_kind(name)
                .field("verify");
        });
    }
    let build = build_registries(vec![soft.declaration(), testing.declaration()]);
    assert!(
        build.diagnostics().next().is_none(),
        "{:?}",
        build.diagnostics().collect::<Vec<_>>()
    );
    build
}

/// A project on disk: its graph, the environment it was compiled in (the
/// default config and `registries`) and its coverage memo.
pub struct Project {
    pub dir: TempDir,
    pub graph: Graph,
    pub env: Environment,
    /// Made over the graph and environment as they are at the first view.
    recorded: std::sync::OnceLock<RecordedCoverage>,
}

impl Project {
    /// The project whose one file is `source`.
    pub fn new(source: &str, registries: RegistryBuild) -> Self {
        let (graph, _) =
            specforge_graph::build_graph(&[specforge_parser::parse(source, "main.spec")]);
        Self::of_graph(graph, registries)
    }

    pub fn of_graph(graph: Graph, registries: RegistryBuild) -> Self {
        Project {
            dir: TempDir::new().unwrap(),
            graph,
            env: Environment::with_registries(registries, None),
            recorded: std::sync::OnceLock::new(),
        }
    }

    /// The project whose one file is `source`, in the environment
    /// `assemble` makes at its root (a temp directory).
    pub fn assembled(source: &str, assemble: impl FnOnce(&Path) -> Environment) -> Self {
        let (graph, _) =
            specforge_graph::build_graph(&[specforge_parser::parse(source, "main.spec")]);
        let dir = TempDir::new().unwrap();
        let env = assemble(dir.path());
        Project {
            dir,
            graph,
            env,
            recorded: std::sync::OnceLock::new(),
        }
    }

    /// The project whose one file is `source`, over `registries`, whose
    /// extension calls go to `runtime`.
    pub fn in_runtime(source: &str, registries: RegistryBuild, runtime: SharedRuntime) -> Self {
        let (graph, _) =
            specforge_graph::build_graph(&[specforge_parser::parse(source, "main.spec")]);
        Project {
            dir: TempDir::new().unwrap(),
            graph,
            env: Environment::with_registries(registries, Some(runtime)),
            recorded: std::sync::OnceLock::new(),
        }
    }

    pub fn view(&self) -> ProjectView<'_> {
        ProjectView::new(
            &self.graph,
            &self.env,
            Some(self.dir.path()),
            self.recorded
                .get_or_init(|| RecordedCoverage::over(&self.graph, &self.env)),
        )
    }

    /// Record, as `specforge collect` would, one passing test per
    /// `(entity, obligation)`.
    pub fn record_passing(&self, proofs: &[(&str, &str)]) {
        let mut results = serde_json::Map::new();
        for (entity, obligation) in proofs {
            results.insert(
                entity.to_string(),
                serde_json::json!({"tests": [
                    {"name": format!("{entity}_test"), "status": "passed", "verify": obligation}
                ]}),
            );
        }
        std::fs::write(
            self.dir.path().join("specforge-report.json"),
            serde_json::json!({"runner": "fixture", "results": results}).to_string(),
        )
        .unwrap();
    }
}

/// `config` read as the `specforge.json` at `root` (not written to disk).
pub fn config_read(root: &Path, config: serde_json::Value) -> ConfigRead {
    specforge_common::parse_project_config(&root.join("specforge.json"), &config.to_string())
}

/// What a load in an in-process runtime serving nothing gave:
/// `declarations`, nothing enabled.
pub fn loaded(declarations: Vec<ExtensionDeclaration>) -> ExtensionLoad {
    ExtensionLoad::In {
        runtime: Arc::new(InProcessRuntime::new()),
        loaded: Loaded {
            enabled: Vec::new(),
            declarations,
        },
    }
}

/// The stats of `graph`, whose `testable` kinds are testable and must
/// declare obligations, with no recorded report, reporting `diagnostics`.
pub fn stats_of(graph: &Graph, testable: &[&str], diagnostics: &[Diagnostic]) -> Stats {
    let project = Project::of_graph(graph.clone(), registries(testable, &[]));
    specforge_ops::stats::stats(&project.view().reporting(diagnostics)).unwrap()
}

/// `plan` checked against `graph`, whose `testable` kinds are testable and
/// must declare obligations.
pub fn plan_check(graph: &Graph, testable: &[&str], plan: &serde_json::Value) -> PlanOutcome {
    let project = Project::of_graph(graph.clone(), registries(testable, &[]));
    specforge_ops::plan::check(
        &project.view(),
        &specforge_ops::plan::AgentPlan::read(plan).expect("an AgentPlan"),
    )
}

/// The contexts of `outcome`'s gaps of `kind`, in order.
pub fn gap_texts(outcome: &PlanOutcome, kind: PlanGapKind) -> Vec<&str> {
    outcome
        .gaps
        .iter()
        .filter(|gap| gap.kind == kind)
        .map(|gap| gap.context.as_str())
        .collect()
}

/// The chain of `entity_id` in `graph`, its expected edges from
/// `registries`; `None` when the graph lacks it.
pub fn chain_in(graph: &Graph, registries: RegistryBuild, entity_id: &str) -> Option<TraceChain> {
    let project = Project::of_graph(graph.clone(), registries);
    specforge_ops::trace::trace(&project.view(), Target::Entity(entity_id))
        .ok()
        .map(|outcome| outcome.chains.into_iter().next().unwrap())
}

/// The chain of `entity_id` in `graph`, with no extension: nothing is
/// expected, so nothing is missing.
pub fn chain(graph: &Graph, entity_id: &str) -> TraceChain {
    chain_in(graph, RegistryBuild::default(), entity_id).unwrap()
}

/// Every entity's chain in `graph`, with `registries`, as the trace
/// operation returns them.
pub fn every_chain(graph: &Graph, registries: RegistryBuild) -> specforge_ops::trace::TraceOutcome {
    let project = Project::of_graph(graph.clone(), registries);
    specforge_ops::trace::trace(&project.view(), Target::Every).unwrap()
}
