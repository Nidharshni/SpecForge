// @specforge/testing behaviors

use "extensions/testing/invariants"
use "extensions/testing/types"
use "types/zero-entity-core"

behavior te_contribute_testability "Contribute Testability to Owned Kinds" {
  features   [te_test_vocabulary]
  category   command
  invariants [te_testable_kinds_from_one_table, te_owners_declare_no_test_vocabulary]
  types      [TestingVerifyKind]
  contract   """
    @specforge/testing MUST contribute testability as entity enhancements
    carrying verify kinds, one per testable kind, each kind declared by an
    optional peer of @specforge/testing. Applying such an enhancement marks
    the target kind testable with exactly those verify kinds. An
    enhancement of a kind no loaded extension declares is skipped silently.
  """
  requires {
    kinds_registered "the owning extensions' kinds are registered before enhancements apply"
  }
  ensures {
    kinds_testable "each table kind is testable with the table's verify kinds"
    owner_optional "a kind whose extension is not loaded produces no diagnostic"
  }
  verify unit "testing makes software and governance kinds testable with their verify kinds"
  verify unit "an enhancement with verify kinds makes its target kind testable"
}

behavior te_validate_unverified_testable "W004: Unverified Testable Entities" {
  features   [te_test_vocabulary]
  category   query
  invariants [te_testable_kinds_from_one_table]
  types      [ValidationRulePattern]
  contract   """
    Detect entities of a testable software kind that declare no verify
    obligations. Union types (`type X = A | B`) are exempt: they have no
    body to hold obligations. Entities marked `abstract true`, through a
    flag their kind's registry entry declares, are exempt: their
    obligations are carried by the concretes that refine them.
    Governance constraints and failure modes are testable but not
    required to declare obligations. Exemption is decided from the
    entity's structure and the registry, never from a field's name: a
    struct member named `verify`, `abstract` or `gherkin` neither
    declares an obligation nor exempts the entity. What W004 exempts is
    exempt from A001, stats, the coverage gate and the verify-stub code
    action too.
  """
  ensures {
    unverified_detected "testable entity with no verify produces W004"
    verified_passes     "testable entity with verify produces no diagnostic"
    union_exempt        "a union type never produces W004"
    abstract_exempt     "an abstract entity never produces W004"
    names_exempt_none   "a field's name alone neither declares an obligation nor exempts"
  }
  verify unit "testable behavior with no verify produces W004"
  verify unit "testable behavior with verify passes"
  verify unit "a union type never produces W004"
  verify unit "an abstract entity never produces W004"
  verify unit "a field named like an obligation or an exemption exempts nothing from W004"
}

behavior te_validate_verify_kind_allowlist "W009: Verify Kind Outside the Kind's Allowlist" {
  features   [te_test_vocabulary]
  category   query
  invariants [te_testable_kinds_from_one_table]
  types      [ValidationRulePattern, TestingVerifyKind]
  contract   """
    Detect `verify <kind> "..."` statements whose kind is not among the
    verify kinds @specforge/testing registered for the entity's kind.
    A bare `verify "..."` has no kind and always passes.
  """
  ensures {
    allowed_passes    "verify kind in the allowlist produces no diagnostic"
    disallowed_warned "verify kind outside the allowlist produces W009"
  }
  verify unit "behavior with verify unit passes (unit in allowedVerifyKinds)"
  verify unit "invariant with verify load produces W009 (load not in allowedVerifyKinds)"
  verify unit "W009 message includes allowed set"
}

behavior te_coverage_pass "Coverage Analysis Pass" {
  features   [te_coverage_analysis]
  category   query
  invariants [te_testable_kinds_from_one_table]
  types      [TestingVerifyKind]
  contract   """
    @specforge/testing's `coverage` compiler pass (run by `specforge
    analyze coverage`, or `analyze` with every pass) MUST score the
    project at three layers. Intent: a testable entity that declares no
    verify obligations is A001, unless W004 exempts it (a union type, an
    abstract entity, or a kind no rule requires obligations of), and an
    invariant with none is A002, an error when its risk is high. The risk
    grading (the graded kind, its risk field and the error level) is
    testing's own, read from its testable-kinds table and passed to the
    shared coverage rule, which names no kind; the host's per-entity
    coverage views pass none and read no risk (ADR 0009). Exempt
    entities that declare nothing leave the testable count and are
    counted apart (testable_exempt). Enforcement: invariants nothing
    references are counted in the summary (the finding is software's
    W003 from `specforge check`, not repeated here). Proof, from the recorded
    tests (specforge-report.json, which `analyze` reads by default after
    `specforge collect`): a test proves an obligation by naming its text.
    An obligation no passing test names is A015, a test naming an
    obligation its entity doesn't declare is A016, and a failing test is
    A014. A skipped test proves nothing and fails nothing. An entity is proven when it declares at least one obligation,
    every obligation is proven, and none of its recorded tests fails: a
    test that proves nothing declared is not proof, so an entity with no
    obligations is never proven, whatever its tests. A formal claim the
    prove pass entailed discharges `verify property` obligations without
    executable tests, and an entity whose obligations it all discharges
    is proven.
    The summary MUST report the discharge funnel (entities with
    obligations, proven, formally discharged, report failures).
  """
  ensures {
    intent_scored        "entities without obligations are A001, invariants A002"
    unreferenced_counted "unreferenced invariants are counted, not re-reported"
    proof_recorded       "recorded passing tests prove an entity; a failing one is A014"
    obligations_proven   "each obligation needs a passing test that names it (A015); unknown names are A016"
    formal_discharge     "entailed formal claims discharge verify property obligations"
  }
  verify unit "a high-risk invariant without obligations is an A002 error"
  verify unit "invariant references count as enforcement"
  verify unit "recorded test results prove entities and failing tests are A014"
  verify unit "an obligation no passing test names is A015 and a test naming an undeclared obligation is A016"
  verify unit "a skipped test neither proves an obligation nor fails its entity"
  verify unit "a proved formal claim discharges verify property obligations"
  verify unit "an entity with no obligations is never proven, even by passing tests"
  verify unit "an entity whose obligations are all formally discharged is proven without tests"
  verify unit "entities W004 exempts are not A001 and leave the testable count"
  verify unit "an entity counts toward coverage when its kind is testable and it owes or declares obligations, and is exempt when testable and neither"
  verify unit "the rule reads a pass entity as the host decided it, with no risk unless the grading caller sets one"
  verify unit "without a risk grading no kind is risk-tallied and nothing is A002"
}

behavior te_coverage_gate "Proof Coverage Gate" {
  features [te_coverage_analysis]
  category validation
  types    [TestingVerifyKind]
  contract """
    `specforge analyze coverage --min N` MUST exit non-zero (E048) when
    the share of testable entities the coverage pass proved is below N
    percent, after printing the full analysis. Entities W004 exempts
    that declare nothing are not in the denominator, so 100 is reachable. A proven entity of a kind
    that is not testable counts in the discharge funnel but not toward
    the gate: the summary's testable_proven is its numerator. It needs test results
    (the project's specforge-report.json or --test-results) and the
    coverage pass, and a project with nothing testable satisfies any
    threshold. The analyze operation decides the run's verdict with the
    gate in it: a minimum that is not a percentage is invalid input; a
    minimum the coverage pass will not answer (not selected, its extension
    not loaded) is refused before any pass runs (E068); proof coverage
    under the minimum fails the run (E048); a coverage pass that ran
    without a figure the gate reads leaves the run unjudged (E068). The
    analysis document carries the verdict as ok and, with a minimum, where
    the gate landed (gate: status met, below or unjudged, the minimum, and
    the figure).

    Exit codes: below the threshold (E048) exits 1; E068 and missing test
    results exit 2. E048 and E068 take precedence over any other finding.
    Under --format json the document says it and nothing is written to stderr.
    The coverage pass must have run, so `pass` is `coverage` or `all`;
    naming another pass alone does not satisfy the gate. specforge.analyze
    takes min and returns the same ok and gate.
  """
  ensures {
    below_fails   "proof coverage below the threshold fails with E048"
    above_passes  "proof coverage at or above the threshold passes"
    needs_results "the gate refuses to run without test results"
  }
  verify unit "coverage at or above the threshold passes"
  verify unit "coverage below the threshold fails with E048"
  verify unit "the gate needs test results"
  verify unit "a proven entity whose kind is not testable does not raise the gate"
  verify unit "the analysis's ok is the run verdict, the gate included, and its JSON says where the gate landed"
  verify unit "a minimum that is not a percentage is invalid input"
  verify unit "a gate without a readable coverage pass is not met"
  verify unit "a gate without the coverage pass exits 2 with E068"
}

behavior te_stray_test_records "Stray Test Records" {
  features [te_coverage_analysis]
  category validation
  contract """
    `specforge analyze` MUST report each test record that names an entity
    the graph does not know as W097, with the closest known entity id as a
    "did you mean" hint when one is near. Matching stays exact: the record
    is never reassigned to the near match. The operation returns these
    stray test records as data outside the pass reports, so `--strict`
    never promotes them and neither `ok` nor the exit code changes. The
    CLI prints the W097 lines to stderr before the reports under human
    output; under `--format json` they are the document's `stray_records`
    alone, and nothing is written to stderr. The CLI's `--format json` output and the MCP analyze result gain a top-level
    `stray_records` list of `{entity_id, near}` only when it is
    non-empty; with none the output is unchanged.
  """
  ensures {
    warns_with_hint "an unknown entity in a test record warns W097 with a close-match hint and does not fail the run"
    json_alone      "under --format json a stray test record is the document's alone: nothing on stderr"
    optional_field  "stray_records appear in the json output only when records exist"
    not_promoted    "strict neither promotes a stray record nor changes ok or the exit code"
  }
  verify unit "an unknown entity in a test record warns W097 with a close-match hint and does not fail the run"
  verify unit "under --format json a stray test record is the document's alone: nothing on stderr"
  verify unit "stray_records appear in the json output only when records exist"
  verify unit "strict neither promotes a stray record nor changes ok or the exit code"
}
