//! What MCP's tests serve a `TestProject` with.

use serde_json::json;
use specforge_mcp::testing::{Served, rpc};
use specforge_project::SharedRuntime;
pub use specforge_project::testing::TestProject;

use super::extension::{EXT, TestExtension, runtime_of};

/// What MCP's tests serve a `TestProject` with.
pub trait Serve {
    /// Serve `extensions` (SDK-declared, `TestExtension`) in one in-process runtime; enable them unless the test
    /// chose ([`TestProject::enabling`]).
    fn serve(self, extensions: &[TestExtension]) -> Served;
    /// Serve `@test/ext` (unless the test chose) in `runtime`.
    fn serve_in(self, runtime: SharedRuntime) -> Served;
    /// Serve with the project's own component runtime (add, remove, doctor, the builtins).
    fn serve_components(self) -> Served;
}

impl Serve for TestProject {
    fn serve(self, extensions: &[TestExtension]) -> Served {
        let names: Vec<&str> = extensions.iter().map(|e| e.name()).collect();
        let project = match self.enabled() {
            Some(_) => self,
            None => self.enabling(&names),
        };
        Served::initialized(project.serving(runtime_of(extensions)))
    }

    fn serve_in(self, runtime: SharedRuntime) -> Served {
        let project = match self.enabled() {
            Some(_) => self,
            None => self.enabling(&[EXT]),
        };
        Served::initialized_in(project, runtime)
    }

    fn serve_components(self) -> Served {
        Served::initialized(self)
    }
}

// What the fixture serves is what the real build makes of its sources and
// declarations.

/// `alpha`, a behavior on lines 1–4 of `test.spec`.
const ALPHA: &str = "behavior alpha \"Alpha\" {\n    contract \"The system MUST do alpha\"\n    verify unit \"does alpha\"\n}\n";

#[test]
fn the_declaration_is_what_the_builders_declare() {
    let declaration = TestExtension::named("@test/other")
        .kind("task", true)
        .headline("task")
        .string_field("task", "owner")
        .reference("task", "blocks", "task")
        .declaring(|c| {
            c.kind("note", |_| {});
        })
        .declaration();

    assert_eq!(declaration.name(), "@test/other");
    let kinds: Vec<&str> = declaration
        .entities
        .iter()
        .map(|k| k.name.as_str())
        .collect();
    assert_eq!(kinds, ["task", "note"]);
    let task = &declaration.entities[0];
    assert!(task.testable && task.supports_verify);
    let fields: Vec<(&str, &str, bool, bool)> = task
        .fields
        .iter()
        .map(|f| {
            (
                f.name.as_str(),
                f.field_type.as_str(),
                f.normative,
                f.headline,
            )
        })
        .collect();
    assert_eq!(
        fields,
        [
            ("contract", "string", true, true),
            ("status", "string", false, true),
            ("owner", "string", false, false),
            ("blocks", "reference_list", false, false),
        ]
    );
    let blocks = &task.fields[3];
    assert_eq!(blocks.target_kind.as_deref(), Some("task"));
    assert_eq!(blocks.edge.as_deref(), Some("blocks"));
    let edges: Vec<&str> = declaration.edges.iter().map(|e| e.label.as_str()).collect();
    assert_eq!(edges, ["blocks"]);
    assert!(declaration.validation_rules.is_empty());
    assert!(declaration.passes.is_empty());
}

#[test]
fn the_served_graph_is_the_sources_on_disk() {
    let served = TestProject::new()
        .file("test.spec", ALPHA)
        .file(
            "features.spec",
            "feature beta \"Beta\" {\n    behaviors [alpha]\n}\n",
        )
        .serve(&[TestExtension::software()]);

    let graph = served.state().graph();
    let alpha = graph.node("alpha").expect("alpha is served");
    assert_eq!(alpha.kind.raw, "behavior");
    assert_eq!(alpha.title.as_deref(), Some("Alpha"));
    // The parser's spans: 1-based lines and columns.
    assert_eq!(alpha.source_span.file, "test.spec");
    assert_eq!(
        (alpha.source_span.start_line, alpha.source_span.start_col),
        (1, 1)
    );
    assert_eq!(alpha.source_span.end_line, 4);
    let beta = graph.node("beta").expect("beta is served");
    assert_eq!(beta.kind.raw, "feature");
    assert_eq!(beta.source_span.file, "features.spec");
    let edges: Vec<(&str, &str)> = graph
        .edges_from("beta")
        .iter()
        .map(|e| (e.target.as_str(), e.label.as_str()))
        .collect();
    assert_eq!(edges, [("alpha", "behaviors")]);
    assert!(
        served.state().diagnostics().is_empty(),
        "{:?}",
        served.state().diagnostics()
    );
}

#[test]
fn a_reported_diagnostic_joins_the_compile() {
    use specforge_common::DiagnosticData;
    use specforge_extension_sdk::prelude::PassDiagnostic;

    let served = TestProject::new()
        .file("test.spec", ALPHA)
        .serve(&[TestExtension::software()
            .reporting(PassDiagnostic::warning("W900", "x").with_entity("alpha"))]);

    let reported: Vec<_> = served
        .state()
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "W900")
        .collect();
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert_eq!(reported[0].message, "x");
    assert_eq!(
        reported[0].data.as_deref(),
        Some(&DiagnosticData::Subject {
            entity: "alpha".into()
        })
    );
    // A spanless diagnostic that names an entity is placed at it.
    let span = reported[0].span.as_ref().expect("alpha's span");
    assert_eq!((span.file.as_str(), span.start_line), ("test.spec", 1));
    // The pass ran in the extension's runtime.
    assert!(
        served
            .extension_calls()
            .iter()
            .any(|c| c.extension == EXT && c.export == "__pass_report"),
        "{:?}",
        served.extension_calls()
    );
}

#[test]
fn a_write_is_served_by_the_next_request() {
    let mut served = TestProject::new()
        .file("test.spec", ALPHA)
        .serve(&[TestExtension::software()]);
    let ids = |served: &mut Served| -> Vec<String> {
        rpc::tool(served, "specforge.list", json!({}))["entities"]
            .clone()
            .as_array()
            .expect("a list")
            .iter()
            .map(|e| e["id"].as_str().expect("an id").to_string())
            .collect()
    };
    assert_eq!(ids(&mut served), ["alpha"]);

    served.write("more/delta.spec", "invariant delta \"Delta\" {\n}\n");
    assert_eq!(ids(&mut served), ["alpha", "delta"]);

    served.remove("more/delta.spec");
    assert_eq!(ids(&mut served), ["alpha"]);
}

#[test]
fn serve_components_loads_the_builtins() {
    let served = TestProject::new()
        .enabling(&["@specforge/software"])
        .file("test.spec", ALPHA)
        .serve_components();

    let behavior = served
        .state()
        .registries()
        .kinds()
        .get("behavior")
        .expect("behavior is registered");
    assert_eq!(behavior.source_extension, "@specforge/software");
    assert!(served.state().graph().node("alpha").is_some());
    assert!(served.extension_calls().is_empty());
}
