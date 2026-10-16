# Rules are a typed set that runs itself

**Status:** accepted (2026-10-06). Supersedes ADR 0013 D13. Amended by [ADR 0107](0107-the-checks-read-through-the-disk-they-are-given.md) (`files(&input)` is gone; `RuleInput` carries the disk). D1's order gains the host's E016 rules after E006 ([ADR 0118](0118-the-paths-a-field-names-are-read-once.md)).

An extension's validation rule was parsed into `ValidationRulePattern`: ten fields, nine `Option`s
counting its constraint. The parser checked, per check kind, what the rule needed, then built the
same flat struct for every kind, and `execute_pattern` re-checked each `Option`, answering "no
violation" for every combination the parser had already rejected. `cycle_detection` answered
nothing in the engine and ran in `specforge-project`, which rebuilt an edge-label map on every
check and fell back to the raw label. A cycle rule without an edge type registered and never fired,
with no W112; an edge rule naming an undeclared edge type counted every edge, against the spec's
"unloaded targets are inert"; a custom function that failed on real entities was skipped silently.
Custom dispatch was a registry trait, a project adapter built per rule, a probe and a call in
`Environment::load`. (ADR 0019 already gave every check one entity record, fixed `file_exists`'s
base and settled the untargeted obligation rule.) Five fixes in a month (376d47af, 1178c9d4,
029ec940, 5eb023cd, 5c7d7a58) each patched one of these.

## D1. One module owns rules: `specforge_registry::rules`

`Rules::build(declarations, registries) -> (Rules, diagnostics)` turns each descriptor into a
`Rule` (code, severity, template, target, origin) around a private `Check` enum with one variant per
check kind, carrying exactly what that check reads, resolved: the compiled `matches` regex, the peer
kind of an edge-scoped rule, the fields an edge type is written as *(ADR 0060: an edge rule's edge type; the
snapshot types every edge)*. The host's E006 rules and W023
are built there too. `rules.check(&input, &verdicts)` runs every check, cycles included, in a fixed
order (declared rules by code, then E006 by kind and field; entities by id). Queries replace field
reads: `Rule::applies_to(kind)`, `verify_rule_for(kind)`, `obligates(kind)`, `files(&input)`,
`probe(&verdicts)`. `applies_to` and `verify_rule_for` are ADR 0019's `applies_to` and
`obliging_rule`, relocated with the same meaning; `Standing::of` stays the one obligation rule. Nothing
outside the module reads a rule's parts. `validation_engine`, `detect_cycles`,
`run_extension_validation`, `probe_custom_rules`, `register_validation_rules` and
`generate_required_field_rules` are gone.

## D2. Its input is the entity snapshot's records

`Rules::check(&RuleInput<'_>, &dyn CustomVerdicts)` reads ADR 0019's plain, graph-free
`specforge_registry::entity::RuleInput { entities, edges, spec_root }`: field text and presence,
edge counts by peer kind *(by edge type, ADR 0060)*, obligations and exemption from each `EntityRecord`, cycles from the
`EdgeRecord`s. There is no trait; tests build records directly. Rules never stringify a value.

## D3. Custom verdicts are one port; this supersedes ADR 0013 D13

`CustomVerdicts::verdict(CustomCall { extension, function, subject })` with
`Subject::Entity(&EntityRecord)` or `Subject::Probe`, answering `Verdict` or `VerdictError::{Unavailable, Failed}`. Adapters: the
project's `WasmVerdicts` (the snapshot's `validator_context`/`probe_context` through
`ExtensionCalls::validate`, one instance for every rule), closures
in tests, `NoVerdicts` without a runtime. The probe runs through the same port. A failure at check
time is W148, once per rule per check; `Unavailable` is never reported.

## D4. Shape: W112 for what cannot work, W147 for what is ignored

A rule missing what its check requires (including a cycle rule's `edge_type` and an allowlist's
values) is W112 and not registered. A property its check does not read is W147, and the rule is
registered without it; `field` is never W147, because every check's message reads it. A
`conditional_field_required` constraint of another kind is read as `when_field_equals`.

## D5. Semantics settled

A cycle rule without a target kind checks every entity. An edge type resolves through the edge
registry only: undeclared, the rule is inert, and W021 tells the author when neither the extension
nor its loaded peers declare it; the same holds for a rule's target kind. A rule on the kind of an
extension its author works without names it in the descriptor's optional `target_extension`
(protocol 1.1.0; `@specforge/product`'s W078 on governance's `constraint` does): while that extension
is not loaded the rule alone is inert and silent, loaded without the kind or edge type it is W021.
A peer dependency is not the tool for that: peers order extension loading and pin versions, and a
mutual optional pair is a needless cycle (the
registry build's load order drops an optional edge that would close one, and E027 is only for cycles
among required peers, ADR 0041). A cycle follows
every field that writes the edge type *(ADR 0060: it follows the edges of the edge type)*. A rule
is inert when no loaded field can write its edge type's edges at its target kind (ADR 0060). A rule that reads `verify` statements on a declared kind that
accepts none is W112. The files `file_exists` rules read (against the spec root, ADR 0019; each item
of a list field) are a session's check inputs. W148 carries every failed entity as data.

## D6. The wire does not change

`ValidationRuleDescriptor` stays flat and string-typed (ADR 0012 D1; an unknown check name costs one
rule; a tagged enum would be a major protocol change, while ADR 0019's `1.1.0` is a minor one). The SDK's `RuleBuilder` is unchanged; authors meet W112/W147
in `specforge extension validate` and `publish`, which run the registry build. Rejected: a tagged
wire enum (breaks stored declarations and older guests), typed SDK builders (rewrites every rule for
a guarantee validate already gives).

## Consequences

- Dead cycle rules surface as W112; undeclared edge types are inert, not over-firing; failing custom
  functions are visible (W148); a `file_exists` file appearing clears its warning in a session.
- Tests are descriptor in, diagnostics out: `build_registries(vec![declaration]).rules.check(&RuleInput { … }, …)`.
- The cycle walker lives in `specforge_common::cycles`; `specforge-graph` re-exports it.
- New codes W147, W148; W112's and W021's explanations widen (W021 covers a rule's target kind).
- The decisions land in steps (plan 02): the module and D1–D3 first, then each fix of D4–D5 in its
  own change.

## What would reopen it

- A non-Rust SDK or a second protocol major: type the descriptor per check on the wire then.
- A check kind that needs more than one entity's facts and the labelled edges (e.g. reachability):
  `RuleInput` grows a field (ADR 0019's record).

## Amendment (round 9, plan 01)

D5's example (`@specforge/product`'s W078 on governance's `constraint`) is gone: no builtin rule declares a
`target_extension` (ADR 0071). The mechanism is unchanged.
