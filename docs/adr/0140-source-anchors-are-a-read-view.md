# Source anchors are a read view

**Status:** accepted (2026-10-17, architecture round 14, plan 08). Amends ADR 0016 D1 (navigation's interface),
ADR 0015 M14 and P1 (what reads the anchors manifest), extends ADR 0052 D1's list of documents.

Ops offered the anchors manifest as two steps, `source_anchors` (read) and `anchors_of_file` /
`anchors_of_entity` (filter), and MCP composed them three times: `specforge.find_spec_for_source`,
`specforge.find_implementation` and the infer prompt's file scope. One composition decided each entity's kind
from the graph in MCP (`Anchored::of`); the prompt borrowed the tool's wire types; `MatchMode` repeated
`FileMatch` under other names. Two answers were left to that assembly: an id nothing knows was `count: 0` while
find_definition and find_references refused it with E003, and with no project served the file scope answered
"nothing here" instead of the no-project refusal (spec/invariants/mcp.spec).

## Decision

- **D1. One read view, two questions.** `specforge_ops::read::navigate::anchors_of_file(view, file_path)` and
  `anchors_of_entity(view, entity_id)` return the documents `FileAnchors` and `EntityAnchors`; MCP's replies are
  them (`pub use … as Reply`) and the infer prompt lists `FileAnchors::entities`. The manifest, its rows and the
  filters are private to the module.
- **D2. The kind and the match mode are the read view's.** A row names its entity's kind when the graph declares
  the id. `FileMatch` (`exact`, `directory`, `suffix_path`, `none`) is the one spelling of the mode.
- **D3. Not found means nothing knows the id.** `anchors_of_entity` refuses with E003 (subject `entity_id`,
  did-you-mean from the graph) an id neither the graph declares nor the manifest anchors; the anchors of an id the
  graph no longer declares are still listed.
- **D4. The read view needs a project.** A view with no root is `no_project`, never an empty manifest; a missing
  manifest is no anchors.

## Consequences

- `specforge.find_implementation` refuses an unknown id with no anchors (E003, entity_not_found on entity_id).
- The infer prompt's `file:` scope with no project served is the no-project refusal.
- Every other reply is byte-identical; the reply types, output schemas and prompt goldens did not move.
- `specforge.find_spec_for_source` has a behaviour of its own (`provide_mcp_find_spec_for_source_tool`).

## Rejected

- One row shape for both answers: changes two spec types and every snapshot for symmetry alone.
- Refusing every id the graph lacks: hides the anchors of a renamed or deleted entity.
- A version rule for the anchors manifest: nothing writes another version; E071 keeps it to specforge-infer.json.
- A CLI command or an LSP `textDocument/implementation`: no spec behaviour asks for one (ADR 0101 D4).

## What would reopen it

A writer of the anchors manifest in this workspace (it would own the shape and a version rule), or an editor need
for implementations (an LSP adapter over `anchors_of_entity`).
