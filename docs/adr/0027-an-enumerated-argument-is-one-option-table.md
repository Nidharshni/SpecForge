# An enumerated argument is one option table

**Status:** accepted (2026-10-06)

The arguments a command or tool takes from a closed set of names were spelled on every surface.
`specforge model --format` had an emitter enum (with `#[default]`), a CLI mirror enum with
`default_value = "markdown"`, a `match` back to the emitter's enum, an ops parse table (`Named<T>`),
an MCP JSON `enum`, a description saying "(default: markdown)" and an `unwrap_or("markdown")`.
Seven CLI mirrors, four match maps, two ops parsers of different shapes and fifteen MCP lists, and
nothing compared them, so five drifts were live: `json` was an export format on MCP but not the CLI;
`specforge.render` refused `graph`, the name every other surface uses; `specforge.query` answered
`graph` for any unknown format; the CLI refused the analysis passes extensions declare; and the
outline's default was markdown on the CLI and json on MCP, said only in a description.

## Decision

- **D1. One table per argument, in ops, beside its operation.** `specforge_ops::options::OptionTable<T>`
  holds the listed names in order, the aliases each accepts, a one-line help per name and the default.
  It has no error vocabulary of its own: an unknown name is `OpErrorKind::InvalidInput`, reported under
  that kind's name (`invalid_input`; the table used to carry a `code`, `unknown_format` for formats, a
  second spelling of the same failure). The tables: `export::{FORMAT, AGENT_FORMAT}`, `model::{MODEL_FORMAT,
  GROUP_BY, MODEL_FIELDS, OUTLINE_FORMAT, OUTLINE_FIELDS, DEPS}`, `coverage::STATUS`,
  `navigate::DIRECTION`. The value types stay where they are (the emitter's formats, ADR 0007); ops
  re-exports them so surfaces name ops.
- **D2. The surfaces are adapters.** The CLI builds each flag's possible values with
  `options::choice(&TABLE)` (a `PossibleValuesParser` mapped through the table) and its default with
  `default_value = TABLE.default_name()`; MCP declares the argument as a field
  `#[arg(choice = TABLE)]` of the tool's typed arguments (amended by
  [ADR 0033](0033-a-tools-arguments-are-one-typed-definition.md)): its input-schema property
  (`args::choice_schema`) and its reading (`OptionTable::parse`, the refusal on the argument) are
  both derived from that field. No surface spells
  a name or a default. Severity and lint profile names (ADR 0018) stay name lists whose MCP schema
  reads them (`args::names_schema`).
- **D3. One default per argument, on every surface.** The table's default is what an absent
  argument takes on the CLI and over MCP, and what both advertise. The outline's format, which was
  markdown on the CLI and json over MCP, is markdown on both, as the model's always was.
- **D4. Aliases are accepted everywhere and listed only where a client validates**: clap accepts
  them without listing them; MCP's `enum` lists them after the names; refusals list names only, and
  everything a refusal offers is that one list (`OptionTable::refusal`: the message's "Expected:" and
  `specforge.render`'s `available_renderers` are both `names()`).
  `json` is the one alias, of the `graph` export (and so of render's and the published schema's
  format).
- **D5. One refusal**: `Unknown <argument>: <name>. Expected: <names>`, with `did you mean
  '<closest>'?` when one is close. The CLI refuses before compiling with clap's message (exit 2),
  listing the same names.
- **D6. An open set is not a table.** Analysis passes depend on the project's extensions: the CLI
  takes any name and relays the operation's refusal (`UnknownPass`, exit 2), as MCP does. Entity
  kinds likewise (`unknown_kind`).

## Consequences

- User-visible: `specforge export --format json` and `specforge schema --publish --format json`
  work; `specforge analyze <extension pass>` runs; `specforge.render` accepts `graph` and answers
  `"format": "graph"` for `json`; `specforge.query` refuses an unknown format; refusals of an export,
  render, coverage-status or direction name use D5's wording; MCP input schemas carry `default`
  and per-value help; `--help` lists each value's help. `specforge.outline_extensions` with no
  `format` answers markdown, as `specforge outline` and `specforge.model` do; a client that wants
  JSON passes `format: json`.
- The same round deletes the emitter's `emit_*_with_schema` functions (ADR 0007: callers use
  `emit`) and routes `specforge schema --kind`/`--publish` through `ops::schema` (ADR 0015 D8 as
  amended). `specforge schema --kind` now prints the document `specforge.schema` returns (the kind
  under `entity_kinds`, with the edge types that touch it): a script that read `.name` or `.fields`
  at the top level reads `.entity_kinds[0]`.

## What would reopen it

A third surface whose argument syntax cannot be built from a name list (positional words, a
free-form grammar), or an enumerated argument whose names depend on the project.

Extended by ADR 0101: every rule a request's values follow is the operation's.

Amended by ADR 0152: `specforge.export` reads `FORMAT`; `AGENT_FORMAT` is the formats with a published JSON Schema.
