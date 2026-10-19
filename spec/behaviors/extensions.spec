// Extension behaviors — extensions, providers, renderers

use "events/compilation"
use "events/extensions"
use "invariants/core"
use "invariants/extensions"
use "invariants/validation"
use "invariants/wasm"
use "invariants/zero-entity-core"
use "ports/outbound"
use "types/config"
use "types/diagnostics"
use "types/errors"
use "types/wasm"
use "types/zero-entity-core"

defaults behavior {
  features [extension_registry]
  types    [Diagnostic, SchemeRegistryEntry]
}

// load_extension_manifests is the top-level orchestrator for extension loading.
// It reads each extension's declaration through load_extension_declaration and
// hands them all to build_registries_from_declarations.
behavior load_extension_manifests "Load Extension Manifests" {
  features   [extension_management]
  invariants [
    registry_population_before_validation,
    zero_domain_knowledge_core,
    extension_load_order_determinism,
    offline_first_extension_resolution,
    compilation_pipeline_ordering,
  ]
  category   command
  ports      [FileSystem]
  types      [ExtensionDeclaration, CompilerConfig, ExtensionError]
  consumes   [all_files_parsed]
  produces   [extension_manifests_loaded]
  requires {
    all_files_parsed            "all_files_parsed event has fired, confirming all .spec files have been structurally parsed"
    extensions_config_available "Extensions list in specforge.json is available from the parsed project configuration"
  }
  ensures {
    all_extensions_attempted    "All declared extensions have been attempted for loading"
    loaded_manifests_available  "Successfully loaded manifests are available for registry population"
    failed_extensions_diagnosed "Failed extensions have produced diagnostics"
    loaded_event_fired          "extension_manifests_loaded event fires exactly once after all extensions are processed"
  }
  maintains {
    extension_isolation "A failure loading one extension does not prevent loading of remaining extensions"
  }
  contract   """
    At startup, the compiler MUST read the extensions list from specforge.json
    and read each extension's declaration (load_extension_declaration): its
    entity types, edge types, validation rules and optional peer
    dependencies, from the binary itself. Missing extensions or unloadable
    .wasm binaries MUST produce a diagnostic, not a crash. A builtin loads
    from the binary the host embeds; the builtins are one table, each by
    the directory its crate lives in under extensions/ (specforge_component).
    Any other entry names an installed extension by its
    bare name (a legacy name@version entry names the same extension): it
    MUST load from .specforge/extensions/<name>/extension.wasm under that
    name, on every surface, only when the binary's hash is the one its
    specforge.lock entry records; a mismatch, or a binary that declares
    another extension than its entry, MUST be refused with E070, and an
    extension enabled but not installed MUST produce E028 naming the
    command that installs it. A specforge.lock that is there and can't be
    read MUST be reported once (E033), before an E028 for each installed
    extension it leaves unloaded. An entry ending in .wasm names a component
    file instead (path.wasm, or name=path.wasm; a relative path is relative
    to the project root): it MUST load from that file, on every surface,
    under the name the component declares, the one rule the runtime and the
    environment both read an entry by; a name written before = MUST be the
    declared one. Which extension each entry names is decided once (a
    builtin's name before a lock entry; .wasm file entries after every named
    entry), and the extension load and the lock plan of add, init and update
    read that one decision. A file that does not exist, cannot be read, does not load as a
    component, declares another name than the one written, or declares an
    extension another entry already loads MUST produce E028 naming the entry.
    An entry whose declaration can't be read (its handshake, a describe
    category or its protocol major) MUST produce one E028 that says so and
    names what failed, in the same words for every kind of entry. Each
    entry's bytes are read once and loaded under one key, and its
    declaration is read once (one handshake, then each describe category).
    An entry refused at any step — its bytes not a component, its
    declaration unreadable, or the name it declares not the one its entry
    admits — MUST leave nothing loaded. A .wasm file entry's load warnings
    (W153, W138) name the entry. A
    specforge.json that is there and is not used as written MUST produce
    the error E069 naming why, one per problem, before any other
    diagnostic. When the file is not readable, not JSON or not a JSON
    object, or its extensions value is not an array, the compile loads no
    extension, and the I002 that follows MUST say that specforge.json
    could not be read, not that no extensions are configured. A key of the
    wrong type (name, version or spec_root not a string, exclude not an
    array) is replaced by its default, and an extensions or exclude item
    that is not a string, or an exclude item that can match no source
    (resolve_use_imports), is ignored; the rest of the file is used. A
    missing specforge.json is the default config and produces no E069.
    Before any source, the environment MUST report its own diagnostics —
    E069, the extension load's entry by entry (an entry's failure, or what
    it reported though it loaded: W149, W153, W138; the lock's E033 once,
    before the first entry it left unloaded), provider registration (W118,
    E057), then I002 — and then the registry build's, in the build's one
    order (build_registries_from_declarations). Each MUST say the part of
    the setup it is about: the config, the lock, one entry, structural-only,
    the providers, or the registry build's part.
    This behavior orchestrates: for each extension, it loads its
    binary and reads its declaration once. Once all declarations are loaded
    and the extension_manifests_loaded event is produced, the registry build
    (build_registries_from_declarations) validates them and populates the
    KindRegistry, FieldRegistry, and EdgeRegistry.
  """
  verify unit "installed extension manifest is loaded"
  verify unit "an installed extension loads only when its binary is the one its specforge.lock entry pins"
  verify integration "tampered installed binary refused via lockfile hash pin (E070)"
  verify unit "an unreadable specforge.lock is reported once (E033) and each installed extension it leaves unloaded is E028 naming it"
  verify integration "an extension installed from a registry loads through check"
  verify integration "an enabled extension with no installed binary produces E028 naming the command that installs it"
  verify integration "an entry naming a .wasm file loads that component from disk under the name it declares"
  verify unit "the extension load and the lock plan resolve an entry by one rule"
  verify integration "a .wasm file entry that is missing, is not a component, names another extension or repeats a loaded one produces E028 naming the entry"
  verify unit "an entry whose declaration can't be read is E028 saying so and naming what failed"
  verify unit "a .wasm file entry that cannot be read is E028 saying it cannot be read"
  verify unit "an entry refused at any step of its load leaves nothing loaded"
  verify unit "an entry's declaration is read once, with one handshake"
  verify unit "a .wasm file entry's load warnings name the entry"
  verify unit "missing extension produces diagnostic"
  verify unit "every builtin is listed once, by the directory its crate lives in"
  verify unit "every fixture crate is listed once, by its directory under fixtures/"
  verify unit "a declaration declares entity types and validations"
  verify integration "two extensions loaded and registries populated without collision"
  verify unit "unloadable extension binary produces diagnostic instead of crash"
  verify unit "E069 names why specforge.json can't be used"
  verify unit "E069 names a mistyped key or a non-string item, and the rest of specforge.json is used"
  verify integration "a specforge.json that is there and can't be used produces E069 first and an I002 that names it"
  verify integration "the environment reports its own diagnostics, then the registry build's, before any source"
  verify integration "the environment says what part of the setup each of its diagnostics is about, and reports an entry's load diagnostics together"
  verify contract "Load Extension Manifests: extension manifest loading holds — all_files_parsed, extensions_config_available, all_extensions_attempted, loaded_manifests_available, failed_extensions_diagnosed, loaded_event_fired, extension_isolation"
}

behavior load_extension_declaration "Load Extension Declaration" {
  features   [extension_management]
  invariants [zero_domain_knowledge_core, registry_population_before_validation]
  category   command
  ports      [WasmRuntime]
  types      [ExtensionDeclaration, HandshakeResponse]
  contract   """
    The host MUST read an extension's declaration through one loader: its
    handshake, then every describe category (entities, edges,
    shared_fields, enhancements, validation_rules, surfaces, collectors,
    analyzers, passes, feature_flags), the only ones the protocol defines.
    A category that does not parse MUST fail the extension's load (E028)
    naming the category. A describe item key the protocol does not define
    MUST produce W138. The declaration is read once per environment load;
    nothing describes a category again outside it. A handshake whose
    protocol major version differs from the host's MUST fail the
    extension's load (E028), and none of its categories are read. The
    loader, not the handshake call, applies the execution budget the
    handshake declares (sandbox_policy.max_execution_ms) to the
    extension's later calls.
  """
  verify integration "every committed SDK blob's handshake and describe answers match their pinned snapshot byte for byte"
  verify integration "the declaration snapshots are exactly the committed SDK blobs'"
  verify integration "every committed SDK blob's declaration round-trips through the typed declaration byte for byte, but for the legacy keys it declares"
  verify integration "every committed SDK blob answers the host's protocol version; a bridge component answers the version it declares"
  verify unit "a declaration round-trips through its wire answers unchanged"
  verify unit "a describe category that does not parse fails the load naming the category"
  verify unit "a describe item key the protocol does not define produces W138"
  verify unit "the declared categories are the protocol's only describe categories"
  verify unit "an absent short is the name's last segment"
  verify unit "a declaration's default short name is its package name's base"
  verify unit "the SDK's short name reaches the handshake as ext_short"
  verify unit "a short name that is not lowercase kebab case is refused when the extension is built"
  verify unit "a raw category that does not parse panics when the extension is built"
  verify unit "the SDK's peer and optional_peer calls reach the handshake as peer_dependencies, in call order"
  verify integration "the loader reads the handshake and every describe category once"
  verify integration "an extension that only declares commands registers its commands"
  verify integration "an extension that only declares passes has them in its declaration"
  verify integration "the declared short name reaches the registry build"
  verify integration "an unsupported protocol major version fails the load"
  verify unit "the loader applies the execution budget its handshake declares, and a handshake call alone applies none"
  verify unit "the SDK's raw category and the host's load parse a category's items through one function"
}

behavior build_registries_from_declarations "Build Registries From Declarations" {
  features   [extension_management]
  invariants [zero_domain_knowledge_core, registry_population_before_validation]
  category   validation
  types      [ExtensionDeclaration, RegistryBuild]
  produces   [registries_populated]
  contract   """
    The registry build MUST take the loaded declarations in entry order, put
    them in load order (registry_build_load_order) and own everything derived
    from them: identity and shape (E030: an empty name or version, a
    malformed ext_short), self-consistency (W021), peer requirements (E073,
    E027, and E027 for a cycle among required peers), the order of declared
    passes (W145 when their constraints form a cycle, declaration order
    kept), the kind, field and edge registries, the rules, the surfaces and
    the graph inputs (registry_build_graph_inputs). It MUST report its
    diagnostics in one order: the declarations' own, the registries', the
    custom rules' probes, then the surfaces'. Each diagnostic MUST say what
    it is about: a declaration (E030, W021, W145), the peers (E073, E027),
    the registries (W019, W021, I004, W017), a rule (W150, W147, W112), a
    surface (E055), or a collision of two contributions on one name (E026,
    W018, W023, E039). A custom rule whose function
    its extension cannot answer through is W112, after the registries'
    diagnostics, once, when the build is probed in the runtime its
    extensions loaded in. It MUST be pure: the probe, through the
    CustomVerdicts port its caller passes, is the one call it makes out.
    It is read only through its methods; nothing assigns its registries.

    Its outcomes are stated by registry_build_kinds, registry_build_fields,
    registry_build_edges, registry_build_graph_inputs, registry_build_rules,
    registry_build_declaration_consistency,
    registry_build_peer_dependencies and registry_build_load_order. The build produces
    registries_populated: nothing reads a registry before every loaded
    declaration is in it.
  """
  verify integration "the registry build of the builtins matches its pinned snapshot"
  verify unit "a declaration with an empty name or version produces E030"
  verify unit "a malformed ext_short produces E030"
  verify unit "peer dependencies are checked in the registry build"
  verify unit "declared passes are ordered in the registry build"
  verify unit "a pass constraint cycle produces W145 and keeps declaration order"
  verify integration "a passes description that does not parse fails the extension's load"
  verify unit "a build of no declarations has empty registries, no rules and no diagnostics"
  verify unit "the declarations' own diagnostics come in a fixed order: E030, W021, the peers' E073 and E027, the cycles' E027, W145"
  verify unit "the registry build reports its diagnostics in one order: the declarations', the registries', the custom rules' probes, the surfaces'"
  verify unit "probing the build reports each custom rule whose function cannot answer once, as W112 after the registries' diagnostics"
  verify unit "the registry build says what each of its diagnostics is about: a declaration, the peers, the registries, a rule, a surface or a collision"
  verify integration "every loaded declaration is registered before a compile checks anything"
}

// register_extension_entity_types is a thin delegation wrapper that calls
// registry_build_kinds (behaviors/zero-entity-registries.spec)
// for each loaded extension. The detailed registration semantics — including
// KindRegistry population, field registry setup, and edge type registration —
// are defined in the zero-entity-core behaviors.
behavior register_extension_entity_types "Register Extension Entity Types" {
  features   [extension_management]
  invariants [reference_resolution_completeness, zero_domain_knowledge_core]
  category   command
  types      [ExtensionDeclaration, KindRegistryEntry]
  consumes   [extension_manifests_loaded]
  produces   [extension_entity_types_registered]
  requires {
    extension_manifests_loaded_fired "extension_manifests_loaded event has fired, confirming all extension manifests have been loaded and validated"
  }
  ensures {
    kind_registry_populated     "KindRegistry contains entries for all entity kinds from loaded extensions"
    field_registry_populated    "FieldRegistry contains field definitions from loaded extensions"
    edge_registry_populated     "EdgeRegistry contains edge types from loaded extensions"
    registered_event_emitted    "extension_entity_types_registered event fires exactly once after all registries are populated"
    soft_resolution_for_missing "Uninstalled extension kinds produce I004 info diagnostic with suggested extension"
  }
  contract   """
    After loading manifests, the compiler MUST register each extension's
    entity types by delegating to registry_build_kinds
    for kind registration, registry_build_fields for
    field registration, and registry_build_edges for
    edge types. When resolving references, the KindRegistry MUST be
    consulted to determine which extension owns each entity type and
    whether soft resolution applies for cross-extension references.
    When a reference targets an entity kind from an uninstalled extension,
    the compiler MUST emit I004 at the reference, naming the reference,
    the field and the kind ("reference 'x' in field 'f' of 'e' targets
    kind 'k', which no enabled extension provides"), and suggesting the
    extension that declares the kind when a builtin does
    (`specforge add @specforge/{extension}`), the one E024 names.
  """
  verify unit "delegates to registry_build_kinds per extension"
  verify unit "unregistered type triggers soft resolution"
  verify unit "KindRegistry records source extension for each kind"
  verify unit "I004 message includes unresolved kind name and suggested extension"
  verify contract "Register Extension Entity Types: extension entity type registration holds — extension_manifests_loaded_fired, kind_registry_populated, field_registry_populated, edge_registry_populated, registered_event_emitted, soft_resolution_for_missing"
}

behavior load_provider_configurations "Load Provider Configurations" {
  features   [provider_based_ref_validation]
  invariants [reference_resolution_completeness, zero_domain_knowledge_core]
  category   query
  types      [ProviderConfig, CompilerConfig]
  ports      [FileSystem]
  consumes   [extension_manifests_loaded]
  produces   [provider_configured]
  requires {
    extension_manifests_loaded_fired "extension_manifests_loaded event has fired, confirming extension manifests are available for provider lookup"
    specforge_json_available         "specforge.json has been parsed and provider blocks are accessible"
  }
  ensures {
    provider_instances_created  "Provider instances are created for all configured provider blocks"
    aliased_instances_distinct  "Multiple instances of the same provider with different aliases are distinct"
    no_hardcoded_schemes        "No provider schemes or kinds are hardcoded in core"
    provider_configured_emitted "provider_configured event fires exactly once after all providers are instantiated"
  }
  contract   """
    The compiler MUST parse provider blocks from specforge.json and
    create provider instances with their configured settings. In
    specforge.json, providers is an array of {scheme, alias, extension,
    settings} entries, kept in declaration order; scheme, alias and
    extension are required, and an entry missing one, or a providers value
    that is not an array, is W118. The core
    MUST NOT hardcode any provider schemes or kinds — all provider
    configuration comes exclusively from specforge.json and extension
    manifests. Multiple instances of the same provider with different
    aliases MUST be supported, each instance with its own scheme.
  """
  verify unit "single provider instance is created"
  verify unit "multiple aliased instances are created"
  verify unit "provider config settings are passed through"
  verify unit "no hardcoded provider schemes exist in core"
  verify contract "Load Provider Configurations: provider configuration loading holds — extension_manifests_loaded_fired, specforge_json_available, provider_instances_created, aliased_instances_distinct, no_hardcoded_schemes, provider_configured_emitted"
}

behavior register_provider_schemes "Register Provider Schemes" {
  features   [provider_based_ref_validation]
  invariants [reference_resolution_completeness, diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  types      [ProviderConfig, ExtensionDeclaration, SchemeRegistryEntry, Diagnostic]
  ports      [WasmRuntime]
  consumes   [provider_configured]
  produces   [provider_schemes_registered]
  requires {
    provider_configured_fired "provider_configured event has fired, confirming provider instances are created"
    wasm_runtime_available    "WasmRuntime port is available for querying provider extension manifests"
  }
  ensures {
    schemes_registered         "All provider schemes are registered as SchemeRegistryEntry entries"
    duplicate_scheme_warned    "Duplicate schemes produce E057 error listing both providers"
    declaration_order_tiebreak "Duplicate scheme conflicts resolved by specforge.json declaration order"
    schemes_registered_emitted "provider_schemes_registered event fires exactly once after all schemes are registered"
  }
  contract   """
    After loading provider configurations from specforge.json, the compiler
    MUST query each provider extension's manifest for the ref schemes and
    kinds it supports. Each scheme MUST be registered as a SchemeRegistryEntry
    so that validate_provider_refs can route refs to the correct provider.
    The core MUST NOT contain any built-in scheme registrations — all
    schemes come exclusively from provider extensions. Schemes already
    registered by another provider MUST produce an E057 error listing
    both providers; the provider declared first in the specforge.json
    providers array MUST win the scheme registration as a deterministic
    tiebreaker. Unresolvable provider extensions MUST produce an
    ExtensionError diagnostic. The providers are registered once per
    environment: the compile's I005 check and the providers listing
    (CLI and MCP) read that one registration, never specforge.json again.
  """
  verify unit "provider schemes registered from manifest"
  verify unit "duplicate scheme from two providers produces E057"
  verify unit "duplicate scheme resolved by specforge.json declaration order"
  verify unit "unresolvable provider extension produces ExtensionError"
  verify unit "no built-in schemes exist before provider loading"
  verify integration "Wasm-based provider scheme registered and validates ref"
  verify integration "the providers listing reads the environment's registration, never specforge.json again"
  verify contract "Register Provider Schemes: provider scheme registration holds — provider_configured_fired, wasm_runtime_available, schemes_registered, duplicate_scheme_warned, declaration_order_tiebreak, schemes_registered_emitted"
}

behavior validate_provider_refs "Validate Provider Refs" {
  features   [provider_based_ref_validation]
  invariants [reference_resolution_completeness, zero_domain_knowledge_core]
  category   validation
  types      [SchemeRegistryEntry, Diagnostic]
  ports      [RefValidator]
  consumes   [provider_schemes_registered]
  produces   [provider_ref_validated]
  requires {
    provider_schemes_registered_fired "provider_schemes_registered event has fired, confirming scheme-to-provider routing is established"
    ref_validator_available           "RefValidator port is available for delegating validation"
  }
  ensures {
    known_scheme_delegated   "Refs with known schemes are delegated to the corresponding provider"
    unknown_scheme_diagnosed "Refs with unknown schemes emit I005 diagnostic"
    ref_validated_emitted    "provider_ref_validated event fires for each validated ref"
  }
  contract   """
    When a ref entity uses a registered scheme, the compiler MUST
    delegate validation to the corresponding provider. The provider
    MUST validate the kind and identifier format. Unknown schemes
    MUST emit I005.
  """
  verify unit "known scheme delegates to provider"
  verify unit "unknown scheme emits I005"
  verify unit "provider validates identifier format"
  verify unit "no built-in ref validation logic exists in core"
  verify contract "Validate Provider Refs: provider ref validation holds — provider_schemes_registered_fired, ref_validator_available, known_scheme_delegated, unknown_scheme_diagnosed, ref_validated_emitted"
}

// remove_extension is the user-facing CLI entry point for extension removal.
// It delegates to uninstall_wasm_extension (behaviors/wasm-lifecycle.spec) for the full
// Wasm lifecycle cleanup. This behavior owns the CLI interaction and post-removal
// diagnostic messaging; uninstall_wasm_extension owns the implementation.
behavior remove_extension "Remove Extension" {
  features   [extension_management]
  invariants [
    reference_resolution_completeness,
    zero_domain_knowledge_core,
    peer_dependency_satisfaction,
  ]
  category   command
  types      [CompilerConfig, ExtensionError, Diagnostic, UnknownKindError]
  ports      [FileSystem]
  produces   [extension_removed]
  requires {
    extension_installed  "Target extension is present in specforge.json extensions list"
    filesystem_available "FileSystem port is available for config and binary operations"
  }
  ensures {
    extension_entry_removed   "Extension entry removed from specforge.json via uninstall_wasm_extension"
    spec_files_unchanged      "Existing .spec files are not modified by the removal"
    extension_removed_emitted "extension_removed event fires exactly once after successful removal"
  }
  contract   """
    When specforge remove <extension-specifier> is invoked, the system MUST
    delegate to uninstall_wasm_extension (behaviors/wasm-lifecycle.spec) for the full
    Wasm lifecycle cleanup: removing the extension entry from specforge.json,
    deleting the .wasm binary, updating
    specforge.lock, and checking peer dependencies. A builtin extension has
    no binary or lock entry: removing it removes its specforge.json entry.
    A .wasm file entry (read by the rule load_extension_manifests loads it
    by) MUST be removable by the name its component declares, as the
    extensions listing names it, or by the entry as specforge.json writes
    it (or its path); removing it MUST only drop that entry from
    specforge.json, never delete the file nor touch specforge.lock, with
    the same dependents (E027) and stranded-entity checks as any extension. A name
    more than one specforge.json entry enables MUST be refused as
    ambiguous (extension_conflict), naming the entries and changing
    nothing. An entry that enables an extension neither builtin nor
    installed (it does not load, E028) MUST be removable by its name,
    dropping only its specforge.json entry; a name no entry or lock entry
    has is extension_not_found (E072 when it is no package name); while
    specforge.lock is there and can't be read, a removal whose answer the lock
    would decide (an entry of unknown origin, a name the project has nothing
    under) MUST be refused with the lock's E033, the refusal add and update
    give. Every
    refusal MUST be decided before anything is
    written, and a specforge.json the compile could not read refuses every
    removal (config_invalid), changing nothing. A removal writes the
    specforge.json entries it judged: when specforge.json no longer holds
    what the compile read, it MUST be refused (file_changed) and write
    nothing. A removal is all or
    nothing: a failure at any step leaves specforge.json, specforge.lock
    and the binary as they were.
    A removal is judged by the one peer rule over the project's peer set
    as the removal leaves it (ADR 0041; the extensions its specforge.json
    entries load), as add judges an install: removing an extension a
    member of the set requires as a non-optional peer MUST fail with E027
    ("removing '<name>' breaks '<dependent>': ...", its suggestion naming
    every such dependent), unless --force is given; a locked extension no
    entry enables is not judged and keeps nothing from being removed.
    specforge remove --dry-run reports what the removal would do, the
    same refusal included, and writes nothing. The CLI and the MCP
    remove_extension tool MUST run the same removal. This behavior is the
    user-facing CLI entry point; uninstall_wasm_extension handles the
    implementation. Existing .spec files using the extension's entities
    MUST NOT be modified. On the next compilation, entity blocks using the
    removed extension's keywords MUST produce E024 (unknown entity kind)
    since the keyword is no longer in the KindRegistry. Reference list
    entries pointing to those entities MUST produce E003 (dangling
    reference). The user MUST either reinstall the extension or remove
    the affected entity blocks. Its JSON output is the remove document
    specforge.remove_extension answers (McpRemoveExtensionResult: the
    extension, dry_run, its source and version, the stranded entities), with the
    files it wrote or deleted as files_written (absent from a dry run).
  """
  verify unit "delegates to uninstall_wasm_extension for lifecycle cleanup"
  verify unit "extension is removed from extensions list"
  verify unit "removed extension keywords produce E024 on next compile"
  verify unit ".spec files are not modified by removal"
  verify contract "Remove Extension: extension removal holds — extension_installed, filesystem_available, extension_entry_removed, spec_files_unchanged, extension_removed_emitted"
  verify unit "specforge remove for non-existent extension reports error"
  verify unit "an enabled entry that is not installed is removed from specforge.json by its name"
  verify integration "remove --format json lists the files it wrote in files_written"
  verify unit "the remove document names the extension's source and version whatever its origin"
  verify unit "specforge remove with no lock file reports error"
  verify unit "a removal that fails changes nothing"
  verify integration "removing an installed extension drops its specforge.json entry"
  verify integration "removing an extension another installed extension requires fails with E027 unless --force"
  verify integration "a .wasm file entry is removed by the name it declares or by its entry as written, leaving its file in place"
  verify integration "a dry-run removal of a .wasm file entry names the entry without saying it loaded"
  verify integration "a name more than one specforge.json entry enables is refused as ambiguous, naming the entries"
  verify integration "removing a .wasm file entry another extension requires fails with E027 unless --force"
  verify unit "remove and check agree: a removal check would report E027 for is refused, and a removal that passes leaves none"
  verify integration "a locked extension no specforge.json entry enables does not keep its peer from being removed"
  verify integration "a removal with an unreadable specforge.json is config_invalid and changes nothing"
  verify unit "a removal over a specforge.json changed since the compile writes nothing"
  verify integration "a removal judged on the compiled specforge.json never writes another"
}

// Read-only query. (produces [] declared below; no event of its own.)
behavior list_installed_extensions "List Installed Extensions" {
  features   [extension_management]
  invariants [diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  types      [ExtensionDeclaration, KindRegistryEntry, McpExtensionsResult]
  requires {
    kind_registry_ready "KindRegistry is populated with entity kinds from loaded extensions"
  }
  ensures {
    all_extensions_listed  "Every extension the project enables, locks or loads is included in the output"
    entity_counts_included "Each extension entry includes entity count and registered entity types"
    output_deterministic   "Output order is alphabetical by extension name"
  }
  contract   """
    When specforge extensions is invoked, the system MUST list every extension
    the project has (enabled by a specforge.json entry, locked in specforge.lock, or loaded: the extension inventory) with their name, version, entity count, and registered entity types.
    The listing MUST query the KindRegistry to enumerate entity kinds per
    extension. Each entry MUST carry its source: where the extension load
    loaded it from (the entry that loaded it, read by the load's one rule,
    load_extension_manifests: builtin for a builtin's name, the lock
    entry's source for an installed one, file:<path> for a .wasm file
    entry, listed under the name it declares), else where its first entry
    in load order would load it from, else its lock entry's source, else
    unknown; its version: the
    loaded one, else an installed extension's locked one, else the version
    a legacy name@version entry writes; and its status (loaded, not_loaded,
    not_configured). The CLI and the MCP extensions tool MUST answer the same
    document (McpExtensionsResult: the entries, their count, the lock's
    entries and the kinds the graph uses).
    Output order MUST be deterministic (alphabetical by extension name).
  """
  verify unit "list shows all installed extensions"
  verify unit "list includes entity counts and entity types"
  verify unit "output order is deterministic"
  verify integration "a .wasm file entry is listed under the name it declares, loaded, with source file:<path>"
  verify unit "an extension's source is where the extension load takes it from, whatever order its entries are in"
  verify integration "the CLI and the MCP extensions tool answer the same document"
  verify integration "the human listing names its rows the project's extensions, never installed ones, and counts those loaded"
  verify integration "the human listing counts one entity as one entity"
  verify contract "List Installed Extensions: extension listing holds — kind_registry_ready, all_extensions_listed, entity_counts_included, output_deterministic"
}

// Read-only query. (produces [] declared below; no event of its own.)
behavior list_configured_providers "List Configured Providers" {
  features   [provider_based_ref_validation]
  invariants [diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  types      [ProviderConfig, SchemeRegistryEntry]
  requires {
    scheme_registry_ready "SchemeRegistryEntry set is populated from provider registration"
  }
  ensures {
    all_providers_listed       "All configured providers are included in the output"
    schemes_and_kinds_included "Each provider entry includes registered schemes and supported kinds"
    aliases_shown_separately   "Providers with multiple instances show each alias separately"
    output_deterministic       "Output order is deterministic"
  }
  contract   """
    When specforge providers is invoked, the system MUST list all configured
    providers with their alias, extension, registered schemes, and supported
    kinds. The listing MUST query the SchemeRegistryEntry set to show which
    schemes each provider handles, with each provider's status there:
    registered, extension_not_loaded, not_a_provider or scheme_taken.
    Providers with multiple instances MUST show each alias separately.
    Output order MUST be deterministic (declaration order). The CLI and the
    MCP providers tool MUST list the same entries.
  """
  verify unit "list shows all configured providers"
  verify unit "list includes scheme and kind registrations"
  verify unit "multiple aliases shown separately"
  verify unit "output order is deterministic"
  verify integration "the CLI and the MCP providers tool list the same entries"
  verify contract "List Configured Providers: provider listing holds — scheme_registry_ready, all_providers_listed, schemes_and_kinds_included, aliases_shown_separately, output_deterministic"
}

// The management operations are operations over the project view, as the
// read views are (ADR 0015, "Management operations").
behavior management_operations_over_the_project_view "Management Operations over the Project View" {
  features   [extension_management, mcp_project_management_tools]
  invariants [diagnostic_determinism, zero_domain_knowledge_core]
  category   command
  types      [ExtensionDeclaration, Diagnostic]
  ports      [CompilerApi, McpProtocol, FileSystem]
  requires {
    project_compiled "A compiled project, one-shot or a project session's, supplies the project view"
  }
  ensures {
    one_project_read "Each operation reads the config, the enabled entries, the lock, the loaded declarations and the reported diagnostics of the compile behind its view, never specforge.json or specforge.lock again"
    root_for_disk    "Whatever an operation reads or writes on disk is at the view's root; without a root it refuses with no_project, except the listings and doctor"
  }
  contract   """
    The extensions listing, the providers listing, doctor, remove,
    collect, and inference progress and gaps MUST each be one operation
    over the project view and a request, shared by the CLI and MCP; a
    surface builds the view from the project it holds, maps its arguments
    and renders the outcome.
    The extensions listing, doctor and remove MUST read the project's
    extensions from one inventory over the view, one row per name: every
    extension a specforge.json entry enables, the lock pins or the compile
    loaded, with its origin, status, entries, declaration and lock entry; so
    they cannot disagree about which extensions the project has, where each
    comes from or whether it loaded. An operation MUST read the project's config,
    what each specforge.json entry enabled, the specforge.lock the compile
    read (absent, read, or unreadable with its E033 problem), the loaded
    declarations and what the surface reports for the project from the
    view, never by reading specforge.json or specforge.lock again. The
    installed binaries, the source files and the recorded test report MUST
    be read and written at the view's root; specforge.lock is written
    there, at the one path the environment reads it from. Without a root, remove, collect and inference
    progress and gaps MUST refuse with no_project; the listings list what
    the view enabled and loaded, and doctor skips the installation checks.
    add and update run before or instead of a compile, so they read
    specforge.json themselves, through the function the compile reads it
    with; add, update and remove MUST refuse a specforge.json the compile
    reports as E069 and an edit cannot go around, with one refusal
    (config_invalid, the E069 reason as its message), before they write
    anything.
  """
  verify unit "an operation that reads or writes the project on disk refuses a view without a root"
  verify unit "the extensions listing reads the config entries from the view, never specforge.json again"
  verify unit "doctor reads the setup diagnostics of its view's environment, and E013 from what its view reports"
  verify unit "list, doctor and remove read the lock the compile read, once"
  verify unit "list, doctor and remove read one inventory of the project's extensions"
  verify integration "add, update and remove refuse an unusable specforge.json with one refusal, before they write"
  verify unit "collect maps test results to the entities of its view"
  verify contract "Management Operations over the Project View: management operations hold — project_compiled, one_project_read, root_for_disk"
  verify unit "a config problem that leaves the sources or the extensions unknown blocks a rewrite"
}

// Called imperatively by validate_provider_refs (which consumes provider_schemes_registered).
// Depends on SchemeRegistryEntry data populated during provider registration.
behavior validate_ref_target_format "Validate Ref Target Format" {
  features   [provider_based_ref_validation]
  invariants [reference_resolution_completeness, zero_domain_knowledge_core, diagnostic_determinism]
  category   validation
  ports      [RefValidator]
  requires {
    scheme_registry_populated "SchemeRegistryEntry data is populated from provider registration"
    ref_validator_available   "RefValidator port is available for identifier validation"
  }
  ensures {
    valid_identifier_passes        "Valid ref identifiers pass validation without diagnostics"
    malformed_identifier_diagnosed "Malformed ref identifiers produce E011 diagnostic"
  }
  contract   """
    When a provider is installed, the validator MUST check that ref
    identifiers match the provider's expected pattern by delegating to the
    RefValidator port's validateIdentifier method. Malformed identifiers
    MUST produce an E011 diagnostic.
  """
  verify unit "valid ref identifier passes"
  verify unit "malformed ref identifier produces E011"
  verify unit "no built-in format patterns exist in core"
  verify contract "Validate Ref Target Format: ref target format validation holds — scheme_registry_populated, ref_validator_available, valid_identifier_passes, malformed_identifier_diagnosed"
}

// Called imperatively by validate_provider_refs (which consumes provider_schemes_registered).
// Depends on SchemeRegistryEntry data populated during provider registration.
behavior validate_provider_kinds "Validate Provider Kinds" {
  features   [provider_based_ref_validation]
  invariants [reference_resolution_completeness, zero_domain_knowledge_core, diagnostic_determinism]
  category   validation
  ports      [RefValidator]
  requires {
    scheme_registry_populated "SchemeRegistryEntry data is populated from provider registration"
    ref_validator_available   "RefValidator port is available for kind validation"
  }
  ensures {
    valid_kind_passes      "Valid scheme and kind combinations pass validation"
    unknown_kind_diagnosed "Unknown kind for a known scheme produces E013 diagnostic listing valid kinds"
  }
  contract   """
    When a ref uses a known scheme but an unregistered kind, the validator
    MUST delegate to the RefValidator port's validateKind method and
    MUST produce an E013 diagnostic listing the valid kinds for that scheme.
  """
  verify unit "valid scheme and kind passes"
  verify unit "valid scheme with unknown kind produces E013"
  verify unit "no built-in kind registrations exist in core"
  verify contract "Validate Provider Kinds: provider kind validation holds — scheme_registry_populated, ref_validator_available, valid_kind_passes, unknown_kind_diagnosed"
}

// -- Registry Behaviors -----

behavior resolve_registry_source "Resolve Registry Source" {
  invariants [
    registry_integrity,
    multi_error_collection,
    extension_operation_atomicity,
    offline_first_extension_resolution,
  ]
  category   query
  types      [RegistryConfig, PackageMetadata, VersionList, CompilerConfig, ExtensionError]
  ports      [RegistryClient]
  consumes   [registries_configured]
  produces   [registry_resolved]
  requires {
    registries_configured_fired "registries_configured event has fired, confirming registry entries are parsed and available"
    registry_client_available   "RegistryClient port is available for network queries"
  }
  ensures {
    scope_routed              "Scope-prefixed specifiers are routed to the matching scope-specific registry"
    default_fallback_used     "Specifiers with no matching scope fall back to the default registry"
    no_registry_refused       "A name no scope_filter matches, with no default registry, is refused with R-OPS-001 before any network call"
    network_error_diagnosed   "Network errors produce ExtensionError diagnostic with retry guidance"
    registry_resolved_emitted "registry_resolved event fires on successful resolution"
    address_from_root         "Each call goes to the registry's root, then the protocol's version prefix and the call's path; the server mounts the same routes"
  }
  contract   """
    When resolving an extension specifier with @scope/name format, the
    system MUST query the configured registry for that scope. Scope routing
    MUST use the scope_filter field from RegistryConfig entries in
    specforge.json. If no scope-specific registry matches, the system MUST
    fall back to the default registry. "The default registry" means the
    registries entry marked `default_registry: true` in specforge.json:
    SpecForge ships no registry, so no registry URL is a constant in source.
    With no scope match and no default registry, no registry serves the
    name: the system MUST refuse with R-OPS-001, naming the scope, before
    any network call, and MUST NOT ask another registry. add, update and
    publish choose the registry for a name by this one rule (ADR 0045).
    Every call MUST go to the configured registry's root followed by the
    protocol's version prefix (/v1) and the call's path; the server MUST
    mount the same routes (ADR 0075).
    Network errors MUST produce an ExtensionError diagnostic with retry guidance.
  """
  verify unit "scope-specific registry queried for matching scope"
  verify unit "default registry used when no scope filter matches"
  verify unit "network error produces ExtensionError with retry guidance"
  verify unit "successful query returns PackageMetadata"
  verify unit "a fetch requests the name and version it was given, from the registry it was given"
  verify unit "a name no registry serves is refused with R-OPS-001 before any request"
  verify integration "the registry server answers every call in the JSON its client reads"
  verify unit "every call goes to its address under the registry's root, where the server mounts it"
  verify contract "Resolve Registry Source: registry source resolution holds — registries_configured_fired, registry_client_available, scope_routed, default_fallback_used, no_registry_refused, network_error_diagnosed, registry_resolved_emitted, address_from_root"
}

behavior search_registry "Search Registry" {
  invariants [diagnostic_determinism, multi_error_collection, offline_first_extension_resolution]
  category   query
  types      [RegistryConfig, SearchQuery, SearchResults, SearchHit, CompilerConfig]
  ports      [RegistryClient]
  produces   [registry_search_completed]
  requires {
    registries_available      "At least one registry is configured in specforge.json"
    registry_client_available "RegistryClient port is available for network queries"
  }
  ensures {
    all_registries_queried   "All configured registries are queried with the search term"
    results_deduplicated     "Results from multiple registries are deduplicated by name + version"
    output_deterministic     "Output is sorted by name, then version"
    category_filtered        "With --contributes, only packages whose declaration declares that category are listed"
    failures_reported_once   "Each registry that fails is reported once, with its code; search fails when every registry failed"
    search_completed_emitted "registry_search_completed event fires after results are collected"
  }
  maintains {
    partial_failure_resilience "Error from one registry does not abort search of remaining registries"
  }
  contract   """
    When specforge search is invoked, the system MUST query all configured
    registries with the search term. A search matches a package when the
    query is an ASCII case-insensitive substring of its name, its
    description or one of its keywords; % and _ in the query are text. The
    server, the in-memory client and the in-memory registry MUST match by
    this one rule (specforge_package::search::matches).
    Each registry lists a package once,
    when any of its versions matches the term, at the version specforge
    add <name> would install from it (latest: the highest release, a
    pre-release only when the package has none). Results MUST be
    filterable by declared category (--contributes, one of entities,
    edges, shared_fields, enhancements, validation_rules, surfaces,
    collectors, analyzers, passes, feature_flags): a registry lists only
    the packages whose listed version's stored declaration declares
    something in that category, and an unknown category is refused before
    any request. Results from multiple
    registries MUST be merged and deduplicated using a composite key of
    name + version — when the same name + version appears from multiple
    registries, the first registry in specforge.json declaration order
    wins, and each result names that registry. Output MUST be
    deterministic — sorted by name, then version. Each registry that
    fails MUST be reported once, with its code (on stderr under human
    output, in the document's diagnostics under --format json, each in the shape
    every command writes a diagnostic); the document names --contributes only
    when it is given; the others are still
    asked, and the search fails only when every registry failed. Each
    registry is asked with the credential kept for it
    (support_private_registries).
    With no registry configured, search MUST make no network call and MUST
    fail with E063, whose suggestion names the specforge.json registries key.
    A specforge.json that is there and can't be used MUST be refused as add
    refuses it (config_invalid, naming E069), before any network call; so
    for login and publish.
  """
  verify unit "with no registry configured, search makes no network call and reports how to configure one"
  verify unit "an unusable specforge.json is refused with the refusal add gives, before any network call"
  verify unit "queries all configured registries"
  verify unit "a search matches a case-insensitive substring of a package's name, description or one keyword, wildcards included as text"
  verify unit "filters by declared category"
  verify unit "a registry lists each package at the version add installs from it"
  verify unit "each failed registry is reported once, and search fails when every registry failed"
  verify integration "under --format json a failed registry is reported once, in the document's diagnostics, and not on stderr"
  verify integration "search's document writes its diagnostics as check does and carries no null"
  verify unit "deduplicates results across registries"
  verify unit "output is deterministic"
  verify unit "error from one registry does not abort search of others"
  verify contract "Search Registry: registry search holds — registries_available, registry_client_available, all_registries_queried, results_deduplicated, output_deterministic, category_filtered, failures_reported_once, search_completed_emitted, partial_failure_resilience"
}

// CLI entry point: `specforge publish`. Delegates Wasm binary packaging
// to publish_wasm_extension in behaviors/wasm-lifecycle.spec.
behavior publish_to_registry "Publish to Registry" {
  invariants [registry_integrity, multi_error_collection, credential_secrecy]
  category   command
  types      [ExtensionDeclaration, RegistryConfig, PublishReceipt, ExtensionError]
  ports      [RegistryClient, FileSystem]
  produces   [extension_published_to_registry]
  requires {
    declaration_valid         "The binary loads, and the registry build of its declaration alone reports no error"
    wasm_binary_available     "The .wasm component named, or the one its crate directory builds, exists on the filesystem"
    registry_client_available "RegistryClient port is available for upload"
    credentials_available     "Authentication credentials are available for the target registry"
  }
  ensures {
    sha256_computed             "SHA256 hash of .wasm binary is computed and included in the upload"
    duplicate_version_rejected  "A version the registry already holds is refused (R007): a published version is immutable"
    signature_checked_on_upload "The registry refuses an upload whose signature is missing (UNSIGNED_PACKAGE), isn't a package signature, or doesn't verify over its name, version, binary and exact manifest (INVALID_SIGNATURE), before storing anything"
    refusal_reported            "A registry's refusal reaches the publisher as R008 with the registry's code and reason, not as a network error"
    receipt_checked             "Publish checks the registry's receipt names the uploaded name, version, SHA-256, size and key id, else R009"
    registry_url_returned       "Successful publish returns the registry URL for the published version"
    published_event_emitted     "extension_published_to_registry event fires on successful publish"
  }
  contract   """
    When specforge publish is invoked with a registry target, the system
    MUST load the binary's declaration, refuse it when its registry build
    reports an error, and upload the declaration as the package's manifest,
    with the .wasm binary and its SHA256 hash, before any network call
    deciding whether to refuse; the request MUST be authenticated. The
    registry MUST refuse a manifest that is not an extension declaration,
    and takes the description and keywords it shows from the declaration.
    The registry MUST refuse an upload with no signature (UNSIGNED_PACKAGE),
    or whose signature is not a package signature or does not verify over
    the name and version in the upload path, the binary and the exact
    manifest uploaded (INVALID_SIGNATURE), before it stores anything, so a
    version it refuses stays publishable; it judges no key's trust
    (ADR 0061). A version the registry
    already holds MUST be refused (R007): a published version is immutable.
    On the registry server, the first accepted publish into a scope claims
    the scope for its publisher; only that publisher publishes into it
    after. A publish the registry refuses (an invalid upload, a duplicate
    version, a storage failure) claims no scope: the claim and the version
    are recorded together or not at all.
    A registry's refusal MUST reach the publisher as R008 naming the
    registry's code and reason. Publish MUST check that the registry's
    receipt names what it uploaded.
    The package MUST go to the one registry that serves its name, the
    registry add and update fetch that name from (ADR 0045). The upload MUST
    be authenticated: with no credential for that registry
    (SPECFORGE_REGISTRY_TOKEN unset or blank, and none stored for its alias)
    publish MUST refuse with R001 before any network call and MUST NOT
    create a publisher signing key; a signing key that can't be read or
    created MUST be refused with E074 before any network call. Publish
    refuses in one order, each refusal before anything after it is read or
    asked: the binary (E040, E028), its declaration's errors, its name and
    version (E072), the registry configuration (E063, E067), the registry
    for the name (R-OPS-001), the credential (R001, R012, R-AUTH-020,
    R-AUTH-021), the signing key (E074); then the registry's answer.
    Successful publish MUST return the registry URL for the
    published version. With no registry configured, publish MUST make no
    network call and MUST fail with E063, whose suggestion names the
    specforge.json registries key.
  """
  verify unit "with no registry configured, publish makes no network call and reports how to configure one"
  verify unit "the declaration is validated before publish"
  verify integration "publish derives the stored declaration from the binary"
  verify unit "publish refuses a binary whose declaration has errors before any network call"
  verify integration "the registry refuses a manifest that is not an extension declaration"
  verify integration "the registry takes a package's description and keywords from its declaration"
  verify integration "the registry refuses a signature that is not a package signature, and the version stays publishable"
  verify integration "the registry refuses a signature that does not verify over the upload"
  verify unit "a registry's refusal reaches the publisher with its reason"
  verify unit "publish checks the registry's receipt against what it uploaded"
  verify unit "SHA256 computed and included in upload"
  verify unit "a version already published is refused with R007"
  verify integration "the first publish into a scope claims it, and only its publisher publishes there after"
  verify integration "a publish the registry refuses claims no scope"
  verify unit "successful publish returns registry URL"
  verify unit "unauthenticated publish produces ExtensionError"
  verify unit "the registry refuses a name or version that is not a package name or version"
  verify unit "publish refuses in one order, each refusal before anything after it is read or asked"
  verify unit "with no credential for its registry, publish refuses with R001 before any network call and creates no signing key"
  verify unit "the environment token wins over the stored credential"
  verify unit "a signing key that can't be read is refused with E074 before any network call"
  verify unit "publish asks the registry add fetches the same name from"
  verify unit "the declaration's warnings are reported even when publish is refused"
  verify unit "what publish uploads is what add installs"
  verify contract "Publish to Registry: registry publishing holds — declaration_valid, wasm_binary_available, registry_client_available, credentials_available, sha256_computed, duplicate_version_rejected, signature_checked_on_upload, refusal_reported, receipt_checked, registry_url_returned, published_event_emitted"
}

behavior verify_registry_integrity "Verify Registry Integrity" {
  invariants [registry_integrity, wasm_compile_cache_integrity, offline_first_extension_resolution]
  category   validation
  types      [PackageMetadata, LockFileEntry, ExtensionError]
  ports      [FileSystem]
  produces   [registry_integrity_verified]
  requires {
    wasm_binary_downloaded      "A .wasm binary has been downloaded from a registry"
    registry_response_available "PackageMetadata with declared SHA256 hash is available"
  }
  ensures {
    hash_verified              "SHA256 hash of downloaded binary matches the declared hash"
    mismatch_aborts            "Hash mismatch produces hard error and aborts installation"
    lock_file_updated          "The SHA256 hash and the publisher key id are recorded in specforge.lock"
    integrity_verified_emitted "registry_integrity_verified event fires on successful verification"
  }
  contract   """
    After downloading a .wasm binary from a registry, the system MUST
    verify its SHA256 hash against the hash declared in the package's
    PackageMetadata.
    Mismatches MUST produce a hard error and abort installation. The
    SHA256 hash and, for a signed package, its publisher key id MUST be
    recorded in specforge.lock.
  """
  verify unit "matching SHA256 passes verification"
  verify unit "mismatched SHA256 produces hard error"
  verify unit "the sha256 and the publisher key id are recorded in specforge.lock"
  verify contract "Verify Registry Integrity: registry integrity verification holds — wasm_binary_downloaded, registry_response_available, hash_verified, mismatch_aborts, lock_file_updated, integrity_verified_emitted"
}

// Package trust after the SHA256 check (docs/registry-trust.md). The
// registry is not the trust anchor: these checks use only the bytes
// downloaded, the metadata and manifest served, and the user's pins.

behavior check_registry_reply "Check Registry Reply" {
  invariants [registry_reply_binding, registry_integrity]
  category   validation
  types      [PackageMetadata, ExtensionError]
  ports      [RegistryClient, Registry]
  requires {
    reply_received "The registry answered a request for name@version and its download passed the SHA256 check"
  }
  ensures {
    reply_readable         "A reply that isn't the registry protocol's JSON (a malformed publisher signature among them) is refused with R009, even with --allow-unsigned"
    reply_names_request    "A reply naming another package or version than the one requested is refused with R-TRUST-004"
    manifest_names_request "A served manifest naming another package or version is refused with R-TRUST-004"
    manifest_fails_closed  "A missing or unreadable served manifest is refused with R-OPS-004, never read as declaring no peers"
    peers_from_manifest    "The peers the served declaration declares are the ones the ADR-0001 diamond gate checks"
    declaration_matches    "A binary whose declaration differs from the served one is refused with R-TRUST-004 naming the first differing category"
    answer_from_registry   "A 404 without the protocol's NOT_FOUND error, and a wasm_url that is neither an http or https URL nor a path at the registry's root, are refused with R009: no registry gave that answer"
  }
  contract   """
    A reply that isn't the registry protocol's JSON for the request (a
    publisher signature that isn't one among them) MUST be refused (R009)
    before anything else is checked: no flag accepts it.
    A 404 is the registry's "not found" only when it carries the protocol's
    NOT_FOUND error; a 404 without it (an address no registry serves, such as
    a url that isn't the registry's root) MUST be refused with R009, not
    reported as a missing package. A wasm_url MUST be an http or https URL or
    a path at the registry's root.
    Before a registry package's signature is checked or its key pinned,
    the system MUST refuse the reply when it describes another package or
    version than the one requested, or when its manifest does: the
    signature covers the name and version the reply carries, and the pin
    is keyed by name, so an answer for another package would otherwise be
    verified, pinned and installed in its place (R-TRUST-004). The served manifest is the package's
    declaration, whose peers decide the diamond gate before anything is
    loaded: a missing manifest, or one that isn't a readable extension
    declaration (one published before declarations, carrying
    manifestVersion, is refused with a suggestion to re-publish it), MUST
    be refused (R-OPS-004) rather than read as declaring no peers. Once
    loaded, the binary's declaration MUST equal the served one, else the
    package is refused (R-TRUST-004) naming the first differing category.
    A package served with the bytes the project's lock already pins, under
    another version, is loaded and checked as any other: the lock's bytes
    never make another version already present (E028 when the binary
    declares another version).
    A refused reply pins no key and installs nothing.
  """
  verify unit "a reply that is not the protocol's JSON is refused with R009, even with --allow-unsigned"
  verify unit "a 404 without the protocol's NOT_FOUND error is R009, not package not found"
  verify unit "a download URL is an http or https URL or a path at the registry's root, else R009"
  verify integration "a reply for another package is refused and pins nothing"
  verify integration "a manifest that cannot be read is refused and pins nothing"
  verify integration "the peers the served manifest declares reach the package"
  verify unit "add refuses a package whose binary declares other than its published declaration"
  verify unit "the diamond gate decides on the published declaration's peers"
  verify unit "add refuses another version served with the bytes the lock already pins"
  verify integration "a package published with a legacy manifest is refused with a re-publish suggestion"
}

behavior verify_publisher_signature "Verify Publisher Signature" {
  invariants [publisher_trust, registry_integrity]
  category   validation
  types      [PackageMetadata, ExtensionError]
  ports      [RegistryClient]
  requires {
    reply_checked "The registry reply passed the SHA256 check and names the package requested"
  }
  ensures {
    broken_signature_refused "A signature that doesn't verify is refused with R-TRUST-002, even with --allow-unsigned"
    unsigned_refused         "An unsigned package is refused with R-TRUST-001 unless --allow-unsigned is given"
    unsigned_pins_nothing    "An unsigned package accepted with --allow-unsigned pins no key"
    unsigned_reported        "An unsigned package accepted with --allow-unsigned is reported as W155"
  }
  contract   """
    A registry package's Ed25519 publisher signature MUST be verified
    over the canonical payload {name, version, wasmSha256, manifestSha256,
    signedAt} re-derived from the downloaded Wasm bytes and the manifest
    the registry serves, with the public key carried in the signature
    object. A signature that doesn't verify (tampered binary or swapped
    manifest) MUST be refused with R-TRUST-002, and --allow-unsigned MUST
    NOT bypass it; one that isn't a package signature makes the reply
    unreadable (R009, check_registry_reply). A package with no signature MUST
    be refused with R-TRUST-001 unless the user passes --allow-unsigned;
    one accepted that way pins no key. Accepting it is reported as W155 on
    every surface; the check itself writes nothing to the terminal.
  """
  verify integration "a signature over other bytes is refused even with --allow-unsigned"
  verify integration "a swapped manifest breaks the signature"
  verify integration "an unsigned package is refused without --allow-unsigned"
  verify integration "an unsigned package is accepted with --allow-unsigned and pins no key"
  verify integration "an unsigned package accepted with --allow-unsigned is reported as W155"
}

behavior pin_publisher_key "Pin Publisher Key" {
  invariants [publisher_trust, extension_operation_atomicity]
  category   command
  types      [PackageMetadata, LockFileEntry, TrustPolicy, KeyPin, ExtensionError]
  ports      [FileSystem, Registry]
  requires {
    signature_verified "The package's publisher signature verified"
  }
  ensures {
    first_key_pinned    "The key of the first verified install of a package is pinned in the known-keys store"
    same_key_accepted   "A later install signed by the pinned key is accepted unchanged"
    changed_key_refused "A key other than the pinned one is refused with R-TRUST-003 and both key ids, unless the user consents"
    consent_repins      "Consent to a key change (--yes, or yes at the prompt) re-pins the new key"
    denied_key_refused  "A key on denied_keys is refused with R-TRUST-005, even with consent"
    repin_reported      "A re-pinned key is reported as W156 naming both key ids, once it is saved"
    first_use_named     "A first-use pin is named in the operation's outcome (the publisher is pinned now)"
    surface_decides     "Whether to accept a key change is asked of the surface that can ask (the CLI's terminal); MCP and --format json refuse it"
    pinned_when_written "A key is pinned only when the install that fetched it is written, after its lock entry: a refused or failed operation pins nothing"
    store_fails_closed  "A known-keys store that can't be read refuses the install with R-TRUST-006 and is left as it was"
    lock_key_pinned     "With no pin of the user's, the key specforge.lock records for the package is its pin; the user's own pin wins over it"
  }
  contract   """
    Trust on first use: a signed package's publisher key id MUST be pinned
    for the package name in the user's known-keys store, and later installs
    signed by that key MUST be accepted. With no pin of the user's, the key
    the project's specforge.lock records for the package is the pin; the
    user's own pin wins over it. A package signed by another key MUST be
    refused with R-TRUST-003 naming both key ids (and specforge.lock when
    the lock's key is the one expected) unless the user consents (--yes, an
    interactive yes, or the key on trusted_keys); --allow-unsigned is not
    consent. Consent MUST re-pin the new key. A key
    on denied_keys MUST be refused with R-TRUST-005 whatever the consent.
    Fetching a package only decides: its key is pinned when the
    operation's lock plan commits, after the binaries, the lock and
    specforge.json are written, and a pin that can't be saved puts all of
    them back (R-TRUST-006). A refused or failed operation leaves every pin
    as it was and reports no W156. A known-keys store that exists and can't
    be read refuses the install (R-TRUST-006) and is never overwritten. A
    re-pin is reported as W156 once it is saved; a first-use pin is part of
    the outcome. The decision on a key change belongs to the surface: it hands the
    operation who can be asked (the CLI's terminal; nobody under
    `--format json` or over MCP), and the CLI asks on its terminal (y or yes,
    in any case, consents; any other answer refuses).
  """
  verify integration "the key of the first verified install is pinned and accepted again"
  verify integration "a package signed by another key than the pinned one is refused"
  verify unit "a refused key change names the commands that can consent"
  verify integration "consent to a key change re-pins the new key"
  verify integration "a denied key is refused even with consent"
  verify integration "consent to a key change is reported as W156"
  verify integration "a key change is decided by the surface that can ask"
  verify unit "the CLI asks its terminal about a key change: yes at the prompt consents, and with nobody to ask nothing is asked"
  verify integration "a fetch pins nothing; its pins are recorded when the install is written"
  verify unit "a lock plan pins the keys its fetches decided only when it commits"
  verify integration "a refused install pins no key, so the locked version installs again"
  verify integration "a refused update pins no key"
  verify integration "an install refused when its binary loads pins no key"
  verify integration "a refused add of several specifiers pins no key"
  verify integration "a known-keys store that can't be read refuses the install and is left as it was"
  verify integration "with no pin of the user's, the key specforge.lock records is the pin"
  verify integration "a user's pin wins over the key specforge.lock records"
}

behavior configure_registries "Configure Registries" {
  invariants [
    diagnostic_determinism,
    zero_domain_knowledge_core,
    registry_integrity,
    registry_api_openness,
    offline_first_extension_resolution,
  ]
  category   command
  types      [RegistryConfig, CompilerConfig]
  ports      [FileSystem]
  produces   [registries_configured]
  requires {
    specforge_json_parsed "specforge.json has been parsed and registries array is accessible"
    filesystem_available  "FileSystem port is available for reading configuration"
  }
  ensures {
    registry_entries_created      "All registries array entries are parsed into RegistryConfig objects"
    scope_filters_set             "Scope filters are configured for routing @scope/ specifiers"
    no_registries_diagnosed       "Empty registries configuration produces I003 info diagnostic"
    no_hardcoded_urls             "No registry URLs are compiled-in constants in compiler source"
    registries_configured_emitted "registries_configured event fires after all entries are parsed"
    root_urls                     "Each entry's url is the registry's root: an http or https URL with no query, fragment or user info, not ending in the protocol's version prefix /v1; another is refused with E067 naming the root"
    unreadable_entry_refused      "An entry that can't be read refuses every registry operation with E067 before any request: no other registry serves its names"
  }
  contract   """
    At startup, the compiler MUST parse the registries array from
    specforge.json into RegistryConfig entries. Each entry MUST have an
    alias (unique identifier) and a url naming the registry's root (an http
    or https URL with no query, fragment or user info; the client adds the
    protocol's version prefix /v1, and a url ending in it is refused with
    E067 naming the root), and MAY have a scope_filter.
    When resolving extension specifiers, scope_filter MUST route @scope/
    prefixed specifiers to the matching registry. A configuration with no
    registries array, or with no entry marked `default_registry: true`,
    MUST produce an I003 info diagnostic. SpecForge ships no registry: the
    registries array in specforge.json is the only source of registry URLs,
    and no registry URL is a compiled-in constant (SpecForge does not own
    the specforge.dev domain). A registry is NEVER contacted until the user
    initiates a registry operation. With no registry configured, the
    registry operations (add from a registry, update, search, publish,
    login) MUST make no network call and MUST fail with an E063 diagnostic
    whose suggestion names the specforge.json registries key. Builtin
    extensions and local .wasm files MUST install with no registry
    configured. First-use MUST NOT require network access — registries are
    opt-in configuration, and first use is always local/offline per P8.
    An entry that can't be read (E067) refuses every registry operation
    before any request, so no other registry serves the names its
    scope_filter claims. The other diagnostics of reading the registries
    array (W140, I003) are shown by an operation only once it has asked a
    registry: an add of an
    exact version already installed asks none and shows none; a dry run of
    a registry package asks its registry for the package's manifest and
    shows them.
  """
  verify unit "registries parsed from specforge.json"
  verify unit "scope_filter routes to correct registry"
  verify unit "no registries configured produces I003 info diagnostic"
  verify unit "scope mismatch falls back to default registry URL"
  verify unit "duplicate alias produces warning"
  verify property "registry API schema is published as open specification"
  verify integration "all registries disabled produces fully offline mode"
  verify integration "first specforge init succeeds without any registry authentication"
  verify unit "builtins and local .wasm files install with no registry configured"
  verify unit "no registry URL on the specforge.dev domain is compiled into non-test source"
  verify unit "an operation shows the registry configuration's diagnostics once it has asked a registry"
  verify unit "a registry url is the registry's root, and one ending in the protocol's version prefix is refused with E067 naming the root"
  verify unit "a registries entry that can't be read refuses every registry operation with E067 before any request"
  verify contract "Configure Registries: registry configuration holds — specforge_json_parsed, filesystem_available, registry_entries_created, scope_filters_set, no_registries_diagnosed, no_hardcoded_urls, registries_configured_emitted, root_urls, unreadable_entry_refused"
}

// ── Registry Authentication ──────────────────────────────────

// Called imperatively during registry operations (resolve, search, publish) —
// not event-driven. Auth is request-time, triggered by 401 responses or
// pre-configured credentials.
behavior authenticate_registry_request "Authenticate Registry Request" {
  features   [registry_authentication]
  invariants [
    registry_integrity,
    multi_error_collection,
    credential_secrecy,
    offline_first_extension_resolution,
  ]
  category   command
  types      [RegistryConfig, RegistryCredential, TokenVerified, ExtensionError, RegistryError]
  ports      [RegistryClient]
  produces   [registry_authenticated]
  requires {
    credential_configured     "~/.specforge/credentials.json has an entry for the registry's alias, or SPECFORGE_REGISTRY_TOKEN is set for a publish"
    registry_client_available "RegistryClient port is available for authentication requests"
  }
  ensures {
    token_resolved           "The token is resolved before any request: the secret login stored (OS keyring, else a 0600 file), an environment variable, or a token file"
    auth_header_attached     "The resolved token is attached as an Authorization: Bearer header"
    missing_source_diagnosed "An unset variable is R010 and an unreadable token file R011, before any request"
    unauthorized_diagnosed   "A 401 is R001 naming how to log in again; a 403 is R002 with permission guidance"
    tokens_never_logged      "Raw tokens are never logged, printed or stored in specforge.json"
    authenticated_emitted    "registry_authenticated event fires on successful authentication"
  }
  contract   """
    A registry credential is kept per registry alias in
    ~/.specforge/credentials.json, never in specforge.json. Its token
    comes from one of three sources: the secret specforge login --token
    stored (in the OS keyring, else in a 0600 file under
    ~/.specforge/secrets/), an environment variable (login --token-env),
    or a file whose trimmed content is the token (login --token-file).
    The system MUST resolve the token before any request: an unset or
    blank variable MUST be R010 and a token file that can't be read or
    is empty MUST be R011, each naming the alias and how to fix it; an
    expired stored token is R-AUTH-020 and an unreadable keyring entry
    R-AUTH-021. The resolved token MUST be attached as an Authorization:
    Bearer header. A 401 answer MUST be R001, whose suggestion is
    `specforge login --registry <alias> --token <TOKEN>`; a 403 answer
    MUST be R002 with permission guidance. A 429 answer is retried by
    retry_registry_request. When the registry can't be reached the
    request fails (R004 or R005) with retry guidance; an add of a version
    already installed asks no registry (configure_registries). A raw
    token MUST never be logged, printed, shown in a diagnostic, or stored
    in specforge.json.
  """
  verify unit "token resolved from environment variable"
  verify unit "token resolved from token file"
  verify unit "missing token source produces ExtensionError"
  verify unit "a token file that can't be read is R011 before any request"
  verify unit "a 401 is R001 naming how to log in again"
  verify unit "raw tokens never logged or stored in config"
  verify unit "403 response produces E-level diagnostic with permission guidance"
  verify unit "every registry call reads an answer's status as one error"
  verify contract "Authenticate Registry Request: registry authentication holds — credential_configured, registry_client_available, token_resolved, auth_header_attached, missing_source_diagnosed, unauthorized_diagnosed, tokens_never_logged, authenticated_emitted"
}

behavior retry_registry_request "Retry Registry Request" {
  features   [registry_authentication]
  invariants [registry_integrity, multi_error_collection, credential_secrecy]
  category   command
  types      [RegistryConfig, RegistryError, RegistryErrorBody, ExtensionError]
  ports      [RegistryClient]
  requires {
    registry_request_failed   "A registry request has received a retryable response (429 or timeout)"
    registry_client_available "RegistryClient port is available for retry requests"
  }
  ensures {
    exponential_backoff_applied "A 429 answer is sent again after the longer of the backoff (1 s, doubling, at most 30 s) and Retry-After, at most 3 times"
    timeout_diagnosed           "A network timeout is not retried: it is R004, whose suggestion says to retry"
    retries_exhausted_emitted   "registry_request_retry_exhausted event fires when max retries exceeded"
  }
  contract   """
    When a registry answers a request with 429 (rate limited), the system
    MUST send the request again after a wait: the longer of the backoff
    (1 s, then 2 s, then 4 s) and the registry's Retry-After, at most 3
    times. A Retry-After longer than 30 s is not waited for: the request
    fails at once with R003, naming when to retry. When the retries are
    exhausted the request fails with R003. Every registry call is retried
    alike (versions, metadata, download, search, publish, authenticate):
    a publish is safe to send again, since a registry refuses a
    rate-limited publish before it reads the upload. A network timeout is
    not retried: it fails with R004, whose suggestion says to retry.
  """
  produces   [registry_request_retry_exhausted]
  verify unit "429 response retries with exponential backoff"
  verify unit "network timeout produces ExtensionError with retry guidance"
  verify unit "max retries exceeded produces final error"
  verify unit "a Retry-After longer than the longest backoff is not waited for"
  verify integration "a rate-limited publish is sent again once the registry's wait has passed"
  verify integration "a rate-limited answer is R003 on every registry call"
  verify contract "Retry Registry Request: registry request retry holds — registry_request_failed, registry_client_available, exponential_backoff_applied, timeout_diagnosed, retries_exhausted_emitted"
}

behavior validate_registry_credentials "Validate Registry Credentials" {
  features   [registry_authentication]
  invariants [registry_integrity, diagnostic_determinism, credential_secrecy]
  category   validation
  types      [RegistryCredential, RegistryConfig, RegistryError]
  ports      [RegistryClient]
  produces   [registry_credentials_validated]
  requires {
    registry_configured       "Target registry is configured in specforge.json"
    registry_client_available "RegistryClient port is available for test authentication request"
  }
  ensures {
    valid_credentials_stored      "--token is stored as a secret in the OS keyring (0600-file fallback); --token-env and --token-file are stored as references, never as the token"
    invalid_credentials_diagnosed "Invalid credentials produce error diagnostic with guidance"
    raw_token_never_stored        "Raw token value is never stored in specforge.json"
    credentials_validated_emitted "registry_credentials_validated event fires on successful validation"
  }
  contract   """
    When specforge login is invoked, the system MUST validate the
    provided credentials against the target registry by making an authenticated
    test request. The credential MUST be stored under the alias of the
    registry it was validated against: the entry --registry names, else the
    default registry; a --registry that names no entry MUST be refused (E063)
    before any network call. login MUST take exactly one token source:
    --token, --token-env or --token-file (R-LOGIN-001 otherwise, before any
    network call). The token is resolved from it and validated; then --token
    is kept as a secret in the OS keyring (a 0600 file under
    ~/.specforge/secrets/ when no keyring is available) and --token-env and
    --token-file are kept as references, so the variable's or the file's
    token is read on each use. The raw token MUST never be stored in
    specforge.json or in credentials.json. A credentials.json that cannot be
    read MUST be refused (R012) before any network call, and left as it is. The system MUST confirm successful authentication
    with an info message including the registry alias and authenticated scope.
    With no registry configured, login MUST make no network call and MUST
    fail with E063, whose suggestion names the specforge.json registries key.
  """
  verify unit "with no registry configured, login makes no network call and reports how to configure one"
  verify unit "valid credentials stored as RegistryCredential reference"
  verify unit "login with a token file keeps the file's path, not its content"
  verify unit "login takes exactly one token source"
  verify unit "login refuses an unreadable credential store before any request, and keeps it"
  verify unit "invalid credentials produce error with guidance"
  verify unit "raw token never stored in specforge.json"
  verify unit "success message includes registry alias and scope"
  verify unit "login stores the credential under the alias of the registry it validated against"
  verify contract "Validate Registry Credentials: registry credential validation holds — registry_configured, registry_client_available, valid_credentials_stored, invalid_credentials_diagnosed, raw_token_never_stored, credentials_validated_emitted"
}

behavior logout_registry "Logout Registry" {
  features   [registry_authentication]
  invariants [registry_integrity, diagnostic_determinism, credential_secrecy]
  category   command
  types      [RegistryConfig, RegistryCredential, RegistryError]
  ports      [FileSystem]
  produces   [registry_logged_out]
  requires {
    alias_given          "--registry names the alias, or the project at --path has a default registry"
    filesystem_available "FileSystem port is available for credential removal"
  }
  ensures {
    credential_removed        "RegistryCredential entry for the matching alias is removed"
    other_credentials_intact  "Credentials for other aliases and scopes remain untouched"
    missing_credential_silent "No credential for the specified alias succeeds silently"
    no_network_requests       "No network requests are made during logout"
    logged_out_emitted        "registry_logged_out event fires after credential removal"
  }
  contract   """
    When specforge logout --registry <alias> is invoked, the system
    MUST remove the stored credential reference for the given registry alias.
    Without --registry, the alias is the default registry's, read from
    the specforge.json of the project at --path.
    The removal MUST delete only the RegistryCredential entry whose alias matches —
    credentials for other aliases (and their scopes) MUST remain untouched.
    If no credential exists for the specified alias, the command MUST succeed
    silently. A credentials.json that cannot be read MUST be refused (R012):
    no entry is removed and no secret deleted. The system MUST NOT attempt
    any network requests during logout.
  """
  verify unit "logout refuses an unreadable credential store and deletes no secret"
  verify unit "credential reference removed for matching alias"
  verify unit "credentials for other aliases and scopes remain untouched"
  verify unit "no credential for alias succeeds silently"
  verify unit "no network requests made during logout"
  verify unit "logout without --registry forgets the default registry's credential"
  verify contract "Logout Registry: registry logout holds — alias_given, filesystem_available, credential_removed, other_credentials_intact, missing_credential_silent, no_network_requests, logged_out_emitted"
}

behavior support_private_registries "Support Private Registries" {
  features   [registry_authentication]
  invariants [registry_integrity, wasm_sandbox_integrity, credential_secrecy]
  category   command
  types      [RegistryConfig, RegistryCredential, PackageMetadata]
  ports      [RegistryClient]
  requires {
    credentials_configured    "A credential is kept for the registry's alias (credentials.json)"
    registry_client_available "RegistryClient port is available for authenticated fetching"
  }
  ensures {
    authenticated_before_fetch  "Every read (versions, metadata, download, search) carries the credential kept for that registry"
    scope_filter_respected      "Only extensions matching the scope_filter are fetched from authenticated registries"
    same_origin_downloads       "A download from another origin than the registry's carries no credential"
    unusable_credential_refused "A kept credential that can't be used refuses the read before any request"
    no_auth_leaks               "Error messages do not leak authentication details"
  }
  contract   """
    A registry may require a token to read (specforge-registry serve
    --private, or any registry behind authentication). Every read the
    system makes of a registry (its versions, a version's metadata, the
    download, a search) MUST carry the credential the user keeps for that
    registry's alias, when one is kept; with none the read is anonymous.
    SPECFORGE_REGISTRY_TOKEN authenticates a publish only: it names no
    registry, and a search asks every registry. A download MUST carry
    the credential only when its URL has the registry's origin (scheme,
    host and port). A kept credential that can't be used (expired,
    unreadable, an unset variable or an unreadable file) MUST refuse the
    read with its code before any request. The scope_filter on the
    RegistryConfig MUST be respected (resolve_registry_source). A package
    from a private registry passes the same fetch policy as any other:
    integrity, declaration, publisher signature and pin. Private registry
    errors MUST NOT leak authentication details in diagnostic messages.
  """
  verify unit "authentication occurs before fetch from private registry"
  verify unit "scope_filter restricts which extensions are fetched"
  verify unit "error messages do not leak authentication details"
  verify unit "a download from another origin carries no credential"
  verify unit "an unusable stored credential refuses the read before any request"
  verify integration "a private registry refuses an anonymous read and answers an authenticated one"
  verify integration "a registry that requires a token to read is read with the stored credential"
  verify contract "Support Private Registries: private registry support holds — credentials_configured, registry_client_available, authenticated_before_fetch, scope_filter_respected, same_origin_downloads, unusable_credential_refused, no_auth_leaks"
  // Observability: error diagnostics for private registry operations delegate
  // to authenticate_registry_request and retry_registry_request.
}
