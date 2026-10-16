# Navigation reads the text the project was compiled from

**Status:** accepted (2026-10-16, architecture round 12, plan 05). Reopens ADR 0016 D1's parenthesis and amends its D3
(`Entity` precision), ADR 0092's outline row and ADR 0104 D4's outline entry.

ADR 0016 D1 gave the ops `Navigator` a `text_of` closure, "a file read once per navigator: the LSP reads its open
buffer first, MCP the file under the spec root". Two of its three adapters have since moved:

- The LSP's adapter is `state.compiled_text(file)`, which is `view.project().source_text(file)`: the text the project
  was compiled from (`a345731f`, `4758d962`). It is the target, written in the surface.
- MCP's adapter reads the disk now (`tools/mod.rs`), so a block span from the compile can sit next to a name span read
  from another text.
- The tests pass the disk, `|_| None`, a shifted disk or a filtered disk.

MCP's outline asks the disk "is this a source?" (`spec_root.join(file).exists()`), and rename plans over the compile's
graph and the disk's text, then re-reads the disk at `apply` with no check that the text is still the one it planned on.
Reproduced on a debug build of `e0a1a015`:

- R1: `specforge.outline` of `specforge.json`, `.`, a directory and an excluded `.spec` file answers `{"entries":[]}`,
  as for an empty source.
- R2: after a same-length, same-mtime edit (invisible to the stamps, ADR 0100), `find_definition` answers a `name_span`
  1:12-1:25 from the disk beside the compile's `source_span`, where the LSP answers 1:11-1:24.
- R3: a rename over such an edit reports success, writes two files and leaves a reference behind; its own reply then
  carries the E003.
- R4: `apply_edits` over a text that moved panics (`range end index 27 out of range for slice of length 25`).

The precedent is ADR 0055's rejection: "Keeping `exists` as a closure over the keys: a seam whose only honest adapter is
a set lookup, and the door the disk came in by". `text_of` is that seam for texts. Since ADR 0108 D1 every view is over a
compiled project, and D3 shares "the environment, the texts, the report and the memo".

## Decision

- **D1. The navigator reads its view's compiled text.** `Navigator::new(view)` takes no closure; a file's text is
  `view.project().source_text(file)`, indexed once per navigator. No surface hands it a text. `Precision::Entity` now
  means "the compile holds no text of the file, or that text does not spell the id there", which only a graph built by
  hand produces.
- **D2. Outline answers only the compile's sources.** `navigate::outline` returns `None` for a file that is none of
  `CompiledProject::source_keys` (under `match_file`'s rule). MCP refuses it as `file_not_found`, naming the file; a
  source with no entity, or one that could not be read, has an empty outline. The rule lives in ops so the LSP and MCP
  cannot diverge again. Outline's containment check stays: it is now the argument's validation (`outside_root`), not the
  open's guard.
- **D3. A rename plan keeps its planned texts.** `RenamePlan` holds each affected file's text as the plan read it.
  `apply` resolves, reads and compares every file before writing any; a file that is not the planned text refuses the
  whole rename as `Conflict`, code `file_changed` (`conflict` over MCP), and nothing is written. This is MCP's
  counterpart of ADR 0023 D3's `ContentModified`.
- **D4. A test hook gives navigation another text.** `CompiledProject::with_source_texts` (behind the `testing`
  feature) copies a compiled project with the texts a test gives it; `Assembled::source` keeps its text.

## Rejected

- Re-planning inside the call: it needs a session door that re-reads a file whose stamp held (ADR 0100's reopen
  condition) and a handler that refreshes its own target (against ADR 0022), and the reply's edits would no longer be the
  plan a preview showed.
- A `Texts` trait with a compiled adapter and a test adapter: ADR 0055 rejected this shape for `Sources`.
- The MCP disk check kept beside the source keys.
- `invalid_input` for a non-source: it would split one question ("not one of my sources") into two codes by the disk.

## Consequences

- MCP's spans and the LSP's are one text.
- Outline of `specforge.json`, a directory or an excluded file is `file_not_found`.
- A rename over a stamp-invisible edit refuses until the file is saved again.
- 16 signatures lose a parameter.

## What would reopen it

A navigation question that must see text the compile did not read, such as a buffer the session does not hold. It would
go through the session as a held buffer, not through a closure.
