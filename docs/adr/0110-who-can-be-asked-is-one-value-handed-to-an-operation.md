# Who can be asked is one value handed to an operation

**Status:** accepted (2026-10-16, architecture round 11, plan 07). Amends ADR 0029 D4b, ADR 0059 D6, ADR 0002
("Execution") and ADR 0044 D4. Extends ADR 0101 D1. D2's request sentence and D5's builders amended by [ADR 0124](0124-what-the-caller-lends-an-operation-is-one-value.md). D4's R-TRUST-003 text is amended by [ADR 0135](0135-a-remedy-names-a-command-or-the-requests-words-never-a-flag.md): "(`specforge add --yes` and `specforge update --yes` accept it without asking)" replaces ", or with --yes".

"May we ask someone, and who answers?" was spelled three ways across two seams. The registry port took `Trust`
(Refuse, AssumeYes, Prompt) in each request, and the CLI also had to hand the adapter a closure (`.asking(..)`):
a `Prompt` without the closure silently meant no. Collect had its own `Consent` (Yes, Approved, Prompt). The CLI
built each with its own rule (`OutputFormat::trust`, `collect::asks`), which agreed only because `Terminal::ask`
repeats the `interactive` test. The in-memory registry consented under `AssumeYes` only, so no ops test could script
an answer.

Three wordings reached users wrongly:

- R5: over MCP, a key change is refused with R-TRUST-003, whose suggestion says "re-run with --yes". The MCP tool
  has no `yes` argument (R6: `unknown argument 'yes'`).
- R9: over MCP, collect's E059 is rewritten by the surface to "the test command isn't approved ...; run
  `specforge collect` in a terminal once (...)", which keeps `--yes` and `--no-run`, and leaves `suggestion` null.
  MCP was the only surface that rewrote an operation's message.
- D3 (minor): a user who answered no at the terminal is told to run it in a terminal.

## Decision

- **D1. Where the value lives.** A new ops base module `specforge_ops::asking`: `Asking` (`Yes`, `Nobody`,
  `Ask(&dyn Asker)`), the `Asker` port and the `Question` enum (`KeyChange`, `RunCommand`). Collect (analysis) and the
  registry port (base) both reach it; no group edge is added.
- **D2. How the adapter gets it.** Inside `TrustPolicy`, per fetch (`TrustPolicy.asking`). The adapter keeps no asker
  state; `Trust` and `ConfiguredRegistry::asking` are deleted. `AddRequest`, `UpdateRequest` and `collect::Request`
  carry `asking`; `collect::Consent` is deleted. ADR 0059 D6 (consent is asked at fetch) stands.
- **D3. The asker's interface.** A trait, `Asker::answer(&Question) -> bool`: one port, three adapters (the CLI's
  terminal, nobody, a test's `Scripted`). The method is `answer`, so it never shadows `Terminal::ask(&str)`.
- **D4. Wording true on every surface.** The operation's wording names the CLI command that can consent, the only
  place consent is given; MCP's E059 rewrite is deleted.
  - R-TRUST-003 suggestion: "if you trust the new key, accept it with `specforge add` or `specforge update` in a
    terminal, or with --yes; or add '{offered}' to trusted_keys".
  - E059: message "running `{argv}` needs approval for this project", suggestion "approve it once with `specforge
    collect` in a terminal, or run `specforge collect --yes`; reading the report the runner already wrote needs no
    approval".
- **D5. Who builds it.** The CLI builds it once per command from `--yes`, the output format and `facts.interactive`
  (`Asking::of`, the one rule: `--yes` wins, else the asker when someone can answer, else nobody). MCP passes
  `Asking::Nobody`. `Terminal::ask` keeps its `interactive` guard (ADR 0029 D4b: nobody to ask is no).

An `Asker` is called only for a question nothing else settled: the registry checks the user's `trusted_keys` first,
collect checks the consent store first. `Yes` never calls the asker and records no approval.

## Rejected

- D1 (b) in `registry`: collect would import the registry port for a concept that has nothing to do with registries.
  D1 (c) a leaf crate: one crate for about 80 lines plus crate-map churn.
- D2 (b) keep `.asking(..)` on the adapter: the two halves stay. D2 (c) a separate `Registry::fetch` parameter: the
  port changes shape for no gain over the policy.
- D3 (b) a closure: a test asker must record what it was asked, so a `RefCell` in every test. D3 (c) one trait per
  question kind: two ports for one idea.
- D4 (b) `Nobody` carries a per-surface remedy: more machinery, and the remedy varies by operation. D4 (c) keep MCP's
  rewrite and add a second: a surface would edit ops messages in two places, against ADR 0101 D2.
- D5 (b) wording "you declined" apart from "nobody could be asked": the registry's text lives in `registry-client`,
  which would need the arm passed through. Revisit if users report it.

## Consequences

- MCP's E059 has a suggestion and no rewrite; R-TRUST-003 names the CLI commands on every surface.
- Ops tests script answers (`asking::testing::Scripted`) and see what was asked; the registry contract gains C11.
- No CLI behaviour changes. No new diagnostic code.

## What would reopen it

A surface that can ask but is not a terminal (an MCP elicitation request: a fourth `Asker`), or a third kind of
question (it becomes a `Question` variant).
