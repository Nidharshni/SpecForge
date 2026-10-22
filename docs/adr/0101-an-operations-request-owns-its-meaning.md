# An operation's request owns its meaning; a surface only spells it

**Status:** accepted (2026-10-15, round 10, plan 10). Extends ADR 0027 (option tables) and ADR 0033 D4 (one
default) from names and defaults to every rule a request's values follow. Amended by round 13, plan 12 (D1, D2). D4 is extended by [ADR 0135](0135-a-remedy-names-a-command-or-the-requests-words-never-a-flag.md): an operation's remedy names a command or the request's words, never a surface's flag. Amended by round 14, plan 12 (D1: a relation the request's type states). Amended by round 14, plan 09 (D5: one in-process MCP harness). Amended by round 18, plan 07 (D1: an entity a request names, and a depth without its entity). Amended by [ADR 0177](0177-a-reads-selection-is-checked-by-the-project-view.md) (D1: a depth without its entity is one refusal about the entity, export's included). Extended by [ADR 0220](0220-a-kinds-list-written-as-text-is-read-by-one-rule.md) (D1: a kinds list written as text is read by `view::kinds_of_text`).

The CLI and MCP each build an operation's request by hand. Most of that is spelling: copying a field, reading a
shared constant. Three times a surface decided what the arguments *mean*, and the two disagreed:

- `specforge.collect` read `runner: "auto"` as "detect" before the operation saw it, a leftover of the tool ADR 0002
  replaced; the CLI forwards the name as typed and refuses `auto` (E058).
- `specforge.trace` and `specforge://prompts/trace` let `plan` win silently over `entity_id`; `specforge trace`
  refuses an entity with `--plan`.
- `specforge.export` let `no_schema` win silently over `with_schema`; `specforge export` refuses both flags.

No test paired those arguments across surfaces, so nothing caught them. Some rules were already read once in ops
(`format::Mode::of_flags`, `migrate::parse_target`, `check::parse_severity`, the option tables, export's depth
without a scope); one more was still written twice (a model's depth needs a root).

## Decision

- D1. What a request's values mean is the operation's. A default (ADR 0033 D4), a value that stands for
  another, arguments that exclude or need each other, and how several flags combine into one choice are each
  stated once in ops, beside the operation: as a reader the surfaces call (`format::Mode::of_flags`,
  `migrate::parse_target`, `export::Schema::of_flags`, `trace::Ask::of`, `model::root`) or as a check
  the operation makes on the request it is given (export's depth without a scope). A reader that refuses
  refuses about a field of the request (ADR 0072), so each surface names its own argument.
- D2. A surface spells. It names, documents and lists the arguments, reads their argv or JSON types, and copies
  them into the request or hands them to the reader. It maps no value to another and ranks no argument over
  another. `specforge.collect`'s `runner: "auto"` is gone: an absent runner asks for detection on both
  surfaces, and a name no collector answers to is E058.
- D3. The CLI may refuse first. clap's `conflicts_with` and `requires` are the CLI's spelling of a relation the
  operation holds: a usage error (exit 2) before anything compiles, as ADR 0027 D5 refuses an unknown name. A
  relation clap states that the operation does not hold is a drift; D5's table has a row for each.
- D4. Exclusive arguments are refused, never ranked: trace's entity and plan (tool, prompt, command), export's
  with_schema and no_schema. A surface may offer less than another (MCP's trace does not trace every entity),
  never mean otherwise.
- D5. One pairing table: `crates/surfaces/cli/tests/request_pairs.rs` lists each request as a command line and a
  tool call over one project; both answer the same normalized document or both refuse. The in-process MCP call
  and the normalization the harnesses share are `crates/surfaces/cli/tests/support/pairing.rs`.

## Consequences

User-visible over MCP: `specforge.collect` with `runner: "auto"` is E058 on `runner`; `specforge.trace` and
`specforge://prompts/trace` with both `entity_id` and `plan` are invalid input on `plan`; `specforge.export` with
both `with_schema` and `no_schema` is invalid input on `with_schema`; `specforge.model`'s depth-without-root
message is the operation's. The CLI is unchanged.

The refusal codes (`entity_with_plan`, `with_and_without_schema`, `depth_without_root`) are `OpError` code
strings, not catalogue diagnostics; over MCP the wire code stays the kind (`invalid_input`).

## Rejected

- One request decoder in ops over a neutral argument map: spelling is a surface's (ADR 0029 D2, ADR 0033).
- Comparing the two surfaces' requests by value: the CLI is a binary crate and both surfaces' argument structs
  are private; the table compares answers in process instead.
- Ranking (plan wins, no_schema wins) on every surface: it hides a caller's mistake.
- Deleting clap's relations so ops alone refuses: the CLI would lose its usage message and exit 2 (ADR 0027 D5).

## What would reopen it

A third surface that spells requests (a REST surface, an LSP command taking these arguments), or an argument
whose meaning depends on the project (then it is checked against the view, as ADR 0027 D6 does for passes).

## Amendment (architecture round 11, plan 07)

- **D1.** "How several flags combine into one choice" is stated once in ops for consent: `Asking::of`
  ([ADR 0110](0110-who-can-be-asked-is-one-value-handed-to-an-operation.md)).

## Amendment (architecture round 13, plan 12)

- **D2.** It holds for an argument a surface owns, that picks which reader answers: the infer prompt's `scope`
  names one of five (none or `overview`, `kind:<name>`, `file:<path>`, `plan`, `workflow`) and refuses any other
  value as invalid input on `scope`, naming them; it is never read as the overview. An empty `scope` is no scope.
  The grammar stays in the prompt: no other surface spells a scope (ADR 0101 "Rejected": one request decoder in ops).
- **D1.** `trace::Ask` is `Trace(Target)` or `Plan(&AgentPlan)`: it carries the trace operation's own target, so no
  surface maps one onto the other.

## Amendment (architecture round 14, plan 12)

A relation the request's type makes unrepresentable is stated in ops by that type: `SearchRequest.field` is `Option<FieldHolds>`, a field and a value or neither. The one surface that builds the request (`specforge.search`) words the refusal of a half pair and names the missing argument, as D2 keeps the infer scope in its prompt. Reopen with a second surface that searches entities.

## Amendment (architecture round 14, plan 09)

**D5.** Every in-process MCP call in the CLI's tests goes through `support/pairing.rs` (`mcp_on`, `served`) and
`specforge_mcp::testing::rpc`; no test file sends JSON-RPC by hand.

## Amendment (architecture round 18, plan 07)

**D1.** An argument that names an entity, as the request's subject or as a filter (coverage's `entity_id`, the fixes'
entity, search's `references`), is checked by the operation: one the graph lacks is `view::not_found` about that
field. A depth needs the entity it counts from: explore and review refuse it alone (`view::depth_without_entity`,
about the entity), as export and model do; the CLI's `--depth` `requires` the entity (D3) and the pairing table has a
row for each (D5). Arguments that compose are ANDed, never ranked (D4): coverage's `kind` narrows its `entity_id`.
Search's field/value pair stays worded by `specforge.search` (round 14): its reopen trigger has not fired.
