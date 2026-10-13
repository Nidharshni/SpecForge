# Extension calls are typed, in one module

**Status:** accepted (2026-10-06)

The host performs ten operations on a loaded extension: the handshake and describe that read its
declaration (ADR 0012), a command, an MCP tool, an MCP resource, a compiler pass, a collector, a
custom validator, a scanner and the migration hook. Each is one call of one export over the
`WasmRuntime` port (`call_export(extension, export, bytes)`), and each call site, in five crates,
built its input its own way (a `json!` head with the graph spliced in as a string, a private
struct, a `serde_json::Value` with `previous` inserted), parsed the answer its own way and mapped
a trap its own way (an E028 diagnostic, a `String`, an `eprintln!` warning, or nothing).
`specforge-protocol-types` said it was the single source of truth "so the wire format cannot
drift", but the command, pass and collector payloads existed only in the SDK, and the host
re-typed or hand-built them. They had drifted:

- the host sent each pass entity's `exempt`, which the SDK's `PassEntity` could not carry, so the
  testing and formal guests decoded their input twice;
- a command answer that did not parse was exit 0 with the raw bytes, a resource answer that did
  not parse was served as `application/octet-stream`;
- an analyze pass that trapped was a stderr line and no report, a scanner that trapped or answered
  garbage vanished (`infer` counted the file as having no public items);
- a collector answer of `[]` or `{}` read as no results, a collected test without a name was named
  `""`;
- optional pass and collect inputs went on the wire as `null` against ADR 0011's
  "absent, not null".

## D1. Every operational payload is a protocol type; the SDK re-exports

`specforge_protocol_types::calls` defines each operation's input and answer: `CommandInput<G>`
(generic over how the graph is held: the host sends a `RawGraph`, the graph export spliced as
rendered; the SDK reads its indexed `CommandGraph`), `CommandOutput`, `CommandError`,
`CommandFormat`, `McpResourceRequest`, `McpResourceContent`, `PassInput` (each `PassEntity` with
`testable` and `exempt`), `PassDiagnostic`, `PassOutput`, `PassAnswer` (bare diagnostics or
diagnostics with a summary), `CollectInput`, `CollectOutput`, `MigrationInput`, beside the
already-shared `ValidatorContext`/`ValidatorVerdict` and `ScanRequest`/`ScanResponse`. The SDK
re-exports them under the names authors use (`pub type CommandInput =
CommandInput<CommandGraph>`). A payload change is one type edit, seen by the host and every guest
at compile time. Rejected: host-only typed mirrors (a third copy) and a separate wire crate (a
second "single source").

*Amended (ADR 0069, 2026-10-13):* the recorded-test payloads are one type: `TestStatus` and `TestResult`
(with `TestResults` for a pass's `test_results`) replace `CollectTestResult`, `PassTestResult`, `PassTestResults`
and `PassEntityResults`.

## D2. JSON over the bridge `call` export stays the encoding

A typed WIT per operation (what `wit/specforge-extension.wit` sketched for worlds nothing
implemented) was rejected: a second schema language beside serde, a rebuilt ABI for every guest
(ADR 0004's single `call` world, ADR 0011 E), and payloads that are JSON-native. The stale WIT is
deleted; protocol types plus golden tests (`crates/specforge-wasm/tests/wire/`) give Rust guests
compile-time checking and any other language an exact JSON contract
(`docs/extension-protocol.md`, "Operate").

## D3. One module, one function per operation

`specforge_wasm::calls::ExtensionCalls` has `handshake`, `describe`, `run_command`,
`call_mcp_tool`, `read_mcp_resource`, `run_pass`, `collect`, `validate`, `scan` and `migrate`.
It is the host's one caller of a pass's export, whose name is the protocol's rule
(`specforge_protocol_types::pass_export`, `__pass_<name>`) that the SDK routes by too; it owns
encoding (an input encoded once for many passes, `Encoded`, which carries its encoding failure, so
each pass's call fails with its own export and no caller builds an export name or a `CallError`),
strict decoding, and the one mapping of every failure; `handshake` reads the handshake and applies
nothing, the loader applies its sandbox limits (ADR 0012 D2, ADR 0037);
`pass_diagnostics` turns a pass's answer into host diagnostics in canonical order (code, file and
line, message), attaching an entity's span to a span-less diagnostic that names it. Callers keep
what is theirs: building the payload from the graph, and what a failure means for their
operation. Outside the two runtime adapters, nothing else calls `call_export`. Rejected:
per-crate typed helpers (five crates would still map traps) and a trait per operation (no second
implementation would exist; the seam is `WasmRuntime`).

## D4. One failure type, one code, one message shape

Every failure is a `CallError { operation, extension, export, failure }`, `failure` being
`NotLoaded`, `Trapped { kind, message }`, `Malformed { expected, reason }` or
`Unencodable { reason }`, reported as E028 `"<operation> <export>() of '<extension>' <trapped:
kind: message | answered output that is not a <Type>: reason | is not loaded>"` with the
suggestion to report it to the extension's author. What it costs is the operation's: a check
pass's is a compile error, an analyze pass's a finding of that pass (its summary marked `failed`,
the analysis not ok), a command's its error (exit 1, under `--format json` one `CommandError`
object), a tool's or resource's a structured MCP error, a scanner's a reported `scan_failures`
entry that makes the gap report approximate, a collector's the collect error, a custom
validator's the probe's W112, a migration hook's a failure line. E028 messages that named
`surface command x() trapped: k — m` and the like change to the one shape; E028 was never a text
contract.

## D5. Strict on shape, lenient on unknown fields

Required fields are required (a command answer without `exit_code`, a collected test without a
`name`, a resource answer without its `mime_type` is malformed), optional fields default when
absent, unknown fields are ignored so a newer peer may add one. No SDK-built guest ever produced
a refused shape. Rejected: lenient decoding (the bugs above) and `deny_unknown_fields` (it breaks
every additive change).

*Amended (ADR 0069, 2026-10-13):* a collected test whose `status` is not `passed`, `failed` or `skipped` is
malformed, as one without a `name` is.

## D6. `PROTOCOL_VERSION` stays 1.0.0

Every guest-facing change is an additive optional field (`exempt`) or a type moving crates with an
identical wire shape; the host only refuses answers the protocol never allowed. A guest built
with the 1.0 SDK keeps loading and answering.

## D7. Absent, never null

The host's encoders skip unset optional fields: `stdout` of a collect input, `test_results`,
`proved_claims` and `previous` of a pass input, an entity's `span`. Guests decode both.

## D8. Two adapters make the seam real

`WasmRuntime` has two implementations: `ComponentRuntime` (wasmtime, production) and
`specforge_wasm::testing::InProcessRuntime` (feature `testing`), which serves an SDK-declared
extension in the host process through the guest's own routing (`Served::call`, the body of
`component_guest!`), unsandboxed. Host tests declare their extensions with the SDK and give the
answers no SDK guest gives (another protocol version, a trap, bytes that do not parse) through
`answer_raw`; the nineteen hand-written doubles are gone. One contract suite
(`specforge_wasm::testing::assert_runtime_contract`) runs over both adapters, and the greet
fixture answers every call byte-identically from its vendored blob and from source. Running the
contract over the component runtime found that an instance that trapped could not be entered
again, so one panicking call took the extension down for the rest of an MCP session: the
component runtime now gives the extension a fresh instance after a trap. Sandbox and deadline
obligations stay proven on the component runtime only.

*Amended (2026-10-08): a builtin's command behaviour is tested from source.* An extension's command
tests run through the in-process runtime, called as the host calls them
(`ExtensionCalls::run_command`), in the extension's own crate: `@specforge/product`'s 283 live in
`extensions/product/src/tests/` and link their obligations there (ADR 0002). Over the vendored blob
they cost a re-vendor before any feedback and about ten seconds of CPU each. The blob is its sources
at every green commit (`build-builtins --check`), so `specforge-component` tests a builtin only for
what its bytes prove: its declaration snapshots (`tests/declarations.rs`), that every declared
command answers through it as the host calls it (`tests/builtins.rs`, `tests/product_blob.rs`), and
what the in-process runtime cannot hold it to (the component's stack, as `product_blob.rs`'s long
chain; the sandbox and the deadline, above). In this repository the extension crates are outside the
workspace, so their linked tests are recorded by the gate (`cargo gate full` runs them into
`target/specforge` before `collect --no-run`), not by a bare `specforge collect`.

## D9. One `PassDiagnostic`, without the host's diagnostic data

Guests answer the protocol's `PassDiagnostic` (code, severity, message, span, suggestion,
entity); the host decodes exactly that. A guest cannot construct the host's `DiagnosticData`, and
none set it, so it is not carried; what a pass diagnostic is about is its `entity`.

## D10. Operations are declared with their handlers

As commands were (ADR 0011), a pass, a collector, a custom rule, a scanner and the migration hook
are declared together with the function that answers them: `PassBuilder::run`,
`CollectorBuilder::collect`, `RuleBuilder::validate`, `AnalyzerBuilder::scan`,
`ContributionsBuilder::migration_hook_handler`. The SDK routes the export, decoding the protocol
input and encoding the answer once, in code a guest links only for what it declares. A
declaration without its handler panics when the extension is built. The `handler` of
`component_guest!`, `raw_category` and the deprecated `migration_hook` stay for guests not
written with the builders; `answer_export` decodes and encodes for such a handler with the
declared handlers' code. `#[compiler_pass]` was removed with `HostApi` and the free
`describe_dispatch` (plan 16, 2026-10): the function it wrapped is the handler `PassBuilder::run`
takes. A guest builds its declaration once per instance (`Served`, plan 15, 2026-10); `guest_call` and
the builder's `describe_dispatch` are gone.
One SDK source change: `PassOutput::summary` is a JSON object, the keys the host merges into the
pass's report.

## D11. No operation for `classify__`/`map__`

An analyzer still declares `classify_export` and `map_export`, and the rust and typescript guests
answer them from their handler, but the host never calls them; typed operations for exports
nobody calls would be speculative.

## D12. Analyze takes an optional runtime

`specforge_ops::analyze::analyze(view, Option<&dyn WasmRuntime>, options)`: a rootless MCP
analysis has no runtime and runs no extension pass, which the type now says instead of a
production `NoRuntime` double. *(Amended by ADR 0015, "The runtime travels with the
environment": `analyze(view, options)` runs the extension passes in the view's runtime, and none
when its environment was loaded without one. MCP's analysis always has a project: it refuses
`no_project` first.)*

## D13. `WasmValidationRuntime` keeps one method

*Superseded by ADR 0020 D3: custom verdicts are one port, `CustomVerdicts::verdict(CustomCall)`,
with the extension in the call and the probe through the same port.*

The registry's seam for custom rules is `custom_verdict(function, entity, kind) ->
Result<CustomVerdict, String>`; the boolean form only its default bridged is gone, with the
unused `StubWasmRuntime`.

## D14. Dead code goes with its spec promises; W114 retires

What ADR 0012 left of `specforge-wasm`'s dead code goes: trap handling (`handle_wasm_trap`,
`should_skip_extension`), `verify_wasm_integrity` and its `--skip-verify` form (the integrity
checks that run are install's hash check, E032, and load's lock-file pin, E033),
`ExtensionLifecycleState`, `LoadedModule`, `UninstallResult`, the protocol types' glob
re-export. The crate root re-exports 28 names. W114, emitted only by the skip-verify path, is
retired. The behaviours whose only implementation was dead or never written are trimmed from the
spec (ADR 0003): contribution-export validation, dispatch and toggling, trap handling, extension
validators, query extensions and composition, reserved kind names, enhancement conflicts,
discovery, lock-file refresh, `query_scope`, Wasm integrity verification with `--skip-verify`,
with the events, invariants, types and config fields only they used; `entity_kind_uniqueness`
and `enhancement_field_uniqueness` now state what the registry build does (E026, first
registration wins). `call_extension_exports` replaces `dispatch_contribution_exports`, and the
`WasmRuntime` port names its real methods, its contract proven over both adapters.

## Consequences

- R1–R5 are fixed: a malformed command or resource answer is E028; an analyze pass that traps is
  an E028 finding; a scanner failure is reported and makes the gap report approximate; testing and
  formal read `PassEntity::exempt` and decode their input once.
- `ExtensionCalls` is the only caller of `call_export` outside the two adapters; `xtask
  snapshot-builtins` keeps calling it raw, because it pins the exact bytes.
- ADR 0008's command input is the protocol's `CommandInput` now (same fields, same defaults);
  ADR 0009 C's `PassBuildCache` keeps its shape, now a protocol type.
- `docs/extension-protocol.md` documents every operation's export, input and answer.

## What would reopen it

- A non-Rust guest SDK: generate a WIT `types` interface (or JSON Schema) from the protocol types.
- A second protocol major: `ExtensionCalls` then negotiates per extension.
- A host that calls `classify__`/`map__`: two more operations.
- A guest that needs to return typed diagnostic data: a protocol type for it.
