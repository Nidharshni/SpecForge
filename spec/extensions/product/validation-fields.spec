// Field-level validation rules — enum values, date formats, references, version formats
//
// Split from validation-rules.spec. These behaviors describe field-level
// checks declared as ValidationRulePattern entries in the @specforge/product
// manifest. The core declarative validation engine executes these patterns.
//
// Naming convention: validate_* — field-level checks (enum values, date formats, references)

use "types/diagnostics"
use "types/graph"

defaults behavior {
  features [pe_validation_suite]
}

behavior validate_product_vocabularies "Validate Product Field Vocabularies" {
  category validation
  types    [
    ProductFeature,
    ProductJourney,
    ProductDeliverable,
    ProductMilestone,
    ProductPersona,
    ProductChannel,
    ProductRelease,
    Priority,
    FeatureStatus,
    DeliverableStatus,
    MilestoneStatus,
    ReleaseStatus,
    PersonaStatus,
    ChannelStatus,
    Effort,
    ArtifactType,
    TechnicalLevel,
    InteractionModel,
    Diagnostic,
  ]
  contract """
    The @specforge/product extension MUST declare each of its closed
    vocabularies once and declare every field that takes one as an enum of
    it: feature status (FeatureStatus), priority (Priority) and effort
    (Effort); journey priority; deliverable artifact_type (ArtifactType) and
    status (DeliverableStatus); milestone status (MilestoneStatus) and
    priority; persona technical_level (TechnicalLevel) and status
    (PersonaStatus); channel interaction_model (InteractionModel) and status
    (ChannelStatus); release status (ReleaseStatus). A status field
    declares as its default the value an absent status counts as: feature
    proposed, deliverable draft, milestone and release planned, persona and
    channel active. The core refuses a written value outside the
    vocabulary with E061, naming the closest value; an absent field is not
    checked. The list commands filter and sort by the same vocabulary, and
    the lifecycle rules and rollups read an absent status as its default.
  """
  ensures {
    off_vocabulary_e061 "a vocabulary field with a value outside its vocabulary produces E061"
    absent_passes       "a vocabulary field that is not written produces nothing"
    absent_counts_as    "a status field declares the value an absent status counts as"
    lists_agree         "every value check admits, the list filters take"
  }
  verify unit "every vocabulary field is declared an enum of its vocabulary, with its absent value as default, and every closed list filter reads the same vocabulary"
  verify unit "check admits every value the list filters take, and the lists find it"
}

behavior validate_module_family_field "Validate Module Family Field" {
  category validation
  types    [Diagnostic, ProductModule, ModuleFamily]
  contract """
    The @specforge/product extension MUST declare a field_value_constraint
    one_of rule on the module family field with the ModuleFamily members:
    core, platform, extension, integration, advisory. A module whose family
    is present and not one of them MUST produce an I062 info diagnostic:
    "module '{id}' has non-standard family '{value}' — standard families:
    {allowed}", {allowed} being the ModuleFamily members. Info-level allows
    custom family values while suggesting standardization: the ModuleFamily
    enum is an open set. A module with no family field produces nothing.

    Enum semantics:
      core        — fundamental library or framework component
      platform    — user-facing binary (CLI, LSP, MCP server)
      extension   — plugin or extension package
      integration — integration adapter or bridge
      advisory    — non-code module (docs, process, governance)
  """
  ensures {
    fires_non_standard  "module with family value not in ModuleFamily enum produces I062"
    suppresses_standard "module with family value in ModuleFamily enum suppresses I062"
    absent_passes       "module with no family produces no I062"
    lists_standard      "I062 names the value and lists the standard families"
  }
  verify unit "module with family=core passes"
  verify unit "module with family=integration passes"
  verify unit "module with family=advisory passes"
  verify unit "module with non-standard family produces I062 listing the standard families"
  verify unit "module with no family produces no I062"
}

behavior validate_milestone_target_date_format "Validate Milestone Target Date Format" {
  category validation
  types    [Diagnostic, ProductMilestone]
  contract """
    The @specforge/product extension MUST declare a field_value_constraint
    matches rule on the milestone target_date field with the regex
    ^\d{4}-\d{2}-\d{2}$ (YYYY-MM-DD). A milestone whose target_date is
    present and doesn't match MUST produce an I053 info diagnostic:
    "milestone '{id}' has target_date '{value}' — expected YYYY-MM-DD".
    The regex checks the shape only, not that the date exists. Info-level
    respects incremental adoption — projects may use free-form dates
    before standardizing.
  """
  ensures {
    fires_invalid_format "milestone with non-YYYY-MM-DD target_date produces I053"
    suppresses_valid     "milestone with valid YYYY-MM-DD target_date suppresses I053"
    absent_passes        "milestone with no target_date produces no I053"
  }
  verify unit "milestone with valid YYYY-MM-DD target_date passes"
  verify unit "milestone with invalid target_date format produces I053"
  verify unit "milestone with no target_date produces no I053"
}

behavior validate_deliverable_version_format "Validate Deliverable Version Format" {
  category validation
  types    [Diagnostic, ProductDeliverable]
  contract """
    The @specforge/product extension MUST declare a field_value_constraint
    matches rule on the deliverable version field that accepts Semantic
    Versioning 2.0.0 (https://semver.org). The regex is:
      ^\d+\.\d+\.\d+(-[a-zA-Z0-9]+(\.[a-zA-Z0-9]+)*)?(\+[a-zA-Z0-9]+(\.[a-zA-Z0-9]+)*)?$
    This accepts:
      - Core versions: 1.0.0, 0.1.0, 2.3.4
      - Pre-release tags: 1.0.0-alpha, 1.0.0-alpha.1, 1.0.0-0.3.7
      - Build metadata: 1.0.0+build.42, 1.0.0-beta+exp.sha.5114f85
    A deliverable whose version is present and doesn't match (e.g., "v1.0",
    "1.0", "latest") MUST produce an I061 info diagnostic: "deliverable
    '{id}' has version '{value}' — expected semver (e.g., 1.0.0)".
    Info-level respects incremental adoption — projects may use free-form
    versions before standardizing.
  """
  ensures {
    fires_invalid_format   "deliverable with non-semver version produces I061"
    suppresses_valid       "deliverable with valid semver version suppresses I061"
    absent_passes          "deliverable with no version produces no I061"
    accepts_prerelease     "deliverable with pre-release tag (e.g., 1.0.0-alpha.1) suppresses I061"
    accepts_build_metadata "deliverable with build metadata (e.g., 1.0.0+build.42) suppresses I061"
  }
  verify unit "deliverable with valid semver version passes"
  verify unit "deliverable with invalid version format produces I061"
  verify unit "deliverable with no version produces no I061"
  verify unit "deliverable with pre-release tag passes"
  verify unit "deliverable with build metadata passes"
  verify unit "deliverable with version 'v1.0' produces I061"
}

behavior validate_tag_format "Validate Tag Format" {
  category validation
  types    [Diagnostic]
  contract """
    The @specforge/product extension MUST declare, for each of its 9 entity
    kinds, a field_value_constraint matches rule on the tags field. The
    rule sees the tags list joined with ", ", and its regex accepts a list
    whose every entry is empty or matches [a-z0-9][a-z0-9-]{0,48}[a-z0-9]:
    lowercase letters, digits and hyphens, 2 to 50 characters, not starting
    or ending with a hyphen. An entity with any other tag MUST produce one
    I068 info diagnostic naming its tags: "{kind} '{id}' has a tag that is
    not lowercase-hyphenated (2-50 of a-z, 0-9, -, not starting or ending
    with -): tags [{value}]". Empty strings in tags arrays are ignored.
    Entities of other extensions' kinds are not checked.
  """
  ensures {
    fires_on_uppercase     "tag with uppercase characters produces I068"
    fires_on_spaces        "tag with spaces produces I068"
    fires_on_underscores   "tag with underscores produces I068"
    fires_on_single_char   "single-character tag produces I068"
    fires_on_too_long      "tag exceeding 50 characters produces I068"
    suppresses_valid       "tag matching [a-z0-9][a-z0-9-]*[a-z0-9] suppresses I068"
    ignores_empty          "empty string in tags array is silently ignored"
    fires_on_special_chars "tag with special characters (!, @, #, etc.) produces I068"
  }
  verify unit "lowercase hyphen-separated tag passes"
  verify unit "uppercase tag produces I068"
  verify unit "tag with spaces produces I068"
  verify unit "tag with underscores produces I068"
  verify unit "tag with special characters produces I068"
  verify unit "single-character tag produces I068"
  verify unit "tag exceeding 50 chars produces I068"
  verify unit "empty string in tags array is silently ignored"
  verify unit "tags on every product kind are checked"
}

behavior validate_journey_flow_non_empty "Validate Journey Flow Non-Empty" {
  category validation
  types    [ProductJourney, Diagnostic]
  contract """
    The @specforge/product extension MUST declare a field_value_constraint
    non_empty rule on the journey flow field. A journey that declares an
    empty flow (flow []) MUST produce an I050 info diagnostic: "journey
    '{id}' has an empty flow". A journey with no flow field at all gets the
    core's E006 instead, since flow is required, and no I050. A journey
    with at least one flow step is valid.
  """
  ensures {
    fires_when_empty     "journey with empty flow produces I050"
    suppresses_non_empty "journey with at least one flow step suppresses I050"
    missing_is_e006      "journey without a flow field produces E006 and no I050"
  }
  verify unit "journey with empty flow produces I050"
  verify unit "journey with flow steps suppresses I050"
  verify unit "journey without a flow field produces E006 and no I050"
}

behavior validate_channel_references "Validate Channel References" {
  category validation
  types    [Diagnostic]
  contract """
    A channel reference in an entity field MUST resolve to a channel
    declared in the project. The @specforge/product extension declares
    channel fields as references, so the core resolves them: a reference
    to an undeclared channel is an unresolved reference (E003).
  """
  ensures {
    fires_when_missing "reference to undeclared channel produces E003"
    suppresses_valid   "reference to declared channel produces no diagnostic"
  }
  verify unit "a reference to an undeclared channel produces E003"
  verify unit "valid channel references pass without diagnostics"
}

behavior detect_term_see_also_non_term_refs "Detect Term See-Also Non-Term References" {
  category validation
  types    [Diagnostic, ProductTerm]
  contract """
    A term's see_also field is declared as a reference list targeting
    terms. Per decision pe_term_see_also_term_only, a see_also entry that
    resolves to an entity of another kind is the core's E022 error
    ("reference targets wrong kind"); the product extension declares no
    rule of its own for it, and no I056 exists.
  """
  ensures {
    fires_non_term  "see_also reference to a non-term entity produces E022"
    suppresses_term "see_also reference to another term produces no E022"
    empty_passes    "term with empty see_also produces no E022"
  }
  verify unit "term see_also referencing another term produces no E022"
  verify unit "term see_also referencing a module produces E022"
  verify unit "term see_also referencing a feature produces E022"
  verify unit "term with empty see_also produces no E022"
}

behavior detect_term_alias_conflicts "Detect Term Alias Conflicts" {
  category validation
  types    [Diagnostic]
  contract """
    The @specforge/product extension SHOULD detect when a term's alias
    conflicts with another term's alias or another term's ID. When two
    terms share an alias, or a term's alias matches another term's entity
    ID, it SHOULD produce a W086 warning identifying both terms and the
    conflicting alias. This prevents vocabulary ambiguity where the same
    word resolves to different definitions depending on context.
  """
  ensures {
    fires_alias_alias_conflict "two terms sharing the same alias produce W086"
    fires_alias_id_conflict    "term alias matching another term's ID produces W086"
    case_insensitive           "comparison is case-insensitive to catch 'API' vs 'api'"
    identifies_both_terms      "W086 names both conflicting terms and the shared alias"
    suppresses_no_conflict     "terms with unique aliases suppress W086"
  }
  verify unit "two terms with same alias produce W086"
  verify unit "term alias matching another term ID produces W086"
  verify unit "case-insensitive match produces W086"
  verify unit "terms with distinct aliases suppress W086"
}

behavior detect_journey_deprecated_persona "Detect Journey Referencing Deprecated Persona" {
  category validation
  types    [Diagnostic, ProductJourney, ProductPersona, PersonaStatus]
  contract """
    The @specforge/product extension SHOULD detect journeys that reference
    a persona with status=deprecated. Journeys referencing deprecated
    personas SHOULD produce a W075 warning identifying both the journey
    ID and the deprecated persona ID.
  """
  ensures {
    deprecated_detected   "journey referencing deprecated persona produces W075"
    active_suppressed     "journey referencing active persona suppresses W075"
    absent_suppressed     "journey with no persona reference suppresses W075"
    missing_status_active "persona without status field is treated as active (no W075)"
  }
  verify unit "journey referencing deprecated persona produces W075"
  verify unit "journey referencing active persona suppresses W075"
  verify unit "journey with no persona suppresses W075"
  verify unit "persona without status field treated as active"
}

behavior detect_journey_deprecated_channels "Detect Journey Referencing Deprecated Channels" {
  category validation
  types    [Diagnostic, ProductJourney, ProductChannel, ChannelStatus]
  contract """
    The @specforge/product extension SHOULD detect journeys that reference
    channels with status=deprecated. Each deprecated channel reference
    SHOULD produce a W076 warning identifying both the journey ID and
    the deprecated channel ID.
  """
  ensures {
    deprecated_detected    "journey referencing deprecated channel produces W076"
    active_suppressed      "journey referencing active channel suppresses W076"
    absent_suppressed      "journey with no channel references suppresses W076"
    missing_status_active  "channel without status field is treated as active (no W076)"
    per_channel_diagnostic "each deprecated channel in a journey produces a separate W076"
  }
  verify unit "journey referencing deprecated channel produces W076"
  verify unit "journey referencing active channel suppresses W076"
  verify unit "journey with no channels suppresses W076"
  verify unit "channel without status field treated as active"
  verify unit "journey with two deprecated channels produces two W076"
}

behavior detect_priority_effort_mismatch "Detect Priority-Effort Mismatch" {
  category   validation
  invariants [pe_effort_enum_validity]
  types      [ProductFeature, Priority, Effort, Diagnostic]
  contract   """
    The @specforge/product extension SHOULD inform when a feature has a
    high or critical priority but a very small effort estimate, suggesting
    the effort may be underestimated. Specifically:
      - priority=critical with effort in [xs, s] produces I095
      - priority=high with effort=xs produces I095
    All other priority-effort combinations are valid and produce no
    diagnostic. When either priority or effort is absent, the check is
    skipped. I095 is informational — it does not block compilation and
    respects incremental adoption where effort estimates may be rough
    initial guesses.
  """
  ensures {
    fires_critical_xs     "feature with priority=critical and effort=xs produces I095"
    fires_critical_s      "feature with priority=critical and effort=s produces I095"
    fires_high_xs         "feature with priority=high and effort=xs produces I095"
    suppresses_medium_xs  "feature with priority=medium and effort=xs produces no I095"
    suppresses_low_xs     "feature with priority=low and effort=xs produces no I095"
    suppresses_critical_m "feature with priority=critical and effort=m produces no I095"
    suppresses_high_s     "feature with priority=high and effort=s produces no I095"
    absent_effort_skips   "feature without effort field produces no I095"
    absent_priority_skips "feature without priority field produces no I095"
    identifies_feature    "I095 includes the feature ID and suggests reviewing effort estimate"
  }
  verify unit "flags critical+xs combination"
  verify unit "flags critical+s combination"
  verify unit "flags high+xs combination"
  verify unit "no flag for medium+xs"
  verify unit "no flag for high+s"
  verify unit "no flag for critical+m"
  verify unit "no flag when effort absent"
  verify unit "no flag when priority absent"
}
