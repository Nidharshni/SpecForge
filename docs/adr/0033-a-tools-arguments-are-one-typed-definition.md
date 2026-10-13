# A tool's arguments are one typed definition

**Status:** accepted (2026-10-07). Amends ADR 0027 D2 (how MCP builds and parses an enumerated
argument).

Every core MCP tool defined its arguments twice: a hand-written JSON input schema in
`tools/table.rs` (371 lines for 34 tools) and a serde `Args` struct beside its handler. A serde field
tracer and a probe that retried until the struct parsed recovered the struct's names so that a test
could compare them with the schema's. Nothing compared types, defaults or required-ness. A default
was spelled in the schema, in its description, in the handler's `unwrap_or` and on the CLI: query's
depth four times, init's version four times and once more as a literal in the starter spec.
`specforge.format` advertised `write: default true`, whose description said false in check or diff
mode, so a client echoing the advertised default wrote files in check mode. The struct read an
optional argument of the wrong type as absent (kept in 2026-09 so that no call that worked would be
refused): `{"dry_run": "true"}` renamed for real, `{"check": "true"}` formatted, `{"depth": "0"}`
answered depth 1, while `specforge.list` refused `{"limit": "2"}`. An argument no tool declared was
ignored, though extension command tools (ADR 0017 D4) and resources (ADR 0024 D4) refuse one.

## Decision

- **D1. One derive.** A core tool's or prompt's arguments are one struct with `#[derive(Arguments)]`
  (`specforge_mcp::args`, the proc-macro crate `specforge-mcp-macros`). A field's name is the
  argument's, its doc comment the description, its type how a value is read and the JSON type
  listed (`args::Arg`: `String`, `bool`, `usize`, `Vec<String>`, `Map`, `AgentPlan`, `EntityIds`,
  `Option<_>`). `#[arg(default = …)]` gives its default, `#[arg(choice = TABLE)]` its option table
  and `#[arg(names = …)]` its listed name list. The input schema (`args::input_schema`, with the
  target's `path`/`use_cached` and `additionalProperties: false`), the prompt listing and the reading
  (`args::read`) are all derived from it. The tool table holds no input schema.
- **D2. One argument rule.** Absent or `null` is the default; a missing required argument is
  `Missing required parameter: <name>`. A boolean is read from `true`/`false` or `"true"`/`"false"`,
  a count from a non-negative integer or a string holding one, a number (`f64`, e.g.
  `specforge.analyze`'s `min`) from a JSON number or a string holding a finite one, a string from a
  string: through the
  extension commands' `command_args::normalize_arg`, with its wording, naming the argument. A list is
  a list of strings; a prompt's entity ids may also be one comma-separated string. Anything else is
  `invalid_input` on the argument.
- **D3. An undeclared argument is refused** on core tools and prompts: `unknown argument '<name>'`,
  `invalid_input` on it, with the close declared name as a suggestion. The call target's names are
  its own (`TargetSpec::accepted`: `path` for every reach but `Unscoped`, including a `Served`
  entry's unlisted own-root `path`; `use_cached` for `FreshUnlessCached`), and the target reads them
  by D2.
- **D4. A default is spelled once.** A field's `#[arg(default)]` reads an ops constant when another
  surface states the same default, and the CLI's `default_value` reads the same constant
  (`init::DEFAULT_VERSION`, `query::{DEPTH, SEARCH_LIMIT}`, `analyze::EVERY_PASS`,
  `SchemaRequest::default()`). An argument whose default depends on others (`format`'s `write`) is
  an `Option` with no stated default; its description says the rule.
- **D5. Enumerated arguments stay option tables (ADR 0027)**, declared on the field
  (`#[arg(choice = TABLE)]`): the listing is `args::choice_schema`, the reading `OptionTable::parse`
  with its refusal on the argument. A name list (severity and lint profiles, infer_session's states)
  is listed, and the operation refuses a name outside it.

## Consequences

- User-visible over MCP: `specforge.format`'s `write` states no default; a string `"true"` or `"2"`
  is read as the boolean or count it holds on every core tool, and another wrong-typed value is
  refused naming the argument (a preview asked for as a string no longer writes); a list argument
  given a single string is refused; an undeclared argument is refused, and every core input schema
  says `additionalProperties: false`; booleans state `default: false` and counts `minimum: 0` where
  they did not; prompt argument refusals use the same wording and name the argument; `path` must be
  a string and `use_cached: "true"` is honoured. CLI: `specforge init -h` shows `[default: 0.1.0]`,
  and `init --version X` writes `version "X"` in the starter spec too.
- The serde tracer and probe, `lenient`, `PromptArgs` and its description tables, and every
  `unwrap_or` of an argument default in a handler are gone.
- Output schemas stay hand-written: results are built with `json!`, not from a typed result
  (superseded by [ADR 0048](0048-a-core-tools-reply-is-one-typed-definition.md): a core tool's reply
  is one typed definition and its output schema is derived).
- Amended by plan 15 (round 5): a graph resource reads a count in its query by the same rule and wording.

## Rejected

- **schemars**: it covers the schema only, so reading would stay serde with a second attribute
  vocabulary per field. Its output needs a transform (root `title`, `["boolean","null"]`, `"default":
  null`, `"format": "uint64"`). `#[serde(default)]` overwrote a table schema's default with `null`.
  serde errors cannot name the field, and its sorted properties lose a prompt's argument order
  (`schemars/preserve_order` would reorder every JSON document the workspace writes).
- **A declarative argument table**: untyped reads in handlers, or two definitions again; the wire
  `CommandArgType` cannot express lists, objects or tables.
- **Keeping the lenient read**: a preview flag sent as a string wrote files, a P0 under
  `dry_run_side_effect_freedom`.

Amended by plan 15 (round 5): a number argument reads as above (its wording is
`<name> must be a number, got <value>`, not `normalize_arg`'s, which has no number type), and a graph
resource reads a count in its query by the same rule and wording.

Amended by plan 11 (round 8): an operation's refusal names its argument through the tool spec. Each core tool
declares which argument each refusal code of its operations is about (`CoreTool::REFUSAL_ARGUMENTS`), and the
dispatcher names it on the refusal its handler returns; a debug build fails an `invalid_input` refusal that
names none. The five per-tool mechanisms (by code, by a `data` key, by kind, by variant, every refusal) are
gone, and an operation writes no argument name in its refusal's `data` (the spelling is the surface's, ADR 0029
D2). Refusals that shared a code have their own (`unknown_severity`, `unknown_lint_profile`,
`source_root_outside_root`). A surface that parses its own argument through an ops helper (`render`'s `format`,
`infer_session`'s `action`) still names it where it reads it, as D2's argument reading does.

## What would reopen it

A second crate that needs the derive (it moves out of `specforge-mcp`'s namespace), an MCP revision
whose argument model JSON Schema basics cannot state, or typed result structs for tools (then output
schemas could be derived the same way).
