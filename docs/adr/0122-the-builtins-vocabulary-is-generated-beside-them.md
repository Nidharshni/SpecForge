# The builtins' vocabulary is generated beside them and handed to the registry build

**Status:** accepted (2026-10-16, architecture round 12, plan 11). Amends [ADR 0070](0070-every-committed-blob-is-listed-once-and-built-one-way.md)
D2 (`revendor` also writes the builtin vocabulary) and [ADR 0031](0031-the-registry-build-checks-the-entities.md) (the build carries
a candidate vocabulary its checks read for suggestions). Keeps [ADR 0096](0096-an-enhancements-fields-are-credited-to-its-declarer.md)'s
rejection of the index as an owner.

## Context

The registry crate shipped two hand-kept JSON files, `data/keyword-index.json` (22 kind keywords) and
`data/field-index.json` (14 enhancement fields), each mapping a name to the `@specforge/*` builtin that declares it.
Three checks read them through two process-wide statics: E024 and W151 (kinds) and W020 (fields).

- The history: created 2026-09-30 and 2026-10-03, never edited since, while 88 commits changed the builtins' sources.
  The generator the spec decision describes was deleted in `19ae5b56` because the index was kept by hand.
- The spec contradicted itself and the code. The decision says the index is "generated at release time"; the behavior
  spec says "maintained by hand". The decision says E024 only; W151 and W020 read it too. The decision says it is "the
  ONLY bundled domain-aware data file"; the field index is a second.
- The keyword test was one-way: a keyword a builtin stopped declaring stayed in the file and the test passed. The field
  test was an equality. The test also copied the registry's keyword rule without its `name` fallback.
- ARCHITECTURE.md says the core "carries no domain vocabulary". The registry held 36 domain names.

## Decisions

**D1. One module owns the rule.** `specforge_registry::CandidateVocabulary` holds each kind keyword
(`populate::keyword`: the keyword, else the name) and each field an enhancement adds to another extension's kind (credited
per ADR 0096 D2), each with the extension that declares it. It owns its written form and its lookups.

**D2. Suggestions only.** It never decides a diagnostic, an owner or a verdict. `populate::apply_enhancement`'s
silent skip against I004 (ADR 0096 D4) does not read it.

**D3. The host hands it to the build.** `RegistryBuild::with_candidates`; the checks read `RegistryBuild::candidates()`.
The session hands the builtins' vocabulary in `Environment::assemble`. A build handed none suggests `specforge search`.

**D4. The builtins' vocabulary is generated.** `revendor` writes `crates/runtime/component/data/builtin-vocabulary.json`
from the pinned declarations, right after it writes them. `specforge-component` embeds it
(`builtins::BUILTIN_VOCABULARY`). `the_builtin_vocabulary_is_what_the_builtins_declare`, in the session crate, compares
the whole file with what the pinned declarations say.

**D5. A duplicate is refused at generation.** Two candidates declaring one keyword, or one enhancement field on one
kind, make `revendor` stop and name both. (Two builtins declaring one keyword is already E026 when both load.)

## Rejected

- A: generate in place (`revendor` writes the registry's two files). Automates the edit but keeps domain vocabulary in the
  core, a pipeline test reading runtime fixtures by path, and a static where every other check input is a value.
- B-live: the session loads every embedded blob at `Environment::assemble` and asks for its declaration. Nine wasm
  components compiled per environment for suggestion text.
- C: nothing beyond the I004 plan. The spec stays false about the file and the keyword test keeps its gap.
- A `build_registries` parameter (79 call sites in 36 files for suggestion text), and a `RuleInput` field (it would
  not reach `graph_inputs`).
- The drift test in the component crate (it would add a runtime to pipeline edge to `spec/product/modules.spec`).
- "First in load order wins" for a duplicate.

## Consequences

The registry ships no `data/` directory and no static. Adding a kind to a builtin needs only the re-vendor it already
needs. Third-party extensions are unchanged: their keywords suggest `specforge search`. The reference I004 can read
`candidates().kind_owner(..)` inside `graph_inputs` (plan 06), as plain data for the linker.

## What would reopen this

A suggestion that needs more than a name's declarer (for example "which candidate makes a kind testable", hard-coded
in W158), or a host that wants to hand a non-builtin vocabulary.
