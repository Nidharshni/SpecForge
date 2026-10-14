# A path an MCP request names resolves inside the served project

**Status:** accepted (2026-10-14, structure round, plan 13). Amends ADR 0014 D4, D5 and D6 (what "another
project" may be) and ADR 0029 D8 (`out_dir`).

`specforge mcp .` in `proj/`, then `tools/call specforge.format {"path": "../outside"}`, answered `ok` and
rewrote `outside/x.spec`. The server serves one project, and a client that can name any directory could read
or write any directory the user could. A handshake client names its own `projectRoot`; every other path in a
request is the server's to bound. Nothing did: `path` opened any directory as "another project" (ADR 0014 D4)
and resolved a relative one against the process's working directory, `format`'s `paths` were joined onto the
root and walked, `render`'s `out_dir` was used as given when absolute, and `init` created a project wherever
it was told. Only `infer_session` refused (`source_outside_root`, `source_root_outside_root`).

## The audit

Every argument of a tool, prompt or resource that names a file or directory, and what it reaches.

| Argument | Entry | Reaches | Contained before | Evidence |
|---|---|---|---|---|
| `path` | validate, analyze, collect, format, migrate, rename, add_extension, remove_extension (reach `AnyProject`) | opens any directory as a project; format, rename, migrate, add/remove_extension write it | no | `every_tool_with_a_path_refuses_one_outside_the_served_project` (each of the 8 answers ok, format rewrites) |
| `path` | every other tool, prompt and resource (reach `Served`) | refused as `OtherProjectRefused` | yes | `call_target.rs`, "a tool of the served project only takes no path to another project" |
| `path` (relative) | the 8 above | read against the process's directory, not the root | no | `a_path_inside_the_served_project_is_accepted` |
| `path` | init | creates and writes a project anywhere but inside the served one | no | `init_outside_the_served_project_is_refused` |
| `paths` | format | each joined onto the root (an absolute one replaces it), directories walked | no | `format_paths_outside_the_served_project_are_refused` (relative, `..`, absolute, symlink) |
| sources of the project | format, rename, migrate | a symlinked source is read and written through | no | `format_does_not_write_through_a_symlink_that_leaves_the_project` |
| `out_dir` | render | absolute as given, else joined onto the root; `create_dir_all`, writes `graph.json` | no | `render_out_dir_outside_the_served_project_is_refused` |
| `file` | outline | `spec_root.join(file).exists()`: tells whether a file outside exists | no (read-only) | `outline_of_a_file_outside_the_spec_root_is_refused` |
| `source_roots`, `source_file` | infer_session | lexical (`..`, absolute and drive paths refused) | yes, lexically | `infer_session_source_paths_outside_the_project_are_refused` |
| `source_file` | infer_session `mark_analyzed` | hashes `root.join(file)`, through a symlink | no | `infer_session_marks_no_file_behind_a_symlink_that_leaves_the_project` |
| `file_path` | find_spec_for_source, suggest_fixes | compared as text against the manifest or a diagnostic's file; no file access | yes (no access) | by reading `anchors_of_file` |
| `scope` `file:{path}`, `target_spec_directory` | prompts/infer | looked up as a graph key; echoed in the plan; no file access | yes (no access) | by reading `prompts/infer.rs` |
| `specifiers`, `extensions` | add_extension, init | a local `.wasm` or crate directory is read, then installed; `git+` and registry specifiers fetch | read, not written | D7 |
| `uri` | resources | an entity id or kind in a template; no file access | yes | `resources/mod.rs` |
| `projectRoot` | initialize | the client's chosen workspace: it sets the boundary | by design | ADR 0014 D6 |

## Decisions

- **D1. One module owns the rule.** `specforge_mcp::containment`: `Containment` holds a canonical directory and
  `resolve(named)` returns where a name lands, or says it leaves. A relative name is read against that
  directory, never the process's. The result is canonical as far as it exists (`..`, a symlink and an absolute
  name cannot escape; a directory about to be created is its nearest existing ancestor, joined with the rest).
  `Containment::of_state` is the boundary of a request. No tool, prompt or resource compares paths itself.
- **D2. Two directories bound a request.**
  - The boundary of the request: the served project's root; while nothing is served, the server's root (the path
    `specforge mcp` was started with, or the client's `projectRoot`); with neither (a server with no root), no
    boundary, as before. A `path`, and the directory `init` creates, resolve inside it.
  - The call's project: a `paths` entry, `out_dir`, `file` and every file a mutation reads or writes resolve
    inside the root of the project the call acts on.
- **D3. "Another project" is a project inside the boundary.** A `path` that names a project nested in the served
  one (ADR 0080) is opened for the call only, as ADR 0014 D4 says. One outside the boundary is refused. A
  handshake session with no root and nothing served still adopts the project its `path` names (ADR 0014 D5):
  nothing bounds it yet, and what it adopts bounds every later request.
- **D4. `init` creates a project inside the boundary and never inside the served project.** With a project
  served, `init` is therefore refused; it creates the first project of a session.
- **D5. The refusal is `invalid_input`**, naming the argument, with a message that says what the name resolves
  to and what bounds it. It is the code `infer_session` already refuses a source path with. No new diagnostic
  code. An ops operation that gets a path it cannot trust keeps its own refusal (`source_outside_root`).
- **D6. The CLI keeps outside paths.** `specforge format ../x`, `check --path`, `render --out-dir` act for a user
  who is already at the file system. Only MCP, where a client the user did not type names the path, is
  bounded. The LSP names files by the editor's own URIs and is not changed.
- **D7. Left open, by decision.** A local extension (`.wasm` or crate directory) a client names in
  `add_extension` or `init` is read from where it is (a relative one from the project root); it is never
  written, and its bytes are checked and run in the sandbox. Installing from a sibling directory is how
  extensions are developed. Whether MCP may install from outside the project, and from `git+`, is a separate
  decision (round README §9).
- **D8. Symlinks.** A mutation neither reads nor writes through a symlink that leaves the project. `format`
  reports such a source as failed (`invalid_input`) and formats the others; `rename` and `migrate` refuse the whole
  call (`invalid_input`, nothing written); `infer_session` does not hash one (`source_outside_root`). The
  operations take the bound as an argument (`format::run_within`, `migrate::run_within`,
  `specforge_ops::resolves_outside`); the CLI passes none. A symlink that stays inside the project is followed.

## Consequences

- ADR 0014's multi-project reach shrinks: `AnyProject` names the served project and the projects nested in it.
  Tests that opened a sibling directory as "another project" nest it instead.
- `Reach::AnyProject` and `Reach::Served` stay: they decide which entries take a `path`; D2 decides where it
  may point.
- The invariant `mcp_served_project_consistency` states the rule; its tests are `tests/path_containment.rs`.
