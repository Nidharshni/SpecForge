# The package registry's contract is one crate, and the fetch policy runs over the client seam

**Status:** accepted (2026-10-08)

A package registry was reached through two stacked seams, the ops `Registry` port (ADR 0010) and the
client's `RegistryClient` trait, with the code that matters between them. The fetch policy (the reply names
what was asked, integrity, the manifest read as the package's declaration, the legacy refusal, the publisher
signature and its pin) was `HttpRegistry::fetch`, hard-wired to the concrete HTTP client; the trait had no
way to list versions or download, so its two mocks never reached the policy, and the policy's twenty tests
served JSON on real sockets. The JSON itself was written by the server's private structs, read by the
client's mirror structs and written again by three hand-rolled test servers, which had drifted (another
download path, an error without a code). The client read an HTTP status six ways: a rate-limited publish was
a network error, and the same 403 was R005 for a version list and R002 for a version. The ops port's two test
doubles disagreed with production in four ways (an unknown package listed as empty, an unknown version as
R-RES-001, a reply describing another package served as fine), after the one ADR 0036 recorded.

## D1. One crate holds the contract

`specforge-registry-wire` holds the paths, the server's route patterns, every JSON body, the publish form and
the error codes. The server serializes and mounts them; the client requests and deserializes them. It
depends on serde, form_urlencoded and `specforge-protocol-types`. Rejected: a module in the protocol types
(the guest protocol, a blob input of every builtin), in the client (the server would link the keyring,
reqwest and ed25519; a feature gate is ruled out as in ADR 0010) or in the server (every surface would link
axum and rusqlite through the client).

## D2. The client seam is the whole transport, read one way

`RegistryClient` lists versions, describes a version (`metadata`), downloads, searches, publishes and
authenticates, answering the wire types; it chooses no registry and checks no reply. Every call reads a
failure status the same way: 401 and 403 carry the registry's message, 404 is not found, 429 is rate
limited, anything else a network error with the registry's message. A network error's suggestion says to
retry; `resolve_from_registry`, which only appended that to fetches, is gone.

## D3. The fetch policy runs over the seam

The ops port's adapter is `ConfiguredRegistry` (it was `HttpRegistry`): it reads the project's registries the
first time it is asked, asks the one that serves a name, and runs the policy over any `RegistryClient`, HTTP
unless a test gives another. Rejected: the policy in the client (it hands ops a type and codes the client
cannot name) and in ops (signatures and the pin store would reach the LSP, ADR 0010).

## D4. Each seam has an in-memory adapter and one contract

`specforge_ops::registry::testing::MemoryRegistry` sits beside the port and
`specforge_registry_client::testing::MemoryClient` beside the client, as `InProcessRuntime` sits beside
`WasmRuntime` (ADR 0013). `assert_registry_contract` holds `MemoryRegistry` and `ConfiguredRegistry` (over
HTTP and over `MemoryClient`) to one behaviour; `assert_client_contract` holds `MemoryClient` and the HTTP
client against the real server. A test that crosses HTTP uses the real server in process
(`specforge_registry_server::testing::LocalRegistry`), which stores packages past the publish checks. No test
writes registry JSON by hand. Rejected: the real server as the only stand-in (it cannot serve what the policy
refuses, and ops' unit tests cannot link it); one in-memory type for both seams (it would live where ops'
tests cannot depend on it).

## D5. The configuration's diagnostics show once a registry was asked

`ConfiguredRegistry::reported()` is what reading the registries reported (E067, W140, I003), once an
operation has asked it anything. The CLI and MCP show it after the operation, success or failure; they no
longer decide from the source, and `update`'s `registry_used` is gone. An add of a version already installed,
or a dry run of an exact version, asks nothing and shows nothing.

## Consequences

- A field added to a registry's JSON is one change, which both crates compile against.
- The fetch policy's tests run in memory; the HTTP adapter's request paths and the CLI's installs run against
  the real server.
- The CLI test of a tampered download is gone: the real server never serves one, and the policy's own test
  keeps the obligation.
- `MemoryRegistry` answers as production: R-RES-001 for a package it doesn't hold, R006 for a version,
  R-TRUST-001 for an unsigned package unless allowed, and a declaration that is the package's own.

## What would reopen it

- A registry protocol version 2 served beside `/v1`: the wire crate then gains a second module, not a second
  crate.
- A registry the policy must trust differently per source (a mirror that re-signs): the policy then takes
  the trust source from the configuration, still over the seam.

## Amendment (architecture round 5, plan 16): retries, read credentials, search by category

- **A rate-limited call is sent again.** `Retrying<C>` is an adapter of the `RegistryClient` seam over any other:
  a `RateLimited` answer is sent again after the longer of `RetryPolicy::REGISTRY`'s backoff (1 s doubling, at most
  30 s) and the registry's `Retry-After`, at most 3 times; a `Retry-After` beyond 30 s is not waited for. Every call
  is retried alike, publish included (a registry refuses a rate-limited publish before reading it).
  `ConfiguredRegistry::for_project` builds `Retrying::new(HttpRegistryClient::new())`; a test passes its own client.
- **Reads carry the user's credential.** `versions`, `metadata`, `download` and `search` take the credential the
  user keeps for that registry, when any; `download` sends it only to the registry's own origin. A credential at
  the seam is a resolved token (`RegistryCredential { alias, token }`): the client reads no variable or file.
- **Search takes a declared category.** `SearchQuery::contributes` names one of the ten declared categories; a
  registry keeps the latest versions whose stored declaration declares something in it
  (`specforge_registry_wire::declares`). The reference server can require a token to read
  (`ReadAccess::Token`, `serve --private`).
- The contract suite gains the private-registry clauses (K-P1..K-P4) and the port contract a search clause (R-S).

**Amended by ADR 0050 (2026-10-11):** D1's reason for keeping host code out of the protocol types ("a blob input
of every builtin") now holds for the protocol types themselves: the package names and the peer rule moved to
`specforge-package`, which the wire crate depends on. `specforge_registry_wire::declares` is gone: a search
filter reads `ExtensionDeclaration::declares`.

## Amendment (architecture round 7, plan 12): the signature is part of the contract, and refusals are typed

- **D1.** The crate also holds the package signature (`signature`: `PackageSignature`, `SignedContent`,
  `sha256_hex`), its parse and its verification, which both ends run (ADR 0061). It therefore also depends on
  `serde_json`, `sha2`, `hex`, `ed25519-dalek` and `chrono` (and on `specforge-package`, ADR 0050). D1's rejection of
  "a module in the client" stands for its first reason: the registry would link the keyring and reqwest. It now links
  ed25519 on purpose.
- **D2.** "Anything else a network error" is narrowed: a 400 is `Refused` carrying the registry's `ErrorCode` and
  message (R008), and a 2xx body that isn't the call's wire type is `InvalidReply` (R009). Other statuses read
  as before. `PackageMetadata` carries the typed signature and no `key_id`; the error body's `code` is `ErrorCode`;
  the client reads the publish receipt.
- **D4.** The client contract gains K11: a publish whose signature does not verify is refused and stores nothing,
  over HTTP and in memory.

**Amended by ADR 0059 (2026-10-12), to D4:** `MemoryRegistry` decides a publisher against its own pins and the
lock's key, and records pins through `pin`. The contract gains C8-C10, and C3 states that a fetch writes nothing.

## Amendment (round 7, plan 16): one search rule; a claim and a revocation that cannot overreach

- **Search matches by one rule**, `specforge_package::search::matches`: an ASCII case-insensitive
  substring of the name, the description or one keyword; `%` and `_` are text. The server's SQL is
  an escaped prefilter and the rule decides; `MemoryClient` and `MemoryRegistry` call the same
  function, and `assert_client_contract` holds the server and `MemoryClient` to it.
- **A scope is claimed by the publish that stores a package**: the claim and the version insert
  are one transaction (`Database::publish`), after the whole upload validated. A refused upload,
  a lost version race or a failed blob commit (`Database::withdraw`) leaves the scope as it was.
- **An admin revokes a token by its id** (the 8 hex digits the listing shows); any other segment
  is refused (400), and an id two live tokens share revokes neither (409).
