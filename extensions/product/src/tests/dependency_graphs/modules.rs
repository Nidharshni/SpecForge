use super::{layered, reversed};
use crate::tests::host::*;
use serde_json::{json, Value};
use specforge_test::prelude::*;

/// Modules over `depends_on`: api -> util -> core; web -> api, util;
/// cli -> api, util; iso alone.
fn layers() -> G {
    G::default()
        .n("core", "module")
        .n("util", "module")
        .n("api", "module")
        .n("web", "module")
        .n("cli", "module")
        .n("iso", "module")
        .edge("util", "core", "depends_on")
        .edge("api", "util", "depends_on")
        .edge("web", "api", "depends_on")
        .edge("web", "util", "depends_on")
        .edge("cli", "api", "depends_on")
        .edge("cli", "util", "depends_on")
}

#[specforge_test(
    behavior = "surface_module_dependency_depth",
    verify = "product:module-depth returns ModuleDependencyDepthPayload"
)]
fn module_depth_answers_its_payload() {
    let md = json_of("module_depth", json!({"module": "web"}), &layers());
    assert_eq!(
        md,
        json!({"module_id": "web", "depth": 3, "longest_chain": ["web", "api", "util", "core"]})
    );
    assert_eq!(
        human_of("module_depth", json!({"module": "web"}), &layers()),
        "Module: web (depth 3)\nChain: web -> api -> util -> core\n"
    );
}

#[specforge_test(
    behavior = "surface_module_dependency_depth",
    verify = "missing module returns error with suggestion"
)]
fn module_depth_of_a_mistyped_module_suggests_the_nearest() {
    let error = not_found_suggesting("module_depth", json!({"module": "utl"}), &layers());
    assert_eq!(error["suggestion"], "util");
}

#[specforge_test(
    behavior = "pe_query_module_dependency_depth",
    verify = "module with no dependencies returns depth=0"
)]
fn a_module_without_dependencies_is_at_depth_zero() {
    let md = json_of("module_depth", json!({"module": "core"}), &layers());
    assert_eq!(
        (md["depth"].clone(), md["longest_chain"].clone()),
        (json!(0), json!(["core"]))
    );
}

#[specforge_test(
    behavior = "pe_query_module_dependency_depth",
    verify = "module depending on two layers returns depth=2"
)]
fn a_module_two_layers_up_is_at_depth_two() {
    let md = json_of("module_depth", json!({"module": "api"}), &layers());
    assert_eq!(
        (md["depth"].clone(), md["longest_chain"].clone()),
        (json!(2), json!(["api", "util", "core"]))
    );
}

#[specforge_test(
    behavior = "pe_query_module_dependency_depth",
    verify = "module in a cycle returns depth=-1"
)]
fn a_module_on_a_cycle_is_at_depth_minus_one() {
    let g = layers().edge("core", "api", "depends_on");
    let md = json_of("module_depth", json!({"module": "util"}), &g);
    assert_eq!(
        md,
        json!({"module_id": "util", "depth": -1, "longest_chain": ["api", "core", "util"]})
    );
    assert_eq!(
        human_of("module_depth", json!({"module": "util"}), &g),
        "Module: util (depth -1: on or behind a dependency cycle)\nCycle: api, core, util\n"
    );
}

#[specforge_test(
    behavior = "pe_query_module_dependency_depth",
    verify = "longest_chain includes all modules in the longest path"
)]
fn the_chain_follows_the_longest_path_not_the_shortest() {
    // cli reaches util directly and through api: the chain goes through api.
    let md = json_of("module_depth", json!({"module": "cli"}), &layers());
    assert_eq!(md["longest_chain"], json!(["cli", "api", "util", "core"]));
    assert_eq!(md["depth"], 3);
}

#[specforge_test(
    behavior = "pe_query_module_dependency_depth",
    verify = "non-existent module returns ENTITY_NOT_FOUND with suggestion"
)]
fn module_depth_of_no_module_is_not_found() {
    let error = not_found_suggesting("module_depth", json!({"module": "cor"}), &layers());
    assert_eq!(
        (error["entity_id"].clone(), error["suggestion"].clone()),
        (json!("cor"), json!("core"))
    );
    // A feature's id is not a module's.
    let g = layers().n("core2", "feature");
    let error = not_found_suggesting("module_depth", json!({"module": "core2"}), &g);
    assert_eq!(error["message"], "module 'core2' not found");
}

#[specforge_test(
    behavior = "pe_query_module_dependency_depth",
    verify = "result is deterministic across repeated queries"
)]
fn module_depth_is_the_same_every_time_and_in_any_order() {
    for module in ["web", "cli", "iso"] {
        let args = json!({ "module": module });
        let first = json_of("module_depth", args.clone(), &layers());
        assert_eq!(json_of("module_depth", args.clone(), &layers()), first);
        assert_eq!(json_of("module_depth", args, &reversed(layers())), first);
    }
}

#[specforge_test(
    behavior = "surface_module_coupling",
    verify = "product:module-coupling returns ModuleCouplingPayload"
)]
fn module_coupling_answers_its_payload() {
    let mc = json_of("module_coupling", json!({}), &layers());
    assert_eq!(
        mc,
        json!({
            "modules": [
                {"module_id": "util", "fan_in": 3, "fan_out": 1, "coupling": 4},
                {"module_id": "api", "fan_in": 2, "fan_out": 1, "coupling": 3},
                {"module_id": "cli", "fan_in": 0, "fan_out": 2, "coupling": 2},
                {"module_id": "web", "fan_in": 0, "fan_out": 2, "coupling": 2},
                {"module_id": "core", "fan_in": 1, "fan_out": 0, "coupling": 1},
                {"module_id": "iso", "fan_in": 0, "fan_out": 0, "coupling": 0},
            ],
            "avg_fan_in": 1.0, "avg_fan_out": 1.0, "most_coupled_id": "util",
            "total_modules": 6, "total": 6, "offset": 0, "limit": 100, "has_more": false,
        })
    );
    // Paged by offset and limit, with the total.
    let page = json_of(
        "module_coupling",
        json!({"offset": 2, "limit": 2}),
        &layers(),
    );
    assert_eq!(
        page["modules"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["module_id"].clone())
            .collect::<Vec<_>>(),
        [json!("cli"), json!("web")]
    );
    assert_eq!(
        (
            page["total"].clone(),
            page["has_more"].clone(),
            page["most_coupled_id"].clone()
        ),
        (json!(6), json!(true), json!("util"))
    );
    let error = run_in(
        &runtime(),
        "module_coupling",
        json!({"offset": -1}),
        &layers(),
        "json",
    );
    assert_eq!(error.exit, 2);
    assert_eq!(error.error()["code"], "INVALID_INPUT");
    let human = human_of("module_coupling", json!({}), &layers());
    assert!(
        human.starts_with("module  fan_in  fan_out  coupling\nutil    3       1        4\n"),
        "{human}"
    );
}

#[specforge_test(
    behavior = "surface_module_coupling",
    verify = "empty graph returns empty modules array"
)]
fn module_coupling_of_no_modules_is_empty() {
    let mc = json_of("module_coupling", json!({}), &G::default());
    assert_eq!(
        mc,
        json!({"modules": [], "avg_fan_in": null, "avg_fan_out": null, "most_coupled_id": null,
            "total_modules": 0, "total": 0, "offset": 0, "limit": 100, "has_more": false})
    );
}

fn coupling_of(mc: &Value, module: &str) -> Value {
    mc["modules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["module_id"] == module)
        .unwrap()
        .clone()
}

#[specforge_test(
    behavior = "pe_query_module_coupling",
    verify = "module with 3 dependents and 1 dependency has fan_in=3 fan_out=1 coupling=4"
)]
fn a_module_with_three_dependents_and_one_dependency_couples_four() {
    let mc = json_of("module_coupling", json!({}), &layers());
    assert_eq!(
        coupling_of(&mc, "util"),
        json!({"module_id": "util", "fan_in": 3, "fan_out": 1, "coupling": 4})
    );
}

#[specforge_test(
    behavior = "pe_query_module_coupling",
    verify = "isolated module has fan_in=0 fan_out=0 coupling=0"
)]
fn an_isolated_module_couples_nothing() {
    // A feature's depends_on on it does not couple it.
    let g = layers().n("f1", "feature").edge("f1", "iso", "depends_on");
    let mc = json_of("module_coupling", json!({}), &g);
    assert_eq!(
        coupling_of(&mc, "iso"),
        json!({"module_id": "iso", "fan_in": 0, "fan_out": 0, "coupling": 0})
    );
}

#[specforge_test(
    behavior = "pe_query_module_coupling",
    verify = "empty module graph returns empty modules array"
)]
fn a_graph_without_modules_has_no_coupling() {
    let mc = json_of("module_coupling", json!({}), &layered());
    assert_eq!(
        (mc["modules"].clone(), mc["total_modules"].clone()),
        (json!([]), json!(0))
    );
}

#[specforge_test(
    behavior = "pe_query_module_coupling",
    verify = "most_coupled_id identifies the highest-coupling module"
)]
fn the_most_coupled_module_is_named_first_by_id_on_ties() {
    let mc = json_of("module_coupling", json!({}), &layers());
    assert_eq!(mc["most_coupled_id"], "util");
    // Two modules depending on each other tie: the first by id.
    let g = G::default()
        .n("b", "module")
        .n("a", "module")
        .edge("a", "b", "depends_on")
        .edge("b", "a", "depends_on");
    assert_eq!(
        json_of("module_coupling", json!({}), &g)["most_coupled_id"],
        "a"
    );
}

#[specforge_test(
    behavior = "pe_query_module_coupling",
    verify = "averages are computed correctly across all modules"
)]
fn the_averages_are_over_every_module() {
    // Six dependencies among six modules; a duplicate reference counts once.
    let g = layers()
        .edge("web", "api", "depends_on")
        .n("iso2", "module");
    let mc = json_of("module_coupling", json!({}), &g);
    let (fan_in, fan_out) = (
        mc["avg_fan_in"].as_f64().unwrap(),
        mc["avg_fan_out"].as_f64().unwrap(),
    );
    assert!((fan_in - 6.0 / 7.0).abs() < 1e-9, "{fan_in}");
    assert!((fan_out - 6.0 / 7.0).abs() < 1e-9, "{fan_out}");
}

#[specforge_test(
    behavior = "pe_query_module_coupling",
    verify = "result is deterministic across repeated queries"
)]
fn module_coupling_is_the_same_every_time_and_in_any_order() {
    let first = json_of("module_coupling", json!({}), &layers());
    assert_eq!(json_of("module_coupling", json!({}), &layers()), first);
    assert_eq!(
        json_of("module_coupling", json!({}), &reversed(layers())),
        first
    );
}
