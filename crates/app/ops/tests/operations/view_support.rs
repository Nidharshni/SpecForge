//! Projects for the read operations' tests: `Assembled` ones (a graph parsed
//! from source, registries declaring what made-up extensions declare, and a
//! root directory to record a test report in), viewed through `Viewed`.

use std::path::Path;
use std::sync::Arc;

use specforge_common::{ConfigRead, Diagnostic};
use specforge_extension_sdk::PeerDependency;
use specforge_extension_sdk::prelude::*;
use specforge_graph::Graph;
use specforge_ops::read::plan::{PlanGapKind, PlanOutcome};
use specforge_ops::read::stats::Stats;
use specforge_ops::read::trace::TraceChain;
pub use specforge_ops::testing::Viewed;
pub use specforge_project::testing::Assembled;
use specforge_project::{ExtensionLoad, Loaded};
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::{RegistryBuild, build_registries};
use specforge_wasm::testing::InProcessRuntime;

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
    let project = Assembled::over(registries(testable, &[])).graph(graph.clone());
    specforge_ops::read::stats::stats(&project.view().reporting(diagnostics)).unwrap()
}

/// `plan` checked against `graph`, whose `testable` kinds are testable and
/// must declare obligations.
pub fn plan_check(graph: &Graph, testable: &[&str], plan: &serde_json::Value) -> PlanOutcome {
    let project = Assembled::over(registries(testable, &[])).graph(graph.clone());
    specforge_ops::read::plan::check(
        &project.view(),
        &specforge_ops::read::plan::AgentPlan::read(plan).expect("an AgentPlan"),
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
    let project = Assembled::over(registries).graph(graph.clone());
    specforge_ops::read::trace::chain(&project.view(), entity_id)
        .ok()
        .map(|document| plain(document).chain)
}

/// The chain of `entity_id` in `graph`, with no extension: nothing is
/// expected, so nothing is missing.
pub fn chain(graph: &Graph, entity_id: &str) -> TraceChain {
    chain_in(graph, RegistryBuild::default(), entity_id).unwrap()
}

/// Every entity's chain in `graph`, with `registries`, as the trace
/// operation returns them.
pub fn every_chain(graph: &Graph, registries: RegistryBuild) -> specforge_ops::read::trace::Traces {
    let project = Assembled::over(registries).graph(graph.clone());
    plain(specforge_ops::read::trace::every(&project.view()))
}

/// A noticed read's value, its notices let go: for a test of the value.
pub fn plain<T>(read: specforge_ops::Noticed<T>) -> T {
    read.into_value_and_notices().0
}

/// A read that answered: its value.
pub fn ok<T, F: std::fmt::Debug>(read: Result<T, specforge_ops::Refusal<F>>) -> T {
    read.expect("the read answers")
}

/// The coverage view's one row for `entity_id`, counted or not.
pub fn row(
    view: &specforge_ops::view::ProjectView<'_>,
    entity_id: &str,
) -> specforge_ops::read::coverage::CoverageRow {
    let query = specforge_ops::read::coverage::CoverageQuery {
        entity_id: Some(entity_id),
        ..Default::default()
    };
    answered(specforge_ops::read::coverage::coverage(view, &query))
        .rows
        .into_iter()
        .next()
        .expect("the coverage view has a row for the entity")
}

/// A read that answered: its value, its notices dropped.
pub fn answered<T, F: std::fmt::Debug>(
    read: Result<specforge_ops::Noticed<T>, specforge_ops::Refusal<F>>,
) -> T {
    plain(ok(read))
}

/// The document `specforge.schema` answers for `request` over `view`.
pub fn schema_json(
    view: &specforge_ops::view::ProjectView<'_>,
    request: &specforge_ops::read::schema::SchemaRequest<'_>,
) -> serde_json::Value {
    let outcome = plain(specforge_ops::read::schema::schema(view, request).unwrap());
    serde_json::to_value(outcome).expect("a schema serializes")
}
