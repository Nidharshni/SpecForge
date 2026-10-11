//! The order the registry build reads its declarations in.

use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::{DeclaredPass, build_registries};
use specforge_test_macros::test as spec;

use crate::support::{build, coded, codes, peer};

/// A pass's handler that finds nothing.
fn no_findings(_: &PassInput) -> Vec<PassDiagnostic> {
    Vec::new()
}

/// `name`, declaring the kind `Widget` (keyword `widget`) and a pass named
/// after its last segment, with `peers`.
fn widget(name: &str, peers: Vec<PeerDependency>) -> ExtensionDeclaration {
    let pass = name.rsplit('/').next().unwrap().to_string();
    let mut c = crate::support::extension(name);
    c.meta.peer_dependencies = peers;
    c.kind("Widget", |k| {
        k.keyword("widget");
    });
    c.pass(&pass, |p| {
        p.run(no_findings);
    });
    c.declaration()
}

/// `name`, with one required peer `peer_name`.
fn cyclic(name: &str, peer_name: &str) -> ExtensionDeclaration {
    let mut c = crate::support::extension(name);
    c.meta.peer_dependencies = vec![peer(peer_name, "^1.0")];
    c.kind(if name.ends_with('a') { "Alpha" } else { "Beta" }, |k| {
        k.keyword(if name.ends_with('a') { "alpha" } else { "beta" });
    });
    c.declaration()
}

#[spec(
    behavior = "registry_build_load_order",
    verify = "a dependent listed before its peer loads after it"
)]
fn a_dependent_listed_first_loads_after_its_peer() {
    let entries = || {
        [
            widget("@t/dep", vec![peer("@t/base", "^1.0")]),
            widget("@t/base", vec![]),
        ]
    };
    let build = build(entries());
    let names: Vec<&str> = build.declarations().iter().map(|d| d.name()).collect();
    assert_eq!(names, ["@t/base", "@t/dep"]);
    assert_eq!(
        build.kinds().get("widget").unwrap().source_extension,
        "@t/base"
    );
    let e026 = coded(&build, "E026");
    assert_eq!(e026.len(), 1);
    assert_eq!(
        e026[0].message,
        "entity kind 'widget' registered by '@t/dep' conflicts with '@t/base' (first registration wins)"
    );
    let passes: Vec<String> = build.passes().iter().map(DeclaredPass::full_name).collect();
    assert_eq!(passes, ["@t/base:base", "@t/dep:dep"]);

    // Building with the entries swapped gives the same registries and diagnostics.
    let [dep, base] = entries();
    let swapped = crate::support::build([base, dep]);
    let swapped_names: Vec<&str> = swapped.declarations().iter().map(|d| d.name()).collect();
    assert_eq!(swapped_names, names);
    assert_eq!(
        swapped.kinds().get("widget").unwrap().source_extension,
        "@t/base"
    );
    let messages = |b: &specforge_registry::RegistryBuild| -> Vec<String> {
        crate::support::diagnostics(b)
            .iter()
            .map(|d| d.message.clone())
            .collect()
    };
    assert_eq!(messages(&swapped), messages(&build));
}

#[spec(
    behavior = "registry_build_load_order",
    verify = "E026, W018, passes and surfaces follow the load order"
)]
fn first_wins_rules_follow_the_load_order() {
    // A dependent listed first would otherwise own the keyword, and its pass would run first.
    let build = build([
        widget("@t/dep", vec![peer("@t/base", "^1.0")]),
        widget("@t/base", vec![]),
    ]);
    assert_eq!(
        build.kinds().get("widget").unwrap().source_extension,
        "@t/base"
    );
    let passes: Vec<&str> = build
        .passes()
        .iter()
        .map(|p| p.extension.as_str())
        .collect();
    assert_eq!(passes, ["@t/base", "@t/dep"]);
}

#[spec(
    behavior = "registry_build_load_order",
    verify = "Registry Build Orders the Extensions: load order holds — declarations_in_entry_order, dependencies_first, entry_order_kept, deterministic, cycles_failed"
)]
fn the_load_order_holds() {
    let names = |b: &specforge_registry::RegistryBuild| -> Vec<String> {
        b.declarations()
            .iter()
            .map(|d| d.name().to_string())
            .collect()
    };
    // (1) dependencies_first.
    let first = build([
        widget("@t/dep", vec![peer("@t/base", "^1.0")]),
        widget("@t/base", vec![]),
    ]);
    assert_eq!(names(&first), ["@t/base", "@t/dep"]);
    // (2) entry_order_kept: no peer between them.
    let unrelated = build([widget("@t/z", vec![]), widget("@t/a", vec![])]);
    assert_eq!(names(&unrelated), ["@t/z", "@t/a"]);
    // (4) cycles_failed: a required cycle is reported once, its members loading together.
    let cycle = build([
        cyclic("@t/cyca", "@t/cycb"),
        widget("@t/z", vec![]),
        cyclic("@t/cycb", "@t/cyca"),
    ]);
    assert_eq!(names(&cycle), ["@t/cyca", "@t/cycb", "@t/z"]);
    assert_eq!(codes(&cycle), ["E027"]);
    // (3) deterministic: building from the build's own declarations changes nothing.
    for built in [&first, &unrelated, &cycle] {
        let again = build_registries(built.declarations().to_vec());
        assert_eq!(names(&again), names(built));
        assert_eq!(
            crate::support::diagnostics(&again)
                .iter()
                .map(|d| d.message.clone())
                .collect::<Vec<_>>(),
            crate::support::diagnostics(built)
                .iter()
                .map(|d| d.message.clone())
                .collect::<Vec<_>>()
        );
    }
}

#[spec(
    behavior = "registry_build_load_order",
    verify = "a cycle among required peers is one E027 naming its extensions"
)]
#[spec(
    failure_mode = "circular_peer_dependency",
    verify = "Circular Peer Dependency failure mode is handled"
)]
fn a_required_cycle_is_one_e027_naming_its_extensions() {
    let build = build([cyclic("@t/cyca", "@t/cycb"), cyclic("@t/cycb", "@t/cyca")]);
    assert_eq!(codes(&build), ["E027"]);
    let e027 = coded(&build, "E027");
    assert_eq!(
        e027[0].message,
        "cycle detected in peer dependencies: @t/cyca, @t/cycb"
    );
    assert_eq!(
        e027[0].suggestion.as_deref(),
        Some(
            "make one of these peer dependencies optional, or remove it: required peers that require each other can't load dependencies first"
        )
    );
    // Both extensions still load and register their kinds.
    assert!(build.kinds().get("alpha").is_some() && build.kinds().get("beta").is_some());
}
