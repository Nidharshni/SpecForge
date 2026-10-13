# The formal diagnostics the design can check are built; the rest are trimmed

**Status:** accepted (2026-10-07)

The @specforge/formal specs named 27 diagnostic codes the catalog did not have, and two
behaviors used codes the catalog gives to core with another meaning (E060, a dangling edge;
E030, an invalid extension declaration). A third collision turned up while resolving them:
`fa_validate_conditions_without_verify` reported W144, core's invalid build cache. Nothing
emitted any of these codes, so the specs promised analyses SpecForge does not run.

Following [ADR 0003](0003-spec-promises-trimmed-to-the-design.md), each code was either built
from what a formal rule or pass actually receives, or trimmed: the spec now says what
SpecForge does. A rule sees one entity's field texts. A pass sees every entity's field texts
and edges (labelled by the field that writes them: `produces`, `consumes`, `ports`,
`participates_in`, `sub_processes`, ...), and under `specforge analyze` also the recorded test
results and the entailed claims. A trimmed code is not used, and no spec text names it as
reported. No obligation any test proved was touched.

## Built

Every built code is catalogued with owner `formal` (`docs/diagnostics.md`), and its spec
obligations are proven by tests through the real binary (`crates/specforge-cli/tests/formal_diagnostics.rs`,
`crates/specforge-project/tests/builtins.rs`), plus unit tests in `extensions/formal/src/lib.rs`.

**Declarative rules (every check).** None fires on SpecForge's own spec, which declares no
formal entity.

- **W124, W127, W129, W132, W135**: a property, axiom, protocol, refinement or process writes a
  `description` that is empty or only whitespace (`field_value_constraint`, `matches \S`). An
  absent description is not reported, as the contracts said.
- **W133**: a refinement declares no `invariant_deltas`, absent or `[]` (a custom rule, since
  no declarative check covers both). The contract named a `conditions` field the kind does not
  declare; it now names `invariant_deltas`.
- **W136**: a process writes an empty `alphabet`. The field is required, so an absent one is
  already E006; the contract says so.
- W133 and W136 claimed to fire only at `warning_level=strict`. No warning level exists (the
  `fa_progressive_warnings` feature is planned), so they fire in every check like the other
  formal rules.

**The check-phase pass.** `analysis_available` (formal's first check-phase pass) reports
**I015** once per compile when behaviors declare requires or ensures, suggesting
`specforge analyze`. It adds one info to `specforge check .` on this repository.

**Analyze passes** (`specforge analyze` only, so `check` is unaffected):

- **E034** (event_graph_analyze): Tarjan's SCC over the event flow graph (behavior -> each event
  it `produces`, event -> each behavior that `consumes` it); a component holding two or more
  behaviors is a cycle. The contract promised three mitigations: `sync.timeout`, an
  `@idempotent` annotation and a circuit-breaker field. The data model has neither annotation
  nor field, so the mitigation is now a non-empty `sync` on any member (what it says is not
  checked). The finding names one cycle path, found by a breadth-first search from the
  component's first behavior, and suggests a `sync` on one of its members.
- **W032**: a behavior that consumes an event and produces it again, with no `sync` on either.
  A cycle with one behavior is W032, not E034. The two can coexist when a retry sits inside a
  larger cycle.
- **W034**: an event that a behavior produces and that declares no `sync`. An event nothing
  produces carries no messages and is not reported.
- **I009**: once, when the event graph has flow edges and neither an E034 nor a W032 cycle.
- **W033**: a port that two or more behaviors use (`ports`) where the most-referenced has more
  than three times the incoming edges of the least-referenced. A behavior nothing references
  counts as one edge, so one edge against zero does not count as more than 3:1.
- **I011** (condition_check): a behavior with ensures but no requires. The rest of the old
  `fa_validate_condition_consistency` (postconditions referencing undefined state, maintains
  consistency) compared prose and was dropped; the behavior is now I011 alone.
- **W039** (condition_check), reduced: a behavior's `requires` names the same condition more than
  once. A precondition implied by a *different* one (the old promise) needs semantics for
  implication that names and prose don't have; the contract now says that is not detected.
- **W040** (condition_check): an invariant with a guarantee and no `expression`. The contract
  named a "maintains block"; an invariant's formal content is the `expression` claim formal
  adds to it (ADR 0009).
- **I008** (coverage_tracking): a coverage item whose every obligation a passing recorded test
  names, naming those tests. It does not repeat @specforge/testing's coverage pass, which
  reports only gaps (A001, A014, A015, A016). An item proven only by an entailed claim is not
  reported.
- **I014** (coverage_tracking): each behavior's `SpecificationDepthLevel`, a ladder: `prose` (no
  edges), `entity_graph` (edges, no requires/ensures), `conditions` (requires or ensures),
  `invariants` (also `maintains` or `invariants`), `proofs` (also proven under the coverage
  rule). Behaviors at level 2 or deeper are reported with the step to the next level. When
  more than five behaviors sit below level 2, one more I014 suggests adopting requires/ensures.
  The "orthogonal dimensions" and the `heuristic_ok` audit were dropped: nothing computes
  `heuristic_ok`.

## Trimmed

- **E034 for process deadlock** (`fa_detect_process_deadlock`): it flagged parallel processes
  whose alphabets overlap without synchronization. In CSP, a shared alphabet event *is*
  synchronization, so overlap is not a deadlock risk. A real deadlock check needs the
  processes' transitions, and the process kind declares states with no transitions. The
  behavior, and the "deadlock analysis" claims in the event graph pass and the features, were
  removed. Process composition cycles stay E042.
- **W130** (protocol ordering conflict): it checked a protocol's `ordering` field, which the
  protocol kind does not declare (`transitions` are free-form strings), so it had nothing to
  read. The behavior, its failure mode (`protocol_ordering_false_positive`), and the
  ordering-validation claims of the event graph pass were removed.
- **W036** (port-behavior condition compatibility): ports carry no requires/ensures (`methods`
  is a block of signatures), so there are no conditions to compare.
- **W037** (unverifiable condition): it relied on spotting ambiguous language in prose, which
  no sound check can do.
- **W038** (unreachable postcondition): conditions are names and prose descriptions with no
  semantics for contradiction.
- **W058** (feature coverage mismatch): its own contract said it had no implementable algorithm,
  and it depended on `warning_level`, which does not exist.
- **E060 in formal** (payload type mismatch): an event declares one `payload`; producers and
  consumers declare no payload type to compare against it. The code stays core's.
- **E030 in formal** (contradictory precondition patterns): name and keyword heuristics over
  prose, using a core code. Trimmed, along with the condition_check pass's satisfiability and
  reachability claims. condition_check now says it checks which blocks are written (W096, I011,
  W040). E031 is layering_verify's.
- **W144 in formal** (conditions without a `contract`/`property` verify): it collided with
  core's W144, and the `contract` verify kind it asks for is not one formal declares.

## History

**W059, W068, W069 and W074** appear only in decisions. The decisions now say what happened:
W058-W068 was a renumbering range that was never kept (W059-W060 were removed with the
condition entity kind, W058 is trimmed here, W060-W062 are core codes). W069-W074 were
renumbered W131-W136.

## Consequences

- `specforge analyze` on SpecForge's own spec adds, on top of the existing W029 (95) and W035
  (1): W040 147, I011 212, W034 130, W033 1 (`WasmRuntime`), I009 1, I008 311 and I014 555 (554
  behaviors at level 2 or deeper, plus one adoption note for the 56 below it). There is no
  E034, W032 or W039. `specforge check .` is unchanged apart from one I015 (0 errors, the same
  4 warnings).
- The manifest spec now lists what each formal pass reports, the 13 validation rules, and the
  five passes.
- No feature changed status: each formal feature's solution still promises something unbuilt
  (`warning_level`, condition deltas, protocol timeouts, payload checks on the old model, and so
  on).

## Amendment (round 8, plan 01)

A behavior's `requires`, `ensures` and `maintains`, and a behavior's or event's `sync`, are block fields; the
passes read their entries' names, as before. `BehaviorRequiresInvariant`, `BehaviorEnsuresInvariant` and
`BehaviorMaintainsInvariant` are no longer declared: no field wrote them (ADR 0064).
