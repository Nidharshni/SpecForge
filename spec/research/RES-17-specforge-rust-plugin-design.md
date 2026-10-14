---
id: RES-17
kind: research
title: "SpecForge Rust Plugin Design — @specforge/rust Test Traceability for Rust Projects"
status: active
date: 2026-03-02
depends_on: [RES-14, RES-15]
---

# RES-17: SpecForge Rust Plugin Design

## Problem Statement

SpecForge is written in Rust. Its first consumer is itself. The three-layer traceability model (RES-15: intent → linkage → proof) requires a language-specific extension to close the loop for Rust projects: from `.spec` entities through Rust code to passing `cargo test` results in `specforge-report.json`.

RES-16 chose Option B (consume results) over running tests directly. The Rust integration needs to address Rust's specific constraints:

1. **libtest has no reporter plugin API** — unlike vitest (JS) or pytest (Python), Rust's built-in test harness is not extensible
2. **`cargo test --format json` is unstable** — stuck behind `-Z unstable-options` since 2018, no stabilization timeline
3. **Compile-time test discovery** — Rust discovers tests at compile time via `#[test]` attributes, not at runtime
4. **`#[test]` is a lang item** — not a hookable proc macro; cannot intercept test registration
5. **Multiple test binary fragmentation** — each crate and `tests/*.rs` file compiles to a separate binary

This analysis was produced by 10 specialized expert agents researching the Rust test ecosystem, three-layer traceability mapping, report collection architecture, entity-to-test mapping, extension model fit, CI integration, crate architecture, competitive landscape, and developer experience.

---

## Decision Summary

| Decision | Choice |
|----------|--------|
| **Extension name** | `@specforge/rust` |
| **Extension type** | Uses existing Adapter interface for test collection |
| **Crates (crates.io)** | `specforge-test` (runtime lib) + `specforge-test-macros` (proc macro) |
| **Test annotation (primary)** | `#[specforge::test(behavior = "entity_id")]` proc macro attribute |
| **Test annotation (fallback)** | `/// @specforge-behavior entity_id` doc comments |
| **Convention fallback** | `mod entity_id { #[test] fn ... }` module naming |
| **Report collection** | `specforge collect rust` subcommand (Go pattern from RES-16) |
| **Primary test output source** | cargo-nextest JUnit XML (stable) |
| **Secondary test output source** | libtest `--format json` (unstable, behind feature flag) |
| **Report file** | `target/specforge/<binary-name>.json` per test binary, merged by collector |
| **Entity mapping resolution** | `tests` field > proc macro attribute > module name convention |
| **Proc macro behavior** | `Drop`-based guard — detects panicking, records pass/fail |
| **Verify description → fn name** | Slugification: lowercase, spaces to `_`, strip special chars |
| **Naming convention separator** | `{entity_id}__{description_slug}` with double underscore |
| **CI pipeline** | 2 gates: test+collect → coverage |
| **Repository location** | `integrations/rust/` in monorepo (NOT a workspace member) |

---

## Architecture Overview

### What `@specforge/rust` Is

`@specforge/rust` is an Adapter extension that provides test traceability:

```
┌─────────────────────────────────────────────────────────┐
│                    @specforge/rust                        │
│                                                          │
│  ┌──────────────────────────────────────────────────────┐ │
│  │ Adapter: specforge collect rust                      │ │
│  │  - Reads: cargo test output (JUnit XML / JSON)       │ │
│  │  - Reads: test mappings (proc macro / convention)    │ │
│  │  - Emits: specforge-report.json                      │ │
│  └──────────────────────────────────────────────────────┘ │
│                                                          │
│  ┌──────────────────────────────────────────────────────┐ │
│  │ specforge-test crate (crates.io)                     │ │
│  │  - #[specforge::test(behavior = "id")] proc macro    │ │
│  │  - Drop-based TestGuard for result collection        │ │
│  │  - atexit handler writes target/specforge/*.json     │ │
│  └──────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

This is NOT a 4th extension type. It fits within SpecForge's existing three extension mechanisms:

| Mechanism | Role in `@specforge/rust` |
|-----------|--------------------------|
| **Adapter** | `specforge collect rust` — transforms cargo test output to `specforge-report.json` |
| **Extension** | Not applicable — no new entity types |
| **Provider** | Not applicable — no new ref schemes |

### Extension Manifest

```toml
# specforge-plugin.toml
[package]
name = "@specforge/rust"
version = "0.1.0"
description = "Rust test traceability for SpecForge"

[adapter]
binary = "specforge-adapt-rust"  # or built into specforge-cli as `specforge collect rust`
runners = ["cargo-test", "cargo-nextest"]
report_format = "specforge-report-v1"
```

### Configuration

```jsonc
// specforge.json
{
  "name": "my-service",
  "version": "1.0",
  "extensions": ["@specforge/software", "@specforge/product", "@specforge/rust"],
  "test_dirs": ["tests/"],
  "coverage": {
    "threshold": 80,
    "reports": ["target/specforge/"]
  }
}
```

---

## The Rust Test Ecosystem

### The Fundamental Constraint

Every Rust test framework ultimately registers `#[test]` functions with **libtest** — the built-in test harness. There is ONE pipeline to intercept, not N. This is the good news.

The bad news: libtest has no reporter plugin API, no stable JSON output, and `#[test]` is a compiler lang item that cannot be hooked by user code.

### Framework Compatibility Matrix

| Framework | Appears to libtest as | Integration | Notes |
|-----------|----------------------|-------------|-------|
| `#[test]` (built-in) | Single test entry | Easy | The baseline |
| `#[tokio::test]` | Single `#[test]` | Easy | Most common async test macro |
| `#[rstest]` with `#[case]` | Multiple `#[test]` entries | Medium | Names: `fn_name::case_N` |
| `#[test_case]` | Multiple `#[test]` entries | Medium | Names from description strings |
| `proptest! {}` | Single `#[test]` per property | Easy | Runs many inputs internally |
| `quickcheck! {}` | Single `#[test]` per property | Easy | Same model as proptest |
| `criterion` | NOT a test — `harness = false` | Hard | `cargo bench` only. Separate plugin. |
| `datatest-stable` | Custom harness | Hard | File-driven tests |
| `#[traced_test]` | Decorates existing `#[test]` | Easy | Composable |

### Test Output Formats

| Source | Format | Stability | Quality |
|--------|--------|-----------|---------|
| `cargo test` default | Human-readable text | Stable | Poor (fragile parsing) |
| `cargo test -- -Z unstable-options --format json` | NDJSON | **Unstable** | Best (name, status, duration, stdout) |
| `cargo nextest run --profile ci` | JUnit XML | **Stable** | Good (name, classname, duration, failure) |
| `cargo nextest run --message-format libtest-json` | NDJSON | Stable | Good |

**Primary path: cargo-nextest JUnit XML.** Stable, widely adopted, sufficient metadata. nextest is rapidly becoming the standard Rust test runner in CI.

**Secondary path: libtest JSON** behind a `--unstable` flag. Richer metadata but depends on nightly or `-Z unstable-options`.

### What Cannot Be Done

| Impossible | Why |
|-----------|-----|
| Runtime test introspection from within binary | libtest internals are private |
| Adding metadata to libtest JSON output | Format owned by rustc, not extensible |
| Running doc tests through nextest | Architectural incompatibility (rustdoc, not libtest) |
| Stable custom test frameworks | `custom_test_frameworks` unstable since 2018, no timeline |
| Intercepting `#[test]` registration at compile time | `#[test]` is a lang item, not hookable |

---

## Three-Layer Traceability for Rust

### Layer 1: Intent — verify/scenario → Rust Test Expectations

A `verify` statement declares WHAT to test. The mapping to Rust:

| verify kind | Rust construct | Location | Framework |
|-------------|---------------|----------|-----------|
| `unit` | `#[test] fn name() { todo!() }` | `tests/spec/` | std |
| `integration` | `#[test] fn name() { todo!() }` | `tests/spec/` | std |
| `property` | `proptest! { #[test] fn name() { todo!() } }` | `tests/spec/` | proptest |
| `load` | `fn bench_name(c: &mut Criterion) { todo!() }` | `benches/spec/` | criterion |
| `e2e` | `#[test] #[ignore = "e2e"] fn name() { todo!() }` | `tests/spec/` | std |

Scenario blocks map to sequential test functions with given/when/then as structured comments:

```rust
/// @specforge-scenario trace_test_coverage "developer traces fully covered entity"
#[test]
fn developer_traces_fully_covered_entity() {
    // given "a behavior with verify statements and a tests field pointing to an existing test file"
    let fixture = create_spec_fixture_with_verify_and_tests();

    // given "a specforge-report.json with passing results for that behavior"
    let report = create_passing_report(&fixture);

    // when "developer runs specforge trace --test-results"
    let output = Command::new(env!("CARGO_BIN_EXE_specforge"))
        .args(["trace", "--test-results", report.path()])
        .output()
        .expect("specforge trace failed");

    // then "the traceability matrix shows the behavior as passing at all four levels"
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PASS"));
}
```

### Layer 2: Linkage — `tests` Field for Rust

The `tests` field uses **workspace-relative file paths** with an optional `::function_path` suffix:

```spec
behavior detect_dangling_references "Detect Dangling References" {
  verify unit "missing reference produces E001"
  verify unit "valid reference passes silently"
  verify unit "close match produces suggestion"
  tests [
    "crates/specforge-validator/src/passes.rs::tests::check_dangling_refs",
    "crates/surfaces/cli/tests/integration_test.rs::e001_dangling_reference",
  ]
}
```

**Why file paths beat module paths:**

| Criterion | File path | Module path |
|-----------|-----------|-------------|
| Readability | Anyone can find the file | Requires knowing module tree |
| Stability | Stable unless file moves | Breaks on module rename |
| Validation (E016) | `stat()` at compile time | Requires compiling the crate |
| Cross-crate | Works: `crates/foo/tests/bar.rs` | Ambiguous: which `crate::`? |
| Agent workflow | Agent writes file, copies path | Agent traces module hierarchy |

**Granularity guidance:**

| Granularity | When to use | Example |
|-------------|-------------|---------|
| File only | One behavior = one test file | `tests/create_user_test.rs` |
| File + module | Multiple behaviors in one file | `src/trace.rs::tests` |
| File + function | Surgical precision | `src/trace.rs::tests::trace_orphan_entity` |

### Layer 3: Proof — From `cargo test` to `specforge-report.json`

```
cargo test / nextest  →  specforge collect rust  →  specforge-report.json  →  specforge trace
```

The report format follows the `SpecforgeReport` schema from `spec/types/coverage.spec`:

```json
{
  "specforge": "1.0",
  "runner": "@specforge/rust",
  "timestamp": "2026-03-02T14:30:00Z",
  "results": [
    {
      "entityId": "detect_dangling_references",
      "file": "crates/surfaces/cli/tests/integration_test.rs",
      "tests": [
        { "name": "e001_dangling_reference", "status": "pass", "durationMs": 12 },
        { "name": "valid_reference_passes", "status": "pass", "durationMs": 3 }
      ]
    }
  ]
}
```

---

## Entity-to-Test Mapping

### The Three-Level Resolution Model

Entity mapping follows a strict precedence order, analogous to Terraform's explicit > convention > error:

```
1. tests field in .spec file     → authoritative, always wins
2. #[specforge::test(...)] attr  → explicit in-code annotation
3. mod <entity_id> convention    → implicit by naming
```

### Level 1: `tests` Field (Authoritative)

The `.spec` file's `tests` field is the canonical mapping. All other mechanisms are discovery assistance:

```spec
behavior detect_dangling_references {
  verify unit "missing reference produces E001"
  tests ["crates/surfaces/cli/tests/integration_test.rs::e001_dangling_reference"]
}
```

### Level 2: Proc Macro Attribute (Explicit)

The `specforge-test` crate provides attribute macros for explicit in-code annotation:

```rust
use specforge_test::prelude::*;

#[specforge::test(behavior = "detect_dangling_references")]
#[test]
fn missing_ref_produces_e001() { ... }

// With verify description for precise matching:
#[specforge::test(
    behavior = "detect_dangling_references",
    verify = "close match produces suggestion"
)]
#[test]
fn close_match_produces_did_you_mean() { ... }

// Multiple entities covered by one test:
#[specforge::test(behavior = "detect_dangling_references")]
#[specforge::test(invariant = "reference_resolution_completeness")]
#[test]
fn integration_e001() { ... }
```

### Level 3: Module Name Convention (Implicit Fallback)

If a `mod` block's name matches an entity ID, all `#[test]` functions within it are associated with that entity:

```rust
mod parse_scenario_blocks {
    #[test]
    fn all_three_step_kinds() { ... }

    #[test]
    fn multiple_steps_of_same_kind() { ... }
}
```

This handles the proptest decl-macro case where you cannot attach attributes:

```rust
mod traceability_chain_integrity {
    use proptest::prelude::*;
    proptest! {
        #[test]
        fn property_holds(n in 1..1000u32) { ... }
    }
}
```

### Verify Description Slugification

Verify descriptions become function names via deterministic slugification rules:

```
"missing reference produces E001"    →  missing_reference_produces_e001
"p99 < 200ms under 1000 concurrent" →  p99_lt_200ms_under_1000_concurrent
"email uniqueness under concurrent"  →  email_uniqueness_under_concurrent
```

Rules:
1. Lowercase everything
2. Replace spaces with `_`
3. Replace `<` with `lt`, `>` with `gt`, `<=` with `lte`, `>=` with `gte`
4. Strip remaining non-alphanumeric-non-underscore characters
5. Collapse consecutive underscores
6. Trim leading/trailing underscores

### Naming Convention: Double Underscore Separator

Test functions following the naming convention use `{entity_id}__{description_slug}` with `__` separating entity ID from test description. This allows the collector to unambiguously extract the entity ID from a function name:

```rust
#[test]
fn detect_dangling_references__missing_reference_produces_e001() { ... }
//  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^  entity ID
//                                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^  verify slug
```

### Multi-Entity Tests

A single test can cover multiple entities. Handled by:

- **Attributes:** Multiple `#[specforge::test(...)]` on one function
- **`tests` field:** Both entities list the same test file path

```spec
behavior detect_dangling_references {
  tests ["tests/integration_test.rs::e001_dangling_reference"]
}

invariant reference_resolution_completeness {
  tests ["tests/integration_test.rs::e001_dangling_reference"]
}
```

### Parameterized Test Mapping

**rstest:** The `#[specforge::test]` attribute applies to the parameterized function. All generated cases inherit the mapping:

```rust
#[specforge::test(behavior = "parse_scenario_blocks")]
#[rstest]
#[case("given-when-then", 3)]
#[case("given-only", 1)]
fn step_parsing(#[case] input: &str, #[case] expected: usize) { ... }
```

`cargo test` reports `step_parsing::case_1`, `step_parsing::case_2`. All map to `parse_scenario_blocks`.

**proptest (function style):** Attribute works normally:

```rust
#[specforge::test(invariant = "traceability_chain_integrity")]
#[test]
fn chain_integrity() {
    proptest!(|(n in 1..1000u32)| { ... });
}
```

**proptest (decl-macro style):** Use module convention fallback:

```rust
mod traceability_chain_integrity {
    proptest! {
        #[test]
        fn property_holds(n in 1..1000u32) { ... }
    }
}
```

---

## Crate Architecture

### Two Crates, Published to crates.io

```
specforge-test-macros  (proc-macro crate)
        │
        ▼
  specforge-test  (re-exports macros, provides runtime)
        │
        ▼
  user's test code  (dev-dependency on specforge-test)
```

Users add one dependency:

```toml
[dev-dependencies]
specforge-test = "0.1"
```

The macros crate is pulled in transitively, following the same pattern as `tokio`/`tokio-macros`, `serde`/`serde_derive`.

### `specforge-test` (Runtime Library)

| Dependency | Why |
|------------|-----|
| `serde` (with `derive`) | Serialize `SpecforgeReport` to JSON |
| `serde_json` | Write `specforge-report.json` |
| `specforge-test-macros` | Re-export proc macros |

Total: ~12-15 transitive crates. Comparable to adding `tracing`.

### `specforge-test-macros` (Proc Macro)

| Dependency | Why |
|------------|-----|
| `proc-macro2` | Token stream manipulation |
| `syn` (features: `parsing`, `derive`) | Parse attribute arguments. NOT `syn/full`. |
| `quote` | Generate token streams |

### Proc Macro Expansion

The `#[specforge::test(...)]` attribute does NOT replace `#[test]`. It stacks alongside it, adding a `Drop`-based guard:

**Input:**
```rust
#[specforge::test(behavior = "create_user")]
#[test]
fn valid_input_creates_user() {
    let user = repo.create(cmd);
    assert!(user.is_ok());
}
```

**Expansion:**
```rust
#[test]
fn valid_input_creates_user() {
    let __specforge_guard = specforge_test::__private::TestGuard::new(
        specforge_test::EntityKind::Behavior,
        "create_user",
        module_path!(),
        "valid_input_creates_user",
        file!(),
        line!(),
    );
    let user = repo.create(cmd);
    assert!(user.is_ok());
    // __specforge_guard drops here, recording pass/fail
}
```

### The `Drop`-Based Guard

```rust
struct TestGuard { /* entity_kind, entity_id, test_name, ... */ }

impl Drop for TestGuard {
    fn drop(&mut self) {
        let panicking = std::thread::panicking();
        REGISTRY.lock().unwrap().push(TestResult {
            entity_kind: self.kind,
            entity_id: self.entity_id.clone(),
            test_name: self.test_name.clone(),
            status: if panicking { Status::Fail } else { Status::Pass },
        });
    }
}
```

**Why `Drop` over `catch_unwind`:**
- Works with async tests (`#[tokio::test]`)
- Works with `#[rstest]` parameterized tests
- Simpler — no `AssertUnwindSafe` wrappers
- No `catch_unwind` interaction with await points

**`#[should_panic]` limitation:** The `Drop` guard records "fail" because `panicking()` is true during unwinding, even though the harness considers the test passing. This is a known incompatibility. Recommendation: document as unsupported; use `Result`-based error testing or explicit `catch_unwind` instead.

### Report Emission

Each test binary writes to `target/specforge/<binary-name>.json` via an `atexit`-style handler registered by the first `TestGuard` creation. Uses `std::sync::Once` + `libc::atexit` (POSIX, no external crate).

For workspace projects with multiple test binaries, each binary writes its own file. `specforge collect rust` or `specforge coverage` reads `target/specforge/*.json` and merges them.

### Independence from Compiler

The `specforge-test` crate has **zero dependency** on the SpecForge compiler crates (`specforge-common`, `specforge-parser`, `specforge-graph`, etc.). The only coupling is the `specforge-report.json` format — a JSON schema, not a Rust type.

### Repository Location

```
github.com/anthropics/specforge/
  crates/surfaces/cli/         # compiler workspace member
  crates/kernel/parser/      # compiler workspace member
  ...
  integrations/rust/            # NOT a workspace member
    specforge-test/Cargo.toml
    specforge-test-macros/Cargo.toml
```

Separate `Cargo.lock`, separate CI, separate release cycle. Same pattern as `rustfmt` in the `rust-lang/rust` repo.

---

## Report Collection: `specforge collect rust`

### The Go Pattern Applied to Rust

Rust follows the same pattern as Go from RES-16: post-process test output externally because the test harness has no plugin API.

```bash
# Option A: pipe cargo test output
cargo test 2>&1 | specforge collect rust

# Option B: nextest JUnit XML (recommended for CI)
cargo nextest run --profile ci
specforge collect rust --from-junit target/nextest/ci/junit.xml

# Option C: nextest JSON
cargo nextest run --message-format libtest-json | specforge collect rust --format json

# Option D: libtest JSON (unstable)
cargo test -- -Z unstable-options --format json 2>&1 | specforge collect rust --format json
```

### Collection Process

1. Read entity ID mappings from:
   - `target/specforge/*.json` (written by proc macro runtime at test exit)
   - Source file scanning for `#[specforge::test(...)]` attributes (fallback)
   - Module name matching against entity graph (convention fallback)
2. Read test results from stdin pipe, JUnit XML, or JSON
3. Match test names from results against mappings
4. Emit `specforge-report.json`

### Handling Multiple Test Binaries

In a Cargo workspace, `cargo test` runs multiple binaries. Each writes its own mapping file to `target/specforge/<binary-name>.json`. The collector merges all files. nextest handles multi-binary output natively — its JUnit XML contains `<testsuite name="binary_name">` elements.

### Entity ID Validation

**Lenient mode (default, local development):** If no manifest exists, the runtime silently collects mappings without validation. Entity IDs are trusted.

**Strict mode (CI):** `specforge check --emit-manifest` writes `target/specforge-manifest.json` containing all entity IDs. The `specforge_test` runtime reads this at startup and panics if `#[specforge::test(behavior = "typo_id")]` references an unknown entity. Same behavior as `@specforge/vitest` (fails on unknown IDs).

```bash
# CI pipeline
specforge check --emit-manifest    # writes manifest
cargo test                         # proc macro validates IDs at runtime
specforge collect rust             # emits report
specforge coverage --min 90        # gates
```

---

## CI Integration Pipeline

### Three Stages

```
1. specforge check --strict          (< 1s)   spec validity
2. cargo test + specforge collect    (mins)    build, test, report
3. specforge coverage --min=90       (< 1s)   spec coverage gate
```

Fast checks first. Expensive checks last. Fail early.

### Stage 1: Spec Validation

```bash
specforge check --strict
```

Exit 0: no errors, no warnings. Exit 1: any error (E001-E016) or warning (W001-W018, treated as errors in strict mode). If this fails, stop — everything downstream depends on a valid spec graph.

### Stage 2: Build, Test, Collect

```bash
cargo nextest run --profile ci
specforge collect rust --from-junit target/nextest/ci/junit.xml
```

Standard Rust test execution. The `specforge collect` step transforms test results into `specforge-report.json`.

### Stage 3: Coverage Gate

```bash
specforge coverage --min=90
```

This is **spec coverage**, not code coverage. It measures: "Of all testable entities in my spec, what percentage have passing tests?"

From the `CoverageSummary` type:

```
declared (18/20 = 90%)  — "We said we'd test these"
   │
   ▼
linked (16/20 = 80%)    — "We connected them to real test files"
   │
   ▼
executed (15/20 = 75%)  — "The tests actually ran"
   │
   ▼
passing (14/20 = 70%)   — "The tests passed"
```

`--min=90` gates on the **passing** level. Each level is a strict subset of the one above.

### Traceability Report (Optional)

```bash
specforge trace --test-results
```

Informational output showing the full traceability matrix:

```
Entity              | Intent           | Test File                        | Status
--------------------|------------------|----------------------------------|--------
create_user         | 3 verify (u/u/i) | tests/behaviors/create_user.rs   | PASS (3/3)
validate_email      | 1 verify (u)     | tests/behaviors/validate_email.rs| PASS (1/1)
unique_ids          | 2 verify (p/u)   | tests/invariants/unique.rs       | PASS (2/2)
user_created        | 2 verify (i/i)   | --                               | NO TEST  [W018]
delete_user         | --               | --                               | NO INTENT
latency_p99         | 1 verify (load)  | tests/perf/latency.rs            | FAIL (0/1)
```

### Spec Coverage vs. Code Coverage

**These are different dimensions.** Both should be in the pipeline.

| Dimension | `specforge coverage --min=90` | `cargo tarpaulin --min 80` |
|-----------|-------------------------------|---------------------------|
| Measures | % of spec entities with passing tests | % of source code lines executed |
| Numerator | Testable entities that pass | Lines of `.rs` code hit |
| Denominator | Total testable entities in `.spec` | Total instrumentable lines |
| Answers | "Did we test what the spec says?" | "Did our tests touch all the code?" |
| Blind spots | Doesn't know if tests are thorough | Doesn't know if code matches spec |

A project can have 100% code coverage and 20% spec coverage (lots of ad-hoc tests, most specs untested). Both matter. Both should be measured.

### Complete GitHub Actions Workflow

```yaml
name: CI
on: [push, pull_request]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  spec-validation:
    name: Spec Validation
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install SpecForge
        run: cargo install specforge-cli
      - name: Validate spec files
        run: specforge check --strict

  build-and-test:
    name: Build & Test
    needs: spec-validation
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: "rustfmt, clippy" }
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --all-targets --all-features -- -D warnings
      - name: Run tests
        run: cargo nextest run --profile ci
      - name: Generate SpecForge report
        if: always()
        run: specforge collect rust --from-junit target/nextest/ci/junit.xml
      - uses: actions/upload-artifact@v4
        if: always()
        with: { name: specforge-report, path: specforge-report.json }

  coverage-gate:
    name: Coverage Gate
    needs: build-and-test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/download-artifact@v4
        with: { name: specforge-report }
      - run: cargo install specforge-cli
      - name: Spec coverage gate
        run: specforge coverage --min=90
      - name: Traceability report
        if: always()
        run: specforge trace --test-results

  code-coverage:
    name: Code Coverage
    needs: build-and-test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo install cargo-tarpaulin
      - run: cargo tarpaulin --min 80 --out xml
```

### Exit Code Summary

| Command | Exit 0 | Exit 1 |
|---------|--------|--------|
| `specforge check` | No errors | Any error (E001-E016) |
| `specforge check --strict` | No errors, no warnings | Any error or warning |
| `specforge coverage --min=90` | Coverage >= 90% | Coverage < 90% |
| `specforge trace --test-results` | Always 0 (informational) | — |
| `specforge collect rust` | Report written | Parse error or no results |

---

## Developer Journey

### Day 0: Setup

```bash
cd my-service
specforge init                    # creates specforge.json with @specforge/rust extension
cargo add --dev specforge-test    # one dev-dependency
```

### Day 1: First Spec and Test

```bash
# Write spec
vim spec/behaviors/user.spec

# Validate
specforge check

# Write the first test
vim tests/spec/behaviors/user_test.rs

# Run tests and collect
cargo test 2>&1 | specforge collect rust

# See traceability
specforge trace create_user --test-results
```

### Day 7: Full Loop

```bash
specforge check                              # validate
cargo test 2>&1 | specforge collect rust     # test + report
specforge coverage --test-results            # coverage summary
specforge trace --test-results               # full matrix
```

### Day 30: CI

```bash
specforge check --strict                     # gate 1: spec valid
cargo nextest run --profile ci               # gate 2: tests pass
specforge collect rust --from-junit ...      # collect results
specforge coverage --min 80                  # gate 4: coverage threshold
```

### AI Agent Workflow

The agent needs exactly three things:

1. `specforge show <entity> --depth=2 --format=json` — the spec entity plus direct dependencies (types, ports, invariants)
2. The entity graph with type and port definitions — so it knows the signatures
3. The naming convention — `{entity_id}__{description_slug}`

```
Agent reads spec  →  produces implementation + tests  →  adds tests field  →  specforge trace validates
```

The spec IS the prompt. The verify statements ARE the acceptance criteria.

---

## Error Messages

### Test not linked to any spec entity

```
warning: unknown entity ID 'create_userr' in tests/spec/behaviors/user_test.rs:8
  --> tests/spec/behaviors/user_test.rs:8:25
   |
 8 | #[specforge::test(behavior = "create_userr")]
   |                                ^^^^^^^^^^^^ not found in .spec files
   |
   = help: Did you mean 'create_user'?
```

### Spec entity has verify but no tests

```
warning[W018]: 'delete_user' has test declarations but no linked test files
   --> behaviors/user.spec:28:1
    |
 28 | behavior delete_user "Delete User" {
    | ^^^^^^^^^^^^^^^^^^^^ 2 verify statements, 0 test results
    |
    = help: Run tests covering this behavior and collect results
```

---

## Competitive Landscape

### What Nobody Else Does

No existing tool provides all six of:

1. A specification language with compiler validation
2. Test intent declaration in the spec (verify/scenario)
3. Linkage from spec entities to actual test files
4. Consumption of test results for pass/fail proof
5. A four-level traceability matrix (declared → linked → executed → passing)
6. Designed for AI agent consumption

The closest prior art:
- **TRLC (BMW)**: Gets (1) — typed requirements-as-code with validation. No test declaration.
- **Serenity BDD**: Gets (4)+(5) — living documentation mapping test results to requirements. But it's a test framework add-on, not a specification compiler.
- **Allure Framework**: Gets (4) — behavioral hierarchy reports. Multi-language. But no spec language.
- **DOORS/Jama/Polarion**: Gets (1)-(5) in the enterprise space. GUI-first, expensive, developer-hostile.

### What to Steal

| From | What | Priority |
|------|------|----------|
| **Allure** | Behavioral hierarchy (Epic/Feature/Story → capability/feature/behavior) in reports | High |
| **Allure** | Report format compatibility — emit Allure-format alongside specforge-report.json | Medium |
| **Serenity BDD** | Living documentation HTML reports (`specforge trace --html`) | High |
| **Polarion** | "Suspect links" — flag stale tests when specs change | High |
| **TRLC** | Formal verification for invariants (CVC5 solver integration) | Future |
| **cucumber-rs** | Collect cucumber-rs test results via `specforge collect rust --from-cucumber` | Medium |

### The Moat

The scenario-as-agent-prompt concept is SpecForge's biggest differentiator. Current AI test generation is blind — it guesses what to test from code analysis. SpecForge gives the agent structured instructions (verify statements, scenario blocks, contract text) and validates the full chain. That's the right direction: intent-first, not code-first.

---

## Phased Delivery

### Phase 1: Convention-Based (Zero Dependencies)

Ship `specforge collect rust` built into the CLI. Convention-based name matching (`entity_id__description_slug`) and `tests` field file paths. Works with `cargo test` human-readable output. No Rust crate dependency.

**Developer adds:** Nothing to `Cargo.toml`. Just naming conventions.

### Phase 2: Proc Macro Crate

Ship `specforge-test` and `specforge-test-macros` on crates.io. `#[specforge::test(...)]` attributes for explicit entity linkage. `Drop`-based guard writes mapping files.

**Developer adds:** `specforge-test = "0.1"` to dev-dependencies.

### Phase 3: Advanced Features

- nextest JUnit XML integration
- `specforge trace --html` living documentation
- Suspect link detection (flag stale tests when specs change)
- Criterion integration for `verify load`
- Entity ID validation at test runtime via manifest

---

## Open Questions

| Question | Leaning | Status |
|----------|---------|--------|
| Entity ID validation: compile-time vs runtime? | Runtime (proc macros cannot do I/O) or post-hoc in `specforge coverage` | Lean post-hoc |
| `cargo test` JSON stabilization? | Support as secondary behind `--unstable` flag | Waiting on rust-lang/rust#49359 |
| `criterion` vs `divan` for `verify load`? | `criterion` (stable, widely used) | Survey needed |
| Allure-compatible report output? | Yes, but not in v1 | Deferred |
| cucumber-rs integration for scenarios? | Via `specforge collect rust --from-cucumber`, not code generation | Deferred |
| `#[should_panic]` support? | No. Incompatible with Drop guard. Use Result-based testing. | Confirmed |

---

## Summary: The Complete Command Vocabulary

```bash
# Setup
specforge init                              # create specforge.json
cargo add --dev specforge-test              # add proc macro dependency (optional)

# Development
specforge check                             # validate .spec files
cargo test                                  # run tests (standard Rust)
cargo test 2>&1 | specforge collect rust    # collect results into report

# Analysis
specforge trace create_user                 # trace one entity
specforge trace --test-results              # full traceability matrix
specforge coverage                          # coverage summary

# CI
specforge check --strict                    # warnings = errors
cargo nextest run --profile ci              # run tests
specforge collect rust --from-junit ...     # collect from nextest
specforge coverage --min 90                 # gate

# Utility
specforge show create_user --format=json    # agent context loading
specforge stats                             # project health overview
```

Ten commands. A developer is productive with three (`check`, `collect rust`, `coverage`). An AI agent needs two (`show --format=json`, `trace`).
