# The project a path is in is one value, located by one rule

**Status:** accepted (2026-10-12). Amends ADR 0029 D6, ADR 0014 D5 and D6, ADR 0025 D1.

A path became a project four ways. Seventeen CLI commands, `check`, `watch`, the extension commands and the
completions compiled the path as given; format, collect and migrate (three more times inside
`specforge-migrate`) walked up with `project_root_of`; MCP walked up over the canonical path; the LSP walked
up lexically (`enclosing_project_root`); add, update, search, login, logout and publish read `specforge.json`
at the path as given. `cd p/spec && specforge check` passed with I002 on a project with errors, `check nope`
exited 0, `add` from a subdirectory found no project, and `check --help` called the path "the spec root
directory". A second marker, `specforge.spec`, made a directory a project with the default configuration,
because it carries none: the repository's own `spec/specforge.spec` made `spec/` a configless project for MCP,
the LSP and format.

## Decisions

- **D1. `ProjectRoot` is the one rule.** `specforge_common::ProjectRoot::of(path)` locates the project any
  path inside it is in: walking up from the path's directory, symlinks resolved, the first directory holding
  `specforge.json`. The compile, the session open, the environment load, add, update, format, migrate and the
  registry configuration take a `ProjectRoot`, never a raw path, so no surface can choose its own rule;
  `project_root_of`, `find_project_root`, `is_project_root` and `enclosing_project_root` are deleted. A session
  keeps the root it was opened at.
- **D2. Only `specforge.json` marks a project.** `specforge.spec` carries no configuration; as a marker it made
  a configless project inside the real one. It is a source like any other.
- **D3. No project above a path: a loose root.** The path's own directory, with the default configuration;
  its I002 names it and suggests `specforge init`. What needs a configuration refuses as before (`add`,
  `update`, `remove`: `config_not_found`; `collect`: `no_project`).
- **D4. A path that does not exist is `file_not_found`** on every surface, never an empty project. A file path
  names the project of its directory.
- **D5. Spelling.** The root is spelled as the caller's path spells it where that spelling reaches it, else
  canonically; equality is canonical. MCP serves and compares canonical roots (ADR 0014 D6); the LSP keeps
  the editor's spelling.
- **D6. Only `init` takes its directory as given** (it creates the project there). An extension crate's
  directory (`new`, `extension init|build|validate`, `publish`'s extension) is not a project path.
- **D7. One help text.** Every command whose path names a project says "A path inside the project (...)"; a
  test over the CLI's command tree enforces it.

## Consequences

- User-visible: `specforge check <spec root>` and every command run from inside a project act on the whole
  project: CI that passed a spec root now gets the project's diagnostics and fails where it used to pass.
  `check nope` refuses `file_not_found`. `add`, `update`, `search`, `login`, `logout` and `publish` read the
  enclosing project's `specforge.json`. A directory holding only `specforge.spec` is no longer a project:
  MCP does not serve it, init proceeds there, collect refuses it. `export` from a subdirectory writes the
  schema cache at the project root. Paths in output (a collect refusal, E045) are the resolved root, absolute.
- The CLI, watch, the LSP and MCP answer the same for the same project from any directory inside it.

## What would reopen it

A surface that must act on part of a project (a file-scoped check), or a second project marker that carries
configuration.
