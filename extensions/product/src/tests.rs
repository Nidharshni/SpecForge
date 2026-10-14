use crate::queries::*;
use specforge_extension_sdk::prelude::{
    CommandFormat, CommandGraph, CommandInput, GraphEdge, GraphNode,
};
use specforge_extension_sdk::testing::MockHost;

mod coverage_matrices;
mod dates_and_effort;
mod dependency_graphs;
mod feature_dependents;
mod host;
mod journey_coverage;
mod lists;
mod milestone_completion;
mod persona_channel_features;
mod project_wide;
mod registration_spec;
mod rollups;
mod shared_contract;
mod term_analytics;
mod traceability;

/// A graph built entity by entity.
#[derive(Default)]
struct G {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
}

impl G {
    fn node(mut self, id: &str, kind: &str, fields: &[(&str, &str)]) -> Self {
        self.nodes.push(GraphNode {
            id: id.to_string(),
            kind: kind.to_string(),
            title: Some(id.to_string()),
            fields: fields
                .iter()
                .map(|(k, v)| (k.to_string(), serde_json::json!(v)))
                .collect(),
        });
        self
    }

    fn n(self, id: &str, kind: &str) -> Self {
        self.node(id, kind, &[])
    }

    fn edge(mut self, source: &str, target: &str, label: &str) -> Self {
        self.edges.push(GraphEdge {
            source: source.to_string(),
            target: target.to_string(),
            label: label.to_string(),
        });
        self
    }

    fn build(self) -> CommandGraph {
        CommandGraph::new(self.nodes, self.edges)
    }
}

/// The ids `filter` lists over `g`, in order, as features.
fn listed(g: &CommandGraph, filter: &ListFilter) -> Vec<String> {
    list::<FeatureListEntry>(g, filter)
        .items
        .into_iter()
        .map(|e| e.id)
        .collect()
}

// ── lists ──────────────────────────────────────────────────────────────────

#[test]
fn a_list_is_of_its_kind_sorted_by_id() {
    let g = G::default()
        .n("z_feature", "feature")
        .n("a_feature", "feature")
        .n("b1", "behavior")
        .build();
    assert_eq!(
        listed(&g, &ListFilter::all(&FEATURES)),
        ["a_feature", "z_feature"]
    );
    assert_eq!(
        list::<FeatureListEntry>(&CommandGraph::default(), &ListFilter::all(&FEATURES)).total,
        0
    );
}

#[test]
fn list_filters_combine_and_an_absent_status_is_the_first() {
    let g = G::default()
        .node("f1", "feature", &[("status", "done"), ("priority", "high")])
        .node("f2", "feature", &[("priority", "high")])
        .node("f3", "feature", &[("status", "done"), ("priority", "low")])
        .build();
    let with = |equals: Vec<(&'static str, &'static str)>| {
        let mut filter = ListFilter::all(&FEATURES);
        filter.equals = equals;
        listed(&g, &filter)
    };
    assert_eq!(with(vec![("status", "done")]), ["f1", "f3"]);
    assert_eq!(with(vec![("status", "done"), ("priority", "high")]), ["f1"]);
    assert_eq!(with(vec![("status", "proposed")]), ["f2"]);
}

#[test]
fn an_absent_persona_or_channel_status_is_active() {
    // `PersonaStatus` and `ChannelStatus`: absent is treated as active.
    let g = G::default()
        .node("p1", "persona", &[("status", "deprecated")])
        .n("p2", "persona")
        .node("c1", "channel", &[("status", "active")])
        .n("c2", "channel")
        .node("c3", "channel", &[("status", "deprecated")])
        .build();
    let mut personas = ListFilter::all(&PERSONAS);
    personas.equals = vec![("status", "active")];
    let listed_personas: Vec<String> = list::<PersonaListEntry>(&g, &personas)
        .items
        .into_iter()
        .map(|e| e.id)
        .collect();
    assert_eq!(listed_personas, ["p2"]);
    let mut channels = ListFilter::all(&CHANNELS);
    channels.equals = vec![("status", "active")];
    let listed_channels: Vec<String> = list::<ChannelListEntry>(&g, &channels)
        .items
        .into_iter()
        .map(|e| e.id)
        .collect();
    assert_eq!(listed_channels, ["c1", "c2"]);
}

/// Every field with a vocabulary is declared an enum of it (its absent value
/// the default), every declared enum field is one with a vocabulary, and
/// every closed list filter reads the same vocabulary: the filter's arg is
/// `one_of` its values, with the help derived from the filter.
#[specforge_test::prelude::specforge_test(
    behavior = "validate_product_vocabularies",
    verify = "every vocabulary field is declared an enum of its vocabulary, with its absent value as default, and every closed list filter reads the same vocabulary"
)]
fn every_vocabulary_field_is_declared_with_its_vocabulary() {
    let declaration = crate::specforge_extension_build().declaration();
    let declared = |kind: &str, field: &str| {
        declaration
            .entities
            .iter()
            .filter(|k| k.keyword.as_deref() == Some(kind))
            .flat_map(|k| &k.fields)
            .find(|f| f.name == field)
            .unwrap_or_else(|| panic!("{kind}.{field} is not declared"))
            .clone()
    };
    let mut checked = 0;
    for (kind, field, vocabulary) in crate::vocabulary::fields() {
        let declared = declared(kind, field);
        assert_eq!(declared.field_type, "enum", "{kind}.{field}");
        assert_eq!(declared.enum_values, vocabulary.values(), "{kind}.{field}");
        assert_eq!(
            declared.default_value.as_deref(),
            vocabulary.absent(),
            "{kind}.{field}"
        );
        checked += 1;
    }
    assert_eq!(checked, 13, "thirteen vocabulary fields");
    // Both directions: an enum field the declaration holds has a vocabulary.
    for entity in &declaration.entities {
        let kind = entity.keyword.as_deref().unwrap();
        for field in entity.fields.iter().filter(|f| f.field_type == "enum") {
            assert!(
                crate::vocabulary::of(kind, &field.name).is_some(),
                "{kind}.{} is an enum with no vocabulary",
                field.name
            );
        }
    }
    // The retired value rules are gone.
    for rule in &declaration.validation_rules {
        assert!(
            !["W077", "W078", "W079", "W080", "W083", "W084", "W085", "W095"]
                .contains(&rule.code.as_str()),
            "{} is retired to E061",
            rule.code
        );
    }

    let surfaces: serde_json::Value = serde_json::from_str(
        &MockHost::new(crate::specforge_extension_build())
            .describe_json("surfaces")
            .unwrap(),
    )
    .unwrap();
    let commands = surfaces["items"][0]["commands"].as_array().unwrap();
    let kinds = [
        &FEATURES,
        &JOURNEYS,
        &DELIVERABLES,
        &MILESTONES,
        &MODULES,
        &TERMS,
        &PERSONAS,
        &CHANNELS,
        &RELEASES,
    ];
    let mut closed = 0;
    for kind in kinds {
        let args = commands
            .iter()
            .find(|c| c["id"] == kind.plural)
            .and_then(|c| c["args"].as_array())
            .unwrap();
        let arg = |name: &str| args.iter().find(|a| a["name"] == name).unwrap();
        for filter in kind.filters {
            let declared = arg(filter.field());
            assert_eq!(
                declared["description"],
                filter.help(kind),
                "{} --{}",
                kind.plural,
                filter.field()
            );
            match filter {
                FilterArg::Closed(field) => {
                    let vocabulary = kind.vocabulary(field).expect("a closed filter has one");
                    assert_eq!(
                        declared["arg_type"]["enum"]["values"],
                        serde_json::json!(vocabulary.values())
                    );
                    closed += 1;
                }
                FilterArg::Open { .. } => {
                    assert_eq!(
                        declared["arg_type"],
                        "string",
                        "{} --{}",
                        kind.plural,
                        filter.field()
                    )
                }
            }
        }
        assert_eq!(
            arg("sort_order")["arg_type"]["enum"]["values"],
            serde_json::json!(SORT_ORDER)
        );
    }
    assert_eq!(closed, 12, "twelve closed filters");
}

#[test]
fn effort_sorts_smallest_first() {
    let g = G::default()
        .node("a", "feature", &[("effort", "xl")])
        .node("b", "feature", &[("effort", "s")])
        .node("c", "feature", &[("effort", "m")])
        .node("d", "feature", &[("effort", "xs")])
        .n("e", "feature")
        .build();
    let mut filter = ListFilter::all(&FEATURES);
    filter.sort_by = "effort";
    assert_eq!(listed(&g, &filter), ["d", "b", "c", "a", "e"]);
    filter.descending = true;
    assert_eq!(listed(&g, &filter), ["a", "c", "b", "d", "e"]);
}

#[test]
fn a_reference_filter_matches_the_field_or_the_edge() {
    let g = G::default()
        .node("j1", "journey", &[("persona", "dev")])
        .n("j2", "journey")
        .n("j3", "journey")
        .edge("j2", "dev", "persona")
        .build();
    let mut filter = ListFilter::all(&JOURNEYS);
    filter.equals = vec![("persona", "dev")];
    let ids: Vec<String> = list::<JourneyListEntry>(&g, &filter)
        .items
        .into_iter()
        .map(|e| e.id)
        .collect();
    assert_eq!(ids, ["j1", "j2"]);
}

#[test]
fn a_list_sorts_an_enum_by_its_order_and_ties_by_id() {
    let g = G::default()
        .node("c", "feature", &[("priority", "low")])
        .node("b", "feature", &[("priority", "critical")])
        .node("a", "feature", &[("priority", "low")])
        .n("d", "feature")
        .build();
    let mut filter = ListFilter::all(&FEATURES);
    filter.sort_by = "priority";
    assert_eq!(listed(&g, &filter), ["b", "a", "c", "d"]);
    filter.descending = true;
    // Ties stay by id ascending; an entity without the field stays last.
    assert_eq!(listed(&g, &filter), ["a", "c", "b", "d"]);
    assert!(sortable("feature", "priority"));
    assert!(sortable("feature", "tags"));
    assert!(!sortable("feature", "nope"));
    assert!(!sortable("term", "status"));
}

#[test]
fn a_page_counts_before_paging_and_clamps_its_limit() {
    let page = paginate((0..5).collect(), Some(1), Some(2));
    assert_eq!(
        (page.items, page.total, page.has_more),
        (vec![1, 2], 5, true)
    );
    let page = paginate((0..5).collect::<Vec<i32>>(), Some(3), Some(2));
    assert!(!page.has_more);
    assert_eq!(
        paginate((0..5).collect::<Vec<i32>>(), None, Some(0)).limit,
        1
    );
    assert_eq!(
        paginate((0..5).collect::<Vec<i32>>(), None, Some(5000)).limit,
        1000
    );
    assert_eq!(
        paginate((0..5).collect::<Vec<i32>>(), None, None).limit,
        100
    );
    let past = paginate((0..5).collect::<Vec<i32>>(), Some(9), None);
    assert_eq!((past.items.len(), past.total, past.has_more), (0, 5, false));
}

#[test]
fn list_entries_count_their_references() {
    let g = G::default()
        .node("j1", "journey", &[("persona", "dev")])
        .n("dev", "persona")
        .n("cli", "channel")
        .n("f1", "feature")
        .n("mod1", "module")
        .n("mod2", "module")
        .edge("j1", "dev", "persona")
        .edge("j1", "cli", "channels")
        .edge("j1", "f1", "features")
        .edge("mod1", "f1", "features")
        .edge("mod1", "mod2", "depends_on")
        .build();
    let j = &list::<JourneyListEntry>(&g, &ListFilter::all(&JOURNEYS)).items[0];
    assert_eq!((j.channel_count, j.feature_count), (1, 1));
    let p = &list::<PersonaListEntry>(&g, &ListFilter::all(&PERSONAS)).items[0];
    assert_eq!(p.journey_count, 1);
    let c = &list::<ChannelListEntry>(&g, &ListFilter::all(&CHANNELS)).items[0];
    assert_eq!(c.journey_count, 1);
    let m = &list::<ModuleListEntry>(&g, &ListFilter::all(&MODULES)).items[0];
    assert_eq!(
        (m.feature_count, m.depends_on.clone()),
        (1, vec!["mod2".to_string()])
    );
}

// ── milestones and journeys ────────────────────────────────────────────────

#[test]
fn milestone_completion_counts_done_features_in_declared_order() {
    let mut g = G::default()
        .node("f2", "feature", &[("status", "done")])
        .node("f1", "feature", &[("status", "draft")])
        .edge("ms1", "f1", "features")
        .edge("ms1", "f2", "features");
    g.nodes.push(GraphNode {
        id: "ms1".into(),
        kind: "milestone".into(),
        title: None,
        fields: [
            ("status".to_string(), serde_json::json!("active")),
            ("features".to_string(), serde_json::json!(["f2", "f1"])),
        ]
        .into_iter()
        .collect(),
    });
    let mc = milestone_completion(&g.build(), "ms1").unwrap();
    assert_eq!(mc.total_features, 2);
    assert_eq!(mc.done_count, 1);
    assert_eq!(mc.done_features, ["f2"]);
    assert!((mc.completion_ratio - 0.5).abs() < 1e-9);
    assert_eq!(mc.status.as_deref(), Some("active"));
    let ids: Vec<&str> = mc.features.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, ["f2", "f1"]);
}

#[test]
fn milestone_completion_of_a_milestone_only() {
    let g = G::default()
        .n("f1", "feature")
        .n("ms1", "milestone")
        .build();
    assert!(milestone_completion(&g, "nonexistent").is_none());
    assert!(milestone_completion(&g, "f1").is_none());
    let empty = milestone_completion(&g, "ms1").unwrap();
    assert_eq!(empty.total_features, 0);
    assert_eq!(empty.completion_ratio, 0.0);
}

#[test]
fn journey_coverage_counts_done_features_not_module_ownership() {
    let g = G::default()
        .node("j1", "journey", &[("persona", "dev")])
        .n("f1", "feature")
        .node("f2", "feature", &[("status", "done")])
        .n("mod1", "module")
        .edge("j1", "f1", "features")
        .edge("j1", "f2", "features")
        .edge("mod1", "f1", "features")
        .build();
    let jc = journey_coverage(&g, "j1").unwrap();
    assert_eq!(jc.total_features, 2);
    assert_eq!(jc.covered_count, 1);
    assert_eq!(jc.uncovered_features, ["f1"]);
    assert_eq!(jc.persona.as_deref(), Some("dev"));
    assert!(journey_coverage(&g, "f1").is_none());
    assert!(journey_coverage(&g, "nonexistent").is_none());
}

// ── features ───────────────────────────────────────────────────────────────

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

// ── traceability ───────────────────────────────────────────────────────────

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

// ── project-wide ───────────────────────────────────────────────────────────

#[test]
fn bulk_status_aggregates_by_kind() {
    let g = G::default()
        .node("f1", "feature", &[("status", "done")])
        .node("f2", "feature", &[("status", "done")])
        .n("f3", "feature")
        .node("ms1", "milestone", &[("status", "active")])
        .build();
    let results = bulk_status(&g).kinds;
    let feat = results.iter().find(|r| r.kind == "feature").unwrap();
    assert_eq!(feat.total, 3);
    let counts: Vec<(&str, usize)> = feat
        .by_status
        .iter()
        .map(|s| (s.status.as_str(), s.count))
        .collect();
    assert_eq!(counts, [("(none)", 1), ("done", 2)]);
    assert_eq!(
        results
            .iter()
            .find(|r| r.kind == "milestone")
            .unwrap()
            .total,
        1
    );
    assert!(bulk_status(&CommandGraph::default()).kinds.is_empty());
}

#[test]
fn project_health_scores_coverage_and_completeness() {
    assert!((project_health(&CommandGraph::default()).score.overall - 100.0).abs() < 0.01);
    let g = G::default()
        .node("f1", "feature", &[("status", "done")])
        .n("f2", "feature")
        .n("j1", "journey")
        .n("ms1", "milestone")
        .n("ms2", "milestone")
        .edge("j1", "f1", "features")
        .edge("ms1", "f1", "features")
        .build();
    let report = project_health(&g);
    let count = |kind: &str| {
        report
            .entity_counts
            .iter()
            .find(|c| c.kind == kind)
            .unwrap()
            .count
    };
    assert_eq!((count("feature"), count("journey")), (2, 1));
    let unreferenced = report
        .unreferenced_counts
        .iter()
        .find(|c| c.kind == "feature")
        .unwrap();
    assert_eq!((unreferenced.unreferenced, unreferenced.total), (1, 2));
    assert_eq!(report.completeness.features_with_status, 1);
    assert_eq!(report.completeness.features_total, 2);
    assert_eq!(report.completeness.milestones_with_features, 1);
    assert_eq!(report.completeness.milestones_total, 2);
}

// ── rollups ────────────────────────────────────────────────────────────────

#[test]
fn a_rollup_counts_a_missing_status_as_its_kinds_default() {
    // A milestone without a status is planned, not completed; a
    // deliverable without one is draft, not shipped.
    let g = G::default()
        .n("d1", "deliverable")
        .node("d2", "deliverable", &[("status", "shipped")])
        .n("ms1", "milestone")
        .node("ms2", "milestone", &[("status", "completed")])
        .n("r1", "release")
        .edge("d1", "ms1", "milestones")
        .edge("d1", "ms2", "milestones")
        .edge("r1", "d1", "deliverables")
        .edge("r1", "d2", "deliverables")
        .build();
    let dc = deliverable_completion(&g, "d1", false).unwrap();
    assert_eq!((dc.completed_count, dc.milestone_count), (1, 2));
    assert!(dc.milestone_details.is_none());
    let rc = release_completion(&g, "r1").unwrap();
    assert_eq!(
        (rc.shipped, rc.total, rc.completion_ratio),
        (1, 2, Some(0.5))
    );
    assert!(release_completion(&g, "d1").is_none());
}

#[test]
fn a_deliverables_priority_is_its_highest_declared_one() {
    let g = G::default()
        .n("d1", "deliverable")
        .node("ms1", "milestone", &[("priority", "medium")])
        .node("j1", "journey", &[("priority", "high")])
        .n("j2", "journey")
        .edge("d1", "ms1", "milestones")
        .edge("d1", "j1", "journeys")
        .edge("d1", "j2", "journeys")
        .build();
    let dp = deliverable_priority(&g, "d1").unwrap();
    assert_eq!((dp.priority.as_deref(), dp.source_count), (Some("high"), 2));
}

#[test]
fn owners_are_ranked_by_how_much_they_own() {
    let g = G::default()
        .node("f1", "feature", &[("owner", "bo")])
        .node("f2", "feature", &[("owner", " al ")])
        .node("ms1", "milestone", &[("owner", "al")])
        .node("d1", "deliverable", &[("owner", "")])
        .node("mod1", "module", &[("owner", "al")])
        .build();
    let w = owner_workload(&g);
    let owners: Vec<(&str, usize)> = w
        .owners
        .iter()
        .map(|o| (o.owner.as_str(), o.entity_count))
        .collect();
    assert_eq!(owners, [("al", 2), ("bo", 1)]);
    assert_eq!(w.owners[0].entity_ids, ["f2", "ms1"]);
    assert_eq!((w.unowned_count, w.total_entities), (1, 4));
}

// ── dependency graphs ──────────────────────────────────────────────────────

#[test]
fn a_diamond_orders_once_and_a_cycle_is_reported_not_followed() {
    // top -> left, right -> base; c1 <-> c2; self -> self; after -> c1.
    let g = G::default()
        .n("top", "feature")
        .n("left", "feature")
        .n("right", "feature")
        .n("base", "feature")
        .n("c1", "feature")
        .n("c2", "feature")
        .n("selfish", "feature")
        .n("after", "feature")
        .edge("top", "left", "depends_on")
        .edge("top", "right", "depends_on")
        .edge("left", "base", "depends_on")
        .edge("right", "base", "depends_on")
        .edge("c1", "c2", "depends_on")
        .edge("c2", "c1", "depends_on")
        .edge("selfish", "selfish", "depends_on")
        .edge("after", "c1", "depends_on")
        .build();
    let order = feature_ordering(&g);
    assert_eq!(
        order.sorted_features,
        ["base", "left", "right", "top", "after", "c1", "c2", "selfish"]
    );
    assert!(order.has_cycles);
    assert_eq!(order.cycle_members, ["c1", "c2", "selfish"]);
}

#[test]
fn a_long_chain_is_walked_without_recursion() {
    let mut g = G::default();
    for i in 0..5000 {
        g = g.n(&format!("m{i:04}"), "module");
        if i > 0 {
            g = g.edge(&format!("m{i:04}"), &format!("m{:04}", i - 1), "depends_on");
        }
    }
    let g = g.edge("m0000", "m4999", "depends_on").build();
    let depth = module_dependency_depth(&g, "m2500").unwrap();
    assert_eq!(depth.depth, -1);
    assert_eq!(depth.longest_chain.len(), 5000);
}

#[test]
fn five_thousand_entities_chain_and_couple_without_recursion() {
    // Each kind a 5000-long chain, plus a hub every module depends on.
    let mut g = G::default().n("hub", "module");
    for kind in ["feature", "milestone", "module"] {
        for i in 0..5000 {
            let id = format!("{kind}{i:04}");
            g = g.n(&id, kind);
            if i > 0 {
                g = g.edge(&id, &format!("{kind}{:04}", i - 1), "depends_on");
            }
            if kind == "module" {
                g = g.edge(&id, "hub", "depends_on");
            }
        }
    }
    let g = g.build();
    let order = feature_ordering(&g);
    assert_eq!(
        order.sorted_features.first().map(String::as_str),
        Some("feature0000")
    );
    assert_eq!(
        order.sorted_features.last().map(String::as_str),
        Some("feature4999")
    );
    assert_eq!(critical_path(&g).path_length, 5000);
    assert_eq!(
        module_dependency_depth(&g, "module4999").unwrap().depth,
        5000
    );
    let coupling = module_coupling(&g);
    assert_eq!(coupling.most_coupled_id.as_deref(), Some("hub"));
    assert_eq!(coupling.modules[0].fan_in, 5000);
}

#[test]
fn a_module_behind_a_cycle_reports_the_cycle_it_reaches() {
    let g = G::default()
        .n("app", "module")
        .n("a", "module")
        .n("b", "module")
        .n("leaf", "module")
        .edge("app", "a", "depends_on")
        .edge("a", "b", "depends_on")
        .edge("b", "a", "depends_on")
        .edge("app", "leaf", "depends_on")
        .build();
    let depth = module_dependency_depth(&g, "app").unwrap();
    assert_eq!(
        (depth.depth, depth.longest_chain),
        (-1, vec!["a".to_string(), "b".to_string()])
    );
    let leaf = module_dependency_depth(&g, "leaf").unwrap();
    assert_eq!(
        (leaf.depth, leaf.longest_chain),
        (0, vec!["leaf".to_string()])
    );
}

/// A small deterministic generator (xorshift64*), so the randomized
/// graphs below are the same every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A random `depends_on` graph over `n` entities of `kind` (`e00`, ...):
/// with `acyclic`, each entity depends only on lower-numbered ones. The
/// entity `i`'s fields are `fields(i)`. Returns the graph and each entity's
/// dependencies.
fn random_graph(
    rng: &mut Rng,
    kind: &str,
    n: usize,
    acyclic: bool,
    fields: impl Fn(&mut Rng) -> Vec<(&'static str, &'static str)>,
) -> (CommandGraph, Vec<Vec<usize>>) {
    let id = |i: usize| format!("e{i:02}");
    let mut g = G::default();
    let mut succ = vec![Vec::new(); n];
    for (i, deps) in succ.iter_mut().enumerate() {
        let f = fields(rng);
        g = g.node(&id(i), kind, &f);
        for j in 0..n {
            if (!acyclic || j < i) && rng.below(4) == 0 {
                g = g.edge(&id(i), &id(j), "depends_on");
                deps.push(j);
            }
        }
    }
    (g.build(), succ)
}

/// Whether `from` reaches `to` over one edge or more.
fn reaches(succ: &[Vec<usize>], from: usize, to: usize) -> bool {
    let mut seen = vec![false; succ.len()];
    let mut stack = succ[from].clone();
    while let Some(u) = stack.pop() {
        if u == to {
            return true;
        }
        if !std::mem::replace(&mut seen[u], true) {
            stack.extend(&succ[u]);
        }
    }
    false
}

/// The longest chain from `u`, in edges, over the entities `keep` admits
/// (acyclic `succ`).
fn longest(succ: &[Vec<usize>], keep: &[bool], u: usize) -> usize {
    succ[u]
        .iter()
        .filter(|&&v| keep[v])
        .map(|&v| 1 + longest(succ, keep, v))
        .max()
        .unwrap_or(0)
}

fn index_of(id: &str) -> usize {
    id[1..].parse().unwrap()
}

#[test]
fn random_dependency_graphs_order_chain_and_report_cycles_as_brute_force_does() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for round in 0..300 {
        let n = 1 + rng.below(14) as usize;
        let acyclic = round % 2 == 0;
        let on_cycle =
            |succ: &[Vec<usize>]| -> Vec<bool> { (0..n).map(|v| reaches(succ, v, v)).collect() };

        // feature_ordering: every feature once, each after its dependencies
        // unless it is on or behind a cycle; the cycle members exactly.
        let (g, succ) = random_graph(&mut rng, "feature", n, acyclic, |r| match r.below(5) {
            0 => vec![("priority", "critical")],
            1 => vec![("priority", "high")],
            2 => vec![("priority", "low")],
            3 => vec![("priority", "medium")],
            _ => vec![],
        });
        let cyclic = on_cycle(&succ);
        let behind: Vec<bool> = (0..n)
            .map(|v| cyclic[v] || (0..n).any(|c| cyclic[c] && reaches(&succ, v, c)))
            .collect();
        let fo = feature_ordering(&g);
        let at: Vec<usize> = {
            let mut at = vec![usize::MAX; n];
            for (p, id) in fo.sorted_features.iter().enumerate() {
                at[index_of(id)] = p;
            }
            at
        };
        assert_eq!(fo.sorted_features.len(), n, "round {round}");
        assert!(
            at.iter().all(|&p| p < n),
            "round {round}: every feature once"
        );
        for (u, deps) in succ.iter().enumerate() {
            for &v in deps {
                if !behind[u] {
                    assert!(at[v] < at[u], "round {round}: e{v:02} before e{u:02}");
                }
            }
            if behind[u] {
                assert!(
                    (0..n).filter(|&w| !behind[w]).all(|w| at[w] < at[u]),
                    "round {round}: level-less e{u:02} comes last"
                );
            }
        }
        let members: Vec<String> = (0..n)
            .filter(|&v| cyclic[v])
            .map(|v| format!("e{v:02}"))
            .collect();
        assert_eq!(fo.cycle_members, members, "round {round}");
        assert_eq!(fo.has_cycles, !members.is_empty());

        // critical_path: no path while any cycle exists; otherwise a chain
        // of open milestones, earliest first, as long as the longest one.
        let (g, succ) = random_graph(&mut rng, "milestone", n, acyclic, |r| match r.below(3) {
            0 => vec![("status", "completed")],
            1 => vec![("status", "in_progress")],
            _ => vec![],
        });
        let cp = critical_path(&g);
        if on_cycle(&succ).contains(&true) {
            assert!(
                cp.critical_path.is_empty() && cp.message.is_some(),
                "round {round}"
            );
        } else {
            let open: Vec<bool> = (0..n)
                .map(|i| g.node(&format!("e{i:02}")).unwrap().text("status") != Some("completed"))
                .collect();
            let best = (0..n)
                .filter(|&u| open[u])
                .map(|u| 1 + longest(&succ, &open, u))
                .max()
                .unwrap_or(0);
            assert_eq!(cp.path_length, best, "round {round}");
            let path: Vec<usize> = cp
                .critical_path
                .iter()
                .map(|m| index_of(&m.entity_id))
                .collect();
            assert!(
                path.iter().all(|&m| open[m]),
                "round {round}: open milestones only"
            );
            for pair in path.windows(2) {
                assert!(
                    succ[pair[1]].contains(&pair[0]),
                    "round {round}: {path:?} is a chain"
                );
            }
        }

        // module_depth: -1 and the reached cycles' members on or behind a
        // cycle, else the longest chain; module_coupling sums to the edges.
        let (g, succ) = random_graph(&mut rng, "module", n, acyclic, |_| vec![]);
        let cyclic = on_cycle(&succ);
        let all = vec![true; n];
        for u in 0..n {
            let md = module_dependency_depth(&g, &format!("e{u:02}")).unwrap();
            let reached: Vec<String> = (0..n)
                .filter(|&c| cyclic[c] && (c == u || reaches(&succ, u, c)))
                .map(|c| format!("e{c:02}"))
                .collect();
            if reached.is_empty() {
                assert_eq!(md.depth as usize, longest(&succ, &all, u), "round {round}");
                let chain: Vec<usize> = md.longest_chain.iter().map(|m| index_of(m)).collect();
                assert_eq!((chain.len(), chain[0]), (md.depth as usize + 1, u));
                for pair in chain.windows(2) {
                    assert!(succ[pair[0]].contains(&pair[1]), "round {round}: {chain:?}");
                }
            } else {
                assert_eq!((md.depth, md.longest_chain), (-1, reached), "round {round}");
            }
        }
        let mc = module_coupling(&g);
        let edges: usize = succ.iter().map(Vec::len).sum();
        assert_eq!(mc.modules.iter().map(|m| m.fan_in).sum::<usize>(), edges);
        assert_eq!(mc.modules.iter().map(|m| m.fan_out).sum::<usize>(), edges);
    }
}

// ── coverage matrices ──────────────────────────────────────────────────────

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

// ── term analytics ─────────────────────────────────────────────────────────

#[test]
fn term_analytics_read_see_also_between_terms_only() {
    // a -> b -> c, a -> a, a -> f (a feature), d -> e, iso alone.
    let g = G::default()
        .n("a", "term")
        .n("b", "term")
        .n("c", "term")
        .n("d", "term")
        .n("e", "term")
        .n("iso", "term")
        .n("f", "feature")
        .edge("a", "b", "see_also")
        .edge("b", "c", "see_also")
        .edge("a", "a", "see_also")
        .edge("a", "f", "see_also")
        .edge("d", "e", "see_also")
        .edge("d", "e", "see_also")
        .build();
    let tg = |id: &str, hops: Option<usize>| term_graph(&g, id, hops).unwrap().related_terms;
    assert_eq!(tg("a", None), ["b"]);
    assert_eq!(tg("a", Some(2)), ["b", "c"]);
    assert_eq!(tg("a", Some(0)), Vec::<String>::new());
    assert_eq!(term_graph(&g, "a", Some(9)).unwrap().max_hops, 5);
    // A see_also is followed from its source.
    assert_eq!(tg("c", Some(5)), Vec::<String>::new());
    assert!(term_graph(&g, "f", None).is_none());
    let tc = term_clusters(&g);
    let clusters: Vec<(usize, Vec<String>)> = tc
        .clusters
        .iter()
        .map(|c| (c.cluster_id, c.term_ids.clone()))
        .collect();
    assert_eq!(
        clusters,
        [
            (1, vec!["a".into(), "b".into(), "c".into()]),
            (2, vec!["d".into(), "e".into()])
        ]
    );
    assert_eq!((tc.isolated_count, tc.total_terms), (1, 6));
    let td = term_density(&g);
    assert_eq!(
        (td.total_terms, td.total_see_also, td.max_connections),
        (6, 3, 2)
    );
    assert_eq!(td.avg_connections, Some(0.5));
    assert_eq!(td.isolated_terms, ["iso"]);
    assert!(td.hub_terms.is_empty());
    let empty = term_density(&CommandGraph::default());
    assert_eq!((empty.total_terms, empty.avg_connections), (0, None));
}

#[test]
fn random_term_graphs_reach_cluster_and_count_as_brute_force_does() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    for round in 0..300 {
        let n = rng.below(12) as usize;
        let id = |i: usize| format!("t{i:02}");
        let mut g = G::default().n("x", "feature");
        let mut edges: Vec<(usize, usize)> = Vec::new();
        for i in 0..n {
            g = g.n(&id(i), "term");
        }
        for i in 0..n {
            for j in 0..n {
                if rng.below(6) == 0 {
                    g = g.edge(&id(i), &id(j), "see_also");
                    if i != j && !edges.contains(&(i, j)) {
                        edges.push((i, j));
                    }
                }
            }
            if rng.below(4) == 0 {
                g = g.edge(&id(i), "x", "see_also");
            }
        }
        let g = g.build();
        let linked = |u: usize, v: usize| edges.contains(&(u, v)) || edges.contains(&(v, u));

        // term_graph: the terms within h directed hops, by relaxation.
        for t in 0..n {
            let hops = rng.below(7) as usize;
            let mut within = vec![t];
            for _ in 0..hops.min(5) {
                let next: Vec<usize> = edges
                    .iter()
                    .filter(|(u, _)| within.contains(u))
                    .map(|&(_, v)| v)
                    .collect();
                within.extend(next);
                within.sort_unstable();
                within.dedup();
            }
            let expected: Vec<String> = within.into_iter().filter(|&v| v != t).map(id).collect();
            let got = term_graph(&g, &id(t), Some(hops)).unwrap();
            assert_eq!(
                got.related_terms, expected,
                "round {round}: t{t:02}, {hops} hops"
            );
            assert_eq!(got.max_hops, hops.min(5));
        }

        // term_clusters: the components of the undirected links, by
        // repeated merging; isolated terms in none.
        let mut label: Vec<usize> = (0..n).collect();
        loop {
            let mut changed = false;
            for u in 0..n {
                for v in 0..n {
                    if linked(u, v) && label[u] != label[v] {
                        let low = label[u].min(label[v]);
                        label[u] = low;
                        label[v] = low;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let degree: Vec<usize> = (0..n)
            .map(|u| (0..n).filter(|&v| linked(u, v)).count())
            .collect();
        let mut components: Vec<Vec<String>> = (0..n)
            .filter(|&r| label[r] == r && degree[r] > 0)
            .map(|r| (0..n).filter(|&u| label[u] == r).map(id).collect())
            .collect();
        components
            .sort_by(|a: &Vec<String>, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));
        let tc = term_clusters(&g);
        let got: Vec<Vec<String>> = tc.clusters.iter().map(|c| c.term_ids.clone()).collect();
        assert_eq!(got, components, "round {round}");
        let isolated = degree.iter().filter(|&&d| d == 0).count();
        assert_eq!(tc.isolated_count, isolated);
        assert_eq!(
            tc.clusters.iter().map(|c| c.term_count).sum::<usize>() + tc.isolated_count,
            n
        );
        assert!(tc
            .clusters
            .iter()
            .enumerate()
            .all(|(i, c)| c.cluster_id == i + 1));

        // term_density: the counts, average, hubs and isolated terms.
        let td = term_density(&g);
        assert_eq!(
            (td.total_terms, td.total_see_also),
            (n, edges.len()),
            "round {round}"
        );
        let avg = (n > 0).then(|| edges.len() as f64 / n as f64);
        assert_eq!(td.avg_connections, avg);
        assert_eq!(
            td.max_connections,
            degree.iter().copied().max().unwrap_or(0)
        );
        let hubs: Vec<String> = (0..n)
            .filter(|&u| degree[u] >= 3 && degree[u] as f64 > 2.0 * avg.unwrap_or(0.0))
            .map(id)
            .collect();
        assert_eq!(td.hub_terms, hubs, "round {round}");
        let lone: Vec<String> = (0..n).filter(|&u| degree[u] == 0).map(id).collect();
        assert_eq!(td.isolated_terms, lone);
    }
}

// ── dates and effort ───────────────────────────────────────────────────────

#[test]
fn a_date_is_its_days_since_the_epoch_and_anything_else_is_none() {
    assert_eq!(parse_ymd("1970-01-01"), Some(0));
    assert_eq!(parse_ymd("1970-01-02"), Some(1));
    assert_eq!(parse_ymd("1969-12-31"), Some(-1));
    assert_eq!(parse_ymd("2000-03-01"), Some(11_017));
    assert_eq!(parse_ymd("2026-10-03"), Some(20_729));
    assert_eq!(
        parse_ymd("2024-02-29").map(|d| d + 1),
        parse_ymd("2024-03-01")
    );
    for bad in [
        "",
        "2026-1-03",
        "2026/10/03",
        "2026-13-01",
        "2026-00-10",
        "2026-02-29",
        "1900-02-29",
        "2026-04-31",
        "2026-10-00",
        "20261003xx",
        "2026-10-03T00",
        "２０２６-10-03",
        "+026-10-03",
    ] {
        assert_eq!(parse_ymd(bad), None, "{bad}");
    }
    assert_eq!(
        parse_ymd("2000-02-29").map(|d| d + 1),
        parse_ymd("2000-03-01")
    );
}

#[test]
fn every_day_from_1600_to_2400_follows_the_one_before() {
    // Walk the calendar by hand: each valid date is one day after the last.
    let mut expected = parse_ymd("1600-01-01").unwrap();
    for year in 1600..2400 {
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        for (month, days) in [
            31,
            if leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ]
        .into_iter()
        .enumerate()
        {
            for day in 1..=days {
                let date = format!("{year:04}-{:02}-{day:02}", month + 1);
                assert_eq!(parse_ymd(&date), Some(expected), "{date}");
                expected += 1;
            }
            assert_eq!(
                parse_ymd(&format!("{year:04}-{:02}-{:02}", month + 1, days + 1)),
                None
            );
        }
    }
}

#[test]
fn the_timeline_is_by_date_then_id_and_overdue_before_as_of() {
    let g = G::default()
        .node("late", "milestone", &[("target_date", "2026-01-10")])
        .node(
            "done",
            "milestone",
            &[("target_date", "2026-01-01"), ("status", "completed")],
        )
        .node("b", "milestone", &[("target_date", "2026-02-01")])
        .node("a", "milestone", &[("target_date", "2026-02-01")])
        .node("bad", "milestone", &[("target_date", "soon")])
        .n("undated", "milestone")
        .build();
    let as_of = parse_ymd("2026-02-01").unwrap();
    let t = milestone_timeline(&g, as_of);
    let order: Vec<(&str, bool)> = t
        .milestones
        .iter()
        .map(|m| (m.milestone_id.as_str(), m.is_overdue))
        .collect();
    // Due on the day is not overdue; a target date that is no date is
    // undated.
    assert_eq!(
        order,
        [
            ("done", false),
            ("late", true),
            ("a", false),
            ("b", false),
            ("bad", false),
            ("undated", false)
        ]
    );
    assert_eq!(t.overdue_count, 1);
    assert_eq!(t.milestones[4].target_date.as_deref(), Some("soon"));
}

#[test]
fn velocity_paces_the_features_left_by_those_done() {
    let g = G::default()
        .node(
            "ms",
            "milestone",
            &[("start_date", "2026-01-01"), ("target_date", "2026-12-31")],
        )
        .node("f1", "feature", &[("status", "done")])
        .node("f2", "feature", &[("status", "done")])
        .node("f3", "feature", &[("status", "in_progress")])
        .n("f4", "feature")
        .n("f5", "feature")
        .edge("ms", "f1", "features")
        .edge("ms", "f2", "features")
        .edge("ms", "f3", "features")
        .edge("ms", "f4", "features")
        .edge("ms", "f5", "features")
        .build();
    let day = |d: &str| parse_ymd(d).unwrap();
    let v = milestone_velocity(&g, "ms", day("2026-01-11")).unwrap();
    assert_eq!(
        (
            v.total_features,
            v.done_features,
            v.in_progress_features,
            v.remaining_features
        ),
        (5, 2, 1, 2)
    );
    assert_eq!(v.completion_ratio, Some(0.4));
    assert_eq!(v.days_elapsed, Some(10));
    assert_eq!(v.features_per_day, Some(0.2));
    // Three features left at 0.2 a day.
    assert_eq!(v.days_remaining, Some(15));
    // Before the start: no day elapsed, no pace.
    let v = milestone_velocity(&g, "ms", day("2025-12-01")).unwrap();
    assert_eq!(
        (v.days_elapsed, v.features_per_day, v.days_remaining),
        (Some(0), None, None)
    );
    assert!(milestone_velocity(&g, "f1", 0).is_none());
}

#[test]
fn weighted_completion_weighs_each_feature_by_its_effort() {
    let mut g = G::default().n("ms", "milestone").n("empty", "milestone");
    for (i, (effort, status)) in [
        ("xs", "done"),
        ("s", "done"),
        ("m", ""),
        ("l", "done"),
        ("xl", ""),
        ("", "done"),
        ("huge", ""),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("f{i}");
        let mut fields = Vec::new();
        if !effort.is_empty() {
            fields.push(("effort", effort));
        }
        if !status.is_empty() {
            fields.push(("status", status));
        }
        g = g.node(&id, "feature", &fields).edge("ms", &id, "features");
    }
    let g = g.build();
    let w = weighted_milestone_completion(&g, "ms").unwrap();
    // 1 + 2 + 3 + 5 + 8, and two that weigh as m.
    assert_eq!((w.total_effort, w.done_effort), (25, 11));
    assert_eq!(w.completion_ratio, Some(11.0 / 25.0));
    let breakdown: Vec<(&str, usize, usize)> = w
        .effort_breakdown
        .iter()
        .map(|e| (e.effort_level.as_str(), e.total, e.done))
        .collect();
    assert_eq!(
        breakdown,
        [
            ("xs", 1, 1),
            ("s", 1, 1),
            ("m", 3, 1),
            ("l", 1, 1),
            ("xl", 1, 0)
        ]
    );
    let empty = weighted_milestone_completion(&g, "empty").unwrap();
    assert_eq!((empty.total_effort, empty.completion_ratio), (0, None));
    assert!(empty.effort_breakdown.is_empty());
    assert_eq!(
        ["xs", "s", "m", "l", "xl", "huge"].map(effort_weight),
        [1, 2, 3, 5, 8, 3]
    );
}

#[test]
fn random_milestones_weigh_and_pace_as_brute_force_does() {
    let mut rng = Rng(0x94D0_49BB_1331_11EB);
    let efforts = ["xs", "s", "m", "l", "xl", "", "huge"];
    let statuses = ["done", "in_progress", "proposed", ""];
    for round in 0..300 {
        let n = rng.below(10) as usize;
        let mut g = G::default().node("ms", "milestone", &[("start_date", "2026-01-01")]);
        let (mut total, mut done_weight, mut done, mut in_progress) = (0, 0, 0, 0);
        for i in 0..n {
            let (effort, status) = (
                efforts[rng.below(7) as usize],
                statuses[rng.below(4) as usize],
            );
            let weight = match effort {
                "xs" => 1,
                "s" => 2,
                "l" => 5,
                "xl" => 8,
                _ => 3,
            };
            total += weight;
            if status == "done" {
                done_weight += weight;
                done += 1;
            }
            in_progress += usize::from(status == "in_progress");
            let fields: Vec<(&str, &str)> = [("effort", effort), ("status", status)]
                .into_iter()
                .filter(|(_, v)| !v.is_empty())
                .collect();
            let id = format!("f{i}");
            g = g.node(&id, "feature", &fields).edge("ms", &id, "features");
        }
        let g = g.build();
        let w = weighted_milestone_completion(&g, "ms").unwrap();
        assert_eq!(
            (w.total_effort, w.done_effort),
            (total, done_weight),
            "round {round}"
        );
        assert_eq!(
            w.effort_breakdown.iter().map(|e| e.total).sum::<usize>(),
            n,
            "round {round}"
        );
        assert_eq!(
            w.completion_ratio,
            (n > 0).then(|| done_weight as f64 / total as f64)
        );
        let elapsed = rng.below(400) as i64;
        let v = milestone_velocity(&g, "ms", parse_ymd("2026-01-01").unwrap() + elapsed).unwrap();
        assert_eq!(
            (
                v.done_features,
                v.in_progress_features,
                v.remaining_features
            ),
            (done, in_progress, n - done - in_progress),
            "round {round}"
        );
        assert_eq!(v.days_elapsed, Some(elapsed));
        let pace = (done > 0 && elapsed > 0).then(|| done as f64 / elapsed as f64);
        assert_eq!(v.features_per_day, pace, "round {round}");
        let left = n - done;
        let expected = if left == 0 {
            Some(0)
        } else {
            // The smallest whole number of days d with d * done >= left * elapsed.
            pace.map(|_| {
                (0..)
                    .find(|d| d * done as i64 >= left as i64 * elapsed)
                    .unwrap()
            })
        };
        assert_eq!(v.days_remaining, expected, "round {round}");
    }
}

#[test]
fn days_remaining_is_a_whole_quotient_when_the_pace_divides_evenly() {
    // 1 done in 49 days leaves 1 feature 49 days away; a float pace,
    // 1/49, puts 1 / (1/49) just above 49 and would round it to 50.
    let g = G::default()
        .node("ms", "milestone", &[("start_date", "2026-01-01")])
        .node("a", "feature", &[("status", "done")])
        .node("b", "feature", &[])
        .edge("ms", "a", "features")
        .edge("ms", "b", "features")
        .build();
    let as_of = parse_ymd("2026-01-01").unwrap() + 49;
    let v = milestone_velocity(&g, "ms", as_of).unwrap();
    assert_eq!((v.days_elapsed, v.days_remaining), (Some(49), Some(49)));
}

// ── commands ───────────────────────────────────────────────────────────────

/// Run the declared command behind `export`, as the host's call would.
fn run(export: &str, input: &CommandInput) -> Option<specforge_extension_sdk::CommandOutput> {
    crate::specforge_extension_build().call_command(export, input)
}

fn input(args: serde_json::Value, graph: CommandGraph) -> CommandInput {
    CommandInput {
        args: args.as_object().unwrap().clone(),
        cwd: "/p".into(),
        graph,
        ..Default::default()
    }
}

/// [`input`], asked for json.
fn json_input(args: serde_json::Value, graph: CommandGraph) -> CommandInput {
    CommandInput {
        format: CommandFormat::Json,
        ..input(args, graph)
    }
}

fn sample() -> CommandGraph {
    G::default()
        .node(
            "f1",
            "feature",
            &[("status", "proposed"), ("priority", "high")],
        )
        .node("f2", "feature", &[("status", "done")])
        .build()
}

#[test]
fn a_list_command_renders_human_by_default_and_json_on_request() {
    let human = run(
        "cmd__product_features",
        &input(serde_json::json!({}), sample()),
    )
    .unwrap();
    assert_eq!(human.exit_code, 0);
    assert_eq!(
        human.stdout,
        "id  title  status    priority\nf1  f1     proposed  high\nf2  f2     done      -\n"
    );
    let paged = run(
        "cmd__product_features",
        &input(serde_json::json!({"limit": 1}), sample()),
    )
    .unwrap();
    assert!(
        paged
            .stdout
            .ends_with("1 of 2 features; --offset 1 for more\n"),
        "{}",
        paged.stdout
    );
    let json = run(
        "cmd__product_features",
        &json_input(serde_json::json!({"status": "done"}), sample()),
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&json.stdout).unwrap();
    assert_eq!(value["total"], 1);
    assert_eq!(value["features"][0]["id"], "f2");
    assert!(value["features"][0].get("priority").is_none());
}

#[test]
fn a_list_command_refuses_an_arg_it_cannot_use() {
    for (args, says) in [
        (
            serde_json::json!({"limit": -1}),
            "must be a non-negative integer",
        ),
        (
            serde_json::json!({"offset": "many"}),
            "must be a non-negative integer",
        ),
        (
            serde_json::json!({"status": "draft"}),
            "status must be one of proposed,",
        ),
        (
            serde_json::json!({"priority": "urgent"}),
            "priority must be one of",
        ),
        (
            serde_json::json!({"sort_by": "nope"}),
            "a feature has no field 'nope'",
        ),
        (
            serde_json::json!({"sort_order": "up"}),
            "sort_order must be one of asc, desc",
        ),
    ] {
        let out = run("cmd__product_features", &input(args.clone(), sample())).unwrap();
        assert_eq!(out.exit_code, 2, "{args}");
        assert_eq!(out.stdout, "", "{args}");
        assert!(out.stderr.contains(says), "{args}: {}", out.stderr);
    }
}

#[test]
fn a_query_about_a_missing_entity_fails_on_stderr() {
    let out = run(
        "cmd__product_milestone_completion",
        &input(serde_json::json!({"milestone": "nope"}), sample()),
    )
    .unwrap();
    assert_eq!(out.exit_code, 1);
    assert_eq!(out.stdout, "");
    assert_eq!(out.stderr, "error: milestone 'nope' not found\n");
}

#[test]
fn an_id_list_says_none_when_empty() {
    let out = run(
        "cmd__product_feature_dependents",
        &input(serde_json::json!({"feature": "f1"}), sample()),
    )
    .unwrap();
    assert_eq!(out.stdout, "Features depending on 'f1':\n  (none)\n");
}

#[test]
fn every_declared_command_answers_its_export() {
    let described: serde_json::Value = serde_json::from_str(
        &MockHost::new(crate::specforge_extension_build())
            .describe_json("surfaces")
            .unwrap(),
    )
    .unwrap();
    let commands = described["items"][0]["commands"].as_array().unwrap();
    assert_eq!(commands.len(), 40);
    for command in commands {
        assert_eq!(
            command["export"],
            format!("cmd__product_{}", command["id"].as_str().unwrap())
        );
    }
    assert!(run("cmd__product_nope", &input(serde_json::json!({}), sample())).is_none());
}

/// Every command, called with every arg it declares set (an entity id of
/// the kind it names, a date, a sort field the kind has), answers: none
/// reads an arg it does not declare, or as another type (a panic here, a
/// trap in the host), on the path a successful call takes.
#[test]
fn every_command_answers_with_every_arg_set() {
    let kinds = [
        "feature",
        "journey",
        "deliverable",
        "milestone",
        "module",
        "term",
        "persona",
        "channel",
        "release",
    ];
    let graph = kinds
        .iter()
        .fold(G::default(), |g, kind| g.n(&format!("{kind}1"), kind))
        .build();
    let outputs = specforge_extension_sdk::testing::call_every_command(
        &crate::specforge_extension_build(),
        &graph,
        "2026-10-04",
        |_, arg| match arg {
            kind if kinds.contains(&kind) => format!("{kind}1"),
            "as_of" => "2026-10-04".to_string(),
            "sort_by" => "id".to_string(),
            _ => "x".to_string(),
        },
    );
    assert_eq!(outputs.len(), 40);
    for (id, out) in outputs {
        assert_eq!(out.exit_code, 0, "{id}: {out:?}");
    }
}

#[test]
fn a_missing_entity_id_is_invalid_input() {
    let out = run(
        "cmd__product_milestone_completion",
        &input(serde_json::json!({}), sample()),
    )
    .unwrap();
    assert_eq!(
        (out.exit_code, out.stderr.as_str()),
        (2, "error: missing required arg 'milestone'\n")
    );
}

// ── errors and tables ──────────────────────────────────────────────────────

#[test]
fn the_nearest_id_of_the_kind_within_two_edits_is_suggested() {
    let g = G::default()
        .n("launch", "milestone")
        .n("lunch", "milestone")
        .n("launchx", "feature")
        .build();
    assert_eq!(suggest(&g, "milestone", "launc").as_deref(), Some("launch"));
    // Equally near: the first by id.
    assert_eq!(
        suggest(&g, "milestone", "laanch").as_deref(),
        Some("launch")
    );
    assert_eq!(suggest(&g, "milestone", "lnch").as_deref(), Some("lunch"));
    assert_eq!(suggest(&g, "milestone", "dinner"), None);
    // Only ids of the kind asked about.
    assert_eq!(suggest(&g, "feature", "launch").as_deref(), Some("launchx"));
    assert_eq!(suggest(&g, "journey", "launch"), None);
    let error = not_found(&g, "milestone", "launc");
    assert_eq!(error.code, "ENTITY_NOT_FOUND");
    assert_eq!(error.entity_id.as_deref(), Some("launc"));
    assert_eq!(error.suggestion.as_deref(), Some("launch"));
    assert_eq!(invalid_input("bad").code, "INVALID_INPUT");
}

#[test]
fn an_error_is_json_on_stderr_when_json_was_asked_for() {
    let out = run(
        "cmd__product_milestone_completion",
        &json_input(serde_json::json!({"milestone": "nope"}), sample()),
    )
    .unwrap();
    assert_eq!((out.exit_code, out.stdout.as_str()), (1, ""));
    let error: serde_json::Value = serde_json::from_str(&out.stderr).unwrap();
    assert_eq!(
        error,
        serde_json::json!({"code": "ENTITY_NOT_FOUND", "message": "milestone 'nope' not found",
            "entity_id": "nope"})
    );
    let out = run(
        "cmd__product_features",
        &json_input(serde_json::json!({"limit": -1}), sample()),
    )
    .unwrap();
    assert_eq!(out.exit_code, 2);
    let error: serde_json::Value = serde_json::from_str(&out.stderr).unwrap();
    assert_eq!(error["code"], "INVALID_INPUT");
}

#[test]
fn a_table_aligns_its_columns_under_a_header() {
    let rows = vec![
        vec!["feature".to_string(), "done".into(), "12".into()],
        vec!["milestone".to_string(), "in_progress".into(), "3".into()],
    ];
    assert_eq!(
        crate::commands::table(&["kind", "status", "count"], &rows),
        "kind       status       count\nfeature    done         12\nmilestone  in_progress  3\n"
    );
    assert_eq!(crate::commands::table(&["id"], &[]), "id\n");
}

// ── the surfaces payload ───────────────────────────────────────────────────

/// FNV-1a, 64 bits: a fingerprint stable across Rust releases.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The `surfaces` describe payload, byte for byte, as the host receives
/// it. A deliberate surface change updates the pin, saying why:
///
/// - 35633 bytes, `0xcd3d_3de9_1ccf_a2b0`: the hand-written
///   `describe_surfaces.json`, which declaring the commands with the SDK's
///   builder reproduced byte for byte.
/// - 39985 bytes, `0xd8a4_3928_7a28_0e5e`: the 12 closed list filters
///   (`--status`, `--priority`, `--artifact-type`, `--technical-level`,
///   `--interaction-model`) and the 9 `--sort-order` args are `one_of` their
///   values rather than `string`; nothing else changed. The SDK refuses
///   another value with the message the command gave (`INVALID_INPUT`,
///   exit 2), the CLI first, and the MCP tools' schemas list the values.
/// - 40797 bytes, `0x8b30_7f56_fa96_83ca`: counts carry `minimum: 0` (the 29
///   `--limit`, `--offset` and `--max-hops` args, ADR 0017); nothing else
///   changed. The host refuses a negative one on both surfaces before the
///   export runs.
/// - 40920 bytes, `0x3f81_5be7_3dbf_98bc`: `milestone_completion`'s description says it
///   also reports what the recorded tests prove (ADR 0039); nothing else
///   changed.
/// - 40926 bytes, `0xaf70_6cd8_0efd_2f6a`: `health`'s description says "unreferenced
///   counts" where it said "orphan counts"; nothing else changed.
/// - 40406 bytes, `0x918e_c4ab_1cec_3bf5`: the closed filters' help no longer lists the
///   values (clap and MCP list them) and says what an absent value counts
///   as; nothing else changed.
#[test]
fn the_surfaces_payload_is_pinned() {
    let payload = MockHost::new(crate::specforge_extension_build())
        .describe_json("surfaces")
        .unwrap();
    assert_eq!(
        (payload.len(), fnv1a(payload.as_bytes())),
        (40406, 0x918e_c4ab_1cec_3bf5),
        "the surfaces payload changed"
    );
}

// ── lifecycle consistency and delivery evidence (ADR 0039) ────────────────

mod lifecycle_passes {
    use crate::evidence::FeatureEvidence;
    use crate::lifecycle::{pass_delivery_evidence, pass_lifecycle};
    use specforge_extension_sdk::prelude::{
        EntityEvidence, PassEdge, PassEntity, PassInput, TestResult, TestResults, TestStatus,
    };

    fn entity(id: &str, kind: &str, fields: &[(&str, &str)]) -> PassEntity {
        PassEntity {
            id: id.into(),
            kind: kind.into(),
            fields: fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..PassEntity::default()
        }
    }

    fn behavior(id: &str, verifies: &[&str]) -> PassEntity {
        PassEntity {
            testable: true,
            verify_kinds: verifies.iter().map(|_| "unit".to_string()).collect(),
            verify_texts: verifies.iter().map(|v| v.to_string()).collect(),
            ..entity(id, "behavior", &[])
        }
    }

    fn edge(source: &str, target: &str, label: &str) -> PassEdge {
        PassEdge {
            source: source.into(),
            target: target.into(),
            label: label.into(),
        }
    }

    fn codes(input: &PassInput) -> Vec<(String, String)> {
        pass_lifecycle(input)
            .into_iter()
            .map(|d| (d.code, d.entity.unwrap_or_default()))
            .collect()
    }

    #[test]
    fn w154_names_each_unfinished_feature_of_a_completed_milestone() {
        let input = PassInput {
            entities: vec![
                entity("m_done", "milestone", &[("status", "completed")]),
                entity("m_open", "milestone", &[("status", "in_progress")]),
                entity("f_done", "feature", &[("status", "done")]),
                entity("f_gone", "feature", &[("status", "deprecated")]),
                entity("f_open", "feature", &[("status", "in_progress")]),
                entity("f_bare", "feature", &[]),
            ],
            edges: vec![
                edge("m_done", "f_done", "features"),
                edge("m_done", "f_gone", "features"),
                edge("m_done", "f_open", "features"),
                edge("m_done", "f_bare", "features"),
                edge("m_open", "f_open", "features"),
            ],
            ..PassInput::default()
        };
        let found = pass_lifecycle(&input);
        let w154: Vec<&str> = found
            .iter()
            .filter(|d| d.code == "W154")
            .map(|d| d.message.as_str())
            .collect();
        assert_eq!(
            w154,
            [
                "milestone 'm_done' is completed but its feature 'f_open' is in_progress",
                "milestone 'm_done' is completed but its feature 'f_bare' is proposed",
            ]
        );
    }

    #[test]
    fn i063_i064_i065_report_status_contradictions_across_entities() {
        let input = PassInput {
            entities: vec![
                entity("f_done", "feature", &[("status", "done")]),
                entity("f_dep", "feature", &[("status", "accepted")]),
                entity("f_ok", "feature", &[("status", "done")]),
                entity("m_early", "milestone", &[("target_date", "2026-01-01")]),
                entity("m_late", "milestone", &[("target_date", "2026-03-01")]),
                entity("m_undated", "milestone", &[]),
                entity("d_shipped", "deliverable", &[("status", "shipped")]),
                entity("d_draft", "deliverable", &[]),
                entity("m_completed", "milestone", &[("status", "completed")]),
            ],
            edges: vec![
                edge("f_done", "f_dep", "depends_on"),
                edge("f_done", "f_ok", "depends_on"),
                edge("m_early", "m_late", "depends_on"),
                edge("m_late", "m_early", "depends_on"),
                edge("m_early", "m_undated", "depends_on"),
                edge("d_shipped", "m_late", "milestones"),
                edge("d_shipped", "m_completed", "milestones"),
                edge("d_draft", "m_late", "milestones"),
            ],
            ..PassInput::default()
        };
        assert_eq!(
            codes(&input),
            [
                ("I063".to_string(), "f_done".to_string()),
                ("I064".to_string(), "m_early".to_string()),
                ("I065".to_string(), "d_shipped".to_string()),
            ]
        );
    }

    fn evidence_input(test_results: Option<TestResults>) -> PassInput {
        PassInput {
            entities: vec![
                entity("f_proven", "feature", &[("status", "done")]),
                entity("f_half", "feature", &[("status", "done")]),
                entity("f_alone", "feature", &[("status", "done")]),
                entity("f_ahead", "feature", &[("status", "in_progress")]),
                behavior("b1", &["a"]),
                behavior("b2", &["a", "b"]),
                behavior("b3", &["a"]),
            ],
            edges: vec![
                edge("b1", "f_proven", "features"),
                edge("b1", "f_half", "features"),
                edge("b2", "f_half", "features"),
                edge("b3", "f_ahead", "features"),
                // A journey naming a feature does not implement it.
                edge("j1", "f_alone", "features"),
            ],
            test_results,
            ..PassInput::default()
        }
    }

    fn passing(results: &[(&str, &str)]) -> TestResults {
        let mut recorded = TestResults {
            runner: Some("fixture".into()),
            ..TestResults::default()
        };
        for (id, verify) in results {
            recorded
                .results
                .entry(id.to_string())
                .or_default()
                .tests
                .push(TestResult {
                    name: format!("{id}_{verify}"),
                    status: TestStatus::Passed,
                    verify: Some(verify.to_string()),
                    duration_ms: None,
                });
        }
        recorded
    }

    #[test]
    fn i071_names_done_features_the_recorded_tests_do_not_prove() {
        let input = evidence_input(Some(passing(&[("b1", "a"), ("b2", "a"), ("b3", "a")])));
        let out = pass_delivery_evidence(&input);
        let found: Vec<(&str, &str)> = out
            .diagnostics
            .iter()
            .map(|d| (d.entity.as_deref().unwrap(), d.message.as_str()))
            .collect();
        assert_eq!(
            found,
            [
                (
                    "f_alone",
                    "feature 'f_alone' is done but no behavior implements it, so no recorded test can prove it"
                ),
                (
                    "f_half",
                    "feature 'f_half' is done but the recorded tests prove 1 of the 2 behaviors implementing it (2/3 obligations)"
                ),
            ]
        );
        assert!(out.diagnostics.iter().all(|d| d.code == "I071"));
        assert_eq!(out.summary["done"], 3);
        assert_eq!(out.summary["proven"], 2);
        assert_eq!(out.summary["done_unproven"], 2);
        assert_eq!(out.summary["proven_not_done"], 1);
    }

    #[test]
    fn without_recorded_tests_delivery_evidence_reports_nothing() {
        let out = pass_delivery_evidence(&evidence_input(None));
        assert!(out.diagnostics.is_empty());
        assert_eq!(out.summary["evidence"], "none");
    }

    #[test]
    fn a_feature_is_proven_when_every_implementer_is() {
        let scores = |id: &str| match id {
            "ok" => Some(EntityEvidence {
                obligations: 2,
                proven: 2,
                failing: 0,
            }),
            "failing" => Some(EntityEvidence {
                obligations: 1,
                proven: 1,
                failing: 1,
            }),
            _ => None,
        };
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(FeatureEvidence::of("f", &ids(&["ok"]), scores).proven);
        assert!(!FeatureEvidence::of("f", &ids(&["ok", "failing"]), scores).proven);
        assert!(!FeatureEvidence::of("f", &ids(&["ok", "unscored"]), scores).proven);
        assert!(
            !FeatureEvidence::of("f", &[], scores).proven,
            "no implementer, no proof"
        );
    }
}

#[test]
fn milestone_completion_reports_proven_features_beside_done_ones() {
    use specforge_extension_sdk::prelude::{CommandEvidence, EntityEvidence};
    let g = G::default()
        .node("f1", "feature", &[("status", "done")])
        .node("f2", "feature", &[("status", "done")])
        .n("b1", "behavior")
        .n("b2", "behavior")
        .node("ms", "milestone", &[("status", "completed")])
        .edge("ms", "f1", "features")
        .edge("ms", "f2", "features")
        .edge("b1", "f1", "features")
        .edge("b2", "f2", "features")
        .build();
    let scored = |b1_proven: usize| CommandEvidence::Recorded {
        entities: [
            (
                "b1".to_string(),
                EntityEvidence {
                    obligations: 1,
                    proven: b1_proven,
                    failing: 0,
                },
            ),
            (
                "b2".to_string(),
                EntityEvidence {
                    obligations: 1,
                    proven: 0,
                    failing: 0,
                },
            ),
        ]
        .into_iter()
        .collect(),
    };
    let r = milestone_completion(&g, "ms")
        .unwrap()
        .with_evidence(&g, &scored(1));
    assert_eq!(r.done_count, 2, "the declared count is unchanged");
    assert_eq!(r.proven_count, Some(1));
    assert_eq!(r.proven_features.as_deref(), Some(&["f1".to_string()][..]));
    assert_eq!(r.proven_ratio, Some(0.5));
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(json["evidence"]["state"], "recorded");
    assert_eq!(json["feature_evidence"][1]["proven_obligations"], 0);

    let none = milestone_completion(&g, "ms")
        .unwrap()
        .with_evidence(&g, &CommandEvidence::None);
    let json = serde_json::to_value(&none).unwrap();
    assert_eq!(json["evidence"]["state"], "none");
    assert!(json.get("proven_count").is_none());
    let plain = serde_json::to_value(milestone_completion(&g, "ms").unwrap()).unwrap();
    assert!(plain.get("evidence").is_none(), "not asked, not reported");
}
