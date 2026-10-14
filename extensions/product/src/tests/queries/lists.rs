use crate::queries::*;
use crate::tests::G;
use specforge_extension_sdk::prelude::CommandGraph;
use specforge_extension_sdk::testing::MockHost;

/// The ids `filter` lists over `g`, in order, as features.
fn listed(g: &CommandGraph, filter: &ListFilter) -> Vec<String> {
    list::<FeatureListEntry>(g, filter)
        .items
        .into_iter()
        .map(|e| e.id)
        .collect()
}

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
