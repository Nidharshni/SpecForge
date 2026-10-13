// @specforge/formal structured conditions — requires/ensures/maintains blocks + port contracts
//
// Moved from @specforge/software formal-contracts.spec per 10-expert panel.
// Terminology: "Design by Contract" -> "Structured Conditions"
// All behavior IDs renamed se_ -> fa_
//
// Conditions are block fields on behaviors: each entry a named condition
// (ConditionEntry). They are not graph entities and write no edge.
// Inline:    requires { name "description" }

use "extensions/formal/invariants"
use "extensions/formal/types"
use "types/zero-entity-core"

// ── Structured Conditions ───────────────────────────────────

behavior fa_parse_requires_block "Parse Requires Block" {
  category command
  types    [RequiresBlock, ConditionEntry]
  contract """
    Recognize the requires field on behavior entities. Parses inline
    blocks into ordered ConditionEntry lists:
    - Inline block: requires { name "description" } — parsed as ordered
      ConditionEntry list (inline conditions, not graph nodes)
  """
  requires {
    entity_is_behavior "the enclosing entity has kind=behavior"
  }
  ensures {
    inline_parsed       "inline requires block parsed into RequiresBlock AST node with ordered ConditionEntry list"
    named_conditions    "each inline condition has a name (identifier) and description (string)"
    empty_permitted     "empty requires block produces empty ConditionEntry list"
    non_behavior_warned "requires block on non-behavior entity produces warning"
  }
  features [fa_structured_conditions]
  verify unit "inline requires block with named conditions parsed"
  verify unit "empty requires block produces empty ConditionEntry list"
  verify unit "requires block on non-behavior entity produces warning"
}

behavior fa_parse_ensures_block "Parse Ensures Block" {
  category command
  types    [EnsuresBlock, ConditionEntry]
  contract """
    Recognize the ensures field on behavior entities. Parses inline
    blocks into ordered ConditionEntry lists:
    - Inline block: ensures { name "description" } — parsed as ordered
      ConditionEntry list (inline conditions, not graph nodes)
  """
  requires {
    entity_is_behavior "the enclosing entity has kind=behavior"
  }
  ensures {
    inline_parsed    "inline ensures block parsed into EnsuresBlock AST node with ordered ConditionEntry list"
    named_conditions "each inline condition has a name (identifier) and description (string)"
    empty_permitted  "empty ensures block produces empty ConditionEntry list"
    standalone_info  "ensures without requires produces info diagnostic (I011, condition_check)"
  }
  features [fa_structured_conditions]
  verify unit "inline ensures block with named conditions parsed"
  verify unit "empty ensures block produces empty ConditionEntry list"
  verify unit "ensures without requires produces info diagnostic"
}

behavior fa_parse_maintains_block "Parse Maintains Block" {
  category command
  types    [MaintainsBlock, ConditionEntry]
  contract """
    Recognize the maintains field on behavior and invariant entities as
    frame invariants (must hold before AND after). Parses inline blocks:
    - Inline block: maintains { name "description" }
  """
  ensures {
    behavior_permitted  "maintains block on behavior entity parsed"
    invariant_permitted "maintains block on invariant entity parsed"
    other_warned        "maintains block on feature/event/type/port produces warning"
    named_conditions    "each inline condition has a name and description"
  }
  features [fa_structured_conditions]
  verify unit "maintains block on behavior parsed"
  verify unit "maintains block on invariant parsed"
  verify unit "maintains block on feature produces warning"
}

behavior fa_validate_condition_consistency "I011: Ensures Without Requires" {
  category   query
  invariants [fa_condition_consistency]
  types      [RequiresBlock, EnsuresBlock, ConditionEntry]
  contract   """
    Note a behavior whose conditions are one-sided: it declares an
    ensures block but no requires block (or an empty one), so it
    guarantees its postconditions for every input. Reported as I011
    info by the condition_check pass (specforge analyze), with a
    suggestion to state what callers must establish, or to leave
    requires out when the behavior accepts every input. Condition names
    and descriptions are not cross-checked against each other or
    against the behavior's scope: no sound check over prose exists.
  """
  requires {
    blocks_parsed "requires, ensures, maintains blocks are parsed into AST"
  }
  ensures {
    missing_requires_info "ensures without requires produces I011 info"
    both_sides_pass       "a behavior with requires and ensures produces no I011"
  }
  features   [fa_structured_conditions]
  verify unit "consistent requires and ensures passes"
  verify unit "ensures without requires produces I011 info"
}

behavior fa_condition_check_pass "Condition Check Compiler Pass" {
  category   command
  invariants [fa_condition_consistency]
  types      [RequiresBlock, EnsuresBlock, ConditionEntry]
  produces   [fa_condition_check_complete]
  contract   """
    The condition_check compiler pass (specforge analyze) checks the
    structure of every behavior's requires/ensures blocks and every
    invariant's formal content after resolution. It reads which blocks
    are written, not what their conditions say:
    - W096: a behavior declares requires but no ensures (a caller's
      obligation buys no guarantee).
    - W039: a behavior's requires names a condition more than once.
    - I011: a behavior declares ensures but no requires.
    - W040: an invariant states its guarantee with no expression.
    Layering conditions (E031, named-condition set inclusion) are
    checked by layering_verify.
  """
  requires {
    graph_constructed "entity graph is fully built"
    conditions_parsed "all requires/ensures blocks are parsed"
  }
  ensures {
    one_sided_obligation "a behavior with requires and no ensures produces W096"
    repeated_condition   "a requires naming a condition twice produces W039"
    one_sided_guarantee  "a behavior with ensures and no requires produces I011"
    prose_invariant      "an invariant with a guarantee and no expression produces W040"
  }
  features   [fa_structured_conditions]
  verify unit "behavior with requires and no ensures produces W096"
  verify unit "pass runs after graph construction"
}

behavior fa_detect_redundant_precondition "W039: Redundant Precondition" {
  category query
  types    [RequiresBlock, ConditionEntry]
  contract """
    Detect a requires block that names the same condition more than
    once: the repeat states nothing its first occurrence does not.
    Reported as a W039 warning by the condition_check pass (specforge
    analyze), one per repeated name. Conditions are names and prose
    with no semantics for implication, so a precondition implied by a
    different one, or by the entity's types, is not detected.
  """
  ensures {
    redundant_warned     "a requires naming a condition twice produces W039"
    non_redundant_passes "a requires naming each condition once passes"
    suggestion           "W039 includes suggestion to remove the repeated condition"
  }
  features [fa_structured_conditions]
  verify unit "precondition repeated in its requires block produces W039"
  verify unit "independent precondition passes"
}

behavior fa_detect_invariant_without_property "W040: Invariant Without Expression" {
  category query
  contract """
    Detect invariant entities whose guarantee is prose only: they write
    a guarantee but no expression, the machine-checkable claim formal
    adds to invariants (proof role claim). A prose-only invariant relies
    on review and tests; one with an expression is also checked by
    specforge analyze --prove. Reported as a W040 warning by the
    condition_check pass (specforge analyze).
  """
  ensures {
    prose_only_warned "invariant with a guarantee but no expression produces W040"
    formal_passes     "invariant with an expression passes"
    suggestion        "W040 includes suggestion to add an expression for automated checking"
  }
  features [fa_structured_conditions]
  verify unit "invariant with prose-only guarantee produces W040"
  verify unit "invariant with an expression passes"
}

// ── Port Conditions ──────────────────────────────────────────

behavior fa_parse_port_operation_conditions "Parse Port Operation Conditions" {
  category command
  types    [RequiresBlock, EnsuresBlock]
  contract """
    Recognize requires/ensures blocks on individual port operations
    inside a port entity's methods block.
  """
  ensures {
    conditions_parsed     "port operation with requires/ensures blocks parsed"
    consistency_validated "port operation conditions validated for internal consistency"
  }
  features [fa_structured_conditions]
  verify unit "port operation with requires/ensures parsed"
  verify unit "port operation conditions validated for consistency"
}
