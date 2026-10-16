/// The document as JSON.
fn json_of(outcome: &AnalyzeOutcome) -> serde_json::Value {
    serde_json::to_value(outcome.document()).expect("a document serializes")
}

use super::*;
use crate::testing::{ScriptedSolver, Viewed};
use serde_json::{Value, json};
use specforge_extension_sdk::prelude::*;
use specforge_project::testing::Assembled;
use specforge_test_macros::test as specforge_test;
use specforge_wasm::runtime::{WasmCallResult, WasmTrapInfo};
use specforge_wasm::testing::InProcessRuntime;

const EXT: &str = "@t/x";
const TESTING: &str = "@specforge/testing";

/// `@t/x`, declaring `scan` (warns W900) and `hidden` (check phase);
/// the runtime records the input every pass receives.
fn scanning() -> ContributionsBuilder {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new(EXT, "1.0.0"));
    c.pass("scan", |p| {
        p.run(|_: &PassInput| vec![PassDiagnostic::warning("W900", "scanned")]);
    });
    c.pass("hidden", |p| {
        p.phase("check")
            .run(|_: &PassInput| Vec::<PassDiagnostic>::new());
    });
    c
}

/// The `proved_claims` each `scan` call received (`null` when absent).
fn proved_seen(runtime: &InProcessRuntime) -> Vec<Value> {
    runtime
        .calls()
        .into_iter()
        .filter(|c| c.export == "__pass_scan")
        .map(|c| c.input.get("proved_claims").cloned().unwrap_or(Value::Null))
        .collect()
}

struct Project {
    assembled: Assembled,
    /// The runtime the environment loaded its extensions in: what it
    /// records is what a test then runs.
    runtime: std::sync::Arc<InProcessRuntime>,
}

impl Project {
    /// A project at a temp root whose `specforge.json` enables
    /// `entries`, each loaded from `runtime` through the production
    /// load (a named entry that is no builtin installed first, ADR 0028
    /// D3), with an empty graph. The runtime's calls are cleared after
    /// the load: what it records is what the test then runs.
    fn serving(runtime: InProcessRuntime, entries: &[&str]) -> Self {
        let runtime = std::sync::Arc::new(runtime);
        let assembled = Assembled::assembling(|root| loaded_at(root, &runtime, entries));
        Self { assembled, runtime }
    }

    /// `@t/x` alone, as most tests serve it.
    fn new() -> Self {
        Self::serving(InProcessRuntime::new().with(scanning), &[EXT])
    }

    fn view(&self) -> ProjectView<'_> {
        self.assembled.view()
    }

    fn run(&self, options: &AnalyzeOptions) -> Result<AnalyzeOutcome, AnalyzeError> {
        analyze(&self.view(), options)
    }
}

/// The environment of the project at `root` whose `specforge.json` enables
/// `entries`, each loaded from `runtime` through the production load (a named
/// entry that is no builtin installed first, ADR 0028 D3). The runtime's calls
/// are cleared after the load: what it records is what a test then runs.
fn loaded_at(
    root: &std::path::Path,
    runtime: &std::sync::Arc<InProcessRuntime>,
    entries: &[&str],
) -> specforge_project::Environment {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(
        root.join("specforge.json"),
        json!({ "extensions": entries }).to_string(),
    )
    .unwrap();
    let entries: Vec<String> = entries.iter().map(|e| e.to_string()).collect();
    specforge_installed::testing::install_enabled(root, &entries, &specforge_project::builtins());
    let env = specforge_project::Environment::load(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        Some(runtime.clone()),
    );
    assert!(
        env.diagnostics().all(|d| d.code != "E028"),
        "{:?}",
        env.diagnostics().collect::<Vec<_>>()
    );
    runtime.clear_calls();
    env
}

fn names(outcome: &AnalyzeOutcome) -> Vec<&str> {
    outcome.passes.iter().map(|p| p.name.as_str()).collect()
}

fn pass(name: &str) -> AnalyzeOptions<'static> {
    AnalyzeOptions {
        pass: name.to_string(),
        ..Default::default()
    }
}

/// A `@specforge/testing` whose `coverage` pass answers `summary`.
fn coverage(summary: Value) -> impl Fn() -> ContributionsBuilder + Send + Sync + 'static {
    move || {
        let summary = summary.clone();
        let mut c = ContributionsBuilder::new(ExtensionMeta::new(TESTING, "1.0.0"));
        c.pass("coverage", |p| {
            p.run(move |_: &PassInput| PassOutput {
                diagnostics: Vec::new(),
                summary: summary.as_object().cloned().unwrap_or_default(),
            });
        });
        c
    }
}

fn tally(proven: usize, total: usize) -> Value {
    serde_json::to_value(coverage::Summary {
        testable_proven: proven,
        testable_total: total,
        ..Default::default()
    })
    .unwrap()
}

fn write_report(project: &Project, ids: &[&str]) {
    let results: serde_json::Map<String, Value> = ids
        .iter()
        .map(|id| {
            (
                id.to_string(),
                json!({"tests": [{"name": "t", "status": "passed"}]}),
            )
        })
        .collect();
    std::fs::write(
        project.assembled.root().join("specforge-report.json"),
        json!({"runner": "r", "results": results}).to_string(),
    )
    .unwrap();
}

fn add_entity(project: &mut Project, id: &str) {
    add_entity_with(project, id, "behavior", []);
}

/// A node of `kind` carrying `fields`.
fn add_entity_with(
    project: &mut Project,
    id: &str,
    kind: &str,
    fields: impl IntoIterator<Item = (&'static str, specforge_parser::FieldValue)>,
) {
    use specforge_common::{SourceSpan, Sym};
    use specforge_parser::{EntityId, EntityKind, FieldMap};
    let mut map = FieldMap::new();
    for (name, value) in fields {
        map.push(Sym::new(name), value);
    }
    project
        .assembled
        .graph_mut()
        .add_node(specforge_graph::Node {
            id: EntityId { raw: Sym::new(id) },
            kind: EntityKind {
                raw: Sym::new(kind),
            },
            title: None,
            fields: map,
            source_span: SourceSpan {
                file: Sym::new("t.spec"),
                start_line: 1,
                start_col: 1,
                end_line: 1,
                end_col: 1,
            },
            methods: Vec::new(),
            obligations: Vec::new(),
        });
}

mod gate;
mod passes;
mod refusals;
mod stray;
