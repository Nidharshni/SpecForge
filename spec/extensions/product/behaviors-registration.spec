// @specforge/product — Registration and field validation behaviors
//
// Entity kind registration, edge type registration, field definitions,
// validation rules, and field-level validation for product entities.

use "extensions/product/invariants"
use "extensions/product/ports"
use "extensions/product/types"
use "product/features"
use "types/diagnostics"
use "types/zero-entity-core"

behavior pe_register_entity_kinds "Register Product Entity Kinds" {
  category   command
  invariants [
    pe_feature_non_testable,
    pe_persona_non_testable,
    pe_channel_non_testable,
    pe_product_verify_support,
  ]
  types      [
    EntityKindDescriptor,
    ProductFeature,
    ProductJourney,
    ProductDeliverable,
    ProductMilestone,
    ProductModule,
    ProductTerm,
    ProductPersona,
    ProductChannel,
    ProductRelease,
    ProductEntityKindsRegisteredPayload,
    ProductEntityRegistrationPayload,
    RegistrationError,
  ]
  produces   [pe_entity_kinds_registered]
  contract   """
    The @specforge/product extension MUST register 9 entity kinds with
    full metadata in the KindRegistry.
  """
  requires {
    manifest_loaded    "ExtensionDeclaration is parsed and schema-validated"
    no_duplicate_kinds "KindRegistry has no entries with names matching this extension's kinds"
  }
  ensures {
    journey_registered     "KindRegistry contains journey: testable=false, supportsVerify=false, semanticToken=event, lspIcon=Event, dotShape=ellipse"
    deliverable_registered "KindRegistry contains deliverable: testable=false, supportsVerify=false, semanticToken=struct, lspIcon=Package, dotShape=box3d"
    milestone_registered   "KindRegistry contains milestone: testable=false, supportsVerify=false, semanticToken=namespace, lspIcon=Folder, dotShape=hexagon"
    module_registered      "KindRegistry contains module: testable=false, supportsVerify=false, semanticToken=namespace, lspIcon=Module, dotShape=component"
    term_registered        "KindRegistry contains term: testable=false, supportsVerify=false, semanticToken=string, lspIcon=Text, dotShape=note"
    feature_registered     "KindRegistry contains feature: testable=false, supportsVerify=false, semanticToken=class, lspIcon=Class, dotShape=box"
    persona_registered     "KindRegistry contains persona: testable=false, supportsVerify=false, semanticToken=variable, lspIcon=Variable, dotShape=ellipse"
    channel_registered     "KindRegistry contains channel: testable=false, supportsVerify=false, semanticToken=interface, lspIcon=Interface, dotShape=rectangle"
    release_registered     "KindRegistry contains release: testable=false, supportsVerify=false, semanticToken=constant, lspIcon=Constant, dotShape=doubleoctagon"
    nine_kinds_total       "KindRegistry has exactly 9 domain entries after registration"
  }
  ports      [ProductRegistrationPort, KindRegistryPort]
  features   [pe_core_entity_kinds, product_entity_registration]
  verify unit "journey registered with testable=false"
  verify unit "deliverable registered with testable=false, supportsVerify=false"
  verify unit "milestone registered with dotShape=hexagon"
  verify unit "module registered with lspIcon=Module"
  verify unit "term registered with testable=false"
  verify unit "feature registered with testable=false, supportsVerify=false, dotShape=box"
  verify unit "persona registered with testable=false and dotShape=ellipse"
  verify unit "channel registered with testable=false and dotShape=rectangle"
  verify unit "release registered with testable=false"
}

behavior pe_register_edge_types "Register Product Edge Types" {
  category command
  types    [EdgeTypeDescriptor, ProductEdgeTypesRegisteredPayload]
  produces [pe_edge_types_registered]
  contract """
    The @specforge/product extension MUST register 20 edge types that
    model relationships between the 9 entity kinds.
  """
  requires {
    kinds_registered "all 9 entity kinds are in KindRegistry"
  }
  ensures {
    feature_depends_on     "EdgeTypeSet contains FeatureDependsOn (feature->feature)"
    feature_relates_to     "EdgeTypeSet contains FeatureRelatesTo (feature->feature)"
    journey_feature        "EdgeTypeSet contains JourneyExercisesFeature (journey->feature)"
    journey_persona        "EdgeTypeSet contains JourneyTargetsPersona (journey->persona)"
    journey_channel        "EdgeTypeSet contains JourneyUsesChannel (journey->channel)"
    deliverable_journey    "EdgeTypeSet contains DeliverableSupportsJourney (deliverable->journey)"
    deliverable_module     "EdgeTypeSet contains DeliverableContainsModule (deliverable->module)"
    deliverable_milestone  "EdgeTypeSet contains DeliverableTrackedByMilestone (deliverable->milestone)"
    deliverable_depends_on "EdgeTypeSet contains DeliverableDependsOn (deliverable->deliverable)"
    milestone_feature      "EdgeTypeSet contains MilestoneDeliversFeature (milestone->feature)"
    milestone_module       "EdgeTypeSet contains MilestoneScopesModule (milestone->module)"
    milestone_depends_on   "EdgeTypeSet contains MilestoneDependsOn (milestone->milestone)"
    module_feature         "EdgeTypeSet contains ModuleContainsFeature (module->feature)"
    module_depends_on      "EdgeTypeSet contains ModuleDependsOn (module->module)"
    term_see_also          "EdgeTypeSet contains TermReferencesRelatedTerm (term->term)"
    term_module            "EdgeTypeSet contains TermBelongsToModule (term->module)"
    release_deliverable    "EdgeTypeSet contains ReleaseIncludesDeliverable (release->deliverable)"
    release_milestone      "EdgeTypeSet contains ReleaseCompletesMilestone (release->milestone)"
    release_depends_on     "EdgeTypeSet contains ReleaseDependsOn (release->release)"
    persona_feature        "EdgeTypeSet contains PersonaPrioritizesFeature (persona->feature)"
    twenty_edges_total     "EdgeTypeSet has exactly 20 entries"
  }
  ports    [ProductRegistrationPort, EdgeTypeRegistryPort]
  features [pe_core_entity_kinds, product_entity_registration]
  verify unit "all 20 edge types registered in edge set"
  verify unit "JourneyExercisesFeature edge has sourceKind=journey and targetKind=feature"
  verify unit "ModuleDependsOn edge has sourceKind=module and targetKind=module"
  verify unit "FeatureDependsOn edge has sourceKind=feature and targetKind=feature"
  verify unit "DeliverableContainsModule edge has sourceKind=deliverable and targetKind=module"
  verify unit "ModuleContainsFeature edge has sourceKind=module and targetKind=feature"
  verify unit "JourneyTargetsPersona edge has sourceKind=journey and targetKind=persona"
  verify unit "JourneyUsesChannel edge has sourceKind=journey and targetKind=channel"
  verify unit "MilestoneScopesModule edge has sourceKind=milestone and targetKind=module"
  verify unit "TermReferencesRelatedTerm edge has sourceKind=term and targetKind=term"
  verify unit "MilestoneDependsOn edge has sourceKind=milestone and targetKind=milestone"
  verify unit "DeliverableTrackedByMilestone edge has sourceKind=deliverable and targetKind=milestone"
  verify unit "DeliverableDependsOn edge has sourceKind=deliverable and targetKind=deliverable"
  verify unit "ReleaseIncludesDeliverable edge has sourceKind=release and targetKind=deliverable"
  verify unit "ReleaseCompletesMilestone edge has sourceKind=release and targetKind=milestone"
}

behavior pe_register_field_definitions "Register Product Field Definitions" {
  category command
  types    [FieldDescriptor, EntityKindDescriptor, ProductFieldsRegisteredPayload]
  produces [pe_field_definitions_registered]
  contract """
    The @specforge/product extension MUST register field definitions for
    each entity kind with name, type, edge mapping, and target kind.
  """
  requires {
    kinds_and_edges_registered "all 9 kinds and 20 edge types are registered"
  }
  ensures {
    feature_fields     "feature has: description(string), problem(string, required), solution(string), priority(enum Priority), status(enum FeatureStatus, default proposed), acceptance(string_list), depends_on(reference_list->feature, FeatureDependsOn), features(reference_list->feature, FeatureRelatesTo), refs(string_list), reason(string), owner(string), contributors(string_list), effort(enum Effort), tests(string_list)"
    journey_fields     "journey has: persona(reference->persona, JourneyTargetsPersona), description(string), channels(reference_list->channel, JourneyUsesChannel), features(reference_list->feature, JourneyExercisesFeature), flow(string_list, required), priority(enum Priority)"
    deliverable_fields "deliverable has: description(string), artifact_type(enum ArtifactType, required), status(enum DeliverableStatus, default draft), journeys(reference_list->journey, DeliverableSupportsJourney), modules(reference_list->module, DeliverableContainsModule), version(string), milestones(reference_list->milestone, DeliverableTrackedByMilestone), depends_on(reference_list->deliverable, DeliverableDependsOn), reason(string), owner(string), contributors(string_list)"
    milestone_fields   "milestone has: description(string), status(enum MilestoneStatus, default planned), features(reference_list->feature, MilestoneDeliversFeature), exit_criteria(string_list), target_date(string), start_date(string), modules(reference_list->module, MilestoneScopesModule), depends_on(reference_list->milestone, MilestoneDependsOn), blockers(string_list), priority(enum Priority), reason(string), owner(string), contributors(string_list), refs(string_list)"
    module_fields      "module has: family(string), description(string), features(reference_list->feature, ModuleContainsFeature), depends_on(reference_list->module, ModuleDependsOn), reason(string)"
    term_fields        "term has: definition(string, required), context(string), aliases(string_list), see_also(reference_list->term, TermReferencesRelatedTerm), module(reference->module, TermBelongsToModule)"
    persona_fields     "persona has: description(string, required), technical_level(enum TechnicalLevel), goals(string_list), pain_points(string_list), status(enum PersonaStatus, default active), reason(string), key_features(reference_list->feature, PersonaPrioritizesFeature)"
    channel_fields     "channel has: description(string, required), interaction_model(enum InteractionModel), url(string), status(enum ChannelStatus, default active), reason(string)"
    release_fields     "release has: description(string), version(string, required), status(enum ReleaseStatus, default planned), deliverables(reference_list->deliverable, ReleaseIncludesDeliverable), milestones(reference_list->milestone, ReleaseCompletesMilestone), target_date(string), release_date(string), changelog(string), depends_on(reference_list->release, ReleaseDependsOn), owner(string), contributors(string_list), reason(string), refs(string_list)"
    shared_tags        "every kind also gets the shared field tags(string_list)"
    status_defaults    "each status field declares the value an absent status counts as as its default_value; no other field declares one"
  }
  ports    [FieldRegistryPort]
  features [pe_core_entity_kinds, product_entity_registration]
  verify unit "feature problem field registered as string type"
  verify unit "feature depends_on field registered with FeatureDependsOn edge"
  verify unit "journey features field registered with JourneyExercisesFeature edge"
  verify unit "module depends_on field registered with ModuleDependsOn edge"
  verify unit "deliverable modules field registered with DeliverableContainsModule edge"
  verify unit "module features field registered with ModuleContainsFeature edge"
  verify unit "milestone modules field registered with MilestoneScopesModule edge"
  verify unit "milestone depends_on field registered with MilestoneDependsOn edge"
  verify unit "term see_also field registered with TermReferencesRelatedTerm edge"
  verify unit "deliverable milestones field registered with DeliverableTrackedByMilestone edge"
  verify unit "deliverable depends_on field registered with DeliverableDependsOn edge"
  verify unit "persona pain_points field registered as string_list type"
}

behavior pe_register_validation_rules "Register Product Validation Rules" {
  category command
  types    [ValidationRulePattern, ValidationPatternKind, ProductValidationError]
  contract """
    The @specforge/product extension MUST register declarative validation
    rules in its manifest. A value outside a field's vocabulary is the
    core's E061 (validate_product_vocabularies), not a rule.
  """
  requires {
    field_definitions_registered "all field definitions for 9 kinds and 20 edge types are in FieldRegistry"
  }
  ensures {
    rules_registered "50 declarative validation rules over 38 diagnostic codes are registered: E007, E015, E052, W041-W046, W049, W057, W092, W093, I010, I046-I048, I050, I053-I055, I057, I059-I062, I066-I070, I080-I083, I086, I087, I089 (I068 once per product kind; I080 once each for feature, milestone, deliverable and release; I048 twice, for a missing and an empty acceptance)"
    rules_sorted     "rules are sorted by diagnostic code for deterministic execution"
    rules_count      "50 rules total: field values via field_value_constraint (W093, I048, I050, I053, I061, I062, I068 x9, I086, I087), unreferenced entities (W041, W042, W044, I046, I047 via no_incoming_edges; I010 via no_edges), missing relationships via no_outgoing_edges (W043, W046, I055, I067, I082, I083), missing fields via missing_required_field (I048, I054, I080 x4, I081), conditional fields (W057, I057, I059, I060, I066, I069, I070, I089), milestone without features (W049), dependency cycles (E007, E015, E052, W045, W092)"
  }
  ports    [ProductValidationPort]
  features [pe_core_entity_kinds, pe_validation_suite, product_validation]
  verify unit "validation rules registered from manifest"
  verify unit "rules include E007, E015, E052, W041-W046, W049, W057, W092, W093, I010, I046-I048, I050, I053-I055, I057, I059-I062, I066-I070, I080-I083, I086, I087, I089"
  verify unit "rules sorted by diagnostic code"
}

behavior pe_validate_persona_fields "Validate Persona Fields" {
  invariants [persona_channel_lifecycle]
  category   validation
  types      [ProductPersona, TechnicalLevel, Diagnostic]
  contract   """
    The @specforge/product extension MUST declare the persona description
    field required, so the core reports a persona without one (E006). The
    technical_level field is an enum of TechnicalLevel; another value is E061.
  """
  ensures {
    description_required "persona without description produces E006"
  }
  features   [pe_validation_suite]
  verify unit "persona with valid fields passes"
  verify unit "persona without description produces E006"
}

behavior pe_validate_channel_fields "Validate Channel Fields" {
  category validation
  types    [ProductChannel, InteractionModel, Diagnostic, ProductValidationPayload]
  produces [pe_validation_complete]
  contract """
    The @specforge/product extension MUST declare the channel description
    field required, so the core reports a channel without one (E006). The
    interaction_model field is an optional enum of InteractionModel; another
    value is E061.
  """
  ensures {
    description_required "channel without description produces E006"
  }
  features [pe_validation_suite]
  verify unit "channel with valid fields passes"
  verify unit "channel without description produces E006"
}

behavior pe_validate_deliverable_completeness "Validate Deliverable Completeness" {
  invariants [pe_validation_deterministic]
  category   validation
  types      [ProductDeliverable, Diagnostic, ProductDiagnosticCounts]
  produces   [pe_validation_summary]
  contract   """
    The @specforge/product extension MUST validate deliverable completeness
    with two no_outgoing_edges rules on deliverables: one scoped to
    DeliverableSupportsJourney (W043, no journeys) and one scoped to
    DeliverableContainsModule (W046, no modules). A deliverable with
    neither journeys nor modules gets both warnings.
  """
  ensures {
    journeys_checked "deliverable with no journeys produces W043"
    modules_checked  "deliverable with no modules produces W046"
  }
  features   [pe_validation_suite]
  verify unit "deliverable with journeys and modules passes both checks"
  verify unit "deliverable with no journeys produces W043"
  verify unit "deliverable with no modules produces W046"
}

behavior pe_validate_milestone_status "Validate Milestone Status Consistency" {
  invariants [milestone_status_consistency]
  category   validation
  types      [ProductMilestone, MilestoneStatus, Diagnostic, ProductEntityDiagnostic]
  produces   [pe_validation_rule_fired]
  contract   """
    The @specforge/product extension MUST validate milestone status consistency.
    This behavior orchestrates three underlying validation rules:
    validate_product_vocabularies (E061 for a status outside MilestoneStatus),
    detect_completed_milestone_without_criteria (W057 for completed without exit_criteria),
    detect_blocked_milestone_without_blockers (I060 for blocked without blockers).
  """
  ensures {
    delegates_to_rules "milestone status validation delegates to three individual validation rules"
    all_three_executed "E061, W057 and I060 are all reported during milestone validation"
  }
  features   [pe_validation_suite]
  verify unit "milestone status validation runs all three sub-rules"
}
