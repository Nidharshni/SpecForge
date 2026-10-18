// Validation behaviors — core graph validation rules

use "events/compilation"
use "invariants/core"
use "invariants/validation"
use "invariants/zero-entity-core"
use "ports/outbound"
use "types/core"
use "types/diagnostics"
use "types/errors"
use "types/graph"
use "types/zero-entity-core"

behavior detect_dangling_references "Detect Dangling References" {
  features   [structural_validation]
  invariants [
    reference_resolution_completeness,
    diagnostic_determinism,
    validation_pipeline_ordering,
  ]
  category   validation
  types      [Diagnostic, EntityId, ValidationCode, Severity, ValidationError, Graph, Edge]
  consumes   [graph_built]
  // Diagnostics from this validator flow into the declarative_validation_executed
  // aggregate and ultimately to validation_complete. No separate event produced.
  requires {
    graph_built_fired "graph_built event has fired, confirming the in-memory graph is fully constructed"
  }
  ensures {
    resolver_integrity_verified "Every reference list entry naming an existing entity has its graph edge after every link, cold or incremental"
    no_duplicate_diagnostics    "A reference to a missing entity is the linker's E003 alone; nothing reports it again"
  }
  contract   """
    This behavior is the linker's postcondition, not a check a user
    sees. After every link (the cold build's and every incremental
    update's), each entry of a reference list that names an existing
    entity MUST have its graph edge, labeled with the field. A build with
    debug assertions MUST fail on a reference without its edge: that is
    a SpecForge bug, never a spec error. A reference to a missing entity
    is E003, emitted once by the linker. E060, which reported the bug as
    a diagnostic, is retired.
  """
  verify unit "a reference to an existing entity without its edge fails the linker's assertion"
  verify unit "reference with corresponding graph edge passes"
  verify unit "an empty graph passes the linker's assertion"
  verify contract "Detect Dangling References: dangling reference detection holds — graph_built_fired, resolver_integrity_verified, no_duplicate_diagnostics"
}

behavior detect_duplicate_entity_ids "Detect Duplicate Entity IDs" {
  features   [structural_validation]
  invariants [string_interning_consistency, entity_id_uniqueness]
  category   validation
  types      [Diagnostic, DuplicateIdError]
  consumes   [all_files_parsed]
  // Diagnostics from this validator flow into the declarative_validation_executed
  // aggregate and ultimately to validation_complete. No separate event produced.
  requires {
    all_files_parsed "all_files_parsed event has fired, confirming every .spec file is parsed and entity IDs collected"
  }
  ensures {
    duplicate_ids_diagnosed "Every duplicate entity ID has an E002 diagnostic emitted naming both declaration sites"
  }
  contract   """
    The validator MUST detect entity IDs declared more than once across
    all .spec files. Duplicate IDs MUST produce an E002 diagnostic that
    names both declaration sites (file, line, column).
  """
  verify unit "duplicate ID in same file produces E002"
  verify unit "duplicate ID across files produces E002"
  verify unit "E002 includes both source locations"
  verify contract "Detect Duplicate Entity IDs: duplicate entity ID detection holds — all_files_parsed, duplicate_ids_diagnosed"
}

behavior require_block_titles "Require Block Titles" {
  features   [structural_validation]
  invariants [entity_id_uniqueness]
  category   validation
  types      [Diagnostic]
  consumes   [all_files_parsed]
  contract   """
    The graph build MUST report E076 for every block that becomes a node
    and has no title, or a blank one: entity blocks, unions, spec and ref.
    A define block is never a node (W143) and needs none. The block MUST
    still be placed in the graph, so hover, completion and navigation keep
    working while it is typed. The check needs no extension and names no
    kind; it follows the file in an incremental build.
  """
  verify unit "a block with no title or a blank one is E076 and is still placed"
  verify unit "an untitled union is E076"
  verify unit "define, spec and ref blocks are not E076"
  verify unit "E076 needs no extension and follows the file in an incremental build"
}

// ── Domain-Specific Validation ──────────────────────────────
// Domain-specific validations (unreferenced entity checks, unused reference
// warnings, unverified entity warnings, trigger consistency checks, etc.)
// are declared as ValidationRulePatterns in extension manifests and are
// defined in their owning extension directories:
//   - spec/extensions/software/validation-rules.spec
//   - spec/extensions/product/validation-rules.spec
//   - spec/extensions/governance/validation-rules.spec
// The core compiler executes these patterns generically via the declarative
// validation engine (see behaviors/zero-entity-core.spec).
// Diagnostic codes (W001, W003, W004, W007, etc.) are defined by
// their owning extensions, not by the core compiler.

// ── Structural Validation (core — domain-agnostic) ───────────
// DiagnosticBag accumulator pattern: core structural validators do not emit
// individual diagnostic events. Instead, each validator appends Diagnostic
// values into a shared DiagnosticBag. After all structural validators have
// run, the bag is drained into the declarative_validation_executed aggregate
// and forwarded to validation_complete. This avoids O(n) event fan-out for
// large graphs and lets the pipeline batch diagnostics for deterministic
// ordering (see diagnostic_determinism invariant).
// Core structural validations operate on graph topology and field presence
// WITHOUT knowledge of entity semantics. They check: dangling references,
// duplicate IDs, import cycles, unreferenced structural nodes (W012 for any
// grammar-level structural kind — ref, spec — with zero incoming edges),
// and file-reference existence. Extension-defined entity kinds opt into
// generic unreferenced detection via extension-defined `no_incoming_edges`
// ValidationRulePatterns. Domain-specific unreferenced rules (e.g., W001 unreferenced
// entity) are extension-defined ValidationRulePatterns.

behavior detect_unreferenced_refs "Detect Unreferenced Refs" {
  features   [structural_validation]
  invariants [
    reference_resolution_completeness,
    diagnostic_determinism,
    validation_pipeline_ordering,
  ]
  category   validation
  types      [Diagnostic, EntityId, Graph]
  consumes   [graph_built]
  // Diagnostics from this validator flow into the declarative_validation_executed
  // aggregate and ultimately to validation_complete. No separate event produced.
  // H1: ref and spec are grammar-level structural constructs — parsed by the
  // core grammar (like `use` and `define`), NOT extension-defined entity kinds.
  // Because they are structural, their unreferenced detection belongs in core, not in
  // extension manifests. Extension-defined entity kinds that want unreferenced
  // detection declare a `no_incoming_edges` ValidationRulePattern in their
  // extension manifest, which the declarative validation engine handles
  // separately.
  requires {
    graph_built_fired "graph_built event has fired, confirming the in-memory graph is fully constructed with all edges"
  }
  ensures {
    unreferenced_detected  "Every ref with zero incoming edges produces a W012 warning; a spec block, the root container, does not"
    referenced_nodes_clean "Structural nodes with at least one incoming edge produce no warning"
  }
  contract   """
    The registry build's checks MUST report every ref entity that has
    zero incoming edges in the compiled graph as a W012 warning
    identifying the unreferenced node. A spec block is the project's
    root container: nothing references it, and it produces no W012.

    This is a generic structural check applied uniformly to all
    grammar-level structural kinds — it does not encode domain
    knowledge about any specific kind. The set of structural kinds is
    defined by the grammar (currently ref and spec), not by
    extensions.

    Extension-defined entity kinds that want generic unreferenced detection
    declare a `no_incoming_edges` ValidationRulePattern in their
    extension manifest, which the declarative validation engine
    handles separately.
  """
  verify unit "unreferenced ref produces W012"
  verify unit "referenced ref suppresses W012"
  verify unit "unreferenced structural node of any grammar-level kind produces W012"
  verify unit "structural node with at least one incoming edge suppresses W012"
  verify contract "Detect Unreferenced Refs: unreferenced ref detection holds — graph_built_fired, unreferenced_detected, referenced_nodes_clean"
  verify unit "spec block is a root container and does not produce W012"
}

// Core structural validation: checks file existence for ANY field declared as
// a file reference in extension metadata. This is purely structural and
// domain-agnostic — the core checks that referenced files exist, just like it
// checks that referenced entity IDs exist (E003). This mechanism applies to
// any file-reference field registered by any extension (e.g., gherkin from
// @specforge/software, or a future openapi field from another extension).
// W018 (missing file-reference field on supported kind) is an extension-level
// concern: it is declared as a missing_field_when_flag_set ValidationRulePattern
// by the owning extension, not hardcoded in core. The core only handles E016.
behavior validate_file_reference_paths "Validate File Reference Paths" {
  features   [structural_validation, se_gherkin_bridge]
  // reference_resolution_completeness applies here because file paths are a form
  // of reference that must resolve: just as entity-ID references must resolve to
  // graph nodes, file-path references must resolve to existing filesystem entries.
  // Both are "references" in the graph-integrity sense — unresolved file paths
  // violate the same completeness guarantee as dangling entity references.
  category   validation
  invariants [reference_resolution_completeness, validation_pipeline_ordering]
  types      [Diagnostic]
  ports      [FileSystem]
  consumes   [graph_built]
  // Diagnostics from this validator flow into the declarative_validation_executed
  // aggregate and ultimately to validation_complete. No separate event produced.
  requires {
    graph_built_fired    "graph_built event has fired, confirming the in-memory graph is fully constructed"
    filesystem_available "FileSystem port is available for checking file existence"
  }
  ensures {
    missing_files_diagnosed "Every non-existent file path in a file-reference field produces an E016 diagnostic"
    existing_files_pass     "File-reference fields pointing to existing files produce no diagnostic"
  }
  contract   """
    This behavior performs generic file-path validation for any entity
    field whose extension metadata declares it as a file reference.
    The validation is domain-agnostic: the core does not interpret the
    semantics of the referenced file — it only checks existence.
    Any extension can register file-reference fields (e.g., gherkin
    from @specforge/software, openapi, protobuf, schema) and they
    will all be validated by this same mechanism.

    File existence (E016): every file path in a file-reference field
    MUST reference an existing file relative to the spec root.
    Non-existent files MUST produce an E016 diagnostic. A field is a
    file reference on the kinds whose registry entry declares it so: a
    field of the same name on another kind is not one. Its value names
    its paths: a single value's text, quoted or a bare word, or each item of
    a list; an empty value names none, and so do a block and an expression
    group. E016 is a rule the host builds for each such field; it runs with
    the rule set, after E006.

    Field-presence warnings (e.g., W018 for missing file-reference
    field on a supported kind) are NOT handled here — they are declared
    as missing_field_when_flag_set ValidationRulePatterns in the owning
    extension manifest and executed by the declarative validation engine.
  """
  verify unit "non-existent file reference produces E016"
  verify unit "existing file reference passes silently"
  verify unit "multiple file references in same entity each validated independently"
  verify unit "relative path resolved from the spec root"
  verify unit "a field another kind declares as a file reference is not one on this kind"
  verify unit "a single path on a file-reference field is checked like a one-item list"
  verify unit "a bare word on a file-reference field is a path"
  verify integration "a compile checks a bare word on a file-reference field as a path"
  verify contract "Validate File Reference Paths: file reference validation holds — graph_built_fired, filesystem_available, missing_files_diagnosed, existing_files_pass"
}

// W017 (testable kind without verify support) is registry_build_kinds, in behaviors/zero-entity-registries.spec.

// -- The Prove Pass ------------------------------------------------------------

behavior prove_declared_bounds_and_claims "Prove Declared Bounds and Claims" {
  features   [structural_validation]
  invariants [diagnostic_determinism]
  category   validation
  types      [Diagnostic, ProofRole, FieldDescriptor, SolveFailure]
  ports      [SmtSolver]
  requires {
    roles_declared "the fields the pass reads carry a proof role their extension declared (ADR 0009)"
  }
  ensures {
    bounds_consistent "contradictory declared bounds are E046 citing every bound of the unsat core"
    claims_checked    "an entailed claim is a proved claim; a claim not entailed is W139 with a counterexample"
    one_warning       "a solver that does not run is one W098, worded where its adapter is"
    bounded           "every call to the solver ends at its timeout"
  }
  contract   """
    specforge analyze --prove runs the prove pass before the other passes
    and reports it last. It asks the SMT solver once whether it runs here.
    When it does not, the pass reports one W098 saying why (not on PATH, or
    what `z3 --version` did), solves nothing, and its summary's solver is
    null; specforge doctor states the same W098, word for word, as its
    toolchain finding. When it runs, every bound's parseable lines are
    asserted together: unsat is E046 citing every bound of the unsat core.
    Each claim is checked as bounds and not claim: unsat proves it (a proved
    claim, which the extension passes then receive), sat is W139 with a
    counterexample rendered in the declared units, anything else is I098.
    Every call to the solver ends at its timeout (the probe at 5 seconds, a
    solve at --prove-timeout, 30 seconds by default), even when the solver
    leaves a child process; a solve that timed out or failed is W098 and the
    checks it did not answer are skipped. --prove-timeout without --prove is
    a usage error. Without --prove, and always over MCP, no prove pass runs
    and the extension passes receive no proved claims.
  """
  verify unit "contradictory declared bounds are E046 citing every bound of the unsat core"
  verify unit "a claim the bounds do not entail is W139 with a counterexample in the declared units"
  verify unit "a claim the bounds entail reaches the extension passes as a proved claim"
  verify unit "a solver that does not run is one W098, the finding doctor states, and nothing is solved"
  verify unit "a solver that times out or fails mid-run is W098 and the checks it did not answer are skipped"
  verify unit "a solver call ends at its timeout, even when the solver leaves a child process"
  verify integration "specforge analyze --prove-timeout sets the solver's timeout and needs --prove"
}

// -- The Contracts Pass --------------------------------------------------------

behavior report_entities_without_contract_obligations "Report Entities Without Contract Obligations" {
  features   [structural_validation]
  invariants [diagnostic_determinism]
  category   validation
  types      [Diagnostic]
  requires {
    contract_targets_declared "an extension declares which kinds are contract targets (contract_target)"
  }
  ensures {
    unconstrained_reported "each entity of a contract-bearing kind that references no contract target is one A010"
    counted                "the summary counts the contract entities, the unconstrained ones and the references"
  }
  contract   """
    specforge analyze contracts runs the host's one built-in pass. A
    contract-bearing kind is one with a reference field whose target kind an
    extension declares a contract target. Each entity of such a kind that
    references nothing in any of those fields (a list item or a single
    reference) is one A010, an info, at the
    entity's span; its suggestion names the kind's contract fields, sorted.
    The summary counts contract_entities (entities with at least one such
    reference), unconstrained_entities and obligation_references (the
    non-empty contract fields). Entities are visited in ID order.
  """
  verify unit "an entity of a contract-bearing kind that references no contract target is A010, an info"
  verify unit "a single contract reference is a contract obligation"
  verify unit "the contracts pass's suggestion names the kind's contract fields, sorted"
}
