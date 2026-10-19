# Quick Reference

Single-page lookup for the CLI, the configuration file, and every entity kind, edge type, and validation code (provided by the four builtin extensions). For full details, see the [entity model](entity-model.md) or individual [entity docs](entities/).

---

## CLI Commands

```bash
# Project lifecycle
specforge init                       # scaffold a new project
specforge check                      # validate .spec files
specforge check --strict             # promote warnings to errors
specforge check --severity error     # show only errors (the exit code still counts everything)
specforge check --cache              # record statuses in specforge-cache.json (history rules)

# Graph access (for agents and humans)
specforge export --format=context    # token-efficient context for an agent
specforge export --format markdown --scope <id> --depth 1   # a review document for people who sign
specforge query <id> --depth 2       # multi-resolution neighborhood query
specforge query <id> --format context --include-coverage   # an agent's slice with coverage
specforge trace <id>                 # traceability chain for an entity
specforge trace --plan plan.json     # check an agent plan (AgentPlan JSON) against the graph
specforge schema                     # emit the Graph Protocol schema
specforge model                      # render the logical data model
specforge outline                    # render the extension architecture
specforge stats                      # project statistics
specforge explore [<id>]             # starting points, hubs and unconnected entities
specforge review [<id>]              # coverage gaps around an entity (or the whole project)
specforge infer-guide [<kind>]       # what to look for in code to write a kind's entities

# Editing entities (no hand-written .spec): each edit is checked, then written all or nothing (E080)
specforge new behavior <id> "<title>" --in <file> --contract "…" --invariants a,b   # a new entity; its kind's fields are flags
specforge set <id> <field> <value>   # set a field; `field.entry` edits one entry of a block field
specforge set <id> --title "<title>" # change the header title
specforge unset <id> <field>         # remove a field
specforge link <id> <field> <target> # add a reference (on the relation's owning side); unlink removes it
specforge link <id> --verify unit:"<text>"   # add a verify obligation; unlink removes it
specforge delete <id> [--unlink-references]  # delete an entity (and, with the flag, every reference to it)
# Every edit takes --dry-run (print the diff, write nothing) and --format json. MCP: specforge.edit takes a list.

# Extensions & registry
specforge extensions                 # list enabled builtins and installed extensions
specforge add @specforge/product     # enable a builtin, or install @scope/name@version from a registry
specforge add <spec> <spec>...       # several at once: one change, all or nothing
specforge remove <name>              # disable a builtin, drop an entry, or uninstall an extension
specforge search <query>             # search registries
specforge publish                    # publish an extension to a registry
# There is no built-in registry: list yours under "registries" in specforge.json
# (see registry-trust.md); without one, add/update/search/publish fail with E063.

# Tooling
specforge format                     # format .spec files
specforge migrate                    # bring .spec files to the current format: titles, and repeated reference lists moved into `defaults`
specforge collect                    # run the test runners (with your approval) and record what they prove
specforge doctor                     # health-check installed extensions
specforge mcp                        # start the MCP server (stdio); specforge.edit applies a list of typed edits
specforge explain E001               # explain a diagnostic code
```

## Configuration

Projects are configured via **`specforge.json`** (like `tsconfig.json`):

```json
{
  "name": "payments",
  "version": "0.1.0",
  "spec_root": "spec",
  "extensions": ["@specforge/software"]
}
```

`specforge init --extensions @specforge/software` creates this for you; `specforge add` / `specforge remove` edit the `extensions` list afterwards.

---

## File defaults

A file states a kind's shared fields once (ADR 0149): `defaults <kind> { <field> <value> … }`.

1. It sits at the top level, before the file's first block (else E078), once per kind; it takes no title, `verify` or `method`.
2. It applies to every block of that kind in the same file: not another kind, file or importer.
3. A block that writes a field keeps its own value whole; `ports []` says "none".
4. A default sets a field the kind declares (else E077, not applied).
5. A block field equal to its default, and a default no block takes, is W161. A typed edit never writes one.
6. `defaults` is reserved: never a kind, E013 as an id.
7. The graph holds the resolved value; its spans stay the default's, so diagnostics, references and rename point at the default's line.

```
defaults behavior {
  ports    [TaskRepository]
  features [task_management]
}

behavior create_task "Create a Task" {
  category command
  produces [task_created]
}
```

`specforge migrate` hoists the reference lists a file's blocks repeat into its defaults.

---

## Entities

### spec
> Module: core | ID: singleton | "What project is this?"

| Required | Optional |
|----------|----------|
| name, version, extensions | persona, surface — *(planned: not yet implemented)* providers, gen |

No graph edges. Root configuration declaring which extensions the project uses.

---

### ref
> Module: core | ID: `scheme.kind:identifier` | "What external resource is this connected to?"

| Required | Optional |
|----------|----------|
| scheme, identifier, title | provider-specific fields |

No outgoing edges. Leaf node.
Incoming: any entity (`links_to`)

---

`imports` (file to file) and `links_to` (any entity to a `ref`) are structural edges: no field declares them.

Every other kind, field and relation below is generated from the builtin extensions' declarations. A relation is written on its owning field only; the other side is derived (`query` shows it as incoming edges, `trace` as upstream).

<!-- relations:begin (generated: SPECFORGE_BLESS=1 cargo nextest run -p specforge-cli -E 'test(the_quick_reference_relations_are_generated)'; never edit by hand) -->
### channel
> @specforge/product — A communication or distribution channel for the product

Required: `description`
Optional: `interaction_model` (enum: request_response, event_driven, batch, streaming, bidirectional, manual), `reason`, `status` (enum: active, deprecated), `tags`, `url`
Written on the other side (derived, never a field here): journey `channels` (`JourneyUsesChannel`)

### deliverable
> @specforge/product — A shippable unit of work with dependencies and milestones

Required: `artifact_type` (enum: cli, service, library, web_app, mobile_app, api, extension, documentation, package)
Optional: `contributors`, `depends_on`, `description`, `journeys`, `milestones`, `modules`, `owner`, `reason`, `status` (enum: draft, in_progress, shipped, deprecated), `tags`, `version`
Writes: `depends_on` → deliverable (`DeliverableDependsOn`), `journeys` → journey (`DeliverableSupportsJourney`), `milestones` → milestone (`DeliverableTrackedByMilestone`), `modules` → module (`DeliverableContainsModule`)
Written on the other side (derived, never a field here): release `deliverables` (`ReleaseIncludesDeliverable`)

### feature
> @specforge/product — A user-facing capability that solves a specific problem

Required: `problem`
Optional: `acceptance`, `contributors`, `depends_on`, `description`, `effort` (enum: xs, s, m, l, xl), `features`, `owner`, `priority` (enum: critical, high, medium, low), `reason`, `refs`, `solution`, `status` (enum: proposed, accepted, in_progress, done, deferred, deprecated), `tags`, `tests`
Writes: `depends_on` → feature (`FeatureDependsOn`), `features` → feature (`FeatureRelatesTo`)
Written on the other side (derived, never a field here): journey `features` (`JourneyExercisesFeature`), milestone `features` (`MilestoneDeliversFeature`), module `features` (`ModuleContainsFeature`), persona `key_features` (`PersonaPrioritizesFeature`), behavior `features` (`BehaviorImplementsFeature`, read here as `behaviors`), constraint `governs_features` (`ConstraintGovernsFeature`), decision `affects_features` (`DecisionAffectsFeature`), failure_mode `threatens_features` (`FailureModeThreatensFeature`)

### journey
> @specforge/product — A user journey through the product experience

Required: `flow`
Optional: `channels`, `description`, `features`, `persona`, `priority` (enum: critical, high, medium, low), `tags`
Writes: `channels` → channel (`JourneyUsesChannel`), `features` → feature (`JourneyExercisesFeature`), `persona` → persona (`JourneyTargetsPersona`)
Written on the other side (derived, never a field here): deliverable `journeys` (`DeliverableSupportsJourney`)

### milestone
> @specforge/product — A significant project checkpoint with tracked progress

Optional: `behaviors`, `blockers`, `contributors`, `depends_on`, `description`, `exit_criteria`, `features`, `modules`, `owner`, `priority` (enum: critical, high, medium, low), `reason`, `refs`, `start_date`, `status` (enum: planned, in_progress, completed, blocked), `tags`, `target_date`
Writes: `behaviors` → behavior (`MilestoneIncludesBehavior`) (from @specforge/software), `depends_on` → milestone (`MilestoneDependsOn`), `features` → feature (`MilestoneDeliversFeature`), `modules` → module (`MilestoneScopesModule`)
Written on the other side (derived, never a field here): deliverable `milestones` (`DeliverableTrackedByMilestone`), release `milestones` (`ReleaseCompletesMilestone`)

### module
> @specforge/product — A logical grouping of related features and behaviors

Optional: `depends_on`, `description`, `family` (identifier), `features`, `ports`, `ports_defined`, `reason`, `tags`
Writes: `depends_on` → module (`ModuleDependsOn`), `features` → feature (`ModuleContainsFeature`), `ports` → port (`ModuleConsumesPort`) (from @specforge/software), `ports_defined` → port (`ModuleDefinesPort`) (from @specforge/software)
Written on the other side (derived, never a field here): deliverable `modules` (`DeliverableContainsModule`), milestone `modules` (`MilestoneScopesModule`), term `module` (`TermBelongsToModule`)

### persona
> @specforge/product — A user archetype representing a target audience segment

Required: `description`
Optional: `goals`, `key_features`, `pain_points`, `reason`, `status` (enum: active, deprecated), `tags`, `technical_level` (enum: expert, advanced, intermediate, beginner, non_technical)
Writes: `key_features` → feature (`PersonaPrioritizesFeature`)
Written on the other side (derived, never a field here): journey `persona` (`JourneyTargetsPersona`)

### release
> @specforge/product — A versioned product release bundling deliverables

Required: `version`
Optional: `changelog`, `contributors`, `deliverables`, `depends_on`, `description`, `milestones`, `owner`, `reason`, `refs`, `release_date`, `status` (enum: planned, in_progress, released, recalled), `tags`, `target_date`
Writes: `deliverables` → deliverable (`ReleaseIncludesDeliverable`), `depends_on` → release (`ReleaseDependsOn`), `milestones` → milestone (`ReleaseCompletesMilestone`)

### term
> @specforge/product — A glossary entry defining domain-specific vocabulary

Required: `definition`
Optional: `aliases`, `context`, `module`, `see_also`, `tags`
Writes: `module` → module (`TermBelongsToModule`), `see_also` → term (`TermReferencesRelatedTerm`)

### behavior
> @specforge/software — A testable unit of system functionality with a defined contract

Required: `contract`
Optional: `abstract`, `category` (identifier), `consumes`, `description`, `diagnostic` (identifier), `ensures`, `features`, `invariants`, `maintains`, `ports`, `produces`, `refines`, `refs`, `requires`, `satisfies`, `severity` (identifier), `status` (identifier), `sync`, `tags`, `types`
Writes: `consumes` → event (`BehaviorConsumesEvent`), `features` → feature (`BehaviorImplementsFeature`), `invariants` → invariant (`BehaviorEnforcesInvariant`), `ports` → port (`BehaviorUsesPort`), `produces` → event (`BehaviorProducesEvent`), `refines` → behavior (`BehaviorRefinesBehavior`) (from @specforge/formal), `satisfies` → property (`BehaviorSatisfiesProperty`) (from @specforge/formal), `types` → type (`BehaviorReferencesType`)
Written on the other side (derived, never a field here): milestone `behaviors` (`MilestoneIncludesBehavior`), constraint `constrains` (`ConstraintConstrainsBehavior`), constraint `enforced_by` (`ConstraintEnforcedByBehavior`), failure_mode `affected_behaviors` (`FailureModeAffectsBehavior`), refinement `abstract_entity` (`RefinementRefinesAbstract`), refinement `concrete_entity` (`RefinementRefinesConcrete`)

### event
> @specforge/software — A significant occurrence in the system that triggers reactions

Optional: `category` (identifier), `channel`, `channel_type` (identifier), `contract`, `description`, `follows_protocol`, `participates_in`, `payload`, `refs`, `sync`, `tags`
Writes: `follows_protocol` → protocol (`EventFollowsProtocol`) (from @specforge/formal), `participates_in` → process (`EventParticipatesInProcess`) (from @specforge/formal), `payload` → type (`EventCarriesPayloadType`)
Written on the other side (derived, never a field here): behavior `consumes` (`BehaviorConsumesEvent`), behavior `produces` (`BehaviorProducesEvent`)

### invariant
> @specforge/software — A system-wide constraint that must always hold true

Required: `guarantee`
Optional: `description`, `expression`, `refs`, `risk` (enum: critical, high, medium, low), `tags`
Written on the other side (derived, never a field here): behavior `invariants` (`BehaviorEnforcesInvariant`, read here as `enforced_by`), constraint `protects` (`ConstraintProtectsInvariant`), decision `invariants` (`DecisionProtectsInvariant`), failure_mode `invariant` (`FailureModeTargetsInvariant`), axiom `assumes` (`AxiomAssumesInvariant`)

### port
> @specforge/software — An interface boundary between system components

Required: `direction` (enum: inbound, outbound, bidirectional)
Optional: `category`, `description`, `methods`, `refs`, `tags`, `types`
Writes: `types` → type (`PortReferencesType`) (derived)
Written on the other side (derived, never a field here): module `ports` (`ModuleConsumesPort`), module `ports_defined` (`ModuleDefinesPort`), behavior `ports` (`BehaviorUsesPort`)

### type
> @specforge/software — A data structure or domain model definition

Optional: `composed_types`, `description`, `extends`, `refs`, `tags`
Writes: `composed_types` → type (`TypeComposesType`) (derived), `extends` → type (`TypeExtendsType`)
Written on the other side (derived, never a field here): behavior `types` (`BehaviorReferencesType`), event `payload` (`EventCarriesPayloadType`), port `types` (`PortReferencesType`)

### constraint
> @specforge/governance — A technical or business constraint that limits design choices

Required: `description`
Optional: `category` (identifier), `constrains`, `enforced_by`, `governs_features`, `metric`, `priority` (enum: critical, high, medium, low), `protects`, `refs`, `scope`, `tags`, `threshold`
Writes: `constrains` → behavior (`ConstraintConstrainsBehavior`), `enforced_by` → behavior (`ConstraintEnforcedByBehavior`), `governs_features` → feature (`ConstraintGovernsFeature`), `protects` → invariant (`ConstraintProtectsInvariant`)
Written on the other side (derived, never a field here): decision `constraints` (`DecisionImposesConstraint`)

### decision
> @specforge/governance — An architectural or design decision with rationale and status

Required: `context`, `decision`, `status` (enum: proposed, accepted, deprecated, superseded)
Optional: `affects_features`, `alternatives`, `authors`, `consequences`, `constraints`, `date`, `deciders`, `description`, `invariants`, `reason`, `refs`, `superseded_by`, `tags`
Writes: `affects_features` → feature (`DecisionAffectsFeature`), `constraints` → constraint (`DecisionImposesConstraint`), `invariants` → invariant (`DecisionProtectsInvariant`), `superseded_by` → decision (`DecisionSupersedesDecision`)

### failure_mode
> @specforge/governance — A potential failure scenario with severity and mitigation

Required: `cause`, `effect`, `severity` (enum: critical, high, medium, low)
Optional: `affected_behaviors`, `description`, `detection` (enum: certain, likely, moderate, unlikely, undetectable), `invariant`, `mitigation`, `occurrence` (enum: certain, likely, occasional, unlikely, rare), `post_detection` (enum: certain, likely, moderate, unlikely, undetectable), `post_mitigation`, `post_occurrence` (enum: certain, likely, occasional, unlikely, rare), `post_severity` (enum: critical, high, medium, low), `refs`, `risk`, `rpn`, `tags`, `threatens_features`
Writes: `affected_behaviors` → behavior (`FailureModeAffectsBehavior`), `invariant` → invariant (`FailureModeTargetsInvariant`), `threatens_features` → feature (`FailureModeThreatensFeature`)

### axiom
> @specforge/formal — An assumed-true foundational assertion

Required: `expression`
Optional: `assumes`, `description`, `justification`, `refs`, `tags`
Writes: `assumes` → invariant (`AxiomAssumesInvariant`)

### process
> @specforge/formal — A CSP-style communicating process with states and transitions

Required: `alphabet`, `initial_state`
Optional: `composition`, `description`, `refs`, `states`, `sub_processes`, `tags`
Writes: `sub_processes` → process (`ProcessComposesProcess`)
Written on the other side (derived, never a field here): event `participates_in` (`EventParticipatesInProcess`)

### property
> @specforge/formal — A temporal property assertion (safety, liveness, fairness)

Required: `expression`, `property_type` (enum: safety, liveness, fairness)
Optional: `description`, `refs`, `scope`, `tags`
Written on the other side (derived, never a field here): behavior `satisfies` (`BehaviorSatisfiesProperty`)

### protocol
> @specforge/formal — A shared synchronization contract between processes

Required: `alphabet`, `initial_state`
Optional: `description`, `refs`, `states`, `tags`, `transitions`
Written on the other side (derived, never a field here): event `follows_protocol` (`EventFollowsProtocol`)

### refinement
> @specforge/formal — A mapping from abstract to concrete specification

Required: `abstract_entity`, `concrete_entity`
Optional: `chains_to`, `description`, `invariant_deltas`, `refs`, `tags`
Writes: `abstract_entity` → behavior (`RefinementRefinesAbstract`), `chains_to` → refinement (`RefinementChainsToRefinement`), `concrete_entity` → behavior (`RefinementRefinesConcrete`)

### Relations

A relation is written on its owning field only; the other side is derived (ADR 0150).

| Owning field | From | To | Edge type | Extension |
|---|---|---|---|---|
| `deliverable.depends_on` | deliverable | deliverable | `DeliverableDependsOn` | @specforge/product |
| `deliverable.journeys` | deliverable | journey | `DeliverableSupportsJourney` | @specforge/product |
| `deliverable.milestones` | deliverable | milestone | `DeliverableTrackedByMilestone` | @specforge/product |
| `deliverable.modules` | deliverable | module | `DeliverableContainsModule` | @specforge/product |
| `feature.depends_on` | feature | feature | `FeatureDependsOn` | @specforge/product |
| `feature.features` | feature | feature | `FeatureRelatesTo` | @specforge/product |
| `journey.channels` | journey | channel | `JourneyUsesChannel` | @specforge/product |
| `journey.features` | journey | feature | `JourneyExercisesFeature` | @specforge/product |
| `journey.persona` | journey | persona | `JourneyTargetsPersona` | @specforge/product |
| `milestone.behaviors` | milestone | behavior | `MilestoneIncludesBehavior` | @specforge/software |
| `milestone.depends_on` | milestone | milestone | `MilestoneDependsOn` | @specforge/product |
| `milestone.features` | milestone | feature | `MilestoneDeliversFeature` | @specforge/product |
| `milestone.modules` | milestone | module | `MilestoneScopesModule` | @specforge/product |
| `module.depends_on` | module | module | `ModuleDependsOn` | @specforge/product |
| `module.features` | module | feature | `ModuleContainsFeature` | @specforge/product |
| `module.ports` | module | port | `ModuleConsumesPort` | @specforge/software |
| `module.ports_defined` | module | port | `ModuleDefinesPort` | @specforge/software |
| `persona.key_features` | persona | feature | `PersonaPrioritizesFeature` | @specforge/product |
| `release.deliverables` | release | deliverable | `ReleaseIncludesDeliverable` | @specforge/product |
| `release.depends_on` | release | release | `ReleaseDependsOn` | @specforge/product |
| `release.milestones` | release | milestone | `ReleaseCompletesMilestone` | @specforge/product |
| `term.module` | term | module | `TermBelongsToModule` | @specforge/product |
| `term.see_also` | term | term | `TermReferencesRelatedTerm` | @specforge/product |
| `behavior.consumes` | behavior | event | `BehaviorConsumesEvent` | @specforge/software |
| `behavior.features` | behavior | feature | `BehaviorImplementsFeature` | @specforge/software |
| `behavior.invariants` | behavior | invariant | `BehaviorEnforcesInvariant` | @specforge/software |
| `behavior.ports` | behavior | port | `BehaviorUsesPort` | @specforge/software |
| `behavior.produces` | behavior | event | `BehaviorProducesEvent` | @specforge/software |
| `behavior.refines` | behavior | behavior | `BehaviorRefinesBehavior` | @specforge/formal |
| `behavior.satisfies` | behavior | property | `BehaviorSatisfiesProperty` | @specforge/formal |
| `behavior.types` | behavior | type | `BehaviorReferencesType` | @specforge/software |
| `event.follows_protocol` | event | protocol | `EventFollowsProtocol` | @specforge/formal |
| `event.participates_in` | event | process | `EventParticipatesInProcess` | @specforge/formal |
| `event.payload` | event | type | `EventCarriesPayloadType` | @specforge/software |
| `port.types` | port | type | `PortReferencesType` | @specforge/software |
| `type.composed_types` | type | type | `TypeComposesType` | @specforge/software |
| `type.extends` | type | type | `TypeExtendsType` | @specforge/software |
| `constraint.constrains` | constraint | behavior | `ConstraintConstrainsBehavior` | @specforge/governance |
| `constraint.enforced_by` | constraint | behavior | `ConstraintEnforcedByBehavior` | @specforge/governance |
| `constraint.governs_features` | constraint | feature | `ConstraintGovernsFeature` | @specforge/governance |
| `constraint.protects` | constraint | invariant | `ConstraintProtectsInvariant` | @specforge/governance |
| `decision.affects_features` | decision | feature | `DecisionAffectsFeature` | @specforge/governance |
| `decision.constraints` | decision | constraint | `DecisionImposesConstraint` | @specforge/governance |
| `decision.invariants` | decision | invariant | `DecisionProtectsInvariant` | @specforge/governance |
| `decision.superseded_by` | decision | decision | `DecisionSupersedesDecision` | @specforge/governance |
| `failure_mode.affected_behaviors` | failure_mode | behavior | `FailureModeAffectsBehavior` | @specforge/governance |
| `failure_mode.invariant` | failure_mode | invariant | `FailureModeTargetsInvariant` | @specforge/governance |
| `failure_mode.threatens_features` | failure_mode | feature | `FailureModeThreatensFeature` | @specforge/governance |
| `axiom.assumes` | axiom | invariant | `AxiomAssumesInvariant` | @specforge/formal |
| `process.sub_processes` | process | process | `ProcessComposesProcess` | @specforge/formal |
| `refinement.abstract_entity` | refinement | behavior | `RefinementRefinesAbstract` | @specforge/formal |
| `refinement.chains_to` | refinement | refinement | `RefinementChainsToRefinement` | @specforge/formal |
| `refinement.concrete_entity` | refinement | behavior | `RefinementRefinesConcrete` | @specforge/formal |

Edge types no field writes: `ExternalRef`, `PropertyDependsOnInvariant`, `References`.
<!-- relations:end -->

---

## Validation Codes

Every diagnostic code, with its meaning, owner and fix, is listed in [Diagnostic Codes](diagnostics.md), generated from the catalog that `specforge explain <CODE>` prints.
