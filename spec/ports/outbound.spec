// Outbound ports — interfaces the system requires from the outside world

use "types/config"
use "types/core"
use "types/diagnostics"
use "types/errors"
use "types/wasm"
use "types/zero-entity-core"

port FileSystem {
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

port SourceParser {
  direction outbound
  category  "compiler/parser"
  method parseSource(content: string, path: string) -> Result<SpecFile, ParseError>
  method parseIncremental(content: string, path: string, previousTree: string) -> Result<SpecFile, ParseError>
  verify integration "SourceParser contract is satisfied"
}

port GraphSerializer {
  direction outbound
  category  "io/output"
  method serializeJson(data: JsonValue) -> Result<string, EmitterError>
  method serializeDot(data: JsonValue) -> Result<string, EmitterError>
  method writeOutput(path: string, content: string) -> Result<void, EmitterError>
  verify integration "GraphSerializer contract is satisfied"
}

port RefValidator {
  direction outbound
  category  "validation/refs"
  method validateScheme(scheme: string) -> Result<boolean, never>
  method validateKind(scheme: string, kind: string) -> Result<boolean, never>
  method validateIdentifier(scheme: string, kind: string, identifier: string) -> Result<boolean, ValidationError>
  method resolveUrl(scheme: string, kind: string, identifier: string) -> Result<string, EmitterError>
  verify integration "RefValidator contract is satisfied"
}

port WasmRuntime {
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

port RegistryClient {
  direction outbound
  category  "io/registry"
  // The transport to a package registry (ADR 0044): it chooses no registry
  // and checks no reply. Its adapters are the HTTP client (production) and
  // the in-memory client (tests); both keep one contract suite.
  // A read carries the credential the user keeps for the registry, when
  // there is one; a download carries it only to the registry's own origin.
  // A publish is always signed; a registry refuses a signature that isn't
  // one or doesn't verify over the upload (INVALID_SIGNATURE), and the
  // in-memory client refuses it the same way.
  method versions(name: PackageName, registry: RegistryConfig, credential: RegistryCredential) -> Result<string[], RegistryError>
  method metadata(name: PackageName, version: string, registry: RegistryConfig, credential: RegistryCredential) -> Result<PackageMetadata, RegistryError>
  method download(wasmUrl: string, registry: RegistryConfig, credential: RegistryCredential) -> Result<u8[], RegistryError>
  method search(query: SearchQuery, registry: RegistryConfig, credential: RegistryCredential) -> Result<SearchHit[], RegistryError>
  method publish(wasm: u8[], declaration: ExtensionDeclaration, manifest: string, signature: PackageSignature, registry: RegistryConfig, credential: RegistryCredential) -> Result<PublishReceipt, RegistryError>
  method authenticate(registry: RegistryConfig, credential: RegistryCredential) -> Result<string, RegistryError>
  verify integration "RegistryClient contract is satisfied"
}

port Registry {
  direction outbound
  category  "io/registry"
  // What operations reach a package registry through (ADR 0010, 0036,
  // 0044, 0045): it lists a package's versions, fetches one that passed the
  // fetch policy and publishes one, each to the one registry that serves
  // the name. Its adapters are the configured registry (production) and
  // the in-memory registry (tests); both keep one contract suite.
  method versions(name: PackageName) -> Result<string[], ExtensionError>
  method fetch(name: PackageName, version: string, allowUnsigned: boolean, trust: string) -> Result<RegistryPackage, ExtensionError>
  method publish(name: PackageName, version: string, wasm: u8[], declaration: ExtensionDeclaration) -> Result<RegistryPublished, ExtensionError>
  method search(query: string, contributes: string) -> Result<RegistrySearched, ExtensionError>
  verify integration "Registry contract is satisfied"
}

port Editor {
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
