# Init writes its starter's values through the value writer

**Status:** accepted (2026-10-23, architecture round 25, plan 06). Extends [ADR 0189](0189-init-checks-and-lays-out-its-starter-with-the-registry-build-of-the-extensions-it-enables.md) D4 (the laid-out starter is held to a parse before it is written) and [ADR 0153](0153-a-value-is-spelled-by-one-module.md) D1 (init's starter spells through `spell`).

## Context

`init` filled the starter's `{version}` by raw text replacement into a quoted slot, by `.replace` for a template and by
`format!` for the structural starter. Three defects followed:

- A version holding `"` wrote a starter that does not parse. Init exited 0 and the first `check` reported E001 twice
  (CLI and MCP, structural, software and product starters).
- A version holding `\` wrote a starter that parses and checks clean but states another version than `specforge.json`:
  `v1\nbeta\q` read back with a line feed. Nothing reported it.
- Init never asked whether the laid-out starter parses. `canonical` keeps the formatter's text and drops its
  diagnostics, so any template (a local `.wasm` extension's included) was written as it came.

ADR 0153 D1 says no other code quotes a `.spec` value. Init was a speller it did not see, and the third fix to "the
starter init writes is not one its first check accepts" (ADR 0189 was the first two).

## Decision

- **D1. Every value init fills into a starter is spelled by `spell`.** `spell::escaped` is the inside of a quoted
  value (`quoted` is it between two quotes). `{project}` and `{version}` stand for the inside of a `.spec` string; a
  template writes them between quotes, as both builtins and the documented example do.
- **D2. The structural starter is a template**, `STRUCTURAL_STARTER`, filled by the same one line as an extension's.
- **D3. The laid-out starter is parsed (`specforge_parser::parse`) before anything is written.** A parse error refuses
  `starter_unparsed` (`InvalidInput`, about `Extensions`), naming the extension whose template it is, with nothing
  written. It is the parse `check` starts with, not the compile.

```
 req.version ──▶ template | STRUCTURAL_STARTER ──▶ escaped(..) ──▶ canonical ──▶ parse ──┬─▶ stage, write
                                                                                          └─▶ starter_unparsed
```

## Rejected

- The placeholder as the whole quoted value (`version {version}`, filled with `spell::quoted`): it changes both
  builtin templates (blob inputs) and breaks every third-party template written to the documented example.
- Refusing a version that needs escaping: it narrows input nobody asked to narrow, and `specforge.json` holds any
  text.
- The formatter's diagnostics as the judge of the starter: a whole-file failure is one E001 and a partly read form is
  W142; `check` reads the parser's error list.
- The compile as the judge: ADR 0189 rejected it and still stands.
- A catalog code for the refusal: the starter is never on disk, so `check` never reports it; it is an op code like
  init's others.

## Consequences

- A version may hold any text; the spec block reads back what `specforge.json` records.
- A template that places `{version}` bare is the author's to quote; if the result does not parse, init refuses it.
- Projects created before this keep the starter they were given; nothing migrates them.

## What would reopen it

A placeholder that is not a string (a list, a reference); a starter that needs the compile.
