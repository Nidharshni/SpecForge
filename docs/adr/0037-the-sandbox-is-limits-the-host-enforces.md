# An extension's sandbox is the limits the host enforces

**Status:** accepted (2026-10-07)

An extension's handshake declared a sandbox policy of seven fields and each surface an override of
three. The host read one: `max_execution_ms`, clamped to 30 s. `max_memory_mb` was read by nobody,
so a guest could grow to 4 GiB whatever it declared (an extension declaring 1 MB took the host
process to 970 MB). The fuel budget documented as per call was spent across an instance's life, so
a long session failed one call in a while with an anonymous `call_failed`. The five permission
fields and the three override flags granted nothing, since the component runtime grants no
capability at all, which the sandbox probe proves. Docs and spec described enforcement of all of it.

## D1. A component is granted no capability

Its WASI context preopens no directory, passes no environment, arguments or stdin, discards stdout
and stderr, and refuses TCP, UDP and name lookup, written out in `no_capabilities()` rather than left
to wasmtime-wasi's defaults. Clocks and randomness are WASI's own. This holds for every export of
the instance whatever the declaration says; `crates/runtime/component/tests/sandbox_probe.rs` is
its proof.

## D2. The policy declares limits only

`SandboxPolicy` is `max_execution_ms` and `max_memory_mb`. `specforge_wasm::sandbox::Sandbox::of`
turns the handshake's `sandbox_policy` into `Limits`: each declared limit as declared, held to the
host's ceiling (`Limits::CEILING`: 30 000 ms, 512 MB), the ceiling when undeclared. The loader
applies them once the handshake's protocol major is checked (`load_declaration` →
`WasmRuntime::apply_limits`, required: the component runtime enforces, the in-process runtime
records); a handshake call applies none. There is no project-level override and
no total across extensions: the ceiling bounds every extension, and a shared total would let one
extension's memory fail another (`extension_isolation`).

## D3. The component runtime enforces every limit

A memory limiter traps a growth past `max_memory_mb` (`memory_limit_exceeded`); epoch interruption
bounds a call's time (`deadline_exceeded`); the fuel budget is refilled before every call
(`fuel_exhausted`). A limit's trap names the limit; the call fails with E028; the next call gets a
fresh instance under the same limits. A handshake runs under the ceiling, since its limits are not
known before it answers.

## D4. What a declaration asks for that the host does not give is W153

A declared limit above the ceiling; a `sandbox_policy` key other than the two limits whose value asks
for something (`network_access: true`, a non-empty `allowed_paths`, a misspelled limit); a surface's
`sandbox` key. They are load warnings, in `Loaded::warnings` before the W138s, so `check`, the LSP,
MCP, `specforge publish` and `specforge extension validate` show them. A key asking for nothing
(`false`, `[]`) is not reported: older SDKs wrote every field.

## D5. The permission fields leave the protocol; the version does not move

`allowed_domains`, `allowed_paths`, `allowed_output_extensions`, `network_access`,
`file_system_access`, `SurfaceSandboxOverride` and the SDK's `SandboxBuilder` are gone. An older
guest still decodes (serde ignores the keys; W153 names them) and an older host reads a new guest
(its fields default), so `PROTOCOL_VERSION` stays 1.1.0. The package registry no longer refuses
`network_access` (`SANDBOX_POLICY_REJECTED`): no host grants network, and publish shows W153 first.
Rejected: granting them (preopens for `allowed_paths`, sockets for `network_access`): it would hand
third-party code real host directories, `allowed_domains` cannot be enforced at the socket layer
(addresses, not names; rebinding), and no extension needs it (an analyzer is handed each file's
content). Rejected: keeping them as ignored fields, the lie this decision removes.

## Consequences

- A guest's memory is bounded; the 970 MB growth of an extension declaring 1 MB is a trap at the
  declared limit.
- E028 says which limit stopped a call.
- `default_sandbox_policy`, `set_execution_deadline_ms`, the two sandbox events of the spec and the
  three manifest claims of builtins that declare no policy are gone.
- Amends ADR 0012: D6 (the server no longer refuses `sandbox_policy.network_access`) and D11 (the
  environment's load warnings are W153, then W138).

## What would reopen it

- The planned host-function surface (`spec/behaviors/wasm-host-functions.spec`): the permissions it
  needs (paths it may read, domains it may fetch, output extensions it may write) come with it, as
  fields that surface defines and enforces in the same change.
- A project that must tighten a specific third-party extension below its own declaration: a
  `specforge.json` override, threaded through every load path.
- A guest that needs more than 512 MB: raise `Limits::CEILING`, with a measurement.
