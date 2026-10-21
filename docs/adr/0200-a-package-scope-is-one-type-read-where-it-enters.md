# A package scope is one type, read where it enters

**Status:** accepted (2026-10-21, architecture round 22, plan 01). Extends [ADR 0036](0036-one-package-name.md) D1, [ADR 0075](0075-a-registry-is-named-by-its-root.md) D6 and [ADR 0185](0185-a-scope-is-owned-by-an-account.md) D3/D4. Amends none.

## Context

- A package scope (`@acme`) was read four ways as raw text. A `registries` entry kept its `scope_filter` as an unchecked
  `String` and the registry for a name was chosen by string equality, so a filter written `sdk`, `@sdk/`, `@SDK` or `""`
  matched nothing and said nothing: every `@sdk/*` name fell to the default registry, `specforge publish` sent a private
  package to the public registry with exit 0, and `add` and `update` installed `@sdk/*` from it (dependency confusion).
- On the reference registry, a token's `--scope`, the admin API's `scope` and `scope assign` were stored raw and matched
  by string prefix, so `''` (which the help called "full access"), `sdk` and `@sdk/` were issued with exit 0 or 201 and
  granted no write at all; `scope assign` validated its argument by building a fake package name.
- ADR 0036 D1 reads a package name where it enters; the scope, the one piece of package identity a registry and a token
  name, was left out.

## Decision

- **D1. `specforge_package::Scope`**: `@` and one name part, the rule `PackageName` already applies to the scope of a
  name. `PackageName::scope()` returns one, and `Scope::covers` is the only comparison (equality of scopes, no prefix
  rule) that `Configured::registry_for` and the registry's token filter ask.
- **D2. Every entry refuses text that is not a scope, never repairs it, and names the scope to write.** A
  `scope_filter` is an entry that can't be read: E067, every registry operation refused before any request (ADR 0075
  D6). `token create --scope` and `scope assign` are refused by the argument parser (exit 2), the admin request's `scope`
  by the handler (400 `BAD_REQUEST`). Repairing `sdk` to `@sdk` would keep two spellings alive and would widen a token
  that grants nothing today.
- **D3. No filter is every scope; `''` is not a scope.** A CI line `--scope "$SCOPE"` with `SCOPE` unset is an error at
  issue, not a full-access token.
- **D4. A token's filter is a scope, not a package name.** `--scope @org/soft` is refused with "write '@org'";
  ownership (ADR 0185) is per scope, so a package filter would narrow inside a scope the account owns anyway.
- **D5. The wire stays text; the handler parses.** Typing the wire field would make the extractor answer its own 422
  instead of the protocol's error envelope. SQL keys stay `&str`.
- **D6. A stored token whose scope is not a scope authenticates nothing** (401), and `token list` (the admin list: 500)
  refuses, naming the token id, the stored text and `specforge-registry token revoke <id>`, instead of dropping the row.
  Rejected: refusing to serve a data directory holding one, and revoking it at open (it changes the operator's data
  unasked). No migration (pre-release).

## Consequences

- Breaking: a `specforge.json` whose `scope_filter` is not a scope refuses every registry operation until edited (the
  JSON schema gains a `pattern`); `token create --scope` and `scope assign` refuse such text; `POST /v1/admin/tokens`
  with it is 400 (was 201); `PackageName::scope()` returns `Option<Scope>`; `RegistryConfig::scope_filter` is
  `Option<Scope>`. No wire shape, schema or lock format changes.
- Deleted: the registry server's prefix matcher, the `{scope}/x` shortcut, `AssignRefused::NotAScope`, one of two token
  row readers.
- Reopen: per-package tokens (add a `TokenFilter` beside `Scope`, do not loosen it); a registry that must serve scopes
  outside the name rule (ADR 0036's own reopen clause).
