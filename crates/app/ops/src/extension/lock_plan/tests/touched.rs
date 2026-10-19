use super::*;
use crate::testing::{NEEDS_SOFTWARE_1, probes};

/// What `touched` names and what `differing` names, each sorted; `touched` runs first so what it
/// appends is in the "after" set.
fn both(plan: &mut LockPlan<'_>) -> (Vec<String>, Vec<String>) {
    let mut touched = plan.touched().ok().unwrap().names;
    let mut differing = plan.differing().ok().unwrap().names;
    touched.sort();
    differing.sort();
    (touched, differing)
}

/// The rule a removal is judged by is the difference between the peer sets; an enabling change is
/// judged by what it stages and appends. They are one rule while every member an enabling change
/// moves is a name it stages or appends: the day a verb installs and drops an entry, this fails.
#[test]
fn every_member_an_enabling_change_moves_is_a_name_it_touches() {
    let installed = nowhere(vec![
        entry("@acme/base", "1.0.0", "registry", &[]),
        entry("@acme/app", "1.0.0", "registry", &[("@acme/base", "^1")]),
    ]);
    let acme = runtime();
    let probes = probes();
    let none = nowhere(Vec::new());

    let mut rows: Vec<(&str, LockPlan<'_>, Vec<&str>)> = Vec::new();

    let mut install = plan(&installed, &acme, Verb::Install);
    stage(&mut install, local(&acme, BASE, "base.wasm"));
    rows.push((
        "an install replaces a locked one",
        install,
        vec!["@acme/base"],
    ));

    let mut closure = plan(&none, &probes, Verb::Install);
    stage(&mut closure, local(&probes, NEEDS_SOFTWARE_1, "n.wasm"));
    rows.push((
        "the closure's builtin is touched",
        closure,
        vec!["@probe/needs-software", "@specforge/software"],
    ));

    let mut companion = plan(&none, &probes, Verb::Install);
    companion.enable(Ask(0), "@specforge/software");
    companion.accompany("@specforge/software", "@specforge/testing");
    rows.push((
        "an enable and its companion",
        companion,
        vec!["@specforge/software", "@specforge/testing"],
    ));

    let mut update = plan(&installed, &acme, Verb::Update);
    stage(&mut update, local(&acme, BASE, "base.wasm"));
    rows.push(("an update moves a version", update, vec!["@acme/base"]));

    for (case, mut plan, expected) in rows {
        let (touched, differing) = both(&mut plan);
        assert_eq!(touched, expected, "{case}");
        assert!(
            differing.iter().all(|name| touched.contains(name)),
            "{case}: {differing:?} is not within {touched:?}"
        );
    }

    // The difference alone is not the rule for an install: what no entry enables does not move.
    let lone = nowhere(vec![entry("@acme/base", "1.0.0", "registry", &[])]);
    let mut update = LockPlan::updating(
        &lone,
        opened::<&str>(&[]),
        &acme,
        fetching(no_registry()),
        crate::testing::caller(),
    )
    .unwrap();
    stage(&mut update, local(&acme, BASE, "base.wasm"));
    let (touched, differing) = both(&mut update);
    assert_eq!(touched, ["@acme/base"]);
    assert!(differing.is_empty(), "{differing:?}");
}
