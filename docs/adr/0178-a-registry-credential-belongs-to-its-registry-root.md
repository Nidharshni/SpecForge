# A registry credential belongs to its registry root

**Status:** accepted (2026-10-21, architecture round 20, plan 01). Amends [ADR 0045](0045-publish-is-one-operation-and-one-registry-serves-a-name.md) (D5 and the round-5 and round-16 amendments' credential sentences: a credential is kept under its registry's root, and login and logout go through `specforge_ops_registry::User`), [ADR 0010](0010-operations-reach-the-registry-through-a-port.md) (credential health is read through `User`; the store holds no OS keyring) and [ADR 0075](0075-a-registry-is-named-by-its-root.md) (the root also keys the user's credential).

## Context

- `~/.specforge/credentials.json` was keyed by a registry alias, and a project's `specforge.json` picks its aliases. A
  clone, or a project an MCP call names, that calls its registry `main` therefore got the token the user validated
  against another root, and `add`, `search`, `update` and the MCP `add_extension` dry run sent it to the URL that project
  names. Two entries with one alias in one project (W140) shared one credential too. Reproduced over the CLI and over MCP
  with a listener that logged the `Authorization` header of each request.
- The rule "which key a user credential is kept, read and removed under" was spelled in six modules (the store, the
  secret backend, login, logout, doctor, the fetch adapter) and 11 call sites outside the store; `User`, the module that
  was to own the user's files, was three path joins. Six commits in four crates had moved the same rule.
- The documented "OS keyring" was keyring-rs's in-memory mock in every build (`keyring = "3"` with no platform
  feature): the secret always landed in the file fallback. A kept token whose secret file was gone resolved to an empty
  token and was sent as `Bearer ` with doctor silent. The store read `$HOME` itself, so the suite deleted a secret of the
  developer who ran it.

## Decision

- **D1. The key is the registry root.** `credentials.json` maps a `RegistryRoot` (ADR 0075) to its entry. The alias is
  project input and never a key.
- **D2. A credential goes only to its root.** `RegistryCredential` carries the `RegistryRoot` it was kept for; the HTTP
  client sends it only to a registry whose `url` is that root (and, for a download, the same origin), and drops it
  otherwise. Roots, not origins, are compared: two roots on one origin are two registries.
- **D3. The user's registry files are one module, `specforge_ops_registry::User`.** It reads the directory it is given
  (`User::current()` is `$HOME/.specforge`, the one place HOME is read for registry files) and owns the store, the
  secrets, the signing key and the pins' path. Fetch, search, publish, login, logout and doctor ask it; the CLI parses and
  prints. This replaces the round-16 sentence "login and logout stay in the CLI". Ops still never names the store (ADR
  0010): the module is an adapter.
- **D4. A kept secret is a 0600 file** `secrets/<first 16 hex of SHA-256 of the root>.json` in the user's directory, as
  it already was in every build. The keyring dependency and its mocks go. A secret that is missing or unreadable is
  R-AUTH-021, before any request.
- **D5. `logout --url <ROOT>`** forgets a root's credential with no project; `logout --registry <ALIAS>` resolves the
  alias through the project at `--path`.
- **D6. An existing file.** An alias-keyed `credentials.json` does not parse (a key that is not a root), so every reader
  refuses it with R012 and its suggestion to delete it; the user logs in once more. Old `secrets/<alias>.json` files stay
  on disk, unused; delete `~/.specforge/secrets/`.
- Messages name the root; the re-login suggestion keeps `specforge login --registry <alias>` as a template. W140 stays a
  warning: with roots as keys a duplicate alias no longer shares a credential. Key pins stay per package name (ADR 0061).

## Rejected

- Keeping the alias as the key and recording the root in each entry: one credential per alias across every project (a
  login in one project overwrites another's), and the alias stays a key the project picks.
- Login and logout staying in the CLI with only the key changed: the store rules stay in two crates and untested in
  process.
- `logout --registry` taking an alias or a root: an alias that happens to be a URL is ambiguous.
- Making the keyring real (platform features and a `Secrets` seam): new platform dependencies and a Windows path nobody
  can verify here, for secrets at rest that a 0600 file already holds against other users.
- Key pins per root: a second registry serving a name would start a fresh trust on first use.

## What would reopen it

A user asking for OS-protected secrets at rest is the second adapter that makes a `Secrets` seam real. A registry
protocol that names a registry by something other than its root would change D1.

## Amendment (round 20 review): platform

The 0600 file and 0700 directory of D4 are unix modes. On Windows `write_private` sets nothing, so a kept secret relies on
the user profile directory's default ACL, which is private to the user. No ACL is set; nothing here can verify one on
Windows. Open until a Windows user asks for more.
