use crate::queries::*;
use crate::tests::G;
use specforge_extension_sdk::prelude::CommandGraph;

/// A deliverable with a journey and two modules over three features, f1
/// reached both ways.
fn shipped() -> CommandGraph {
    G::default()
        .n("d1", "deliverable")
        .n("d2", "deliverable")
        .n("j1", "journey")
        .n("j2", "journey")
        .n("mod1", "module")
        .n("dev", "persona")
        .n("cli", "channel")
        .n("f1", "feature")
        .n("f2", "feature")
        .n("f3", "feature")
        .edge("d1", "j1", "journeys")
        .edge("d1", "j2", "journeys")
        .edge("d1", "mod1", "modules")
        .edge("j1", "f1", "features")
        .edge("j1", "f2", "features")
        .edge("mod1", "f1", "features")
        .edge("mod1", "f3", "features")
        .edge("j1", "dev", "persona")
        .edge("j1", "cli", "channels")
        .build()
}

#[test]
fn deliverable_traceability_unions_both_paths() {
    let g = shipped();
    let dt = deliverable_traceability(&g, "d1").unwrap();
    assert_eq!(dt.transitive_features, ["f1", "f2", "f3"]);
    assert_eq!((dt.journey_path_count, dt.module_path_count), (2, 2));
    let empty = deliverable_traceability(&g, "d2").unwrap();
    assert!(empty.transitive_features.is_empty());
    assert!(deliverable_traceability(&g, "j1").is_none());
}

#[test]
fn feature_deliverables_follow_both_reverse_paths() {
    let g = shipped();
    let fd = feature_deliverables(&g, "f1").unwrap();
    assert_eq!(fd.deliverables, ["d1"]);
    assert_eq!((fd.via_journey_count, fd.via_module_count), (1, 1));
    assert_eq!(feature_deliverables(&g, "f3").unwrap().via_journey_count, 0);
    assert!(feature_deliverables(&g, "d1").is_none());
}

#[test]
fn persona_channels_are_the_channels_of_its_journeys() {
    let g = shipped();
    let pc = persona_channels(&g, "dev").unwrap();
    assert_eq!((pc.channels, pc.count), (vec!["cli".to_string()], 1));
    assert!(persona_channels(&g, "cli").is_none());
}

#[test]
fn deliverable_personas_name_the_journeys_that_reach_them() {
    let g = shipped();
    let dp = deliverable_personas(&g, "d1").unwrap();
    assert_eq!(dp.personas, ["dev"]);
    // j2 targets no persona: it connects none.
    assert_eq!(dp.via_journey_ids, ["j1"]);
    assert_eq!(dp.count, 1);
    assert!(deliverable_personas(&g, "d2").unwrap().personas.is_empty());
}

#[test]
fn feature_dependents_are_the_depends_on_sources() {
    let g = G::default()
        .n("f1", "feature")
        .n("f2", "feature")
        .n("f3", "feature")
        .edge("f2", "f1", "depends_on")
        .edge("f3", "f1", "depends_on")
        .build();
    let fd = feature_dependents(&g, "f1").unwrap();
    assert_eq!(
        (fd.dependents, fd.count),
        (vec!["f2".to_string(), "f3".into()], 2)
    );
    assert!(feature_dependents(&g, "nonexistent").is_none());
}

#[test]
fn persona_features_follow_the_field_or_the_reference() {
    let by_field = G::default()
        .n("p1", "persona")
        .node("j1", "journey", &[("persona", "p1")])
        .n("f1", "feature")
        .edge("j1", "f1", "features")
        .build();
    assert_eq!(persona_features(&by_field, "p1").unwrap().features, ["f1"]);
    let by_edge = G::default()
        .n("p1", "persona")
        .n("j1", "journey")
        .n("f1", "feature")
        .edge("j1", "p1", "persona")
        .edge("j1", "f1", "features")
        .build();
    let pf = persona_features(&by_edge, "p1").unwrap();
    assert_eq!(
        (pf.features, pf.via_journey_ids),
        (vec!["f1".to_string()], vec!["j1".into()])
    );
    assert!(persona_features(&by_edge, "f1").is_none());
    assert!(persona_features(&by_edge, "nonexistent").is_none());
}

#[test]
fn channel_features_deduplicate_across_journeys() {
    let g = G::default()
        .n("ch1", "channel")
        .n("j1", "journey")
        .n("j2", "journey")
        .n("f1", "feature")
        .edge("j1", "ch1", "channels")
        .edge("j2", "ch1", "channels")
        .edge("j1", "f1", "features")
        .edge("j2", "f1", "features")
        .build();
    let cf = channel_features(&g, "ch1").unwrap();
    assert_eq!(cf.features, ["f1"]);
    assert_eq!(cf.via_journey_ids, ["j1", "j2"]);
    assert_eq!(cf.count, 1);
    assert!(channel_features(&g, "f1").is_none());
    assert!(channel_features(&g, "nonexistent").is_none());
}
