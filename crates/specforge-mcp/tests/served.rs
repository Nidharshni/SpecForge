//! `specforge_mcp::testing::Served`: a server over a `TestProject`.

use crate::support::*;
use serde_json::json;

const EXT: &str = crate::support::extension::EXT;

#[test]
fn the_fixture_serves_its_declaration_through_the_registry_build() {
    let served = TestProject::new()
        .file(
            "test.spec",
            "behavior alpha \"Alpha\" {\n}\ninvariant gamma \"Gamma\" {\n}\n",
        )
        .serve(&[TestExtension::software().obligating("behavior")]);

    assert!(served.state().session().project().root().is_some());
    let registries = served.state().registries();
    let declared: Vec<&str> = registries.declarations().iter().map(|d| d.name()).collect();
    assert_eq!(declared, [EXT]);
    for kind in ["behavior", "invariant", "feature"] {
        let entry = registries
            .kinds()
            .get(kind)
            .unwrap_or_else(|| panic!("{kind} is not registered"));
        assert_eq!(entry.source_extension, EXT, "{kind}");
    }
    // One W004 rule, targeting `behavior`: the behavior that declares no
    // obligation is reported, the invariant (a testable kind no rule
    // obligates) is not.
    let w004: Vec<String> = served
        .state()
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "W004")
        .map(|d| d.message)
        .collect();
    assert_eq!(
        w004,
        ["behavior 'alpha' is testable but declares no verify obligations"],
        "{w004:?}"
    );
}

/// `Served::rooted` is what `specforge mcp <root>` serves: a server built with `McpServer::with_project_root`
/// and an `initialize` that names no project answers what the plain `with_project_root` server answers.
#[test]
fn rooted_serves_what_specforge_mcp_serves() {
    let project = TestProject::new().enabling(&["@specforge/software"]).file(
        "main.spec",
        "behavior alpha \"Alpha\" {\n    contract \"The system MUST do alpha\"\n}\n",
    );
    let mut by_hand = specforge_mcp::McpServer::with_project_root(project.root().to_path_buf())
        .with_solver(z3_ready());
    call(&mut by_hand, "initialize", json!({}));
    let expected = tool(&mut by_hand, "specforge.validate", json!({}));

    let mut rooted = Served::rooted(project);
    assert_eq!(rooted.tool("specforge.validate", json!({})), expected);
    assert!(rooted.state().graph().node("alpha").is_some());
}
