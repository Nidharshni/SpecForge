//! Search (`specforge_ops::read::query::search`): the entities a text and filters select, ranked once.

use specforge_extension_sdk::prelude::FieldType;
use specforge_ops::read::query::{FieldHolds, SearchRequest, search};
use specforge_test::prelude::*;

use crate::view_support::{Assembled, Viewed, plain, registries_with};

/// `alpha` (a behavior whose contract says "system"), `beta_feature` (a feature listing alpha), `gamma_orphan`
/// (an invariant), and `held` (an invariant enforced by alpha, guarantee "always").
fn project() -> Assembled {
    let build = registries_with(&["behavior", "invariant"], &[], |c| {
        c.edge("behaviors", |_| {});
        c.edge("enforced_by", |_| {});
        c.kind("feature", |k| {
            k.keyword("feature");
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
            e.field("enforced_by", |f| {
                f.field_type(FieldType::ReferenceList)
                    .edge("enforced_by")
                    .target_kind("behavior");
            });
        });
    });
    Assembled::over(build).source(
        "behavior alpha \"Alpha Behavior\" {\n  contract \"The system MUST do alpha\"\n  verify unit \"does alpha correctly\"\n}\n\
         feature beta_feature \"Beta Feature\" {\n  behaviors [alpha]\n}\n\
         invariant gamma_orphan \"Gamma Orphan\" {\n}\n\
         invariant held \"Held\" {\n  guarantee \"always\"\n  enforced_by [alpha]\n}\n",
    )
}

/// The ids `request` finds, in rank order.
fn ids(project: &Assembled, request: &SearchRequest) -> Vec<String> {
    plain(search(&project.view(), request))
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
    let unknown = SearchRequest {
        referencing: Some("nope"),
        ..text("")
    };
    assert!(
        ids(&project, &unknown).is_empty(),
        "an unknown target has no referrers"
    );
}
