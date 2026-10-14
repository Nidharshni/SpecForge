use crate::queries::*;
use crate::tests::G;

#[test]
fn feature_impact_groups_what_it_touches_by_kind() {
    let g = G::default()
        .n("f1", "feature")
        .n("f2", "feature")
        .n("f3", "feature")
        .n("f4", "feature")
        .n("j1", "journey")
        .n("ms1", "milestone")
        .n("mod1", "module")
        .n("d1", "deliverable")
        .n("d2", "deliverable")
        .edge("j1", "f1", "features")
        .edge("ms1", "f1", "features")
        .edge("mod1", "f1", "features")
        .edge("d1", "j1", "journeys")
        .edge("d1", "mod1", "modules")
        .edge("d2", "mod1", "modules")
        .edge("f1", "f2", "depends_on")
        .edge("f3", "f1", "depends_on")
        .edge("f4", "f3", "depends_on")
        .build();
    let fi = feature_impact(&g, "f1").unwrap();
    assert_eq!(fi.affected_journeys, ["j1"]);
    assert_eq!(fi.affected_milestones, ["ms1"]);
    assert_eq!(fi.affected_modules, ["mod1"]);
    assert_eq!(fi.affected_deliverables, ["d1", "d2"]);
    // Transitively: f4 depends on f3, which depends on f1.
    assert_eq!(fi.dependent_features, ["f3", "f4"]);
    assert_eq!(fi.total_affected_entities, 7);
    assert!(feature_impact(&g, "j1").is_none());
    assert!(feature_impact(&g, "nonexistent").is_none());
}

#[test]
fn a_dependency_cycle_ends_the_impact_walk() {
    let g = G::default()
        .n("f1", "feature")
        .n("f2", "feature")
        .edge("f1", "f2", "depends_on")
        .edge("f2", "f1", "depends_on")
        // A feature depending on itself is not its own dependent.
        .edge("f1", "f1", "depends_on")
        .build();
    assert_eq!(feature_impact(&g, "f1").unwrap().dependent_features, ["f2"]);
    assert_eq!(feature_impact(&g, "f2").unwrap().dependent_features, ["f1"]);
}

#[test]
fn a_feature_that_only_relates_to_the_feature_is_not_a_dependent() {
    let g = G::default()
        .n("f1", "feature")
        .n("f2", "feature")
        .n("f3", "feature")
        .edge("f2", "f1", "features")
        .edge("f3", "f1", "depends_on")
        .build();
    let fi = feature_impact(&g, "f1").unwrap();
    assert_eq!(fi.dependent_features, ["f3"]);
}
