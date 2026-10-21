# Operations reach the package registry through a port

**Status:** accepted (2026-10-03). Amended by [ADR 0178](0178-a-registry-credential-belongs-to-its-registry-root.md) (architecture round 20, plan 01): credential health is read through `specforge_ops_registry::User`, and the client holds no OS keyring.

Splitting out the Package registry client (`specforge-registry-client`: reqwest, the OS keyring,
ed25519) did not keep it out of the LSP. `specforge-ops` used it for four things, so every
surface linking an operation linked the client, and the LSP links `specforge-ops` for one pure
function, `rename::plan`. The four were: `HttpRegistry`, the adapter behind the `Registry` trait
`add` and `update` already took (with an in-memory fake in their tests); the publisher signature
check `add` ran on what `fetch` returned; `unify_diamond`, pure semver; and the credential health
`specforge doctor` prints.

We inverted the dependency rather than gate it:

- **`specforge-ops` names only the port.** `Registry::fetch` now takes the trust policy
  (`allow_unsigned`, `Trust`) and returns a `Package` that has passed integrity and the TOFU
  signature check, with the `key_id` it was signed by. `fetch_checked` keeps the order it had
  (integrity, signature, diamond gate, declared identity). `Unconfigured` is the registry an
  operation that never reaches one passes (`init`, which installs builtins and local files).
- **`specforge-ops-registry` is the adapter**: `HttpRegistry`, which now runs `check_and_pin`,
  and `configured` (E063/E067), which `login`, `publish` and `search` also use. The CLI and MCP
  depend on it; MCP must, since `specforge.add_extension` downloads.
- **The pure parts moved to their owners.** `unify_diamond` is private to
  `specforge_ops::extension::diamond`, the ADR 0001 gate that uses it; the unused
  `resolve_diamond` is gone. Credential health is `specforge_registry_client::credential_health`,
  beside the store it reads; only the CLI's `doctor` uses it, so `DoctorReport` is unchanged.

A cargo feature on `specforge-ops` was rejected: features unify across a workspace build, so the
LSP would still link the client whenever it is built with the CLI, and the guarantee would rest
on how a binary is built rather than on the dependency graph. `cargo tree -p specforge-lsp -e
normal` now lists no reqwest, keyring or ed25519.

**What would reopen it:** an LSP feature that needs the registry (completing package names from
it, say). It would then link the adapter knowingly, or reach it through a narrower port.

**Amended by ADR 0036 (2026-10-07):** the port takes a `PackageName` and a `Version`, and has two
methods, `versions` and `fetch`; `resolve_version` is gone and ops resolves a requirement itself
(`VersionRequirement::pick`).

**Amended by ADR 0044 (2026-10-08):** the adapter is `ConfiguredRegistry` (it was `HttpRegistry`), which runs
the fetch policy over the `RegistryClient` seam rather than a concrete HTTP client; the port has a second,
in-memory adapter (`specforge_ops::registry::testing::MemoryRegistry`), and both are held to
`assert_registry_contract`. The HTTP shapes live in `specforge-registry-wire`, which neither `specforge-ops`
nor the LSP links.

**Amended by ADR 0045 (2026-10-08):** the port has three methods, `versions`, `fetch` and
`publish`; the adapter also holds the publish credential and the signing key, read from the user's
`~/.specforge` (`specforge_ops_registry::User`). `publish` is an operation in ops, and the CLI only
presents it.

**Amended by ADR 0059 (2026-10-12):** `fetch` takes a `TrustPolicy` (the caller's two choices and the key the
lock records for the name) and writes nothing; the port gains `pin`, which saves the publisher keys a committed
lock plan accepted. `check_and_pin` is split into `trust_flow::check` and `trust_flow::record`.

**Amended by ADR 0088 (2026-10-14):** the guarantee is checked. Gate's `layers` check holds `APART`
(`gate/src/layers.rs`): `specforge-lsp` must not reach `specforge-registry-client`, `specforge-registry-wire` or
`specforge-ops-registry` through any chain of normal or build dependencies in `cargo metadata`, and the message
names the chain that does. The rule judges workspace crates. The members that depend on reqwest, keyring or
ed25519 today are the client and the wire crate; a new direct dependency on one of them from another member
would escape the rule.

**Amended by ADR 0086 (2026-10-14):** `Trust` is defined in `specforge_ops::registry`, beside `TrustPolicy`, and named `registry::Trust`; the port names no extension module.
