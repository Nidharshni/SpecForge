# MCP serves projects from disk; its tests declare their extensions

**Status:** accepted (2026-10-06)

MCP could serve a project three ways: a session opened from disk, a graph handed to it with an
environment (`McpState::serve_graph`, `serve_session`), and either of those edited in place
(`edit_graph`, `edit_environment`, `serve_in_memory_at`). Only the first had a production caller.
The other five existed for tests, and the server paid for them in production code:

- `Call::project` kept a lazily built runtime for a served project that had none
  (`in_memory_runtime`).
- `Call::wrote` and the dispatcher replaced an in-memory project with the project on disk at its
  root after a mutation, a third refresh path next to `ensure_fresh` and a recompile.
- outline and the navigator looked a file up relative to the server's working directory when the
  project had no root, so with nothing served, outline answered `[]` for any file that happened to
  exist there.

The tests paid as well. 92 calls in 15 files wrote hand-built graphs and registry literals
(`KindRegistryEntry`, `FieldRegistryEntry`, `ValidationRulePattern`) into the served environment.
About 176 test call sites served a graph with no root and so exercised the no-project fallback instead of what a
client gets. Some served a graph their own project directory contradicted. Adding one registry field
(`fc2c2ca5`) or embedding descriptors in registry entries (`8b9b58ba`) each changed ten test files
across MCP and the LSP and no MCP source.

## Decisions

- **D1. MCP serves only a project opened from disk.** *(Round 6 plan 12: a project is a directory
  holding `specforge.json` or `specforge.spec`, found at or above the root the server is given; a
  directory that is no project is never served, and `McpState::serve` asserts it in debug builds.)*
  `McpState::serve` (initialize, adopting a call's
  `path`) is the one way a project becomes served. The five in-memory methods, the lazy runtime,
  the in-memory refresh arms and the working-directory fallbacks are deleted. A served session always
  has a root and a runtime (`Call::project` refuses one that has neither as no project). With nothing
  served, tools that answer without a project read the empty session, and a read that names a file or
  an entity is refused as no project (`target::without_project`), never as not found. The dispatchers
  of tools, prompts and resources apply that rule to what their handler refused with.
- **D2. MCP's tests serve declared extensions.** A test declares `@test/ext` with the SDK builders
  (`tests/support::TestExtension`), writes its sources and `specforge.json` into a temporary
  directory (`TestProject`), and serves them through the real `initialize` with an
  `InProcessRuntime` (`McpState::extension_runtime`, the `WasmRuntime` port's test adapter), or with
  the project's own component runtime for builtins and installed extensions (`serve_components`).
  What a test serves is what its sources and its declarations make. A diagnostic a test needs either
  has a real cause in its sources or is answered by a `check`-phase pass of its extension.
- **D3. One request-helper set** (`tests/support::rpc`): `call`, `call_tool`, `tool`, `tool_json`,
  `tool_text`, `get_prompt`, `prompt_payload`, `read_resource`, `resource`, `events`.
- **D4. `ProjectSession::from_graph` and `Origin::InMemory` are deleted.** A session is opened from
  disk or detached.
- **D5.** The LSP's tests build the registries they pass to hover, completion and semantic tokens from
  an SDK declaration through `build_registries`.

## Consequences

- A change to the registry's entry types, to `ValidationRulePattern`, or to how an environment is
  loaded changes no MCP test. The SDK builders and the build absorb it.
- With nothing served, a read that names a file or an entity (outline, inspect, the find tools, a
  scoped export, the context prompt, `graph/{id}`) is the no-project refusal, `precondition_failed`,
  not `file_not_found`/`entity_not_found`; reads of the whole project answer over the empty session
  (user-visible). A prompt or resource refuses with -32603, its `McpError` as the error's `data`; a
  resource's not-found refusal now carries its `McpError` (`entity_not_found`) as `data` as well.
- Amends ADR 0014 D7 and D10, and ADR 0017 D14 (in-memory serving is gone).
