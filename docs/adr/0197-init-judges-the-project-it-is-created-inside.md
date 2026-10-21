# Init judges the project it is created inside

**Status:** accepted (2026-10-21, architecture round 21, plan 06). Applies [ADR 0080](0080-a-projects-sources-end-at-a-nested-project.md) D1 and D2 at creation; amends [ADR 0053](0053-the-project-a-path-is-in.md) D6 (init takes its directory as given, and judges the project it lies inside); notes [ADR 0092](0092-a-path-an-mcp-request-names-resolves-inside-the-served-project.md) D4.

## Context

Since ADR 0080 a project's sources end at a nested `specforge.json`. `specforge init` in a directory below an enclosing project's spec root therefore took every source there away from that project: its next `check` reported E003 on references that resolved a moment before, while init printed "the new project is separate" and exited 0. Init at the enclosing spec root did the opposite: the enclosing project read the new starter as one of its own sources (a second `spec` block, no diagnostic). Over MCP with nothing served the same happened, silently. Three modules ruled on "init inside a project" with three answers: ops `init` checked only its directory, the CLI added a person-only note after the write, and MCP refused only inside the served project.

The case commit `47080b04d` fixed (a stray `~/specforge.json`) is refused only when the directory holds `.spec` files that stray project reads; those files would then be no project's source.

## Decision

- **D1.** Init finds the enclosing project (the nearest directory above its own holding `specforge.json`) and judges it before anything is written, over the same `SourceRoot` the compile walks. It reads that project's configuration as its compile reads it (an unusable one is the default configuration).
- **D2.** It refuses, `inside_project_sources` (conflict, about `dir`, nothing written), when that project would read fewer sources (the directory lies strictly below its spec root and holds some) or the new starter (the directory, or its `spec/`, is that spec root). An `exclude` entry of the enclosing project leaves a directory out. The message names the first five sources and counts the rest.
- **D3.** The outcome names the enclosing project (`Outcome::enclosing`); the CLI note presents it, for a person only. With D2 the note's "the new project is separate" is true whenever it is printed.
- **D4.** MCP's served-project refusal stands as ADR 0092 D4's serving rule (a session's init creates its first project). Ops judges every other case, on both surfaces.

## Rejected

- A `--nested` option or `"nested": true`: a new option on both surfaces for a case `exclude` already covers.
- Answering with what the enclosing project no longer reads (a document field and a spec type): the enclosing project would stay broken, and a machine client would have to read a new field to notice.
- A document field for the enclosing project under refusal: it changes nothing for the client.
- Refusing because the enclosing configuration is broken: it would block an unrelated user.
- Judging inference sources (ADR 0113 ends them at a nested project too): nesting a project inside a code tree the outer one infers from is the monorepo shape, and the loss shows as gaps in the next `infer` run; judging it would need the inference configuration in init.

## What would reopen it

A workspace concept (ADR 0080's own reopener), or an init that creates its spec root elsewhere than `spec/`.

## Consequences

- Breaking: an init in a directory whose files an enclosing project reads, or at that project's spec root, exits 1 (MCP: `conflict` on `path`) instead of writing. The way out is an `exclude` entry or moving the files.
- No new diagnostic code (`inside_project_sources` is an operation code, like `starter_exists`); no blob input changes.
- Not judged: a project whose `spec_root` points outside its own root at the new directory (it is not found by walking up, ADR 0053 D1), and a directory created later inside the new project.
