// Extension composition patterns and missing extension behavior (P4)
//
// Documents how extensions compose in common deployment patterns and
// what happens when peer dependencies are absent. This is Phase 4 of
// the 10-expert analysis recommendations.

use "types/diagnostics"
use "types/wasm"
use "types/zero-entity-core"

// ---------------------------------------------------------------------------
// Pattern 1: Software Team
// ---------------------------------------------------------------------------

behavior cp_software_team "Composition Pattern: Software Team" {
  category command
  features [pe_cross_extension_cooperation]
  contract """
    When @specforge/product, @specforge/software, @specforge/coverage,
    and @specforge/governance are all installed, the full software
    engineering graph MUST be active.

    Entity kinds registered (17 total):
      - product (9): journey, deliverable, milestone, module, term, feature, persona, channel, release
      - software (5): behavior, invariant, event, type, port
      - governance (3): decision, constraint, failure_mode

    Cross-extension edges active:
      - BehaviorImplementsFeature (behavior -> feature) via software's peer_dependency on product
      - MilestoneIncludesBehavior (milestone -> behavior) via software's entity_enhancement
      - ConstrainsBehavior (constraint -> behavior) via governance's peer_dependency on software
      - DecisionInvariant (decision -> invariant) via governance cross-ref
      - ProtectsInvariant (constraint -> invariant) via governance cross-ref
      - FailureModeInvariant (failure_mode -> invariant) via governance cross-ref

    Entity enhancements active:
      - milestone gains behaviors field (from software)
      - module gains ports, ports_defined fields (from software)

    Coverage traces all testable entities: behavior, invariant, event
    (from software) and constraint (from governance). The four-level
    traceability model (Declared -> Specified -> Executed -> Passing)
    applies uniformly across all testable kinds.

    All 17 entity kinds participate in graph export. Validation rules
    from all four extensions fire. No I004 diagnostics are emitted for
    cross-extension references because all peer dependencies are satisfied.
  """
  requires {
    product_installed    "@specforge/product is installed and manifest loaded"
    software_installed   "@specforge/software is installed and manifest loaded"
    coverage_installed   "@specforge/coverage is installed and manifest loaded"
    governance_installed "@specforge/governance is installed and manifest loaded"
  }
  ensures {
    seventeen_entity_kinds "KindRegistry contains exactly 17 entity kinds from 3 entity-declaring extensions"
    all_cross_edges_active "All cross-extension edges resolve without I004 diagnostics"
    enhancements_applied   "milestone and module have enhancement fields from software"
    coverage_traces_four   "Coverage discovers 4 testable kinds: behavior, invariant, event, constraint"
    all_validation_fires   "Validation rules from all 4 extensions execute"
  }
  verify unit "17 entity kinds registered across product, software, governance"
  verify unit "BehaviorImplementsFeature edge resolves behavior to feature without I004"
  verify unit "MilestoneIncludesBehavior edge resolves milestone to behavior"
  verify unit "ConstrainsBehavior edge resolves constraint to behavior"
  verify unit "coverage discovers behavior, invariant, event, constraint as testable"
  verify unit "entity enhancements add behaviors field to milestone and ports fields to module"
}

// ---------------------------------------------------------------------------
// Pattern 2: Regulated Team
// ---------------------------------------------------------------------------

behavior cp_regulated_team "Composition Pattern: Regulated Team" {
  category command
  features [pe_cross_extension_cooperation]
  contract """
    When @specforge/product, @specforge/software, @specforge/coverage,
    @specforge/governance, and @specforge/compliance are all installed,
    the graph contains two independent traceability chains in one project.

    Software chain: behavior -> verify -> test -> specforge-report.json
    Compliance chain: regulation -> control -> evidence -> audit

    Entity kinds registered (20 total):
      - product (8): journey, deliverable, milestone, module, term, feature, persona, channel
      - software (5): behavior, invariant, event, type, port
      - governance (3): decision, constraint, failure_mode
      - compliance (4): regulation, control, evidence, audit

    Additional entity enhancements from compliance:
      - journey gains controls field (from compliance)
      - deliverable gains regulations field (from compliance)

    Coverage traces 5 testable kinds: behavior, invariant, event (software),
    constraint (governance), control (compliance). All participate in the
    same four-level model. Threshold gating applies uniformly — a team can
    set different thresholds per kind via coverage configuration.

    Both chains coexist without conflict. Software validation rules target
    software entity kinds. Compliance validation rules target compliance
    entity kinds. Neither set interferes with the other because
    validation_rules with target_kind only fire when that kind is
    registered in the KindRegistry.
  """
  requires {
    product_installed    "@specforge/product is installed"
    software_installed   "@specforge/software is installed"
    coverage_installed   "@specforge/coverage is installed"
    governance_installed "@specforge/governance is installed"
    compliance_installed "@specforge/compliance is installed"
  }
  ensures {
    twenty_entity_kinds      "KindRegistry contains exactly 20 entity kinds from 4 entity-declaring extensions"
    dual_traceability_chains "Software and compliance traceability chains coexist independently"
    compliance_enhancements  "journey gains controls field, deliverable gains regulations field"
    coverage_traces_five     "Coverage discovers 5 testable kinds across software, governance, compliance"
    no_cross_chain_conflicts "Validation rules from software and compliance do not interfere"
  }
  verify unit "20 entity kinds registered across product, software, governance, compliance"
  verify unit "journey has both items (product) and controls (compliance enhancement) fields"
  verify unit "deliverable has both journeys (product) and regulations (compliance enhancement) fields"
  verify unit "coverage discovers control as testable alongside behavior, invariant, event, constraint"
  verify unit "software validation rules do not fire on compliance entity kinds"
  verify unit "compliance validation rules do not fire on software entity kinds"
}

// ---------------------------------------------------------------------------
// Pattern 3: Compliance-Only
// ---------------------------------------------------------------------------

behavior cp_compliance_only "Composition Pattern: Compliance-Only" {
  category command
  features [pe_cross_extension_cooperation]
  contract """
    When only @specforge/product and @specforge/compliance are installed
    (no @specforge/software, no @specforge/governance), the graph supports
    regulatory tracking without any software engineering entities.

    Entity kinds registered (12 total):
      - product (8): journey, deliverable, milestone, module, term, feature, persona, channel
      - compliance (4): regulation, control, evidence, audit

    Compliance entity enhancements active:
      - journey gains controls field (from compliance)
      - deliverable gains regulations field (from compliance)

    Software entity enhancements are NOT active:
      - feature does NOT have a behaviors field (software not installed)
      - milestone does NOT have a behaviors field
      - module does NOT have ports or ports_defined fields

    Journeys map to controls via entity_enhancement. Milestones schedule
    regulatory checkpoints using the generic items field. Features
    describe regulatory capabilities (problem/solution pairs) without
    any connection to behaviors.

    Governance edges targeting software kinds (ConstrainsBehavior) are
    irrelevant — governance is not installed. Compliance's own edge
    types (Governs, ImplementedBy, ProvidedBy, Audits) form a
    self-contained traceability chain.

    If a .spec file contains a reference to a behavior or invariant
    entity, the resolver emits I004 (info) suggesting installation of
    @specforge/software. The reference is stored but unresolved.
  """
  requires {
    product_installed        "@specforge/product is installed"
    compliance_installed     "@specforge/compliance is installed"
    software_not_installed   "@specforge/software is NOT installed"
    governance_not_installed "@specforge/governance is NOT installed"
  }
  ensures {
    twelve_entity_kinds       "KindRegistry contains exactly 12 entity kinds from product and compliance"
    compliance_enhancements   "journey gains controls, deliverable gains regulations"
    no_software_enhancements  "feature, milestone, module do NOT have software enhancement fields"
    compliance_chain_complete "regulation -> control -> evidence -> audit edges all resolve"
    software_refs_emit_i004   "References to behavior or invariant emit I004 info diagnostic"
  }
  verify unit "12 entity kinds registered across product and compliance"
  verify unit "feature has items field but NOT behaviors field"
  verify unit "milestone has items field but NOT behaviors field"
  verify unit "compliance traceability chain is fully functional without software"
  verify unit "reference to behavior entity emits I004 suggesting @specforge/software"
}

// ---------------------------------------------------------------------------
// Pattern 4: Minimal (Product Only)
// ---------------------------------------------------------------------------

behavior cp_minimal "Composition Pattern: Minimal (Product Only)" {
  category command
  features [pe_cross_extension_cooperation]
  contract """
    When only @specforge/product is installed, the graph contains the
    minimal domain vocabulary: what ships, to whom, when. No software
    engineering, no governance, no compliance entities exist.

    Entity kinds registered (9 total):
      - product (9): journey, deliverable, milestone, module, term, feature, persona, channel, release

    Product edge types active (20): FeatureDependsOn, FeatureRelatesTo,
      JourneyExercisesFeature, JourneyTargetsPersona, JourneyUsesChannel,
      DeliverableSupportsJourney, DeliverableContainsModule,
      DeliverableTrackedByMilestone, DeliverableDependsOn,
      MilestoneDeliversFeature, MilestoneScopesModule, MilestoneDependsOn,
      ModuleContainsFeature, ModuleDependsOn, TermReferencesRelatedTerm,
      TermBelongsToModule, ReleaseIncludesDeliverable,
      ReleaseCompletesMilestone, ReleaseDependsOn, PersonaPrioritizesFeature

    No entity enhancements are active: milestone has no behaviors field
    and module has no ports or ports_defined fields.

    Product validation rules fire, for example:
      - E007 (module dependency cycle), E015 (milestone dependency cycle)
      - W041 (unreferenced feature), W042 (unreferenced journey), W044 (unreferenced module)
      - I010 (term with no edges)

    No testable entities exist in this configuration (product declares
    no testable kinds). Coverage, if installed, discovers zero testable
    entities and reports 0/0 coverage.

    This is the simplest useful configuration. A team can start here
    and add extensions incrementally as needs grow.
  """
  requires {
    product_installed          "@specforge/product is installed"
    no_other_entity_extensions "No other entity-declaring extensions are installed"
  }
  ensures {
    nine_entity_kinds        "KindRegistry contains exactly 9 entity kinds from product"
    twenty_edge_types        "EdgeRegistry contains exactly 20 edge types from product"
    no_enhancements          "No entity enhancements are active"
    product_validation_fires "Product validation rules (E007, E015, W041, W042, W044, I010, ...) fire"
    zero_testable_entities   "No testable entity kinds are registered"
    no_i004_within_product   "All intra-product references resolve without I004"
  }
  verify unit "9 entity kinds registered from product only"
  verify unit "20 edge types registered from product only"
  verify unit "milestone has no behaviors field"
  verify unit "no testable kinds registered — coverage reports 0/0"
  verify unit "all product validation rules fire correctly"
}

// ---------------------------------------------------------------------------
// Missing Extension Behavior: I004 Soft Resolution
// ---------------------------------------------------------------------------

behavior cp_missing_product_from_software "Missing @specforge/product: Software Features Field" {
  features [pe_cross_extension_cooperation, reference_resolution]
  types    [PeerDependency]
  category command
  contract """
    When @specforge/software is installed but @specforge/product is NOT
    installed, and a behavior entity declares a features field referencing
    feature entities:

      behavior user_login {
        features [user_authentication]
      }

    The compiler MUST:
    1. Parse the features field and store the reference list normally.
    2. During resolution, detect that "feature" is not a registered kind
       in the KindRegistry.
    3. Emit I004 (info severity) naming the reference, the field, and the
       kind no enabled extension provides ("reference 'x' in field
       'features' of 'user_login' targets kind 'feature', which no enabled
       extension provides"), suggesting `specforge add @specforge/product`,
       the extension E024 names for the kind.
    4. Store the reference as unresolved — it is NOT discarded.
    5. NOT emit E003 (error) — the reference is soft, not hard.

    The BehaviorImplementsFeature edge (behavior -> feature) is declared in software's
    manifest but its target_kind "feature" is unregistered. The edge
    type is still loaded in the EdgeRegistry but no edges of this type
    can be instantiated because the target kind does not exist.

    Software's entity_enhancements targeting product kinds (milestone gains
    behaviors, module gains ports and ports_defined) are silently
    ignored per the graceful absence rule — no diagnostic emitted for
    skipped enhancements.

    If the user later installs @specforge/product, the stored references
    resolve on the next compilation without any .spec file changes.
  """
  requires {
    software_installed    "@specforge/software is installed"
    product_not_installed "@specforge/product is NOT installed"
    features_field_used   "A behavior entity declares features [...]"
  }
  ensures {
    reference_stored     "The features reference list is parsed and stored"
    i004_emitted         "I004 info diagnostic emitted for unresolved feature kind"
    e003_not_emitted     "E003 error is NOT emitted for cross-extension soft references"
    enhancements_skipped "entity_enhancements targeting product kinds are silently ignored"
    edge_type_loaded     "BehaviorImplementsFeature edge type exists in EdgeRegistry but no instances created"
    future_resolution    "Installing @specforge/product resolves the references without .spec changes"
  }
  verify unit "features field parsed and stored when product not installed"
  verify unit "I004 names the kind no enabled extension provides"
  verify unit "E003 not emitted for soft cross-extension reference"
  verify unit "entity_enhancements silently skipped — no warning or error"
  verify unit "references resolve after product is installed"
}

behavior cp_missing_software_from_governance "Missing @specforge/software: Governance Cross-Refs" {
  features [pe_cross_extension_cooperation, reference_resolution]
  types    [PeerDependency]
  category command
  contract """
    When @specforge/governance is installed but @specforge/software is
    NOT installed, and governance entities reference software kinds:

      constraint response_time_sla {
        constrains [user_login]
      }

      decision use_event_sourcing {
        protects [audit_trail_invariant]
      }

    The compiler MUST:
    1. Parse the constrains and protects reference lists normally.
    2. During resolution, detect that "behavior" and "invariant" are not
       registered kinds in the KindRegistry.
    3. Emit I004 (info severity) for each unresolved cross-extension
       reference with the message:
       "Unknown entity kind 'behavior' — install @specforge/software
       (`specforge add @specforge/software`) to resolve behavior references."
    4. Store all references as unresolved — they are NOT discarded.
    5. NOT emit E003 (error) — governance declares @specforge/software
       as an optional peer dependency for exactly this reason.

    Governance's ConstrainsBehavior edge (constraint -> behavior) cannot
    be instantiated because the target kind "behavior" is unregistered.
    DecisionInvariant, ProtectsInvariant, and FailureModeInvariant edges
    similarly cannot be instantiated. The edge types exist in the
    EdgeRegistry but produce no graph edges.

    Governance's own entity kinds (decision, constraint, failure_mode)
    remain fully functional. Intra-governance relationships work normally.
    Only cross-extension edges to software kinds are affected.
  """
  requires {
    governance_installed   "@specforge/governance is installed"
    software_not_installed "@specforge/software is NOT installed"
    cross_refs_used        "Governance entities declare references targeting software kinds"
  }
  ensures {
    references_stored       "Cross-extension reference lists are parsed and stored"
    i004_emitted            "I004 info diagnostic emitted for each unresolved software kind"
    e003_not_emitted        "E003 error is NOT emitted — peer dependency is optional"
    governance_still_works  "decision, constraint, failure_mode entities function normally"
    cross_edges_not_created "ConstrainsBehavior and related edges exist in registry but no instances created"
  }
  verify unit "constrains field parsed and stored when software not installed"
  verify unit "I004 emitted for unresolved behavior kind reference"
  verify unit "I004 emitted for unresolved invariant kind reference"
  verify unit "E003 not emitted — optional peer dependency"
  verify unit "governance entities validate and export normally"
}

behavior cp_validation_rules_skip_absent_kinds "Validation Rules Skip Absent Target Kinds" {
  features [declarative_validation_rules]
  types    [ValidationRulePattern, KindRegistryEntry]
  category command
  contract """
    Validation rules declared in extension manifests include a target_kind
    field that specifies which entity kind the rule applies to. When the
    target_kind is not registered in the KindRegistry (because the owning
    extension is not installed), the validation rule MUST be silently
    skipped — no error, no warning, no info diagnostic.

    This is by design, not a bug. Examples:

    1. @specforge/software declares a validation rule with
       target_kind="feature" (e.g., E010 invalid behavior range in
       milestone). When @specforge/product is not installed, "feature" is
       not in the KindRegistry. The rule silently does not fire.

    2. @specforge/governance declares ConstrainsBehavior validation with
       target_kind="behavior". When @specforge/software is not installed,
       the rule silently does not fire.

    3. @specforge/compliance declares Governs validation with
       target_kind="control". This always fires because "control" is
       declared by compliance itself — target_kind within the same
       extension always resolves.

    The skip mechanism works at the validation dispatch level: before
    executing a validation rule, the dispatcher checks whether
    target_kind exists in the KindRegistry. If not, the rule is skipped.
    This is O(1) per rule (hash lookup) and produces no output.

    This design ensures that extensions can declare cross-extension
    validation rules without creating hard dependencies. The rules
    activate only when the full composition is present.
  """
  requires {
    validation_rules_loaded "Extension manifests have been loaded and validation rules extracted"
    kind_registry_populated "KindRegistry is populated from installed extensions"
  }
  ensures {
    absent_kind_skipped    "Rules with target_kind not in KindRegistry are silently skipped"
    present_kind_fires     "Rules with target_kind in KindRegistry execute normally"
    no_diagnostic_for_skip "No error, warning, or info diagnostic emitted for skipped rules"
    intra_extension_always "Rules targeting kinds from the same extension always fire"
    o1_skip_check          "Skip check is O(1) hash lookup per rule"
  }
  verify unit "validation rule with absent target_kind is silently skipped"
  verify unit "validation rule with present target_kind fires normally"
  verify unit "no diagnostic emitted when rule skipped due to absent kind"
  verify unit "intra-extension validation rules always fire"
}

behavior cp_milestone_behavior_edge_absent "Product MilestoneIncludesBehavior Edge When Software Absent" {
  features [pe_cross_extension_cooperation, entity_enhancement]
  types    [EdgeTypeDescriptor, PeerDependency]
  category command
  contract """
    The MilestoneIncludesBehavior edge is declared in @specforge/software's
    manifest with source_kind="milestone" (product) and
    target_kind="behavior" (software). This edge supports the
    entity_enhancement that adds a behaviors field to milestone.

    When @specforge/software is NOT installed:

    1. The MilestoneIncludesBehavior edge type does not exist in the EdgeRegistry
       (it is declared by software, which is not loaded).
    2. The milestone entity kind has no behaviors field (the
       entity_enhancement from software is not applied).
    3. If a .spec file attempts to use a behaviors field on milestone:

         milestone alpha_release {
         }

       The compiler MUST emit a diagnostic for the unknown field
       "behaviors" on entity kind "milestone". This is a field
       validation error (the field is not registered for this kind),
       not a reference resolution error.

    When @specforge/product is installed but @specforge/software is NOT:

    4. Product's own MilestoneDeliversFeature edge (milestone -> feature) works
       normally — both kinds are owned by product.
    5. Only software-specific enhancements are absent.

    This demonstrates graceful degradation: milestones remain fully
    functional for product-level planning. Software-specific traceability
    (which behaviors does this milestone deliver?) activates only when
    software is installed.
  """
  requires {
    product_installed      "@specforge/product is installed"
    software_not_installed "@specforge/software is NOT installed"
  }
  ensures {
    no_milestone_behavior_edge "MilestoneIncludesBehavior edge type not in EdgeRegistry"
    no_behaviors_field         "milestone entity kind does not have behaviors field"
    unknown_field_diagnostic   "Using behaviors field on milestone emits field validation error"
    milestone_feature_works    "MilestoneDeliversFeature edge (product-internal) works normally"
    graceful_degradation       "Milestones remain functional for product-level planning"
  }
  verify unit "MilestoneIncludesBehavior edge absent when software not installed"
  verify unit "milestone has no behaviors field when software not installed"
  verify unit "behaviors field on milestone emits unknown field diagnostic"
  verify unit "MilestoneDeliversFeature edge works normally without software"
}
