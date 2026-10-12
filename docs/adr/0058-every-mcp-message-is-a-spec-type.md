# Every document MCP sends is one typed definition its spec type states

**Status:** accepted (2026-10-12, architecture round 7, plan 07). Extends ADR 0048 D8 (the reflection test)
from tool replies to every document MCP sends; applies ADR 0049 D1 to prompts; amends ADR 0024 D6 (what
the deltas carry) and the `initialize` result ADR 0004 left; extends ADR 0052 D5 to watch's lines.

ADR 0048 typed every core tool's reply and held it to `spec/types/mcp.spec`. Everything else MCP sent was
built with `json!`, and the reflection test skipped 22 spec types by name. Four of those documents had
drifted. `specforge/graphChanged` sent node IDs, so a client still had to re-read the graph the delta was
meant to spare it; `specforge/diagnosticsChanged` sent code, severity and message, so an agent could not
find a diagnostic a notification said was added; `initialize` sent 78-85 KB of listings no MCP revision
defines, outputSchemas a 2025-03-26 `tools/list` strips, and templates as resources, in a shape the spec's
`McpCapabilities` did not describe; the context prompt sent keys its spec type lacked, and the infer prompt
had no spec type, a `null` cursor and a fake file entry. The prompts kept the macro and fn-pointer table ADR
0049 removed from tools. `GraphDelta.changed_fields` mixed field names with the aspects kind, title,
methods and edges, while extensions declare fields named `kind` and `methods`.

## Decisions

- **D1. One rule for what the spec states.** A document whose content SpecForge decides is one type
  deriving `Serialize` and `Shape`, and its spec type states the same shape: a tool reply, a prompt
  payload, the `initialize` and `server/discover` results, the listing descriptors, the SpecForge
  notifications' params, `McpError`. A frame whose keys MCP fixes follows MCP's schema and is not restated.
  MCP's objects are stated under their wire names.
- **D2. A message names its spec type** (`specforge_mcp::message::Message`); `message::documents()`,
  `CORE_TOOLS` and `CORE_PROMPTS` are what the reflection test walks. Every `Mcp*` type of
  `spec/types/mcp.spec` is reached; no type is exempt by name. The guest-wire types
  (`McpToolContribution`, ...) are declared in `surface.spec` and `wasm.spec`: received from extensions,
  never sent.
- **D3. The deltas are sent whole.** `graphChanged`'s params are the update's `GraphDelta`;
  `diagnosticsChanged`'s are `DiagnosticsDelta`, each diagnostic as every surface presents one.
- **D4. A modified node's changes are fields and aspects, apart** (`changed_fields`, `changed_aspects`).
- **D5. `initialize` answers MCP's InitializeResult**: revision, capabilities, server info and the
  instructions `server/discover` gives; no listing.
- **D6. A core prompt is one module** implementing `CorePrompt` (name, description, spec type, typed
  arguments, typed payload, `render`); `CORE_PROMPTS` lists each once.
- **D7. Watch prints the delta** it applied, the one MCP notifies, under `--format json`.

## Consequences

- User-visible over MCP: `graphChanged` nodes are `{id, kind, file, line}` objects and modified nodes
  `{id, changed_fields, changed_aspects, file, line}`, with `affected_files`; `diagnosticsChanged` entries
  are whole diagnostics; `initialize` is under 1 KB and carries `instructions`; the infer prompt's payload
  is tagged by `scope`, its plan has no `plan` key, no `null` and no fake file entry; a prompt payload's
  keys follow its type's field order (a document that passes through a `serde_json::Value`, such as
  `McpError` and the notification params, still sorts its keys).
- User-visible on the CLI: `specforge watch --format json`'s `rebuilt` and `rechecked` lines carry `delta`
  in place of five counts, and no `verification: null`.
- `McpCapabilities` and `McpSubscription` leave the spec; `McpToolGroup` moves to `surface.spec`;
  `InferencePlan` and `InferenceKindPriority` leave `infer.spec`.

## Rejected

- **Keeping the listings in `initialize`, typed**: a second copy of the listing that drifted twice, for no
  reader.
- **IDs in graphChanged with a fixed spec**: the delta exists so a client need not re-read the graph (ADR
  0024, Rejected).
- **One `changed_fields` list with prefixed aspects**: a convention in strings over an open namespace.
- **A runtime schema check of prompt payloads**: MCP gives prompts no schema; the type is the payload.
- **Typing McpError's `data`**: eight unrelated producers' private payloads in the spec.
- **A spec type for watch's lines now**: the CLI has no reflection harness for its CLI-only documents
  (update's is the same case); the lines are typed and checked against their own derived schema.

## What would reopen it

An MCP revision whose InitializeResult carries listings; a client that needs counts on watch's lines; a
second CLI-only document worth a spec type (then a CLI reflection harness, and watch's lines with it).
