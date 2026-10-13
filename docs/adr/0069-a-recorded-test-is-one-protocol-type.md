# A recorded test is one protocol type

**Status:** accepted (2026-10-13, architecture round 8, plan 08). Amends ADR 0013 D1 and D5, and ADR 0019's Decision.

A recorded test (its name, its status, the obligation it names, its duration) went from a runner's report to the
coverage rule through five shapes: the collect wire's `CollectTestResult`, the report's `ReportedTest`, the pass
input's `PassTestResult`, the coverage rule's `RecordedTest`, and the copies testing, formal, product and xtask made
of the last. Its status was a `String` in four spellings: `pass|fail|skipped` in the binary report `specforge-test`
writes, `passed|failed|skipped` on the collect wire, `pass|fail` in `specforge-report.json` and the pass input, and
`pass|fail|skip|error` in a report schema that described a shape never written. Each reader fell back silently:

- collect counted any status it did not know as skipped, with no diagnostic: a collector answering `ok` proved
  nothing and nobody was told;
- the coverage rule read every status but `pass` as failing: a report in the collect vocabulary (`passed`) was
  all A014;
- cargo-test and vitest mapped any status they did not know to skipped.

## D1. One `TestStatus`, one `TestResult`, in the protocol

`specforge_protocol_types` defines `TestStatus` (`passed | failed | skipped`, serde lowercase) and `TestResult`
(`name`, `status`, `verify`?, `duration_ms`?), and `TestResults` (`runner`?, `results`: per entity id,
`{tests}`) with `TestResults::of(id)`. The collect wire's `CollectEntityResult` and `CollectUnlinkedTest`, the pass
input's `test_results` and the host's `specforge-report.json` (each test a flattened `TestResult` plus its
`runner`) carry them. `PassTestResult`, `PassTestResults`, `PassEntityResults` and `CollectTestResult` are gone.
`passed|failed|skipped` was kept over `pass|fail|skip` because every collector that conformed keeps answering
validly.

## D2. The coverage rule reads them

`specforge_coverage::assess` takes `Option<&TestResults>` and `Verdict::of` takes `&[TestResult]`, the types the
pass input carries: testing's pass hands its input on unconverted, formal and product read `TestResults::of`, and
the host converts its report once (`From<&TestReport> for TestResults`), the conversion it already made for the
pass input. `RecordedTest`, the rule's own `TestResults` and `PASSING_STATUS` are gone. `Passed` proves the
obligation it names, `Failed` fails the entity (A014), `Skipped` is counted among its tests and proves and fails
nothing. Collect still counts skipped tests and does not record them.

## D3. An unknown status is refused where it is decoded

On the collect wire it is not a `CollectOutput` (E028 naming the collector, ADR 0013 D4), in a report it is not a
usable report (E045, ADR 0004 D2-e). A test's `name` is required everywhere, as ADR 0013 D5 made it on the wire.
A runner adapter maps its runner's documented statuses exhaustively and answers its error on any other (cargo-test:
the protocol's own, which `specforge-test` writes; vitest: `passed`, `failed`, `skipped`, `pending`, `todo`,
`disabled`). Rejected: a per-test W115 that drops a test with an unknown status and keeps the rest: it needs a
lenient decoder beside the protocol type, which ADR 0013 D5 rejected.

## D4. `specforge-test` records the protocol's `TestStatus`

The Rust test integration depends on `specforge-protocol-types` and writes `passed`/`failed`. The dev-dependency
cycle this makes (protocol-types dev-depends on `specforge-test`) is legal and compiles protocol-types twice in its
own test target only. Rejected: a second enum kept equal by tests.

## D5. Protocol 1.4.0

The pass input's statuses change meaning and gain `duration_ms`; the collect answer only loses values the protocol
never allowed. A minor version, as ADR 0050 D4's rename was.

## Consequences

- `specforge-report.json` says `passed`/`failed`, has no `file` key and requires names: a report written before
  1.4 is E045 until `specforge collect` rewrites it.
- A collector that answers a status outside the three fails the collect with E028; cargo-test and vitest fail on an
  undocumented status of their runner's.
- A hand-written report's `skipped` test is no longer A014.
- `schema/specforge-report.schema.json` describes the report collect writes; a test validates the committed example
  against it and another holds the example to what collect writes.
- Every builtin is rebuilt (protocol 1.4.0).

## What would reopen it

- A runner whose statuses cannot be read as these three (a "flaky" that is neither): a fourth status, a minor
  version.
- A reader that needs a test's runner inside a pass: `runner` joins `TestResult`.
