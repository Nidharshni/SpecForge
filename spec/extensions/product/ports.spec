// @specforge/product extension ports — integration boundaries
//
// Inbound ports define the query and registration interfaces the product
// extension exposes to the compiler and consumers (CLI, MCP, agents).
// Outbound ports define what the product extension requires from
// the compiler core (KindRegistry, FieldRegistry, graph queries).

use "extensions/product/types"
use "types/zero-entity-core"

port ProductQueryPort "Product Query Port" {
  direction inbound
  category  "api/product-queries"
  method queryMilestoneCompletion(milestoneId: EntityId) -> Result<MilestoneCompletionPayload, ProductQueryError>
  method queryDeliverableTraceability(deliverableId: EntityId) -> Result<DeliverableTraceabilityPayload, ProductQueryError>
  method queryJourneyCoverage(journeyId: EntityId) -> Result<JourneyCoveragePayload, ProductQueryError>
  method queryFeatureOrdering() -> Result<FeatureOrderingPayload, ProductQueryError>
  // asOfDate defaults to the host-passed `today` (CommandInput.today, ADR 0011)
  method queryMilestoneTimeline(asOfDate?: string) -> Result<MilestoneTimelinePayload, ProductQueryError>
  method queryFeatureDeliverables(featureId: EntityId) -> Result<FeatureDeliverablePayload, ProductQueryError>
  // maxHops defaults to 1 when omitted; values > 5 are clamped to 5
  method queryTermGraph(termId: EntityId, maxHops?: integer) -> Result<TermGraphPayload, ProductQueryError>
  method queryDeliverableCompletion(deliverableId: EntityId) -> Result<DeliverableCompletionPayload, ProductQueryError>
  method queryPersonaChannels(personaId: EntityId) -> Result<PersonaChannelPayload, ProductQueryError>
  method queryFeatureDependents(featureId: EntityId) -> Result<FeatureDependentPayload, ProductQueryError>
  method queryDeliverableDependents(deliverableId: EntityId) -> Result<DeliverableDependentPayload, ProductQueryError>
  method queryDeliverablePriority(deliverableId: EntityId) -> Result<DeliverablePriorityPayload, ProductQueryError>
  method queryPersonaFeatures(personaId: EntityId) -> Result<PersonaFeaturePayload, ProductQueryError>
  method queryFeatureImpact(featureId: EntityId) -> Result<FeatureImpactPayload, ProductQueryError>
  method queryMilestoneVelocity(milestoneId: EntityId, asOfDate?: string) -> Result<MilestoneVelocityPayload, ProductQueryError>
  method queryDeliverablePersonas(deliverableId: EntityId) -> Result<DeliverablePersonaPayload, ProductQueryError>
  method queryUnscheduledFeatures() -> Result<UnscheduledFeaturesPayload, ProductQueryError>
  method queryFeatureOverlap(offset?: integer, limit?: integer) -> Result<FeatureOverlapPayload, ProductQueryError>
  method queryPersonaCoverageMatrix(offset?: integer, limit?: integer) -> Result<PersonaCoverageMatrixPayload, ProductQueryError>
  method queryCriticalPath() -> Result<CriticalPathPayload, ProductQueryError>
  // v1.1 methods — ownership, effort, release
  // Paged queries take offset (default 0) and limit (default 100, clamped to
  // [1, 1000]) like the list commands; payloads carry total and has_more.
  method queryOwnerWorkload(offset?: integer, limit?: integer) -> Result<OwnerWorkloadPayload, ProductQueryError>
  method queryWeightedMilestoneCompletion(milestoneId: EntityId) -> Result<WeightedMilestoneCompletionPayload, ProductQueryError>
  method queryReleaseCompletion(releaseId: EntityId) -> Result<ReleaseCompletionPayload, ProductQueryError>
  method queryChannelFeatures(channelId: EntityId) -> Result<ChannelFeaturePayload, ProductQueryError>
  // Term analytics — global views over the TermReferencesRelatedTerm subgraph
  method queryTermClusters() -> Result<TermClusterPayload, ProductQueryError>
  method queryTermDensity() -> Result<TermDensityPayload, ProductQueryError>
  // Module analytics — dependency structure metrics
  method queryModuleDependencyDepth(moduleId: EntityId) -> Result<ModuleDependencyDepthPayload, ProductQueryError>
  method queryModuleCoupling(offset?: integer, limit?: integer) -> Result<ModuleCouplingPayload, ProductQueryError>
  // Channel analytics — symmetric counterpart to queryPersonaCoverageMatrix
  method queryChannelCoverageMatrix(offset?: integer, limit?: integer) -> Result<ChannelCoverageMatrixPayload, ProductQueryError>
  verify unit "ProductQueryPort"
}

port ProductValidationPort "Product Validation Port" {
  direction inbound
  category  "api/product-validation"
  method validateProductEntities() -> Result<ProductValidationPayload, ProductValidationError>
  requires {
    registries_populated "KindRegistry and FieldRegistry contain all 9 product entity kinds"
  }
  ensures {
    all_rules_executed "all E007, E015, E052, W041-W046, W049, W057, W092, W093, I010, I046-I048, I050, I053-I055, I057, I059-I062, I066-I070, I080-I083, I086, I087, I089 rules are evaluated"
    deterministic      "same graph input always produces same diagnostic set"
    no_time_dependency "validation never depends on wall-clock time — overdue detection is query-time only"
  }
  verify unit "ProductValidationPort"
}

port ProductRegistrationPort "Product Registration Port" {
  direction inbound
  category  "api/product-registration"
  method registerEntityKinds() -> Result<ProductEntityRegistrationPayload, RegistrationError>
  method registerEdgeTypes() -> Result<void, RegistrationError>
  method registerFieldDefinitions() -> Result<void, RegistrationError>
  method registerValidationRules() -> Result<void, RegistrationError>
  requires {
    manifest_valid "ExtensionDeclaration has been parsed and schema-validated"
  }
  ensures {
    nine_kinds   "KindRegistry contains exactly 9 product entity kinds"
    twenty_edges "EdgeTypeSet contains exactly 20 product edge types"
  }
  verify unit "ProductRegistrationPort"
}

port KindRegistryPort "Kind Registry Port" {
  direction outbound
  category  "spi/compiler-core"
  method registerKind(kind: EntityKindDescriptor) -> Result<void, RegistrationError>
  method lookupKind(name: string) -> Result<EntityKindDescriptor, ProductQueryError>
  method hasKind(name: string) -> Result<boolean, never>
  verify unit "KindRegistryPort"
}

port FieldRegistryPort "Field Registry Port" {
  direction outbound
  category  "spi/compiler-core"
  method registerField(kindName: string, field: FieldDescriptor) -> Result<void, RegistrationError>
  method lookupFields(kindName: string) -> Result<FieldDescriptor[], ProductQueryError>
  verify unit "FieldRegistryPort"
}

port EdgeTypeRegistryPort "Edge Type Registry Port" {
  direction outbound
  category  "spi/compiler-core"
  method registerEdgeType(edge: EdgeTypeDescriptor) -> Result<void, RegistrationError>
  method lookupEdgesForKind(kindName: string) -> Result<EdgeTypeDescriptor[], ProductQueryError>
  verify unit "EdgeTypeRegistryPort"
}

port GraphQueryPort "Graph Query Port" {
  direction outbound
  category  "spi/compiler-core"
  method getIncomingEdges(nodeId: EntityId, edgeType: string) -> Result<EntityId[], ProductQueryError>
  method getOutgoingEdges(nodeId: EntityId, edgeType: string) -> Result<EntityId[], ProductQueryError>
  method getNodesByKind(kind: string) -> Result<EntityId[], ProductQueryError>
  method detectCycles(edgeType: string) -> Result<EntityId[][], never>
  requires {
    graph_built "in-memory graph has been constructed by the resolver"
  }
  verify unit "GraphQueryPort"
}
