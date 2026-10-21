# A scope is owned by an account, and one rule decides who writes into it

**Status:** accepted (2026-10-21, architecture round 20, plan 02). Cites [ADR 0061](0061-the-registry-verifies-the-signature-it-stores.md) (D2 is kept: the registry binds no key), [ADR 0045](0045-publish-is-one-operation-and-one-registry-serves-a-name.md) (D4: a published version is immutable, which makes a wrongful yank permanent) and [ADR 0044](0044-the-package-registry-contract-is-one-crate.md) (the crate the token bodies live in). Amends none.

## Context

- The reference registry had no account. A scope was owned by the one token that claimed it, so when that token was
  revoked, rotated or expired (90 days by default) its publisher's next token was refused SCOPE_OWNED for good, and the
  admin "scope reassignment" that `docs/registry-trust.md` promised did not exist: the only recovery was editing SQLite.
- "May this token write into scope S" was spelled at four sites (two ownership comparisons, two filter checks), and
  yank skipped the ownership half: any full-access token, or one scoped to the same scope but issued to someone else,
  withdrew another publisher's version. It then 404s for every lock that names it, and the owner cannot publish it again
  (R007, `UNIQUE(name, version)`). The filter refusal also called a package a scope.
- The "account" the metadata showed (`acct_...`) was a hash of the token's hash and carried nothing the token did not.

## Decision

- **D1. A token is issued to an account**, named by the operator: `specforge-registry token create --account <name>` and
  `account` in the admin token request (both required). The account exists from its first token. Names are 1 to 39 of
  `a-z`, `0-9` and `-`, not starting with `-`.
- **D2. A scope is owned by an account**, claimed by the first publish that stores a package in it. The stored
  `publisher` is the account's name.
- **D3. One rule**, `specforge_registry_server::ownership::may_write`: the token's filter covers the package, then the
  scope is unclaimed or owned by the token's account. The publish pre-check (before the body is read), the publish
  transaction and the yank transaction ask it. An unclaimed scope is writable and publish claims it; yank never claims.
  There is no admin override: a takedown is "assign the scope to the operator's account, then yank".
- **D4. The operator assigns a scope**: `specforge-registry scope assign <@scope> --account <name>`, CLI only, to an
  account a token was issued to. A stolen admin bearer token cannot move scopes over HTTP.
- **D5. Schema 2.** An existing data directory (schema 1) is refused with the existing "serve from a new --data-dir"
  error; there is no migration (pre-release).
- The in-memory client models the same ownership and the client contract holds it as K13.

## Rejected

- Binding a scope to a key: ADR 0061 D2 (it makes the registry the trust anchor and blocks key rotation). The account
  binds the scope to an operator-issued identity; a publisher rotates its signing key freely.
- An admin bypass for yank: a stolen admin token would withdraw every package.
- The filter on the account: a CI token narrowed to one scope is a real need, so it stays a property of the token.
- Deriving the account from the token (the old `acct_`): it carries nothing and cannot outlive the token.
- Anonymous accounts with `--same-account-as <token id>`, and an explicit `account create`: more to say for the same
  rotation; a typo shows as `(new)` on creation and is named by the first SCOPE_OWNED.
- Converting schema-1 data directories into `legacy-` accounts: a shim the pre-release rule forbids.

## Consequences

- Rotating or letting a token expire keeps the scope; the replacement is a token of the same account.
- Every token of an account can yank every version in its scopes. A CI token that publishes but cannot yank needs roles.
- Rate limits stay per token, so an account with N tokens has N budgets.
- Breaking: the `specforge-registry` CLI (`--account`, `scope assign`), registry HTTP (a yank by another account is 403
  SCOPE_OWNED; admin token requests need `account`; `publisher` is the account's name), database schema 2.

## What would reopen it

Accounts a publisher manages itself (sign-up, several owners per scope, roles), or a registry that federates identity.
