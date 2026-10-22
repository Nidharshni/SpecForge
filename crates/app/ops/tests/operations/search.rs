//! Search (`specforge_ops::read::query::search`): the entities a text and filters select, ranked once.

use specforge_extension_sdk::prelude::FieldType;
use specforge_ops::read::query::{FieldHolds, SearchRequest, search};
use specforge_test::prelude::*;

use crate::view_support::{Assembled, Viewed, answered, registries_with};

/// `alpha` (a behavior whose contract says "system"), `beta_feature` (an epic listing alpha), `gamma_orphan`
/// (an invariant), and `held` (an invariant covering alpha, guarantee "always").
fn project() -> Assembled {
    let build = registries_with(&["behavior", "invariant"], &[], |c| {
        c.edge("behaviors", |_| {});
        c.edge("covers", |_| {});
        c.kind("epic", |k| {
            k.keyword("epic");
            k.field("behaviors", |f| {
                f.field_type(FieldType::ReferenceList)
                    .edge("behaviors")
                    .target_kind("behavior");
            });
        });
        c.enhance("behavior", |e| {
            e.field("contract", |f| {
                f.field_type(FieldType::String);
            });
        });
        c.enhance("invariant", |e| {
            e.field("guarantee", |f| {
                f.field_type(FieldType::String);
            });
            e.field("covers", |f| {
                f.field_type(FieldType::ReferenceList)
                    .edge("covers")
                    .target_kind("behavior");
            });
        });
    });
    Assembled::over(build).source(
        "behavior alpha \"Alpha Behavior\" {\n  contract \"The system MUST do alpha\"\n  verify unit \"does alpha correctly\"\n}\n\
         epic beta_feature \"Beta Feature\" {\n  behaviors [alpha]\n}\n\
         invariant gamma_orphan \"Gamma Orphan\" {\n}\n\
         invariant held \"Held\" {\n  guarantee \"always\"\n  covers [alpha]\n}\n",
    )
}

/// The ids `request` finds, in rank order.
fn ids(project: &Assembled, request: &SearchRequest) -> Vec<String> {
    answered(search(&project.view(), request))
        .iter()
        .map(|hit| hit.found.node.id.raw.to_string())
        .collect()
}

fn text(text: &str) -> SearchRequest<'_> {
    SearchRequest {
        text,
        ..SearchRequest::default()
    }
}

#[specforge_test(
    behavior = "select_entities_over_the_view",
    verify = "text search finds entities matching by name or contract"
)]
fn search_reads_names_then_string_field_text() {
    let project = project();
    assert_eq!(ids(&project, &text("alpha"))[0], "alpha");
    // Contract text is searched too: "system" is in no id or title of the fixture, only in alpha's contract.
    assert_eq!(ids(&project, &text("system")), ["alpha"]);
}

#[specforge_test(
    behavior = "select_entities_over_the_view",
    verify = "empty query returns all entities up to limit"
)]
fn an_empty_text_finds_every_entity_up_to_the_limit() {
    let project = project();
    let mut all = ids(&project, &text(""));
    all.sort();
    assert_eq!(all, ["alpha", "beta_feature", "gamma_orphan", "held"]);
    let limited = SearchRequest {
        limit: Some(2),
        ..text("")
    };
    assert_eq!(ids(&project, &limited).len(), 2);
}

#[specforge_test(
    behavior = "select_entities_over_the_view",
    verify = "the references filter combines with the other filters"
)]
fn the_references_filter_is_anded_with_the_others() {
    let project = project();
    let referencing = |request: SearchRequest<'_>| {
        let mut found = ids(
            &project,
            &SearchRequest {
                referencing: Some("alpha"),
                ..request
            },
        );
        found.sort();
        found
    };
    assert_eq!(
        referencing(text("")),
        ["beta_feature", "held"],
        "the referrers alone"
    );
    assert_eq!(
        referencing(SearchRequest {
            kinds: vec!["invariant"],
            ..text("")
        }),
        ["held"],
        "kinds narrows them"
    );
    assert_eq!(
        referencing(text("beta")),
        ["beta_feature"],
        "the text narrows them"
    );
    assert!(
        referencing(text("zzz")).is_empty(),
        "no referrer matches zzz"
    );
    let field = |value| SearchRequest {
        field: Some(FieldHolds {
            field: "guarantee",
            value,
        }),
        ..text("")
    };
    assert_eq!(referencing(field("alw")), ["held"], "a field narrows them");
    assert!(referencing(field("never")).is_empty());
}

/// Contracts in every shape a string field takes: a date, an expression group and a bare word (W159), and
/// `bare`'s reference list.
pub(crate) const SHAPES: &str = "behavior dated \"Dated\" {\n  contract 2026-10-22\n}\n\
    behavior gated \"Gated\" {\n  contract expr { latency < 10ms, errors == 0 }\n}\n\
    behavior bare \"Bare\" {\n  contract widget\n  produces [logged_in]\n}\n\
    event logged_in \"Logged in\" {\n}\n";

#[ignore = "flipped by T3"]
#[specforge_test(
    behavior = "select_entities_over_the_view",
    verify = "the text search reads every field by the field-text rule, as the field filter does"
)]
fn the_text_search_reads_every_field_by_the_field_text_rule() {
    let p = crate::navigate::compile(&["@specforge/software"], &[("a.spec", SHAPES)]);
    let view = specforge_ops::view::ProjectView::of(&p.project);
    let hits = |t| answered(search(&view, &text(t)));
    let ids = |t| {
        hits(t)
            .iter()
            .map(|h| h.found.node.id.raw.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids("2026-10"), ["dated"]);
    assert_eq!(ids("widget"), ["bare"]);
    let latency = hits("latency");
    assert_eq!(latency.len(), 1);
    assert_eq!(latency[0].found.on.field_name(), Some("contract"));
    assert_eq!(latency[0].snippet.as_deref(), Some("latency < 10ms"));
    // A list is read by its items: `bare` produces logged_in.
    assert_eq!(ids("logged"), ["logged_in", "bare"]);
}
