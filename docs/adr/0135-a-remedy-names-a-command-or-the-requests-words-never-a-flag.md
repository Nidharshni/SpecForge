# A remedy names a command or the request's words, never a flag

**Status:** accepted (2026-10-17, architecture round 14, plan 03). Extends ADR 0101 D4 and ADR 0072 D3; amends ADR
0110 D4 (R-TRUST-003's text); amended by the round-14 follow-up F2 (D5).

Below the surfaces, 15 string literals told the caller which CLI flag to pass. An operation does not know its
caller, so 8 of them reached MCP callers whose arguments are spelled otherwise (`min`, `run`, `runner`, `force`,
`allow_unsigned`), and one, collect's "run without --no-run to produce one", told an MCP caller (whose default is
not to run) the opposite of the fix, `run: true`. Two such texts had been fixed one at a time (ADR 0110 D4) with
one `contains` each; the rest stayed.

## Decision

- **D1. What a remedy says.** A refusal's message and suggestion, a finding's remediation and a diagnostic's text,
  written below the surfaces, name either a whole command (`specforge collect`, `specforge doctor --path <root>`:
  a flag inside a named command is fine, it is typed in a terminal whatever the surface) or the request's own
  words ("name the runner to use", "force the removal", "allow unsigned packages"), never a bare flag. The words
  come from the request's field name, which the CLI flag and the MCP argument both derive from.
- **D2. One guard.** `specforge-ops`'s `remedy_wording` test parses the `src/` of every crate below the surfaces
  and refuses a string literal with a `--flag` that no program (`specforge`, `cargo`, `z3`) precedes in its clause.
  Attributes, doc comments, `#[cfg(test)]` modules and one-token argv literals are not remedies.
- **D3. The catalog's explanations are exempt.** They document each surface's spelling side by side.
- **D4. A surface's own text is its own.** The CLI may name its flags in errors it raises itself; MCP may name a
  whole CLI command (`specforge check --lint`).
- **D5. A description an extension declares is a remedy for this rule.** `ExtensionCommand::description` is read
  verbatim by the CLI (`long_about`) and by MCP (the tool's description), so it names the request's own words
  ("unless a sort field is given"), not a flag; the option list and the argument schema spell the name. The guard
  reads each builtin's committed `describe_surfaces.json`. An error text an extension raises is the extension's,
  kept by hand; a third-party extension's description is out of the guard's reach.

## Rejected

- **A typed remedy**: a remedy slot naming a request field (and a value: CLI `--no-run` is MCP `run: false`), a
  CLI flag table and a renderer per surface; four field variants that are no refusal subject, and registry
  diagnostics with no field type. ADR 0110 D4 (b) rejected the same for consent.
- **A runtime assertion in `OpError`**: the CLI's own `OpError`s name its flags, correctly, and an assertion fires
  only on paths a test runs.
- **Exempting CLI-only texts**: ops does not know its caller; the next MCP argument inherits them.
- **A placeholder rendered by the host** (`{arg:sort_by}` in a description): a template grammar in free text and an
  SDK change that stales every blob, for the gain ADR 0110 D4 (b) rejected.
- **A host refusal of a description naming a flag**: a protocol change, and it would hide a third-party command over
  a wording choice.

## What would reopen it

A surface that must act on a remedy, not print it (a client retrying with `run: true`): then the remedy is data on
the refusal, spelled by each surface as the subject is.
A description that must show the CLI spelling (a tutorial text a surface renders): then the argument is data in the
description, rendered per surface.
