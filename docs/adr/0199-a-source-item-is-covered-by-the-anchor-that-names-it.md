# A source item is covered by the anchor that names it

**Status:** accepted (2026-10-21, architecture round 21, plan 08). Amends [ADR 0140](0140-source-anchors-are-a-read-view.md) D1 (the read view answers a third question, which source items are covered; inference gaps read it, the manifest stays private). Cites [ADR 0113](0113-inference-sources-are-walked-by-the-shared-discovery-policy.md) D5 (an anchor's file is keyed by the source path rule) and [ADR 0112](0112-every-project-file-an-operation-opens-goes-through-the-requests-containment.md) D3 (an unusable manifest is E071).

## Context

Whether a public source item is covered was answered three ways. Inference gaps counted an item covered when some entity's id equalled its name, in any file. Navigation (`find_implementation`, `find_spec_for_source`, the infer prompt's file scope) read the anchors manifest. The spec said gaps went through the inference manifest's `source_index`, which no code read for this, and which holds a file-level list of entity ids that cannot say which item an entity covers. So an item the anchors manifest anchored to an entity was a gap while navigation called it implemented, an anchored entity covered every same-named item in every other file, and a broken `specforge-anchors.json` gave plausible gap numbers while navigation refused it.

## Decision

- **D1.** An item is covered when an anchor names its file and its name for an entity the graph declares.
- **D2.** An entity anchored anywhere covers only the items its anchors name; an entity with no valid anchor covers the items whose name is its id (without a manifest, today's rule exactly).
- **D3.** Gaps read the anchors manifest before the scan; an unusable one is E071, as for navigation.
- **D4.** An anchor to an id the graph lacks, or to a file outside the root, covers nothing; navigation still lists it (ADR 0140 D3).
- **D5.** An anchor's `item_kind`, `line`, `scanner` and `confidence` are not matched: kinds are free text written by external tooling, and a line drifts with every edit above the item.

## Rejected

- `entities_produced` as the record: it is per file, with no symbol, so it can only narrow a name match.
- Anchors only: nothing in the workspace writes the manifest, so every item of every project would be a gap.
- Matching on line or kind: stale or vocabulary-dependent.
- Ignoring an unusable manifest: it hides a broken file behind plausible numbers.

## What would reopen it

A writer of the anchors manifest (it would own a staleness rule), or a scanner that reports an item's path (`mod::Type`), which would make (file, name) unambiguous.

## Consequences

- `specforge.infer_gaps` and `specforge infer-status --gaps-detail` refuse an unusable `specforge-anchors.json` with E071; projects with anchors count anchored items covered and no longer count other files' same-named items. Projects without the file are unchanged.
- `CoveredItems` is `pub(crate)` in `specforge_ops::read::navigate`; `Gaps::of` takes a predicate.
- No new diagnostic code; no blob input changes.
