use crate::support::{Cli, TestProject};

const SPEC_CONTENT: &str = r#"
behavior alpha "Alpha" { contract "first" }
behavior beta "Beta" { contract "second" }
feature gamma "Gamma" { behaviors [alpha, beta] }
invariant delta "Delta" { guarantee "always" }
"#;

#[test]
fn query_depth_0_returns_single_entity() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", SPEC_CONTENT)]);

    let output = Cli::on(&dir)
        .command(&["query", "alpha", "--depth=0"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let nodes = parsed["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["id"], "alpha");
}

#[test]
fn query_depth_1_returns_neighbors() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", SPEC_CONTENT)]);

    let output = Cli::on(&dir)
        .command(&["query", "gamma", "--depth=1"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let nodes = parsed["nodes"].as_array().unwrap();
    let ids: Vec<&str> = nodes.iter().map(|n| n["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"gamma"));
    assert!(ids.contains(&"alpha"));
    assert!(ids.contains(&"beta"));
}

#[test]
fn query_with_kind_filter() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", SPEC_CONTENT)]);

    let output = Cli::on(&dir)
        .command(&["query", "gamma", "--depth=1", "--kind=behavior"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let nodes = parsed["nodes"].as_array().unwrap();
    for node in nodes {
        let kind = node["kind"].as_str().unwrap();
        // Root (gamma=feature) is always included + behavior nodes
        assert!(
            kind == "feature" || kind == "behavior",
            "unexpected kind: {}",
            kind
        );
    }
}

#[specforge_test_macros::test(
    behavior = "query_graph_multi_resolution",
    verify = "a non-existent entity is E003 naming the closest entity"
)]
fn query_nonexistent_entity_exits_one() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", SPEC_CONTENT)]);

    Cli::on(&dir)
        .command(&["query", "nonexistent", "--depth=1"])
        .assert()
        .code(1)
        .stderr("error[E003]: unresolved entity 'nonexistent' — not found in graph\n");

    // A close id is named.
    Cli::on(&dir).command(&["query", "alph"])
        .assert()
        .code(1)
        .stderr(
            "error[E003]: unresolved entity 'alph' — not found in graph\n  hint: did you mean 'alpha'?\n",
        );
}
