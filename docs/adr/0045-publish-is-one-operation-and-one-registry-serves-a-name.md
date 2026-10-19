# Publish is one operation over the registry port; one registry serves a name

**Status:** accepted (2026-10-08). Amends ADR 0010 (the port's methods) and ADR 0036 (D3). Amended in round 16 (plan 11): one credential store, one read rule.

`specforge publish` was decided in the CLI: it found and read the binary (E040), checked the
declaration, refused an unscoped name or a non-SemVer version (E072), read the registry
configuration, chose the registry (R-OPS-001), chose the credential, loaded or created the signing
key and uploaded. `specforge_ops::publish` held only helpers. The fetch side chose its registry by
another rule: with no scope match and no default entry, `HttpRegistry` asked the first entry, while
publish refused. So `add @other/x` asked a registry scoped to `@acme`, against
`support_private_registries`. On the same path, `login` stored a token under the alias it was given
(`"default"` unless `--registry`), not the alias of the registry it validated, so publish found no
credential, created the user's signing key, and uploaded unauthenticated into the server's 401.

## D1. One registry serves a name

The registry for a package name is the first `registries` entry whose `scope_filter` is the name's
scope, else the first entry marked `default_registry`. With neither, no registry serves it, and
the operation refuses with R-OPS-001, naming the scope, before any request. `add`, `update`,
`publish` and MCP `add_extension` all ask through
`specforge_ops_registry::Configured::registry_for`; the client chooses nothing (ADR 0036 D3), so
`find_registry_for` is gone from it. Rejected: falling back to the first entry (it sends names to a
registry scoped to another owner, and the spec defines the default as the marked entry), and
treating a lone unmarked entry as the default (a second meaning of "default"; I003 already says
none is marked).

## D2. The `Registry` port publishes

The port has three methods: `versions`, `fetch`, `publish(&Upload) -> Published`. A package
registry lists, serves and accepts packages, and one configuration and one rule decide all three.
A separate publish trait was rejected: each adapter (HTTP, the in-memory one tests use) would
implement both on the same type, so the second trait would add a name and no variation.

## D3. Publish is one operation; the adapter holds what needs the user's files

`specforge_ops::publish::publish(extension, &dyn Registry, &dyn WasmRuntime) -> PublishReport`
refuses in one order, each refusal before anything after it is read or asked: the binary (E040,
E028), the declaration's errors, the name and version (E072), then the registry. The HTTP adapter
refuses, still before any request: the configuration (E063, E067), the registry for the name
(R-OPS-001), the credential (R001 when there is none, R012, R-AUTH-020, R-AUTH-021), the signing
key (E074); then the registry answers (R007, R001, R002, ...). `SPECFORGE_REGISTRY_TOKEN`, when set
and not blank, wins over the stored credential. The adapter reads it and `~/.specforge` through
`specforge_ops_registry::User`, so ops still links no HTTP, keyring or ed25519 (ADR 0010). The
report carries the declaration's warnings and whether the registry was asked, whatever the result.
The CLI only presents it.

## D4. A published version is immutable

The registry refuses a version it holds (R007). The client's `force` flag, never settable, and its
pre-upload existence check, which let every error but "not found" through, are gone.

## D5. A credential is kept under its registry's alias

`login` validates against the entry `--registry` names, else the default registry, and stores the
credential under that entry's alias; an unknown `--registry` is E063. `logout` without
`--registry` forgets the default registry's credential of the project at `--path`.

## D6. MCP does not publish

A published version cannot be withdrawn, it is signed with the user's publisher key (created on
the first publish, then pinned by every consumer), its credential is a secret MCP never handles,
and its subject is an extension binary, not the project MCP serves. An agent's mistake there has no
remedy.

## Consequences

- `add`/`update`/`add_extension` of a name no `scope_filter` matches, with no default, refuse
  R-OPS-001 without a request. A configuration with one unmarked registry must mark it
  `"default_registry": true`; I003 says so.
- `publish` with no credential refuses R001 before any request and creates no key; an unreadable
  `credentials.json` without the environment token refuses R012; an unusable signing key is E074
  (it was the uncatalogued `SIGNING_KEY_ERROR`). Its output names the registry it published to.
- `login` without `--registry` stores under the default registry's alias, not `"default"`.
- MCP `add_extension` reports a registry's 401/403 as `permission_denied`, R007 as `conflict`.
- Publish's refusal order and its credential rule are tested in process, over the in-memory
  registry and a recording client.

## What would reopen it

A registry protocol that routes by something other than the scope (a per-package registry entry),
which would grow the rule, not add a second one; for D6, a protocol step through which the server
can require the human's confirmation before an irreversible act (MCP elicitation), and even then
with a dry-run default.

## Amendment (architecture round 5, plan 16): where a credential comes from, and which requests carry it

- `specforge login` keeps a registry's token one of three ways, per alias: a secret in the OS keyring (`--token`;
  a 0600 file when there is no keyring), or a reference to an environment variable (`--token-env`) or a file
  (`--token-file`). A reference resolves before any request (R010, R011).
- A publish authenticates with `SPECFORGE_REGISTRY_TOKEN`, else the kept credential, else refuses (R001), as D4
  says. Every read authenticates with the kept credential only: the environment token names no registry, and a
  search asks every registry.
- `logout --registry X` needs no `specforge.json` entry for X (D5 and 06 D6); the spec no longer says otherwise.
- The `Registry` port also searches (`Registry::search`), so `specforge search` reaches registries the way add,
  update and publish do (ADR 0010).

## Amendment (round 8)

D2's port also reads a version's published manifest without its binary (`Registry::published`), what a dry run judges
(ADR 0065 D3).

**Amended by ADR 0075 (architecture round 9, plan 05), D1:** a `registries` entry that can't be read refuses every
registry operation (E067) instead of being skipped, so the scope it claims never falls to the default registry.

**Amended by ADR 0086 (2026-10-14):** `publish` is `specforge_ops::extension::publish`, with `declare`.

## Amendment (architecture round 16, plan 11): an unreadable credential store is refused by every reader

`credentials.json` either reads, or every reader treats it as R012. Fetch, search and publish already refused it
(Consequences, above). `login` and `logout` read it as an empty store: login then wrote that store back over the
file and dropped every other alias's credential, and logout deleted the alias's secret and said `not_found`. `doctor`
read it as empty too and said "No issues found". Now:

- `login` reads the store before any request and refuses R012; the file is left as it is.
- `logout` reads the store before it removes an entry or deletes a secret, and refuses R012.
- `doctor` counts the unreadable store as one unusable credential (an error line; it exits 1).

No reader writes over a store it could not read. This parallels ADR 0075 D1's rule for an unreadable `registries`
entry. Login and logout stay in the CLI: ops may not name the credential store (ADR 0010).
