# One contract for extension commands; the product surface is its commands

**Status:** accepted (2026-10-03)

ADR 0008 made the `specforge product` queries `@specforge/product`'s commands, run over the graph
the host passes. The product spec still describes a larger surface than that contract can carry:
49 commands (17 built), 39 MCP resources, three output formats the host does not know of
(`json` default, `table`, `brief`), cursor pagination needing a graph revision, timelines needing
a clock, weighted completion needing project configuration, and observability events from a
command. The surface specs also promise export, schema and arg-type validation, a per-contribution
toggle and eight events that nothing calls, reads or emits. This ADR fixes the command contract
once for every extension, trims what the design rules out (ADR 0003), and leaves what is still to
build as unproven obligations.

## A. The host owns `--format`; formats are `human` and `json`

- Every extension command has the host's `--format human|json`, like `--path` and `--help`: the
  CLI adds it, defaulting to `human`, and a command declaring an arg named `format` is refused
  (exit 2), as one declaring `path` is. MCP's auto-promoted tool has no `format` argument: the
  host always asks for `json`, and when the command exits 0 with a JSON object on stdout, the
  tool result carries it as `structuredContent` beside the text.
- `CommandInput` gains `format` (`"human"` or `"json"`, serde default `human`). The extension
  renders both, since only it knows its payloads: `json` is one root object, the payload type
  the spec names for the command; `human` is the extension's layout (a table with a header row
  where the payload is tabular).
- `table` and `brief` are not formats. A human table is what `human` is; `brief` already names an
  export resolution (`specforge export --format brief`), and an agent reads `json`, whose
  payloads are already small (counts, ids, pagination). Core commands use the same pair
  (`OutputFormat { Human, Json }`), so the whole CLI has one rule.

Why host-owned: an agent over MCP can never get prose by leaving a flag out, every command takes
the same flag with the same values, and the extension cannot drift from it.

## B. Errors, exit codes and pagination are the command's, written once

- A command that cannot answer writes one error to stderr and nothing to stdout: under `json`
  the object `{code, message, entity_id?, suggestion?}` (`ProductSurfaceError` for product),
  under `human` `error: <message>` and, when there is one, `did you mean '<id>'?`. The
  suggestion is the nearest id of the same kind within Levenshtein distance 2.
  `ENTITY_NOT_FOUND` exits 1. `INVALID_INPUT` (a value outside a closed enum, a missing required arg, a
  negative offset, an unknown sort field) exits 2, the code clap gives the usage errors it
  catches itself on the same command line. Over MCP a non-zero exit is an `isError` result
  carrying the same object.
- `GRAPH_NOT_READY` is gone: a command only ever runs over a built graph.
- One pagination contract, for the lists and for the project-wide matrix queries (coverage
  matrices, overlap, owner workload, module coupling): `--limit` (default 100, clamped to
  [1, 1000]) and `--offset` (default 0); the payload carries `total` (after filters, before
  paging), `offset`, `limit` and `has_more`. Cursors are dropped: with no graph revision a cursor
  detects nothing an offset does not, and one scheme is one schema. Other queries are not paged.
- An entity-scoped command takes the entity id as a positional arg named after its kind
  (`milestone`, `journey`); the MCP tool's argument has the same name. A tags filter is a
  comma-separated string arg (`--tags a,b`): there is no list arg type.

## C. The host passes the date; no configuration reaches a command

`CommandInput` gains `today`: the host's date when the command is called, UTC, `YYYY-MM-DD`.
Commands that compare dates (`milestone_timeline`, `milestone_velocity`) also declare `--as-of`,
which overrides it. A date is domain-free; passing it keeps a command a function of its input
(tests pass `--as-of`), where a guest clock would not be. Compiles are unchanged: `specforge
check` and compiler passes get no date, so diagnostics stay deterministic (I058 stays query-time).

No project configuration reaches a command. The effort weights are the effort scale's definition
(xs=1, s=2, m=3, l=5, xl=8, a missing effort weighs as m), not settings:
`pe_effort_weights_configurable` is superseded. Per-extension configuration would need a config
schema the manifest declares, a `specforge.json` namespace and a channel to both commands and
passes, for one consumer.

## D. Product declares no MCP resources, and no one-hop lookups

- The 39 `specforge://product/...` resources are trimmed. Each repeated a command that MCP
  already serves as the tool `specforge.product.<id>`, over the served graph, in the same JSON.
  A resource export receives only its URI, so serving them needs the graph passed to resources,
  a clock for `_timestamp` and a second response envelope (`ProductSurfaceResponse`) for the same
  data. The host's extension-resource path stays for extensions with content that needs no
  graph.
- The nine one-hop lookups (`feature_milestones`, `persona_journeys`, `channel_journeys`,
  `module_deliverables`, `milestone_deliverables`, `module_features`, `journey_deliverables`,
  `release_deliverables`, `release_milestones`) are trimmed: each follows one reference whose
  kind pair is unique, which core `specforge query <id> --depth 1 --kind <kind>` and MCP
  `specforge.query` / `specforge.find_references` already answer, with the edges. Nine fewer
  tools is less for an agent to choose between, at no loss. `pe_reverse_query_symmetry` is
  superseded; symmetry is core's query. `feature_dependents` and `deliverable_dependents` stay
  (same-kind `depends_on`, where a depth-1 query mixes both directions), as do the multi-hop and
  computed queries.
- Commands emit no events: `pe_cli_command_executed` and `pe_mcp_resource_accessed` are trimmed.
  MCP records each command as `surface_command_dispatched`, with its duration.

That leaves 40 commands: the 17 built and 23 to build. Their obligations stay unproven until
built.

## E. Surface declarations are checked where they can be

- **Export presence cannot be checked before a call.** A component guest has one `call` export
  that routes by name (ADR 0004's bridge world), so the host has no export list to compare a
  declaration against. `validate_surface_exports` is deleted. A declared export the guest does
  not route answers `unknown export`, which the host reports as E028 when it is dispatched. W055
  (surfaces without a binary) cannot arise: an extension without a binary does not load.
- **Arg types are a closed enum in the protocol**, so an unknown one cannot be represented. A
  `surfaces` description that does not parse now fails the extension's load (E028), as every
  other described category does, rather than silently dropping its surfaces.
  `validate_command_arg_types` is deleted, and with it its "no args is W057" clause (W057 is
  product's milestone code).
- **Tool schemas are checked in the registry build.** An explicit MCP tool whose `input_schema`
  or `output_schema` is not a JSON object is E055 and is not registered. A tool's description is
  a required protocol field, so W056 is trimmed. `validate_mcp_tool_schemas` moves into the
  registry build.
- **No toggle.** Nothing in `specforge.json` disables one contribution, so the `enabled` flag on
  surface entries and `toggle_surface_contribution` go. To hide an extension's commands, disable
  the extension.
- **Events:** the registry build is pure, and its diagnostics are its record. The registration,
  validation and toggle events are trimmed. MCP emits `commands_auto_promoted` and
  `surface_command_dispatched` (both built) and will emit `surface_mcp_tool_dispatched` and
  `surface_mcp_resource_dispatched` when an extension tool or resource returns.

## F. Payloads follow the product spec where the code drifted

`milestone_completion` reports `done_count`, `completion_ratio` in [0, 1] and `done_features`
(ids). `journey_coverage` counts a journey's features whose status is `done` and lists the rest
as `uncovered_features`, not features some module holds (module ownership is a validation
concern). `feature_impact` gains the deliverables reached through journeys and modules, the
transitive `depends_on` dependents and `total_affected_entities`. Ratios are in [0, 1]
everywhere; `health` keeps its 0 to 100 score.

`feature_impact` counted features that only list the feature under `features` (a "relates to"
link) as depending on it. Fixed here: only a `depends_on` reference is a dependency.

## Adjustments made while building it

- **A refused command is on neither surface.** The rule that refuses a command line (an arg
  named `path`, `format` or `help`, or two args of one name) is one function,
  `specforge_protocol_types::command_args::refusal` (ADR 0017; it also refuses a default its
  declaration contradicts); MCP does not promote a command it refuses, so a `format` arg never
  reaches an agent either.
- **The host applies a command's declared defaults.** Both surfaces send the args normalized by
  one rule (ADR 0017): the declared defaults, `false` for an unset flag, each value its declared
  type, no undeclared arg; the SDK runs the same rule, so a guest not built with the SDK gets them
  too, and a command tool's argument errors are the command's `INVALID_INPUT` object.
- **A command whose export trapped answers in the format asked for.** Under `--format json`
  the CLI writes E028 to stderr as one `{code, message}` object, the shape commands write
  their own errors in, rather than the diagnostic line.
- **Extension-command errors go to stderr; core commands' `--format json` errors go to
  stdout.** Core commands print `{"error", "code"}`, with `"suggestion"` when there is one, on stdout under `json`
  (`specforge_cli::outcome::Refusal`), extension commands print `ProductSurfaceError` on
  stderr. The difference is known and kept for now: changing core's would break every script
  reading core errors from stdout, for no consumer that needs the two alike. Reopen it when
  a consumer reads both kinds of command the same way (an agent driving the CLI rather than
  MCP), or at core's next breaking output change; then both write one error object to
  stderr. *(ADR 0029 D5: `specforge_cli::outcome::Refusal` is the one writer of core errors;
  every core command with JSON output writes them there, `stats`, `trace`, `analyze`, `migrate`
  and `init` included.)*
- **Only closed enums are validated.** `--family` takes any value: `ModuleFamily` is open
  (a family outside the standard set is I062, an info), so refusing it would refuse a valid
  project. A filter on a reference (`--persona`) matches the id, unvalidated.
- **An absent lifecycle status is its first value**, as each status type says (feature
  `proposed`, deliverable `draft`, milestone and release `planned`, persona and channel
  `active`): `--status proposed` lists a feature without a status. Entries still report the
  status as written.
- **Sorting.** `--sort-by` takes `id`, `title`, `tags` or a field the kind declares (read
  from the extension's own entity declarations); a closed-enum field sorts in its enum's
  order (`priority` critical first, `effort` xs first), any other by its text; an entity
  without the field is last in either order, but one without a lifecycle status sorts as
  that status's default; ties are by id ascending in either order.

- **Traversals follow references of the kinds they name.** Every hop keeps only targets (or
  sources) of the kind its path names, so a peer extension's reference under the same label
  is not followed. `deliverable_personas`'s `via_journey_ids` are the journeys on a path to a
  persona: a deliverable's journey that targets no persona connects none and is not listed.
- **`feature_impact` reports what the feature affects, not what it needs.** Its payload is
  `FeatureImpactPayload` alone: the `referenced_by_*` lists are `affected_*`, `depended_on_by`
  is the transitive `dependent_features`, and the features it depends on (`depends_on`, which
  deferring it does not touch) are no longer listed; the feature's own references answer that.
- **Rollups count a missing status as its default, and say how they order.**
  `deliverable_completion` counts the milestones whose status is `completed`
  (one without a status is `planned`), `release_completion` the deliverables
  `shipped` (absent is `draft`). The `milestone_details` it gives "when requested"
  are requested by a `--details` flag. `owner_workload` lists owners with the
  most entities first, ties by owner, and pages them; an empty owner string
  is no owner.
- **A dependency cycle is reported, never followed.** The dependency
  queries share one graph of a kind's `depends_on` (levels by Kahn, cycles
  by an iterative Tarjan, both O(V+E)). `feature_ordering` lists every
  feature once: those on a cycle or depending on one have no level and come
  last, ordered as a level is, and only those on a cycle are
  `cycle_members`. `critical_path` gives no path while any milestone cycle
  exists (E015), and says so in `CriticalPathPayload.message`, a field added
  for the "empty path with message" its surface promises; its slack is 0
  on the path, as a critical path's is, and `null` for a milestone without
  a `target_date`. `module_depth` is -1 for a module on a cycle or
  depending on one, its chain the members of the cycles it reaches, since
  neither has a longest chain. Equally long chains are taken by id.
- **A matrix is over every entity, its page only what is shown.** The
  coverage matrices have one entry per persona (or channel), by id; a
  ratio is reachable over all features, 0 for one without journeys and 0
  when there are no features, so it is always in [0, 1].
  `overall_coverage` is the mean over every persona, not the page, and
  `null` without personas. `feature_overlap` lists the features two or
  more deliverables reach (a deliverable reaching one through a journey
  and a module counts once), most shared first, ties by id; its `count`
  is `total`.
- **`see_also` is followed from its source, and counted either way.**
  `term_graph` follows a term's own `see_also` references breadth first
  (a term listing another relates to it; the other need not list it
  back), within `--max-hops` (default 1, above 5 counts as 5, 0 relates
  nothing, a value that is not a count is `INVALID_INPUT`).
  `term_clusters` and `term_density` read a reference either way, as
  `pe_query_term_clusters` says: a term's connections are the terms it
  links to or is linked from, each once. `total_see_also` counts each
  source and target pair once, and `avg_connections` is that over the
  terms, the formula `pe_query_term_density` gives (so a hub has more
  than twice it). A reference to the term itself, or to a non-term, is
  none. Clusters are numbered from 1 in their order.
- **The date commands read one date and refuse a bad one.** `--as-of`
  (else the host's `today`) must be a `YYYY-MM-DD` day the calendar has;
  anything else, or no date at all (a host passing none), is
  `INVALID_INPUT` (exit 2), so a command never guesses a clock. A
  milestone's `target_date` or `start_date` that is not such a date is
  read as absent: the milestone is undated on the timeline and never
  overdue. Due on the as-of day is not overdue. `milestone_velocity`
  counts the days from `start_date` (else `target_date`) to the as-of
  date, 0 before it, and its `days_remaining` is the features not done
  at the done-per-day pace, rounded up (0 when none is left, `null`
  without a date or a pace). `weighted_milestone_completion` weighs an
  effort outside the scale as m, as a missing one; its breakdown counts
  features per effort level they have, smallest first.
- **A command is declared with its handler.** The SDK's `ContributionsBuilder::command`
  (and `mcp_tool`, `mcp_resource`) takes the declaration and the function that answers
  it; the SDK derives the `surfaces` payload and routes the export (`cmd__<prefix>_<id>`
  under `command_prefix`, else `cmd__<id>`) to the handler, so a declared command cannot
  lack its code. A handler reads its args through `CommandCall`, which reads only declared
  args, as their declared type, after the SDK has checked the caller's values: a required
  arg missing or a value of another type (a page arg that is not a count) is
  `INVALID_INPUT`, exit 2, the same error object. A missing entity id is therefore
  `INVALID_INPUT` rather than `ENTITY_NOT_FOUND` for `''`. Product's 40 commands left
  `describe_surfaces.json` with their payload unchanged byte for byte (a pinned
  fingerprint), and the empty files of the other builtins went with it. Product's closed
  filters (`--status`, `--priority`, `--artifact-type`, `--technical-level`,
  `--interaction-model`) and `--sort-order` are `one_of` their values (`--family` stays an
  open `string`): the SDK refuses another value with the message the command gave, so the
  hand-written checks went, and the CLI refuses it first (clap's usage error, exit 2, listing
  the values; under `--format json`, wherever it is on the command line, any usage error clap
  catches is instead the same `INVALID_INPUT` object, `{code, message, suggestion?}`, the
  message the SDK's for a `one_of`, so an agent never parses prose), and the MCP tools'
  schemas carry the enum. That is the one change to the payload (the pin says so), and the drift test checks each `one_of` against the value rule
  of its field. An absent arg takes its declared default on every surface (the host applies
  it on both since ADR 0017, and the SDK by the same rule); a declaration that contradicts itself (a required arg or
  a flag with a default, a required flag, a default its type refuses, two surfaces with one
  export) panics when the extension is built, so its first test finds it.
  `testing::call_every_command` runs every command with every arg set, which a test uses to
  catch a handler reading an arg its command does not declare (a panic, a trap in the host).
  The surface machinery is reached only through a declaration, so a guest that declares no
  surface does not link it. On the wire an unset
  optional surface field is absent, not `null`. Raw JSON (`raw_category("surfaces", ..)`)
  still overrides the builder, for an extension not written with it.

## One operation runs a command (amendment, architecture round 5, plan 09)

Both surfaces assembled a run from five public pieces of `specforge_ops::command`
(`CommandContext`, `CommandContext::now`, `evidence`, `command_input`, `run_command`), in one
order, and each new input landed in both: the format and the date (11586f35, 0635e435) and the
evidence (741d474c, ADR 0039). The CLI built its project view by hand with `ProjectView::new`,
the constructor ADR 0015 keeps for tests and graphs built in memory, over a graph built without
the checks. And the two sent different roots: the CLI the canonical path, MCP the served root as
given, so `specforge mcp .` told every command `cwd: "."`.

- **O1. `specforge_ops::command::run(view, command, given, format)` runs a command**: it
  normalizes the given args by the one arg rule (a refusal is `RunError::Args`, the export not
  called), takes the project root from the view (`RunError::NoProject` without one), computes the
  evidence from the view's recorded report, reads the host's date (UTC), renders the view's graph
  as the graph export and calls the `cmd__` export in the runtime the view's extensions were
  loaded in; a failed call is `RunError::Call` (E028). `RunError::to_command_error` is the error
  object a command writes for each failure. Nothing else of the run path is public.
- **O2. The surfaces read and render.** The CLI builds clap from the derivation, routes, says why
  a refused or shadowed command does not run, prints the output and exits with its code; a failure
  goes to stderr in the format asked for (the error object under `json`), exit 2 for refused args,
  else 1. MCP reads the tool's arguments, shapes the result (the structured object, the `isError`
  object, or text) and records `surface_command_dispatched` with its duration; refused args are the
  command's `INVALID_INPUT` object, a failed call the structured MCP error carrying its E028 (ADR
  0013 D4).
- **O3. The CLI runs a command over its compiled project.** Having routed on the project's
  environment, it compiles from that same environment (`CompiledProject::of`): the sources are read
  and the graph built and checked, as MCP's served session's are, and the view is
  `ProjectView::of`. This supersedes ADR 0008's "(without the checks a compile runs)": the checks
  cost about 0.3 s of a 1 s command in a debug build on this repository, for one kind of view on
  both surfaces; nothing is printed from them.
- **O4. `cwd` is the project root, absolute and canonical** (symlinks resolved; as given when that
  fails), on both surfaces. Under MCP it was the served root as given.
- **O5. A refused command is a routing matter.** No surface routes it (the CLI's command line
  omits it, MCP's table never promotes it), so `run` does not check it again.

Reopen if a third surface runs commands with an input of its own, or a command needs an input the
project view cannot give (ADR 0008's trigger).

## What would reopen this

- A command that must write files, stream, or read beyond the graph and the date (ADR 0008's
  trigger, unchanged).
- A project that needs other effort weights, or a second extension wanting settings (then a
  manifest-declared config schema and its block passed to commands and passes).
- An extension resource whose content depends on the graph (then the host passes the graph to
  `mcp__` resources as it does to commands).
- A consumer that pages a changing graph and must detect the change (then a graph revision in
  `CommandInput` and cursors).
- A need to hide single contributions (then `surfaces.disabled` in `specforge.json`).
- A host that can list a guest's routed exports (then presence is checked at load, E020).

## Amendment (round 9, plan 01)

"Only closed enums are validated" and the drift test: a closed filter's field is an `enum` of the same
`Vocabulary` (ADR 0071), so `check` refuses (E061) what the filter refuses; the drift test now walks every declared
vocabulary field and every closed filter. "An absent lifecycle status is its first value" is the status field's
declared default. The closed filters' help no longer lists the values (clap and the MCP schema list them).

**Amended by ADR 0086 (2026-10-14):** the command operation is `specforge_ops::extension::command`.
