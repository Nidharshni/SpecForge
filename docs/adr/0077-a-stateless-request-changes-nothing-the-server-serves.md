# A stateless request changes nothing the server serves

**Status:** accepted (2026-10-13, architecture round 9, plan 07). Amends ADR 0014 D5 (handshake sessions only),
D6 (the root is looked at again while nothing is served) and D12, supersedes ADR 0014 D11 and ADR 0004 D3-d's
`use_cached`, makes ADR 0004 D4-c's stateless clause precise, and amends ADR 0024 D5, ADR 0033 D1/D3 and ADR 0049 D2
(the target declares `path` only). D3's `follow_root` is called once, by the
router (ADR 0103).

The stateless revision (MCP 2026-07-28, served since 2026-10-01 under ADR 0004 D4-c) promises that nothing a prior
request said changes a request's answer (`mcp-server.spec`, `serve_stateless_mcp_requests`). Five things a request
did broke it. A `path` naming a project while nothing was served was adopted (ADR 0014 D5, written for the
handshake era), so a `validate {path}` changed what every later `resources/read` answered; `init` served the
directory it created; the first stateless request looked for the server's project once and remembered the answer,
so a project created at the root afterwards was never served, though a fresh server serves it (a handshake session
had the same defect after `initialize`); `use_cached` answered with whatever an earlier request had compiled; and
the no-project refusal told a stateless client to "call a tool that takes a `path` first", advice that only worked
by breaking the promise.

## Decision

- **D1. The request's revision decides whether a call may change what is served.** `target::resolve` is told the
  revision and derives `Serving`: `Adopts` for a handshake revision, `Fixed` for the stateless one. It is the one
  place the decision is made; `init`'s completion and the no-project refusal read it from the call target.
- **D2. A stateless request serves nothing it names.** Its `path` is opened as a session for that call only (a
  mutation acts on that session and brings it up to date after it writes, ADR 0047); `init` serves nothing it
  creates; its no-project refusal says "start the server in a project (`specforge mcp <root>`), or create one at
  its root". In a handshake session ADR 0014 D5 holds as amended.
- **D3. What is served follows the server's root on disk.** The server's root is the path `specforge mcp` was
  started with, replaced by a handshake client's `projectRoot`. While no project is served, every request looks
  for the nearest project at or above it before it reads anything (`McpState::follow_root`, called by the
  stateless entry and by the router before every session method), in both revisions. The `served` flag is gone.
- **D4. `use_cached` is gone.** `specforge.validate`, `specforge.analyze` and `specforge.doctor` bring the project
  up to date with disk on every call, as every other tool does; a call that sends `use_cached` is refused as an
  unknown argument (ADR 0033 D3). Freshness applies exactly what changed (ADR 0006, ADR 0014 D1: a stat walk on an
  unchanged project), so the option saved nothing but returned a stale answer.
- **D5. A connection that sent `initialize` is a session.** A handshake request (`initialize`'s `projectRoot`, an
  adopted `path`, `init`, `shutdown`) may change what is served, and a stateless request on the same connection
  reads it. The stdio transport carries one client (ADR 0062); a client that initializes has opted into the
  session.

## Consequences

- User-visible over MCP 2026-07-28: a `path` is never adopted and `init` never serves; a stateless answer is a
  fresh server's on the same disk, whatever stateless requests came before (a test holds it).
- User-visible in both revisions: a project created at or above the server's root is served by the next request;
  `use_cached` is an unknown argument; the `path` property reads "Project root path (the served project if
  omitted)"; `specforge mcp --help` describes the root accordingly.
- A stateless client whose server's root holds no project reads prompts and resources over the empty session; it
  names a project per call only through a tool's `path`.
- Each request while nothing is served costs one `canonicalize` and an ancestor walk.

## Rejected

- **ADR 0014 D5 in both revisions**: stateless answers would depend on request order.
- **No adoption in either revision**: a handshake client whose server starts outside any project could never read
  a prompt or resource, which take no `path`.
- **Stateless requests read the start root even after `initialize` named another**: the extension surface table is
  the served session's, so it needs a table per request, for a mix no conforming client sends.
- **`use_cached` listed only for handshake requests**: a revision-dependent schema and argument set for an option
  that saves a stat walk.
- **No served session for stateless requests (compile per request)**: correct but a full compile per request; the
  served session brought up to date gives the same answer (ADR 0006).
- **Looking at the root inside `ensure_fresh`**: `resolve` compares a `path` with what is served before it brings
  anything up to date, and core-resource subscriptions and listen streams never call it.

## What would reopen it

- A revision of MCP that gives stateless requests a session (a session id carried per request).
- A client that needs prompts or resources over a project other than the root's under the stateless revision.
