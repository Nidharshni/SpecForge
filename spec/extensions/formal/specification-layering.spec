// @specforge/formal specification layering — abstract/refines mechanism
//
// Moved from @specforge/software formal-refinement.spec per 10-expert panel.
// Terminology: "B-Method Refinement" -> "Specification Layering"
// All behavior IDs renamed se_ -> fa_
// E033 was downgraded to W058, then trimmed: a feature's requirements are prose,
// so no structural check can tell whether a behavior satisfies them (ADR 0040)

use "extensions/formal/invariants"
use "extensions/formal/types"
use "types/zero-entity-core"

defaults behavior {
  features [fa_specification_layering]
}

behavior fa_parse_abstract_annotation "Parse Abstract Annotation" {
  category command
  contract """
    Recognize the abstract field with value true on behavior entities.
  """
  ensures {
    abstract_parsed     "abstract true on behavior parsed as specification-only marker"
    empty_verify_ok     "abstract behavior with no verify permitted"
    non_behavior_warned "abstract on non-behavior entity produces warning"
  }
  verify unit "abstract true on behavior parsed"
  verify unit "abstract behavior with no verify permitted"
  verify unit "abstract on non-behavior produces warning"
}

behavior fa_parse_refines_annotation "Parse Refines Annotation" {
  category command
  contract """
    Recognize the refines field on behavior entities as a reference
    to an abstract behavior.
  """
  ensures {
    edge_created        "refines creates directed edge from concrete to abstract behavior"
    multiple_permitted  "multiple behaviors refining same abstract permitted"
    non_abstract_warned "refines referencing non-abstract behavior produces warning"
  }
  verify unit "refines referencing abstract behavior parsed"
  verify unit "refines creates directed edge in graph"
  verify unit "refines referencing non-abstract produces warning"
  verify unit "multiple behaviors refining same abstract permitted"
}

behavior fa_build_layering_chain "Build Specification Layering Chain" {
  category   command
  invariants [fa_layering_dag]
  types      [RefinementChain, RefinementStep, ConditionDelta]
  contract   """
    Build layering chains from all refines edges after graph
    construction. Each chain uses a linked-list structure of
    RefinementStep entries preserving per-step condition deltas,
    rather than a flat list of concrete IDs.
  """
  requires {
    refines_edges_exist "at least one refines edge exists in the graph"
  }
  ensures {
    chains_built         "each chain starts at abstract behavior, follows refines edges to concretes via RefinementStep list"
    step_deltas_recorded "each step records condition delta (added_ensures, removed_requires)"
    dag_enforced         "cycles in layering produce E041, one per cycle, naming its smallest behavior"
    depth_recorded       "each chain records its depth (number of layering levels)"
    deep_chain_warned    "chain depth > 4 produces W031"
  }
  verify unit "layering chain built from abstract to concrete via steps"
  verify unit "each step records condition delta"
  verify unit "cycle in layering produces E041"
  verify unit "chain depth recorded correctly"
  verify unit "chain depth > 4 produces W031"
}

behavior fa_validate_layering_completeness "Validate Layering Completeness" {
  category query
  types    [RefinementChain]
  contract """
    Check that every abstract behavior has at least one concrete
    refinement.
  """
  requires {
    chains_built "layering DAG is fully built"
  }
  ensures {
    complete_passes   "abstract with concrete refinement passes"
    incomplete_warned "abstract with no refinement produces W030"
  }
  verify unit "abstract with concrete refinement passes"
  verify unit "abstract with no refinement produces W030"
}

behavior fa_layering_verify_pass "Layering Verify Compiler Pass" {
  category command
  types    [RefinementChain]
  produces [fa_layering_check_complete]
  contract """
    The layering_verify compiler pass validates all layering chains
    after the condition check pass. This is a structural coverage
    check, not semantic verification.
  """
  requires {
    condition_check_done "condition_check pass has completed"
    chains_built         "layering DAG is fully built"
  }
  ensures {
    completeness_checked "every abstract has at least one concrete refinement"
    postconditions_held  "refined behaviors maintain the abstract's postconditions"
    dag_verified         "layering DAG has no cycles"
    incomplete_warned    "incomplete layering chain produces W030 (via completeness check)"
  }
  verify unit "complete layering chain passes"
  verify unit "incomplete chain produces W030"
  verify unit "pass runs after condition_check pass"
}

behavior fa_parse_refinement_entity "Parse Refinement Entity" {
  category command
  types    [FormalRefinement, ConditionDelta, RefinementStatus]
  contract """
    Parse refinement entity declarations. The abstract_entity and
    concrete_entity fields create RefinementRefinesAbstract and
    RefinementRefinesConcrete edges (refinement -> behavior); chains_to
    creates RefinementChainsToRefinement edges (refinement -> refinement).
    Validates that abstract_entity and concrete_entity reference existing
    behavior entities. Status defaults to proposed if not specified.
  """
  ensures {
    refines_edges      "abstract_entity/concrete_entity create RefinementRefinesAbstract/RefinementRefinesConcrete edges to the named behaviors"
    chain_link_edges   "chains_to creates RefinementChainsToRefinement edges between refinement entities"
    abstract_validated "abstract_entity must reference an existing behavior entity"
    concrete_validated "concrete_entity must reference an existing behavior entity"
    status_defaulted   "status defaults to proposed when not specified"
  }
  features [fa_refinement_layering]
  verify unit "refinement entity parsed with abstract_entity and concrete_entity edges"
  verify unit "refinement chain link edges created"
  verify unit "abstract_entity referencing non-behavior produces error"
  verify unit "concrete_entity referencing non-behavior produces error"
  verify unit "status defaults to proposed"
}

behavior fa_integrate_refinement_with_layering "Integrate Refinement Entities into Layering Chains" {
  invariants [fa_refinement_chain_dag]
  category   command
  types      [FormalRefinement, RefinementChain, RefinementStep]
  contract   """
    Merge field-based layering (abstract/refines annotations on behaviors)
    with refinement entities into unified RefinementChain structures.
    Refinement entity's condition delta takes precedence over inferred
    delta from behavior conditions. Dual-mode: both field-based and
    entity-based layering coexist without conflict.
  """
  requires {
    chains_built       "layering chains from abstract/refines are built (fa_build_layering_chain)"
    refinements_parsed "refinement entities are parsed (fa_parse_refinement_entity)"
  }
  ensures {
    unified_chains        "field-based and entity-based layering merged into unified RefinementChain"
    entity_delta_priority "refinement entity condition delta takes precedence over inferred delta"
    dual_mode_coexist     "both field annotations and refinement entities coexist without conflict"
    chain_dag_maintained  "unified chain maintains DAG property (E041)"
  }
  features   [fa_refinement_layering]
  verify unit "field-based and entity-based layering merge into unified chain"
  verify unit "refinement entity delta overrides inferred delta"
  verify unit "dual-mode coexistence produces no conflict"
  verify unit "unified chain DAG property maintained"
}
