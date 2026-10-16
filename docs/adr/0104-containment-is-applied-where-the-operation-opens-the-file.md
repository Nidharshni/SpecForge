# Containment is applied where the operation opens the file

**Status:** accepted (2026-10-16, architecture round 11, plan 01). Amends ADR 0092 D1 and D8, the closing note of ADR 0098
and the placements of ADR 0086. D2 amended by ADR 0112 and by ADR 0113 (the inference mark reads through the
inference sources, held within the project root).

MCP checked a path a client named (`specforge_mcp::containment`), then handed the raw name to an operation that read it
again its own way: it split it on commas, normalised `..` by name, followed a crate directory's `target/` symlink, or
wrote `.gitignore` and `graph.json` through whatever was there. The two readings disagreed. Seven calls escaped the
served project, each reproduced:

- R1: `init` with `"@specforge/software,../../x.wasm"` installed a `.wasm` from outside the bound (MCP read the entry as
  one path; ops split it).
- R2: the same with a crate directory after a comma: MCP's parse failed and `if let Ok` then checked nothing.
- R3: `add_extension` of a crate directory whose `target/` symlink leaves the project installed the component behind it.
- R4: two "outside" rules disagreed on a missing file behind an escaping symlink (`file_not_found` against
  `invalid_input`); no escape by itself.
- R5: `init` appended to a `.gitignore` that is a symlink leaving the directory.
- R6: `render` overwrote an `out/graph.json` symlink that leaves the project.
- R7: `init` with `a/../../../x.wasm`, `a` a symlink inside the project: MCP followed the link, ops dropped `..` by name.

The rule had been written twice in two days (`resolved`, then `resolves_outside`), and the fourth patch added the check
in the surface while the read stayed in ops.

## Decision

- **D1. One value in the ops base.** `specforge_ops::Containment` says what a request may reach: every path (the CLI,
  ADR 0092 D6 kept) or the paths that land inside one directory (MCP). It is a base module, beside `Writes`, using only
  `error`. It is named `Containment` (ADR 0092's and CONTEXT.md's term); `Reach` is taken five times and `Bound` means a
  solver assumption.
- **D2. An operation applies it where it opens or writes.** Local-extension reads (`LocalFile::read`, the binary a crate
  directory names included), every lock-plan write (config, `Also` files, lock, installed modules), format's and
  migrate's sources, rename's edited files and the inference mark resolve the name through the request's containment and
  open what came back (`Held::real`), naming `Held::spelled`. The name checked is the name opened.
- **D3. Every such request carries it, required.** `containment: &Containment`, no `Default`, no `run_within` twin: a
  caller that forgets it does not compile.
- **D4. MCP resolves only what it targets or opens.** The call target's `path`, init's directory, outline's `file` and
  render's file, through the same resolver. The pre-checks over a name an operation reads (init's and add's local
  extensions, format's `paths`, rename's files) go.
- **D5. The refusal.** `invalid_input`, with the request field where it is known (ADR 0092 D5, ADR 0072), and one
  `OpError` code string, `outside_root`. Migrate, infer and format's per-file failure keep their own codes and wording.
  No catalogue code is added.
- **D6. One landing rule.** Absolute; `..` applies to the real directory reached so far; every symlink is followed, a
  dangling one by its target text; a missing tail is joined onto its nearest real ancestor. So reading, rewriting and
  creating share one verb, `resolve`, and a missing file behind an escaping symlink is outside.

## Rejected

- The surface check with the split fixed: the next reading diverges again.
- The name `Reach`, a place in `specforge_common`, three verbs (`read`, `write`, `create`), `Option<&Path>` or `Default`
  as the way to say "unbounded".

## Consequences

- R1-R7 are refused over MCP; the CLI is unchanged. Local-extension refusals over MCP carry the operation's message.
- A dangling symlink is where it points, no longer "inside".
- Check-then-open stays two system calls; a link swapped between them is not caught.

## What would reopen it

A third surface that bounds requests differently, or an operation that must open a file through a handle (`openat`) to
close check-then-open.

**Amended by ADR 0112 (2026-10-16):** D2: the list of sites grows to every project file an operation opens or writes (collect's report and runner files, analyze's named report, the inference manifest, migrate's sidecars, the lock plan's config and `Also` files), opened through the `Held` they were checked as; a test holds it.
