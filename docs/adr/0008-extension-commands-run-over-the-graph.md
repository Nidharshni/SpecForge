# Extension commands run over the graph the host passes

**Status:** accepted (2026-10-02)

The `specforge product` subcommands (nine lists, six queries, `bulk-status`, `health`) were pure
reads of the graph, written in the CLI binary with the product kinds and fields named in core
(principle 2). MCP could not reach them: its `specforge.list` was a weaker copy. ADR 0003 left
the CLI's half of extension commands as a known gap, since no builtin contributed one; ADR 0007
listed the product module. Both close here: the queries are `@specforge/product`'s commands.

## The contract

- **An extension declares a command** in its surfaces (`id`, `title`, `description`, `export`,
  `args`). The export, `cmd__<ext>_<id>` for the builtins, receives the SDK's `CommandInput` (since
  [ADR 0013](0013-typed-extension-calls.md) the protocol type `specforge_protocol_types::CommandInput`,
  same fields and defaults):
  `args` (what the caller set, typed as declared), `cwd` (the project root) and `graph`, the
  compiled graph in the graph export's shape (`specforge_emitter::emit` with the export's options: entities by
  id, edges by source, target, label). It answers with `CommandOutput` (`exit_code`, `stdout`,
  `stderr`). It reads no files, so one call serves both surfaces. Commands are queries over a
  compiled project; one that needs more than the graph is a new decision.
- **The CLI routes** any first argument that is not a built-in command (clap's external
  subcommand) to the extension of that short name (`ext_short`, else the last segment of its name):
  `specforge product features`, or `product:features`, the form the product spec writes. It reads
  `--path` (default `.`) before parsing and loads only that project's environment (config and
  extensions) to route; it builds the command line from the declarations: a subcommand per enabled
  command, named by its id with `_` as `-`; a required arg is positional, in declaration order, any
  other (and every bool) a `--flag`; enum args take their values, integer args parse; `--path` and
  `--help` are the host's on every command, so a command declaring an arg of either name, or two
  args of one name, is refused (exit 2). Clap prints help and usage errors (exit 2). Only a matched
  command has the project compiled, from the environment that routed it (its checks run since
  ADR 0011, "One operation runs a command", O3).
  `specforge completions` adds the commands of the project in the current directory. The export's
  stdout and stderr are printed as returned, its exit code is the CLI's; a trap is an E028 on
  stderr, exit 1. `specforge_ops::command` holds what both surfaces share:
  the routing table (disabled commands left out), the short name, and the operation that runs a command (`command::run`, ADR
  0011 O1).
- **MCP** auto-promotes each command to `specforge.<ext_short>.<id>` (already built), and now
  calls the export with the same `CommandInput`, over the served session's graph.

## What moved

`crates/specforge-cli/src/product` is deleted, with `ProductAction` and the kind names in
`main.rs`. The queries and their rendering are `extensions/product/src/{queries,commands}.rs`,
declared in `describe_surfaces.json` (now in code, each with its handler: ADR 0011, Adjustments); the duplicate `get_field` helpers are the SDK's
`GraphNode::text`, and the nine compile-query-print bodies are one `render`. The command names,
flags, defaults (`--format human`) and output are those of the built-ins, with two differences:
help comes from the declarations, and `feature-impact` lists referencing entities by id rather than
in edge-insertion order (milestone features keep the order the milestone lists them).

MCP `specforge.list` stays the domain-free lister of any kind and gains `where` (field to value,
exact), `offset` and `limit` over the id-sorted entities. Status and priority filters are the
product's tools' (`specforge.product.features`).

## What would reopen this

A command that must write files, read beyond the graph and the date (test reports, the build
cache), or stream: `CommandInput` would need more than the graph, the format and today's date
(ADR 0011 added those two), and the sandbox override the spec describes for
commands would then have to be enforced. Today it is declared, not applied: the host grants a
`cmd__` export no capability (its WASI context preopens no directory, passes no environment, stdio
or network), so there is nothing for an override to withhold. The command contract (the host's
`--format human|json`, the error object, offset/limit pagination, the date) is ADR 0011's; the
product surface it leaves to build (`--tags`, sorting, `has_more`, the 23 commands not yet built)
is the extension's to add, with no host change.
