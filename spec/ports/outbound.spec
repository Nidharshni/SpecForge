// Outbound ports — interfaces the system requires from the outside world

use "types/config"
use "types/core"
use "types/diagnostics"
use "types/errors"
use "types/wasm"
use "types/zero-entity-core"

port FileSystem "File System" {
  direction outbound
  category  "io/filesystem"
  method readFile(path: string) -> Result<string, EmitterError>
  method writeFile(path: string, content: string) -> Result<void, EmitterError>
  method listFiles(pattern: string) -> Result<string[], EmitterError>
  method watchFiles(patterns: string[]) -> Result<void, EmitterError>
  method exists(path: string) -> Result<boolean, never>
  method atomicWrite(path: string, content: string) -> Result<void, EmitterError>
  method rename(from: string, to: string) -> Result<void, EmitterError>
  verify integration "FileSystem contract is satisfied"
}

port SourceParser "Source Parser" {
  direction outbound
  category  "compiler/parser"
  method parseSource(content: string, path: string) -> Result<SpecFile, ParseError>
  method parseIncremental(content: string, path: string, previousTree: string) -> Result<SpecFile, ParseError>
  verify integration "SourceParser contract is satisfied"
}

port GraphSerializer "Graph Serializer" {
  direction outbound
  category  "io/output"
  method serializeJson(data: JsonValue) -> Result<string, EmitterError>
  method serializeDot(data: JsonValue) -> Result<string, EmitterError>
  method writeOutput(path: string, content: string) -> Result<void, EmitterError>
  verify integration "GraphSerializer contract is satisfied"
}

port RefValidator "Ref Validator" {
  direction outbound
  category  "validation/refs"
  method validateScheme(scheme: string) -> Result<boolean, never>
  method validateKind(scheme: string, kind: string) -> Result<boolean, never>
  method validateIdentifier(scheme: string, kind: string, identifier: string) -> Result<boolean, ValidationError>
  method resolveUrl(scheme: string, kind: string, identifier: string) -> Result<string, EmitterError>
  verify integration "RefValidator contract is satisfied"
}

port WasmRuntime "Wasm Runtime" {
  direction outbound
  category  "runtime/wasm"
  // The byte-level port under call_extension_exports (ADR 0013): its two
  // adapters are the component runtime (production) and the in-process
  // runtime (tests).
  method loadModule(extensionId: string, wasmPath: string) -> Result<void, ExtensionError>
  method callExport(extensionId: string, exportName: string, input: u8[]) -> Result<u8[], WasmTrapInfo>
  method setExecutionDeadline(extensionId: string, maxExecutionMs: integer) -> Result<void, never>
  method loadFailure(extensionId: string) -> Result<Diagnostic, never>
  verify integration "WasmRuntime contract is satisfied"
}

port RegistryClient "Registry Client" {
  direction outbound
  category  "io/registry"
  // The transport to a package registry (ADR 0044): it chooses no registry
  // and checks no reply. Each call goes to the registry's root, the protocol's
  // version prefix and the call's path (specforge_registry_wire::address); an
  // answer no registry gives (a 404 without the protocol's NOT_FOUND error) is
  // R009, not a missing package. Its adapters are the HTTP client (production) and
  // the in-memory client (tests); both keep one contract suite.
  // A read carries the credential the user keeps for the registry, when
  // there is one; a download carries it only to the registry's own origin.
  // A publish is always signed; a registry refuses a signature that isn't
  // one or doesn't verify over the upload (INVALID_SIGNATURE), and the
  // in-memory client refuses it the same way. A publish is addressed by the
  // name and version of the package it uploads; a registry refuses a
  // manifest that is no declaration (INVALID_MANIFEST) or names another
  // package or version (NAME_MISMATCH), and the in-memory client refuses
  // them the same way.
  method versions(name: PackageName, registry: RegistryConfig, credential: RegistryCredential) -> Result<string[], RegistryError>
  method metadata(name: PackageName, version: string, registry: RegistryConfig, credential: RegistryCredential) -> Result<PackageMetadata, RegistryError>
  method download(wasmUrl: string, registry: RegistryConfig, credential: RegistryCredential) -> Result<u8[], RegistryError>
  method search(query: SearchQuery, registry: RegistryConfig, credential: RegistryCredential) -> Result<SearchHit[], RegistryError>
  method publish(package: DeclaredPackage, wasm: u8[], manifest: string, signature: PackageSignature, registry: RegistryConfig, credential: RegistryCredential) -> Result<PublishReceipt, RegistryError>
  method authenticate(registry: RegistryConfig, credential: RegistryCredential) -> Result<string, RegistryError>
  verify integration "RegistryClient contract is satisfied"
}

port Registry "Registry" {
  direction outbound
  category  "io/registry"
  // What operations reach a package registry through (ADR 0010, 0036,
  // 0044, 0045, 0059): it lists a package's versions, reads one's
  // published manifest without its binary (what a dry run judges), fetches
  // one that passed the fetch policy, pins the publisher keys a committed install
  // accepted, and publishes one, each to the one registry that serves the
  // name. A fetch decides its publisher and writes nothing; the lock plan
  // pins what it accepted as the last step of its change. Its adapters are
  // the configured registry (production) and the in-memory registry
  // (tests); both keep one contract suite.
  method versions(name: PackageName) -> Result<string[], ExtensionError>
  method published(name: PackageName, version: string) -> Result<RegistryManifest, ExtensionError>
  method fetch(name: PackageName, version: string, policy: TrustPolicy) -> Result<RegistryPackage, ExtensionError>
  method pin(pins: KeyPin[]) -> Result<void, ExtensionError>
  method publish(package: DeclaredPackage, wasm: u8[]) -> Result<RegistryPublished, ExtensionError>
  method search(query: string, contributes: string) -> Result<RegistrySearched, ExtensionError>
  verify integration "Registry contract is satisfied"
}

port Editor "Editor" {
  direction outbound
  category  "api/lsp"
  // What the LSP's reaction to a change tells the editor (ADR 0043): its
  // two adapters are the tower-lsp client (production) and a recorder
  // (the LSP's tests). Each call returns once the editor has the message;
  // watch returns once the editor answered, and a refusal falls back to
  // the static watchers.
  method publish(path: string, diagnostics: Diagnostic[], version: integer @optional) -> Result<void, never>
  method watch(globs: string[]) -> Result<void, string>
  method log(level: string, message: string) -> Result<void, never>
  method progress(done: boolean, title: string @optional) -> Result<void, never>
  method refreshTokens() -> Result<void, never>
  verify integration "Editor contract is satisfied"
}

port CollectorMachine "Collector Machine" {
  direction outbound
  category  "analysis/collect"
  // What collect asks the machine it runs on (ADR 0125): run a collector's
  // command to its end (answering when it started), and the user's approvals
  // of commands. Its adapters are a child process with the user-level consent
  // store (production) and a scripted machine (tests).
  method run(command: CollectorCommand) -> Result<CollectorRun, CollectorRunFailure>
  method approved(approval: CommandApproval) -> Result<boolean, never>
  method approve(approval: CommandApproval) -> Result<void, string>
  verify integration "CollectorMachine contract is satisfied"
}

port SmtSolver "Smt Solver" {
  direction outbound
  category  "analysis/solver"
  // What the prove pass and doctor ask the SMT solver (ADR 0079):
  // whether it runs here (its version banner, or the W098 that says why not,
  // worded by the adapter) and its answer to one SMT-LIB2 script within the
  // adapter's timeout. Its adapters are z3 from PATH (production) and a
  // scripted solver (tests).
  method availability() -> Result<string, Diagnostic>
  method solve(script: string) -> Result<string, SolveFailure>
  verify integration "SmtSolver contract is satisfied"
}
