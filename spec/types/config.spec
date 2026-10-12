use "types/graph"
use "types/wasm"
use "types/zero-entity-core"

// Configuration types — compiler and extension configuration
//
// DSL-to-type mapping for specforge.json / spec block syntax:
//   providers { <scheme> "<alias>" { extension "..." repo "..." } }
//     → ProviderConfig { scheme, alias (from positional string), extension, settings }

type CompilerConfig {
  schema               string           @optional
  // Serialized as "$schema" in JSON ($ prefix is a JSON convention)
  name                 string           @readonly
  version              string           @readonly
  spec_root            string           @optional
  // Path substrings (not globs), relative to the spec root: a .spec file
  // whose path contains one is not compiled on any surface.
  exclude              string[]         @optional
  strict               boolean          @optional
  namespace            string           @optional
  display_prefix       string           @optional
  extensions           string[]
  providers            ProviderConfig[] @optional
  // Coverage configuration is owned by @specforge/coverage extension.
  // At runtime this is a FieldMap deserialized into the extension's CoverageConfig type.
  coverage             FieldMap         @optional
  // The only source of registries: SpecForge ships none (E063 without one).
  registries           RegistryConfig[] @optional
  // federation config is extension-provided (see @specforge/federation extension)
  // Graph Protocol schema version compatibility range for agent negotiation.
  // Defaults to current major range (e.g., 1.0.0..1.x.x). See ADR graph_protocol_version_management.
  supported_schema_min SchemaVersion    @optional
  supported_schema_max SchemaVersion    @optional
  verify unit "CompilerConfig schema is valid"
}

type ProviderConfig {
  scheme    string   @readonly
  alias     string   @unique
  extension string
  settings  FieldMap @optional
  verify unit "ProviderConfig schema is valid"
}

// CoverageConfig removed — canonical definition lives in
// extensions/coverage/types.spec per Principle 5 (extensions over built-ins).
// CompilerConfig.coverage uses FieldMap; the coverage extension deserializes it.

// ── Registry Types ──────────────────────────────────────────

// One entry of specforge.json's registries. The registry for a package name
// is the first entry whose scope_filter is its scope, else the first marked
// default_registry; none is R-OPS-001 (ADR 0045). A credential is kept per
// alias in ~/.specforge, never here.
type RegistryConfig {
  alias            string  @readonly @unique
  url              string
  scope_filter     string  @optional
  default_registry boolean @optional
  verify unit "RegistryConfig schema is valid"
}

// ── Package registry wire (ADR 0044) ─────────────────────────
// The JSON a package registry serves and a client reads
// (specforge_registry_wire): one definition each, compiled into both
// specforge-registry-server and specforge-registry-client.

// GET {base}/packages/{name}: every version not yanked, oldest first.
type VersionList "Version List" {
  name     string
  versions string[]
  verify unit "VersionList is the JSON a registry serves for a package's versions"
}

// GET {base}/packages/{name}/{version}: what a registry stores for one
// version. signature and manifest are absent when empty; a signature
// that isn't a PackageSignature makes the reply unreadable (R009).
type PackageMetadata "Package Metadata" {
  name         string
  version      string
  sha256       string
  size_bytes   integer          @optional
  description  string           @optional
  keywords     string[]         @optional
  publisher    string           @optional
  published_at string           @optional
  wasm_url     string
  signature    PackageSignature @optional
  manifest     string           @optional
  verify unit "PackageMetadata is the JSON a registry serves for one version"
}

// A publisher's Ed25519 signature over a package's name and version as
// declared and the SHA-256 of the exact binary and manifest uploaded,
// with the public key and time that verify it offline
// (specforge_registry_wire::PackageSignature, ADR 0061). sig is 128 hex,
// public_key 64 hex (not a small-order key), signed_at RFC 3339; no other
// key. Its key id is the first 16 hex of SHA-256 of the public key. Only a
// well-formed one reads.
type PackageSignature "Package Signature" {
  sig        string
  public_key string
  signed_at  string
  verify unit "PackageSignature is the JSON of a publisher signature, and only a well-formed one reads"
}

type SearchHit "Search Hit" {
  name        string
  version     string
  description string @optional
  verify unit "SearchHit is the JSON of one search hit"
}

// GET {base}/search's query (specforge_registry_wire::SearchQuery): what to
// look for, at most how many (50 when absent), and a declared category a
// package must declare.
type SearchQuery "Search Query" {
  q           string
  limit       integer @optional
  contributes string  @optional
  verify unit "SearchQuery is the query string a registry search reads"
}

// GET {base}/search?q=&limit=&contributes=: each package one of whose
// versions matches q, at the version latest picks (the highest release, a
// pre-release only when it has none: what specforge add <name> installs);
// contributes keeps those whose listed version's declaration declares that
// category.
type SearchResults "Search Results" {
  results SearchHit[]
  verify unit "SearchResults is the JSON a registry answers a search with"
}

// PUT {base}/packages/{name}/{version}, answered 201: what the registry
// stored, which the publishing client checks against what it sent.
type PublishReceipt "Publish Receipt" {
  name       string
  version    string
  sha256     string
  size_bytes integer
  // The key id of the signature the registry verified over the upload.
  key_id     string
  verify unit "PublishReceipt is the JSON a registry answers a publish with"
}

// POST {base}/auth/verify.
type TokenVerified "Token Verified" {
  valid      boolean
  scope      string @optional
  label      string
  expires_at string @optional
  verify unit "TokenVerified is the JSON a registry answers a token check with"
}

// What a registry refused, named the same by server and client
// (specforge_registry_wire::ErrorCode); a client reads a code it doesn't
// know as UNKNOWN.
type RegistryErrorCode = "NOT_FOUND"
  | "UNAUTHORIZED"
  | "FORBIDDEN"
  | "ADMIN_REQUIRED"
  | "SCOPE_OWNED"
  | "RATE_LIMITED"
  | "BAD_REQUEST"
  | "INVALID_NAME"
  | "INVALID_VERSION"
  | "UNSIGNED_PACKAGE"
  | "INVALID_SIGNATURE"
  | "INVALID_WASM"
  | "INVALID_MANIFEST"
  | "NAME_MISMATCH"
  | "DUPLICATE_VERSION"
  | "INTEGRITY_VIOLATION"
  | "DB_ERROR"
  | "DB_TASK"
  | "AUTH_TASK"
  | "INTEGRITY_TASK"
  | "STORAGE_ERROR"
  | "STORAGE_TASK"
  | "UNKNOWN"

// Every error answer: {"error": {"code", "message"}}.
type RegistryErrorBody "Registry Error Body" {
  code    RegistryErrorCode
  message string
  verify unit "RegistryErrorBody is the JSON of every registry error"
}

// What the Registry port's fetch hands an operation: a package that passed
// the fetch policy (specforge_ops::registry::Package; its binary omitted here).
type RegistryPackage "Registry Package" {
  name        PackageName
  version     string
  sha256      string
  declaration ExtensionDeclaration
  key_id      string @optional
  verify unit "RegistryPackage is what a package that passed the fetch policy hands an operation"
}

// What the Registry port's search answers (specforge_ops::registry::Searched):
// each package found, with the registry it was found in, and the diagnostic
// of each registry that failed.
type RegistrySearched "Registry Searched" {
  found    SearchHit[]
  failures Diagnostic[]
  asked    integer
  verify unit "RegistrySearched is what a search over the configured registries answers with"
}

// What the Registry port's publish answers: the registry that took the
// package and the publisher key it is signed with (specforge_ops::registry::Published).
type RegistryPublished "Registry Published" {
  registry    string
  url         string
  key_id      string
  key_created boolean
  verify unit "RegistryPublished is what the registry that took a publish answers with"
}

// ── Registry Authentication ───────────────────────────────

// One entry of ~/.specforge/credentials.json, keyed by registry alias:
// where that registry's token comes from. Never in specforge.json. At most
// one of token_env and token_file is set; with neither, login stored the
// secret (in the OS keyring when in_keyring, else in a 0600 file under
// ~/.specforge/secrets/).
type RegistryCredential "Registry Credential" {
  alias      string  @readonly @unique
  token_env  string  @optional
  token_file string  @optional
  in_keyring boolean @optional
  expires_at string  @optional
  verify unit "RegistryCredential schema is valid"
}

type InitConfig {
  name        string
  spec_root   string   @optional
  extensions  string[] @optional
  interactive boolean  @optional
  version     string   @optional
  verify unit "InitConfig schema is valid"
}

// ProjectConfig is the serialization shape of specforge.json — the subset
// of CompilerConfig that users edit directly. CompilerConfig extends this
// with computed fields (schema, watch settings, etc.) derived
// at compile time. ProjectConfig -> CompilerConfig is a one-way transform.
type ProjectConfig {
  name       string
  version    string           @optional
  spec_root  string
  extensions string[]
  providers  ProviderConfig[] @optional
  verify unit "ProjectConfig schema is valid"
}

type InitError {
  kind      "already_exists" | "unresolvable_extension" | "invalid_name" | "io_error"
  message   string
  path      string @optional
  extension string @optional
  verify unit "InitError schema is valid"
}

type BundledExtensionCatalog {
  extensions BundledExtensionEntry[]
  verify unit "BundledExtensionCatalog schema is valid"
}

type BundledExtensionEntry {
  name        string   @readonly
  description string   @optional
  tags        string[] @optional
  // Display ordering hint (lower = more prominent). Ordering MUST NOT
  // favor any single domain — software, compliance, design, data, etc.
  // must receive equal prominence. See P2: zero domain knowledge in core.
  priority    u32      @optional
  verify unit "BundledExtensionEntry schema is valid"
}
