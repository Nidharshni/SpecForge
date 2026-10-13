// @specforge/product — v1.1 behaviors
//
// Release entity kind, release edges, ownership fields, effort field,
// temporal fields, and v1.1-specific query behaviors.

use "extensions/product/invariants"
use "extensions/product/types"
use "types/zero-entity-core"

behavior pe_register_release_kind "Register Release Entity Kind" {
  features   [pe_release_coordination]
  category   command
  invariants [pe_release_non_testable]
  types      [EntityKindDescriptor, ProductRelease, ReleaseStatus]
  contract   """
    The @specforge/product extension MUST register the release entity kind
    with testable=false, supportsVerify=false, and appropriate LSP/DOT metadata.
  """
  requires {
    manifest_loaded "ExtensionDeclaration is parsed and schema-validated"
  }
  ensures {
    release_registered "KindRegistry contains release: testable=false, supportsVerify=false, semanticToken=constant, lspIcon=Constant, dotShape=doubleoctagon"
  }
  verify unit "release kind is registered with testable=false"
  verify unit "release kind has dotShape=doubleoctagon"
}

behavior pe_register_release_edges "Register Release Edge Types" {
  features [pe_release_coordination]
  category command
  types    [EdgeTypeDescriptor]
  contract """
    The @specforge/product extension MUST register 3 release-specific edge types:
    ReleaseIncludesDeliverable (release->deliverable), ReleaseCompletesMilestone
    (release->milestone) and ReleaseDependsOn (release->release).
  """
  requires {
    entity_kinds_registered "All 9 entity kinds are in the KindRegistry"
  }
  ensures {
    release_deliverable_registered "EdgeTypeRegistry contains ReleaseIncludesDeliverable: source=release, target=deliverable"
    release_milestone_registered   "EdgeTypeRegistry contains ReleaseCompletesMilestone: source=release, target=milestone"
    release_depends_on_registered  "EdgeTypeRegistry contains ReleaseDependsOn: source=release, target=release"
  }
  verify unit "ReleaseIncludesDeliverable edge type is registered"
  verify unit "ReleaseCompletesMilestone edge type is registered"
  verify unit "ReleaseDependsOn edge type is registered"
}

behavior pe_register_ownership_fields "Register Ownership Fields" {
  category command
  features [pe_ownership_tracking]
  contract """
    The @specforge/product extension MUST register owner (string @optional)
    and contributors (string[] @optional) as fields of the feature,
    milestone, deliverable, and release entity kinds.
  """
  requires {
    entity_kinds_registered "All 9 entity kinds are in the KindRegistry"
  }
  ensures {
    owner_on_four_kinds        "FieldRegistry contains owner:string @optional for kind=feature,milestone,deliverable,release"
    contributors_on_four_kinds "FieldRegistry contains contributors:string[] @optional for kind=feature,milestone,deliverable,release"
  }
  verify unit "owner field is registered on feature, milestone, deliverable, release"
  verify unit "contributors field is registered on feature, milestone, deliverable, release"
}

behavior pe_register_effort_field "Register Effort Field" {
  features [pe_effort_estimation]
  category command
  types    [Effort]
  contract """
    The @specforge/product extension MUST register effort (an optional enum of
    Effort) on the feature entity kind.
  """
  ensures {
    effort_on_feature "FieldRegistry contains effort:enum (xs, s, m, l, xl) @optional for kind=feature"
  }
  verify unit "effort field is registered on feature kind"
}

behavior pe_register_temporal_fields "Register Temporal Fields" {
  category command
  features [pe_temporal_planning]
  contract """
    The @specforge/product extension MUST register start_date (string @optional)
    on the milestone entity kind and blockers (string[] @optional) on the
    milestone entity kind.
  """
  ensures {
    start_date_on_milestone "FieldRegistry contains start_date:string @optional for kind=milestone"
    blockers_on_milestone   "FieldRegistry contains blockers:string[] @optional for kind=milestone"
  }
  verify unit "start_date field is registered on milestone kind"
  verify unit "blockers field is registered on milestone kind"
}

behavior pe_query_owner_workload "Query Owner Workload" {
  features   [pe_ownership_tracking]
  category   query
  invariants [pe_ownership_field_awareness]
  types      [OwnerWorkloadPayload, OwnerWorkloadEntry, OwnerKindBreakdown]
  contract   """
    The product extension MUST provide a query that aggregates ownership
    across features, milestones, deliverables, and releases. The result
    groups entities by owner string, counts entities per kind, and reports
    the number of unowned entities.
  """
  ensures {
    grouped_by_owner "Each unique owner string appears exactly once in the owners array"
    unowned_counted  "unowned_count equals the number of entities with no owner field"
    total_correct    "total_entities equals sum(owners[].entity_count) + unowned_count"
  }
  verify unit "single owner across multiple kinds returns correct breakdown"
  verify unit "entities without owner contribute to unowned_count"
  verify unit "empty graph returns zero totals"
}

behavior pe_query_weighted_milestone_completion "Query Weighted Milestone Completion" {
  features   [pe_effort_estimation]
  category   query
  invariants [pe_effort_weighted_completion]
  types      [WeightedMilestoneCompletionPayload, EffortBreakdownEntry, Effort]
  contract   """
    The product extension MUST provide a query that computes effort-weighted
    milestone completion. The weights are the effort scale's definition,
    Fibonacci-inspired and fixed: xs=1, s=2, m=3, l=5, xl=8. Features
    without effort weigh as m (3). No project configuration changes them
    (ADR 0011).
  """
  requires {
    milestone_exists "Milestone ID resolves to a milestone entity"
  }
  ensures {
    fixed_weights     "weights are xs=1, s=2, m=3, l=5, xl=8"
    default_to_medium "Features without effort field are weighted as m (default weight 3)"
    null_when_empty   "Milestone with zero features returns completion_ratio=null"
  }
  verify unit "milestone with xs(done) + xl(pending) returns ratio 1/9"
  verify unit "feature without effort defaults to m=3 weight"
  verify unit "empty milestone returns null completion_ratio"
}

behavior pe_query_release_completion "Query Release Completion" {
  features [pe_release_coordination]
  category query
  types    [ReleaseCompletionPayload]
  contract """
    The product extension MUST provide a query that computes release
    completion as the ratio of shipped deliverables to total deliverables.
  """
  requires {
    release_exists "Release ID resolves to a release entity"
  }
  ensures {
    shipped_counted "shipped == count of deliverables with status=shipped"
    ratio_correct   "completion_ratio == shipped / total (null when total=0)"
  }
  verify unit "release with 2/3 shipped returns ratio 0.667"
  verify unit "release with no deliverables returns null ratio"
}

behavior pe_validate_journey_flow_features "Validate Journey Flow Feature References" {
  category validation
  features [pe_journey_flow_validation]
  contract """
    Journey flow steps that contain bracketed entity references SHOULD
    resolve to features declared in the journey's features list.
    Unresolvable references produce I090.

    Extraction regex: \[([a-zA-Z_][a-zA-Z0-9_]{1,59})\]

    This matches a single opening bracket, captures an entity ID
    (2-60 chars: starts with letter or underscore, then letters, digits,
    or underscores), and a closing bracket. The entity ID character set
    matches the spec-wide entity ID format.

    Edge cases:
    - Nested brackets "[[foo]]" — the outer pair is NOT matched; the
      inner "[foo]" IS matched as a valid reference.
    - Empty brackets "[]" — not matched (minimum 2 chars in capture).
    - Escaped brackets "\[foo\]" — not treated as references (the
      backslash prevents the opening bracket from starting a match).
    - Multiple references in one step "Use [auth] then [billing]" —
      each is extracted and validated independently.
    - Non-ID content "[hello world]" — not matched (spaces are not
      valid in entity IDs).
    - Markdown links "[text](url)" — "[text]" is extracted as a
      candidate reference. If "text" is not in the journey's features
      list, I090 fires. Authors should use backticks for code references
      that are not entity IDs: `[not_a_ref]`.

    Each matched reference is checked against the journey's features[]
    field. If the reference is not in the features list, I090 is emitted
    with the unresolved ID and the flow step index.
  """
  ensures {
    regex_extraction   "references extracted via \\[([a-zA-Z_][a-zA-Z0-9_]{1,59})\\] regex"
    references_checked "each extracted entity_id is checked against the journey's features list"
    unknown_warned     "I090 emitted per unresolved reference with entity_id and step index"
    empty_safe         "empty brackets [] produce no match and no I090"
    multi_match        "multiple references in one step are each validated independently"
    escaped_ignored    "backslash-escaped brackets are not treated as references"
  }
  verify unit "flow step referencing declared feature produces no I090"
  verify unit "flow step referencing undeclared feature produces I090"
  verify unit "flow step without bracketed references produces no I090"
  verify unit "empty brackets [] produce no I090"
  verify unit "nested brackets [[foo]] match inner [foo] only"
  verify unit "multiple references in one step are each validated"
  verify unit "escaped \\[foo\\] does not match as reference"
  verify unit "non-ID content [hello world] does not match as reference"
}
