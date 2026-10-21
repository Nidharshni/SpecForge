# A registry is named by its root; the wire builds every address and reads every answer

**Status:** accepted (2026-10-13, architecture round 9, plan 05). Amends ADR 0044 (D1, D2, D5) and ADR 0045 (D1). Amended by [ADR 0178](0178-a-registry-credential-belongs-to-its-registry-root.md) (architecture round 20, plan 01): the root also keys the user's credential.

A `registries` entry named a registry by its "base URL", which by convention ended in the protocol's version prefix
(`https://registry.example/v1`). Nothing enforced or explained the convention: the server printed its root on start,
the client appended its paths to whatever the user wrote and never read the prefix constant, and it read every 404
as "package not found". A registry named by its root therefore looked empty: `update` said R-RES-001, `add` and
`publish` R006, `login` "Package not found: http://host". The server spelled its routes a second time with the
prefix, and the download path a metadata answer carries was "relative to the API base" to the server and
"root-relative" to the client. An entry that couldn't be read was skipped, so the names its scope claimed went to
the default registry.

## D1. A `registries` entry names the registry's root

`url` is a `specforge_registry_wire::RegistryRoot`: an `http` or `https` URL with a host and no query, fragment or
user info, held without a trailing `/`. It may carry a path (a registry served under `/registry`). Every call goes to
the root, then `/v1`, then the call's path: `RegistryRoot::address(&Call)`. Rejected: keeping the prefix in the
user's URL (the protocol's version is not the user's to write; a `/v2` beside `/v1`, ADR 0044's reopen clause, needs
users who never wrote it).

## D2. A url that isn't a root is refused, not repaired

A url ending in `/v1`, without a scheme or with another scheme is E067 naming the root to write. Rejected: stripping
the prefix (a legacy fallback that keeps two spellings), a warning that strips it, and letting the call 404 (it
reports "not a registry" for what the configuration reader can name).

## D3. One template per route

`Endpoint::route()` is the prefix plus the endpoint's path with placeholders; the server mounts it. `Call::path()` is
the same template filled. `specforge_registry_wire::path` and its `route` copy are gone.

## D4. An answer is read one way, and a 404 must be the registry's

`specforge_registry_wire::reply` reads an answer as the call's body, the protocol's error answer, an answer by status
alone, or not a registry's. A 404 is "not found" only with the protocol's `NOT_FOUND` error; a 404 without it, like a
success whose body isn't the call's type, is not a registry's answer: R009, naming the address. Other error statuses
mean what their status means whoever answers them (a proxy's 401, a gateway's 502), as ADR 0044 D2 decided.
Rejected: a new code (R009 already says "the URL isn't a SpecForge registry"), and requiring the envelope at every
status (the reference server's framework answers bare 400s; proxies answer bare 401/429/502).

## D5. A download is an http(s) URL or a path at the root

The server answers `wasm_url` as `Call::Download`'s path (`/v1/packages/{name}/{version}/download`); a client
resolves it at the root (`RegistryRoot::download_address`) and accepts an `http`/`https` URL elsewhere (a CDN, which
never gets the credential: `RegistryRoot::same_origin`). Anything else is R009. Rejected: a path relative to the
prefix (two kinds of path), an absolute URL from the server (it cannot know its public address behind a proxy), and
RFC 3986 resolution (an absolute path would drop a root's own path).

## D6. The registries array is read whole

An entry that can't be read refuses every registry operation with E067 before any request. Rejected: skipping it
(its scope's names went to another owner's registry, which ADR 0045 D1 rejected).

## Consequences

- Users write `"url": "https://registry.example"`. A url ending in `/v1` is E067 until edited.
- The HTTP client composes no URL; `LocalRegistry::root()` is what a test hands a client.
- The reference server's tests read their own answers with `reply`, so server and client agree by construction.
- `RegistryError` is unchanged: an answer no registry gives is `InvalidReply` (R009) with the address in its message.

## What would reopen it

- A protocol `/v2` served beside `/v1`: `address` gains the version per endpoint; the root stays what users write.
- A registry that must be reached through a URL with a query (a signed gateway URL): the root then keeps its query and
  `address` places the path before it.
