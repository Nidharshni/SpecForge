use super::Rng;
use crate::queries::*;
use crate::tests::G;
use specforge_extension_sdk::prelude::CommandGraph;

#[test]
fn a_coverage_matrix_counts_a_feature_two_journeys_share_once() {
    // dev reaches f1 through both its journeys (a diamond), f2 through j2.
    let g = G::default()
        .n("f1", "feature")
        .n("f2", "feature")
        .n("f3", "feature")
        .n("dev", "persona")
        .n("ops", "persona")
        .n("cli", "channel")
        .n("j1", "journey")
        .n("j2", "journey")
        .edge("j1", "dev", "persona")
        .edge("j2", "dev", "persona")
        .edge("j1", "cli", "channels")
        .edge("j1", "f1", "features")
        .edge("j2", "f1", "features")
        .edge("j2", "f2", "features")
        .build();
    let m = persona_coverage_matrix(&g);
    assert_eq!(m.total_features, 3);
    let dev = &m.entries[0];
    assert_eq!(dev.persona_id, "dev");
    assert_eq!(dev.reach.reachable_features, ["f1", "f2"]);
    assert_eq!(dev.reach.unreachable_features, ["f3"]);
    assert_eq!(dev.reach.journey_count, 2);
    assert!((dev.reach.coverage_ratio - 2.0 / 3.0).abs() < 1e-12);
    assert_eq!(m.entries[1].reach.coverage_ratio, 0.0);
    assert_eq!(m.overall_coverage, Some(1.0 / 3.0));
    let c = channel_coverage_matrix(&g);
    assert_eq!(c.entries[0].reach.reachable_features, ["f1"]);
    assert_eq!(
        channel_coverage_matrix(&CommandGraph::default()).overall_coverage,
        None
    );
}

/// A random plan: features, journeys exercising them, personas and channels
/// the journeys target and use, deliverables holding journeys and modules,
/// modules holding features; some references go to a node of another kind.
fn random_plan(rng: &mut Rng) -> CommandGraph {
    let sizes = [
        ("feature", "f", 1 + rng.below(8)),
        ("journey", "j", rng.below(6)),
        ("persona", "p", rng.below(4)),
        ("channel", "c", rng.below(4)),
        ("deliverable", "d", rng.below(5)),
        ("module", "m", rng.below(4)),
    ];
    let mut g = G::default();
    for (kind, prefix, n) in sizes {
        for i in 0..n {
            g = g.n(&format!("{prefix}{i}"), kind);
        }
    }
    let ids = |prefix: &str| -> Vec<String> {
        let n = sizes.iter().find(|s| s.1 == prefix).unwrap().2;
        (0..n).map(|i| format!("{prefix}{i}")).collect()
    };
    for (source, label, target) in [
        ("j", "features", "f"),
        ("j", "persona", "p"),
        ("j", "channels", "c"),
        ("d", "journeys", "j"),
        ("d", "modules", "m"),
        ("m", "features", "f"),
        // A peer's reference under the same label: not followed.
        ("p", "features", "f"),
        ("j", "features", "m"),
    ] {
        for s in ids(source) {
            for t in ids(target) {
                if rng.below(3) == 0 {
                    g = g.edge(&s, &t, label);
                }
            }
        }
    }
    g.build()
}

/// The `kind` sources of `label` references to `id`, and the targets of
/// `id`'s, each by scanning every edge.
fn sources(g: &CommandGraph, id: &str, label: &str, kind: &str) -> Vec<String> {
    let mut s: Vec<String> = g
        .nodes_of_kind(kind)
        .filter(|n| {
            g.edges_from(&n.id)
                .iter()
                .any(|e| e.label == label && e.target == id)
        })
        .map(|n| n.id.clone())
        .collect();
    s.sort();
    s
}

#[test]
fn random_plans_cover_and_overlap_as_brute_force_does() {
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    for round in 0..300 {
        let g = random_plan(&mut rng);
        let mut features: Vec<String> = g.nodes_of_kind("feature").map(|n| n.id.clone()).collect();
        features.sort();
        let reach_of = |journeys: &[String]| -> Vec<String> {
            features
                .iter()
                .filter(|f| {
                    journeys
                        .iter()
                        .any(|j| sources(&g, f, "features", "journey").contains(j))
                })
                .cloned()
                .collect()
        };
        let check =
            |kind: &str, label: &str, entries: Vec<(String, &Reach)>, overall: Option<f64>| {
                let mut ids: Vec<String> = g.nodes_of_kind(kind).map(|n| n.id.clone()).collect();
                ids.sort();
                assert_eq!(
                    entries.iter().map(|e| e.0.clone()).collect::<Vec<_>>(),
                    ids,
                    "round {round}: one entry per {kind}, by id"
                );
                let mut sum = 0.0;
                for (id, reach) in &entries {
                    let journeys = sources(&g, id, label, "journey");
                    let reachable = reach_of(&journeys);
                    assert_eq!(reach.reachable_features, reachable, "round {round}: {id}");
                    assert_eq!(
                        reach.reachable_features.len() + reach.unreachable_features.len(),
                        features.len()
                    );
                    assert!(reach
                        .unreachable_features
                        .iter()
                        .all(|f| !reachable.contains(f)));
                    assert_eq!(reach.journey_count, journeys.len());
                    assert_eq!(
                        reach.coverage_ratio,
                        reachable.len() as f64 / features.len() as f64
                    );
                    sum += reach.coverage_ratio;
                }
                let mean = (!entries.is_empty()).then(|| sum / entries.len() as f64);
                assert_eq!(overall, mean, "round {round}");
            };
        let pm = persona_coverage_matrix(&g);
        check(
            "persona",
            "persona",
            pm.entries
                .iter()
                .map(|e| (e.persona_id.clone(), &e.reach))
                .collect(),
            pm.overall_coverage,
        );
        let cm = channel_coverage_matrix(&g);
        check(
            "channel",
            "channels",
            cm.entries
                .iter()
                .map(|e| (e.channel_id.clone(), &e.reach))
                .collect(),
            cm.overall_coverage,
        );

        // feature_overlap: exactly the features 2+ deliverables reach, each
        // with all of them, most shared first.
        let mut expected: Vec<(String, Vec<String>)> = features
            .iter()
            .filter_map(|f| {
                let mut ds: Vec<String> = g
                    .nodes_of_kind("deliverable")
                    .filter(|d| {
                        let hold = |label: &str, kind: &str| {
                            g.edges_from(&d.id).iter().any(|e| {
                                e.label == label
                                    && g.node(&e.target).is_some_and(|n| n.kind == kind)
                                    && sources(&g, f, "features", kind).contains(&e.target)
                            })
                        };
                        hold("journeys", "journey") || hold("modules", "module")
                    })
                    .map(|d| d.id.clone())
                    .collect();
                ds.sort();
                (ds.len() >= 2).then(|| (f.clone(), ds))
            })
            .collect();
        expected.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
        let overlap = feature_overlap(&g);
        let got: Vec<(String, Vec<String>)> = overlap
            .overlapping_features
            .iter()
            .map(|e| {
                assert_eq!(e.deliverable_count, e.deliverable_ids.len());
                (e.feature_id.clone(), e.deliverable_ids.clone())
            })
            .collect();
        assert_eq!(got, expected, "round {round}");
        assert_eq!(overlap.total_features, features.len());
    }
}
