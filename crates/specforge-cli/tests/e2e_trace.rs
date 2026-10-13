use crate::e2e_fixtures::*;
use specforge_test_macros::test as specforge_test;
use std::collections::BTreeSet;

/// One direction of a trace: its (entity_id, depth) links, in order.
type Links = Vec<(String, u64)>;

/// `specforge trace <id>` in `dir`: (upstream, downstream).
fn trace(dir: &std::path::Path, id: &str) -> (Links, Links) {
    let output = specforge_cmd()
        .args(["trace", id])
        .arg("--path")
        .arg(dir)
        .output()
        .unwrap();
    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
    assert_eq!(parsed["entity_id"], id);
    let links = |direction: &str| {
        parsed[direction]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                (
                    l["entity_id"].as_str().unwrap().to_string(),
                    l["depth"].as_u64().unwrap(),
                )
            })
            .collect()
    };
    (links("upstream"), links("downstream"))
}

fn links(list: &[(&str, u64)]) -> Links {
    list.iter().map(|(id, d)| (id.to_string(), *d)).collect()
}

/// The links as a set, after checking none repeats.
fn once_each(list: &Links) -> BTreeSet<(String, u64)> {
    let set: BTreeSet<_> = list.iter().cloned().collect();
    assert_eq!(set.len(), list.len(), "a node is visited twice: {list:?}");
    set
}

/// `specforge trace --plan <plan>` over `dir`: the exit code, stdout, stderr.
fn trace_plan(dir: &std::path::Path, plan: &str, extra: &[&str]) -> (Option<i32>, String, String) {
    std::fs::write(dir.join("plan.json"), plan).unwrap();
    let output = specforge_cmd()
        .args(["trace", "--plan"])
        .arg(dir.join("plan.json"))
        .arg("--path")
        .arg(dir)
        .args(extra)
        .output()
        .unwrap();
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[specforge_test(
    behavior = "validate_agent_plan",
    verify = "specforge trace --plan exits 1 when an entry names no entity or comes before what it depends on"
)]
fn trace_plan_exits_by_the_plan_verdict() {
    let dir = setup_project(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" }
feature beta "B" { problem "p" solution "s" behaviors [alpha] }
"#,
    )]);
    let entry = |id: &str| format!(r#"{{"entity_id":"{id}","action":"implement"}}"#);
    let plan = |ids: &[&str]| {
        let entries: Vec<String> = ids.iter().map(|id| entry(id)).collect();
        format!(r#"{{"plan_id":"p","entries":[{}]}}"#, entries.join(","))
    };

    assert_eq!(trace_plan(dir.path(), &plan(&["alpha"]), &[]).0, Some(0));
    assert_eq!(trace_plan(dir.path(), &plan(&["ghost"]), &[]).0, Some(1));
    // beta depends on alpha: alpha first.
    assert_eq!(
        trace_plan(dir.path(), &plan(&["alpha", "beta"]), &[]).0,
        Some(0)
    );
    assert_eq!(
        trace_plan(dir.path(), &plan(&["beta", "alpha"]), &[]).0,
        Some(1)
    );

    // The human text ends with the verdict.
    let (_, stdout, _) = trace_plan(dir.path(), &plan(&["alpha"]), &["--format", "human"]);
    assert_eq!(
        stdout.lines().last(),
        Some("plan check: passed"),
        "{stdout}"
    );

    // A file that is not an AgentPlan, or is not there, is the command line's refusal.
    let (code, _, stderr) = trace_plan(dir.path(), r#"{"entries": []}"#, &[]);
    assert_eq!(code, Some(2));
    assert!(
        stderr.contains("plan must be an AgentPlan: plan: missing required plan_id"),
        "{stderr}"
    );
    let output = specforge_cmd()
        .args(["trace", "--plan"])
        .arg(dir.path().join("missing.json"))
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot read"));
    // It conflicts with an entity.
    let output = specforge_cmd()
        .args(["trace", "alpha", "--plan"])
        .arg(dir.path().join("plan.json"))
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

// --- Trace depth tests ---

#[specforge_test(
    behavior = "compute_traceability_chain",
    verify = "trace from entity shows upstream and downstream connections"
)]
fn trace_isolated_entity_has_empty_upstream_and_downstream() {
    let spec = format!(
        "{ISOLATED_SPEC}\nbehavior alpha \"A\" {{ contract \"first\" }}\nfeature beta \"B\" {{ problem \"p\" solution \"s\" behaviors [alpha] }}\n"
    );
    let dir = setup_project(&[("main.spec", &spec)]);

    // Unconnected: nothing either way.
    assert_eq!(trace(dir.path(), "isolated_node"), (links(&[]), links(&[])));
    // beta -behaviors-> alpha: each sees the other on its own side.
    assert_eq!(
        trace(dir.path(), "alpha"),
        (links(&[("beta", 1)]), links(&[]))
    );
    assert_eq!(
        trace(dir.path(), "beta"),
        (links(&[]), links(&[("alpha", 1)]))
    );
}

#[specforge_test(
    behavior = "compute_traceability_chain",
    verify = "trace shows full chain depth"
)]
fn trace_linear_chain_shows_correct_depths() {
    let dir = setup_project(&[("main.spec", DEEP_CHAIN_SPEC)]);

    // inv_deep -enforced_by-> beh_middle -features-> feat_root: the whole
    // chain, each hop at its depth.
    let (upstream, downstream) = trace(dir.path(), "inv_deep");
    assert_eq!(upstream, links(&[]));
    assert_eq!(downstream, links(&[("beh_middle", 1), ("feat_root", 2)]));
}

#[test]
fn trace_includes_edge_labels() {
    let dir = setup_project(&[("main.spec", DEEP_CHAIN_SPEC)]);

    let output = specforge_cmd()
        .args(["trace", "inv_deep"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
    let upstream = parsed["upstream"].as_array().unwrap();

    for link in upstream {
        let label = link["edge_label"].as_str().unwrap();
        assert!(
            !label.is_empty(),
            "every TraceLink must have a non-empty edge_label"
        );
    }
}

#[specforge_test(
    invariant = "graph_traversal_integrity",
    verify = "traversal from any node visits every reachable node exactly once"
)]
fn trace_handles_cycles_without_hanging() {
    let dir = setup_project(&[("main.spec", CYCLE_SPEC)]);

    // Edges: cycle_a -> cycle_c, cycle_b -> cycle_a, cycle_c -> cycle_a.
    // From cycle_c the cycle is walked both ways, each other node once,
    // and the walk stops rather than coming back to cycle_c.
    let (upstream, downstream) = trace(dir.path(), "cycle_c");
    assert_eq!(
        once_each(&upstream),
        BTreeSet::from([("cycle_a".to_string(), 1), ("cycle_b".to_string(), 2)])
    );
    assert_eq!(
        once_each(&downstream),
        BTreeSet::from([("cycle_a".to_string(), 1)])
    );
}

#[specforge_test(
    invariant = "graph_traversal_integrity",
    verify = "traversal from any node visits every reachable node exactly once"
)]
fn trace_cycle_visits_each_node_once() {
    let dir = setup_project(&[("main.spec", CYCLE_SPEC)]);

    // Upstream of cycle_a: cycle_b and cycle_c point at it. Downstream:
    // cycle_a -> cycle_c, whose only edge leads back to cycle_a.
    let (upstream, downstream) = trace(dir.path(), "cycle_a");
    assert_eq!(
        once_each(&upstream),
        BTreeSet::from([("cycle_b".to_string(), 1), ("cycle_c".to_string(), 1)])
    );
    assert_eq!(
        once_each(&downstream),
        BTreeSet::from([("cycle_c".to_string(), 1)])
    );
}

#[specforge_test(
    invariant = "graph_traversal_integrity",
    verify = "identical graph inputs produce identical traversal results"
)]
fn trace_deterministic_output() {
    let dir = setup_project(&[("main.spec", DEEP_CHAIN_SPEC)]);

    let output1 = specforge_cmd()
        .args(["trace", "beh_middle"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    let output2 = specforge_cmd()
        .args(["trace", "beh_middle"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    let stdout1 = String::from_utf8_lossy(&output1.stdout);
    let stdout2 = String::from_utf8_lossy(&output2.stdout);
    assert_eq!(stdout1, stdout2, "trace output should be deterministic");
}

#[specforge_test(
    behavior = "compute_traceability_chain",
    verify = "trace from entity shows upstream and downstream connections"
)]
fn trace_root_entity_has_no_upstream() {
    let dir = setup_project(&[("main.spec", DEEP_CHAIN_SPEC)]);

    // inv_deep is a root: nothing points at it, but it reaches the chain.
    let (upstream, downstream) = trace(dir.path(), "inv_deep");
    assert_eq!(upstream, links(&[]), "inv_deep should have no upstream");
    assert_eq!(downstream, links(&[("beh_middle", 1), ("feat_root", 2)]));

    // beh_middle is in the middle: feat_root and inv_deep point at it, and
    // it points at feat_root.
    let (upstream, downstream) = trace(dir.path(), "beh_middle");
    assert_eq!(
        once_each(&upstream),
        BTreeSet::from([("feat_root".to_string(), 1), ("inv_deep".to_string(), 1)])
    );
    assert_eq!(downstream, links(&[("feat_root", 1)]));

    // typ_leaf has no references either way.
    assert_eq!(trace(dir.path(), "typ_leaf"), (links(&[]), links(&[])));
}

#[specforge_test(
    behavior = "compute_traceability_chain",
    verify = "trace from entity shows upstream and downstream connections"
)]
fn trace_leaf_entity_has_no_downstream() {
    let dir = setup_project(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" }
feature beta "B" { problem "p" solution "s" behaviors [alpha] }
"#,
    )]);

    // alpha has no outgoing edges (it's a leaf), but beta points to it
    let output = specforge_cmd()
        .args(["trace", "alpha"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
    assert_eq!(parsed["entity_id"], "alpha");
    assert_eq!(
        parsed["downstream"].as_array().unwrap().len(),
        0,
        "alpha should have no downstream"
    );
    assert!(
        !parsed["upstream"].as_array().unwrap().is_empty(),
        "alpha should have upstream (beta)"
    );
}

#[test]
fn trace_multi_kind_chain_preserves_entity_kind() {
    let dir = setup_project(&[("main.spec", DEEP_CHAIN_SPEC)]);

    let output = specforge_cmd()
        .args(["trace", "inv_deep"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);

    assert_eq!(parsed["entity_kind"], "invariant");

    let upstream = parsed["upstream"].as_array().unwrap();
    for link in upstream {
        let kind = link["entity_kind"].as_str().unwrap();
        assert!(
            !kind.is_empty(),
            "every TraceLink should have a non-empty entity_kind"
        );
    }

    // beh_middle should have kind "behavior"
    if let Some(beh) = upstream.iter().find(|l| l["entity_id"] == "beh_middle") {
        assert_eq!(beh["entity_kind"], "behavior");
    }
}

// --- Missing links ---

/// software + product loaded: `pay` implements a feature and enforces an
/// invariant; `refund` is linked to nothing.
fn gap_project() -> tempfile::TempDir {
    setup_project(&[
        (
            "specforge.json",
            r#"{"name":"t","version":"0.1.0","extensions":["@specforge/software","@specforge/product"]}"#,
        ),
        (
            "main.spec",
            r#"
feature checkout "Checkout" {
    problem "Buyers need to pay"
    solution "A payment step"
}

behavior pay "Pay" {
    contract "The system MUST take payment once"
    features [checkout]
    invariants [paid_once]
}

behavior refund "Refund" {
    contract "The system MUST return the payment"
}

invariant paid_once "Paid Once" {
    guarantee "An order MUST be paid at most once"
}
"#,
        ),
    ])
}

fn run_trace(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    let output = specforge_cmd()
        .arg("trace")
        .args(args)
        .arg("--path")
        .arg(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// A chain's missing links as (from, edge_label, edge_type, expected_kind),
/// after checking each is marked missing at depth 1.
fn missing_links(chain: &serde_json::Value) -> Vec<(String, String, String, String)> {
    chain["missing"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            assert_eq!(m["status"], "missing");
            assert_eq!(m["depth"], 1);
            (
                m["from"].as_str().unwrap().to_string(),
                m["edge_label"].as_str().unwrap().to_string(),
                m["edge_type"].as_str().unwrap().to_string(),
                m["expected_kind"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn gap(from: &str, label: &str, edge_type: &str, kind: &str) -> (String, String, String, String) {
    (
        from.to_string(),
        label.to_string(),
        edge_type.to_string(),
        kind.to_string(),
    )
}

/// What the software extension declares every behavior may reach, as
/// missing links of `from`.
fn software_gaps(from: &str) -> Vec<(String, String, String, String)> {
    vec![
        gap(from, "consumes", "BehaviorConsumesEvent", "event"),
        gap(from, "ports", "BehaviorUsesPort", "port"),
        gap(from, "produces", "BehaviorProducesEvent", "event"),
        gap(from, "types", "BehaviorReferencesType", "type"),
    ]
}

#[specforge_test(
    behavior = "compute_traceability_chain",
    verify = "missing link in chain is flagged"
)]
fn trace_json_flags_missing_links() {
    let dir = gap_project();

    // pay has its feature and its invariant: only the edges the software
    // extension declares for behaviors that pay leaves out are missing.
    let pay = parse_json_stdout(&run_trace(dir.path(), &["pay"]));
    assert_eq!(missing_links(&pay), software_gaps("pay"));
    for link in pay["downstream"].as_array().unwrap() {
        assert_eq!(link["status"], "resolved");
    }

    // refund also lacks a feature and an invariant.
    let refund = parse_json_stdout(&run_trace(dir.path(), &["refund"]));
    let mut expected = software_gaps("refund");
    expected.push(gap(
        "refund",
        "features",
        "BehaviorImplementsFeature",
        "feature",
    ));
    expected.push(gap(
        "refund",
        "invariants",
        "BehaviorEnforcesInvariant",
        "invariant",
    ));
    expected.sort_by(|a, b| a.1.cmp(&b.1));
    assert_eq!(missing_links(&refund), expected);

    // A feature's only reference fields point at features: a hierarchy,
    // not an expected edge.
    let checkout = parse_json_stdout(&run_trace(dir.path(), &["checkout"]));
    assert_eq!(missing_links(&checkout), vec![]);

    // With no extensions loaded nothing is expected, so nothing is missing.
    let bare = setup_project(&[("main.spec", ISOLATED_SPEC)]);
    let isolated = parse_json_stdout(&run_trace(bare.path(), &["isolated_node"]));
    assert_eq!(isolated["missing"], serde_json::json!([]));
}

#[specforge_test(
    behavior = "compute_traceability_chain",
    verify = "missing link in chain is flagged"
)]
fn trace_human_marks_missing_links() {
    let dir = gap_project();

    let output = run_trace(dir.path(), &["pay", "--format", "human"]);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        concat!(
            "pay [behavior]\n",
            "  upstream:\n",
            "    (none)\n",
            "  downstream:\n",
            "    -features-> checkout [feature] (depth 1)\n",
            "    -invariants-> paid_once [invariant] (depth 1)\n",
            "  missing:\n",
            "    MISSING pay -consumes-> [event] (depth 1)\n",
            "    MISSING pay -ports-> [port] (depth 1)\n",
            "    MISSING pay -produces-> [event] (depth 1)\n",
            "    MISSING pay -types-> [type] (depth 1)\n",
        )
    );

    // A chain with nothing missing has no missing section.
    let output = run_trace(dir.path(), &["checkout", "--format", "human"]);
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.starts_with("checkout [feature]\n"), "{text}");
    assert!(!text.contains("MISSING"), "{text}");
}

#[specforge_test(
    behavior = "serialize_traceability_data",
    verify = "gaps in chain are highlighted"
)]
fn trace_without_entity_serializes_every_chain_with_its_gaps() {
    let dir = gap_project();

    let all = parse_json_stdout(&run_trace(dir.path(), &[]));
    assert!(all["schema_version"].is_string());
    let traces = all["traces"].as_array().unwrap();
    let ids: Vec<&str> = traces
        .iter()
        .map(|t| t["entity_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["checkout", "paid_once", "pay", "refund"]);

    // Each chain carries the same gaps as its own trace.
    for trace in traces {
        let id = trace["entity_id"].as_str().unwrap();
        let single = parse_json_stdout(&run_trace(dir.path(), &[id]));
        assert_eq!(trace["missing"], single["missing"], "{id}");
    }
    assert_eq!(missing_links(&traces[2]), software_gaps("pay"));
    assert_eq!(missing_links(&traces[3]).len(), 6);
}

#[specforge_test(
    behavior = "compute_traceability_chain",
    verify = "trace output includes schema version"
)]
fn trace_output_includes_schema_version() {
    let dir = setup_project(&[("main.spec", ISOLATED_SPEC)]);

    let output = specforge_cmd()
        .args(["trace", "isolated_node"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
    assert!(
        parsed["schema_version"].is_string(),
        "trace output must include schema_version field"
    );
    assert!(!parsed["schema_version"].as_str().unwrap().is_empty());
}
