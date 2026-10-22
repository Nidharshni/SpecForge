// Wasm runtime invariants

invariant wasm_sandbox_integrity "Wasm Sandbox Integrity" {
  guarantee """
    Wasm extensions MUST NOT escape the Wasm sandbox. An extension MUST NOT
    access the host filesystem, network, environment, or memory outside its
    linear memory region; no sandbox policy permits it, since a policy
    declares limits only (ADR 0037). Any call that crosses a limit of its
    sandbox MUST trap the extension and emit a diagnostic (E028).
  """
  risk      high
  verify property "no extension can read or write outside its sandbox boundaries"
  verify unit "sandbox violation traps the extension and emits a diagnostic"
}

invariant extension_load_order_determinism "Extension Load Order Determinism" {
  guarantee """
    Given the same specforge.json extensions, the compiler MUST produce
    the same load order on every invocation and platform: the entries'
    order, except that an extension comes after the peers it declares
    (ADR 0041). The registry build MUST produce it, so every surface
    reads the same order.
  """
  risk      medium
  verify unit "the same extensions give the same load order on every build, dependencies first"
  verify unit "a load order given again comes back unchanged"
}

invariant peer_dependency_satisfaction "Peer Dependency Satisfaction" {
  guarantee """
    If an extension declares peer dependencies, the compiler MUST verify that
    all declared required peers are installed, and that every installed peer
    satisfies its declared semver range. An optional peer may be absent.
    Unsatisfied peer dependencies MUST produce an error diagnostic (E-level), not
    a silent degradation. A range that is not a SemVer requirement satisfies
    no version (E073). `check`, `doctor`, `add`, `init`, `update` and `remove`, and
    the dry runs of `add` and `remove`, judge a peer by one rule over one
    set: the extensions the project's specforge.json entries load
    (ADR 0041). An installed extension is judged by what its module
    declares, which its lock entry must record (E070).
  """
  risk      high
  verify unit "satisfied peer dependencies pass validation"
  verify unit "unsatisfied peer dependency produces an error diagnostic"
  verify unit "peer with wrong version range produces an error diagnostic"
  verify unit "one rule judges a peer: its range read as SemVer, then the version its peer is installed at"
  verify unit "add, init, update and remove judge the set check judges: the extensions the project's entries load"
  verify unit "the lock plan's peer set of the project as it is names what the extension load loads, in entry order, from the entries it loads them by"
  verify unit "the lock plan judges a locked extension by what its module declares, and one whose lock entry records another version or other peers is no member, as the load refuses it"
}

// -- Cache & Isolation Invariants ---------------------------------------------

invariant wasm_compile_cache_integrity "Wasm Compile Cache Integrity" {
  guarantee """
    The Wasm compilation cache MUST be keyed by the engine configuration and
    the exact component bytes that produced each artifact: an entry MUST
    deserialize only for the binary and engine that compiled it. Corrupted
    or mismatched cache entries MUST be ignored — the runtime falls back to
    fresh compilation — never served. The cache directory is managed by the
    runtime engine (wasmtime), selected via SPECFORGE_WASMTIME_CACHE.
    Separately, the integrity of an installed extension binary is enforced
    by the specforge.lock hash pin: a binary that no longer matches its
    recorded hash MUST be refused at load time (E070).
  """
  risk      medium
  verify property "a cache artifact from different bytes or engine config is never reused"
  verify unit "corrupted cache entry falls back to fresh compilation"
}

invariant extension_isolation "Extension Isolation" {
  guarantee """
    An extension failure MUST NOT affect other extensions or the host compiler.
    After an extension traps or fails during any lifecycle phase, the remaining
    extensions MUST continue execution normally. The failed extension MUST be
    excluded from subsequent phases in the current compilation.
  """
  risk      high
  verify property "extension trap does not affect other extensions"
  verify unit "failed extension excluded from subsequent phases"
}

invariant host_function_type_safety "Host Function Type Safety" {
  guarantee """
    Data exchanged between the host and extensions via host functions MUST
    conform to declared schemas. Malformed input from an extension MUST
    produce an ExtensionError diagnostic, not undefined behavior. The host
    MUST validate all extension-provided data before processing.
  """
  risk      high
  verify unit "malformed extension input produces ExtensionError"
  verify unit "valid extension input is processed correctly"
}

// -- Entity Kind Invariants ---------------------------------------------------

invariant entity_kind_uniqueness "Entity Kind Uniqueness" {
  guarantee """
    No two extensions MAY register the same entity kind name: a kind an
    earlier-loaded extension registered is E026, and the first extension
    in load order keeps it. Collisions are detected when the registries are
    built. The compiler never arbitrates conflicts — extension authors
    resolve collisions via renames or peer dependencies.
  """
  risk      high
  verify property "no two extensions can silently register the same entity kind"
}

// -- Entity Enhancement Invariants --------------------------------------------

invariant enhancement_field_uniqueness "Enhancement Field Uniqueness" {
  guarantee """
    No two extensions MAY register the same field name for the same entity
    kind: an enhancement never overwrites a field the kind already has,
    whether the kind's own or an earlier enhancement's, so the first
    registration in load order wins. The resolution is deterministic.
  """
  risk      medium
  verify property "no two extensions can silently claim the same field"
  verify unit "conflict resolution is deterministic across runs"
}

// -- Collector Invariants ---------------------------------------------------

invariant collector_output_conformance "Collector Output Conformance" {
  guarantee """
    What a collector reports is only recorded for entities the graph
    declares: results for any other entity ID are dropped with a W115
    warning, not a hard error, so one stale annotation doesn't block the
    rest of a run. A test's status is passed, failed or skipped, the one
    vocabulary of the collect wire, the recorded test report and a pass's
    test results. Recorded statuses are only passed and failed; skipped
    tests are counted, never recorded as proof.
  """
  risk      medium
  verify unit "unknown entity ID in collector entry produces W115"
  verify unit "skipped tests are not recorded as proof"
  verify unit "a test status is passed, failed or skipped, and no other value decodes"
  verify unit "a test status outside passed, failed and skipped is an error naming the collector, never a skipped test"
}

// -- Registry Invariants ----------------------------------------------------

invariant registry_integrity "Registry Integrity" {
  guarantee """
    Downloaded extension binaries from a registry MUST be verified against
    their declared SHA256 hash before installation. Hash mismatches MUST
    produce a hard error diagnostic and abort installation. The hash and,
    for a signed package, its publisher key id MUST be recorded in
    specforge.lock. At the version specforge.lock pins, the bytes installed
    MUST be the ones it pins.
  """
  risk      high
  verify unit "SHA256 match passes verification"
  verify unit "SHA256 mismatch produces hard error and aborts"
  verify unit "the hash and the key id are recorded in the lock file"
}

invariant publisher_trust "Publisher Trust" {
  guarantee """
    A registry package is installed only when its publisher signature
    verifies over the digests the registry describes (which the downloaded
    bytes must match) and the served manifest, or when it
    is unsigned and the user passed --allow-unsigned; a broken signature is
    never installed. The first verified key for a package is pinned, and a
    package signed by another key is refused unless the user consents. A
    key is pinned only for an install that is written, and a store of pins
    that can't be read trusts nothing.
  """
  risk      high
  verify integration "specforge add refuses an unsigned package without --allow-unsigned"
  verify integration "specforge add pins the publisher key and records it in specforge.lock"
  verify integration "specforge add refuses a package signed by another key than the pinned one"
  verify integration "specforge add --dry-run refuses an unsigned package and a denied publisher key as specforge add does, downloading nothing"
}

invariant registry_reply_binding "Registry Reply Binding" {
  guarantee """
    What a registry install verifies, pins and installs is the package and
    version that was requested, described by a manifest that can be read:
    a reply or manifest for another package or version, or a missing or
    unreadable manifest, is refused before any key is pinned.
  """
  risk      high
  verify integration "a manifest describing another package is refused"
  verify integration "a package served without a manifest is refused"
}

invariant extension_operation_atomicity "Extension Operation Atomicity" {
  guarantee """
    Extension install, uninstall, and update operations MUST be atomic.
    On failure, all changes MUST be rolled back — no partial installs,
    no stray files, no inconsistent lock state, and no publisher key
    pinned for an install that was not written.
  """
  risk      high
  verify unit "failed install rolls back to previous state"
  verify unit "interrupted upgrade preserves original extension"
  verify unit "a pin that cannot be saved puts back everything the install wrote"
  verify unit "a failed change puts back every project file it wrote and removes the directories it created"
  verify unit "a failed change puts back the modules and the lock and removes the directories it made"
  verify integration "concurrent install and uninstall are serialized"
}

invariant credential_secrecy "Registry Credential Secrecy" {
  guarantee """
    Raw authentication tokens MUST never be logged, stored in
    specforge.json, or included in diagnostic output. Only token
    presence/absence and validity status may be reported. The user's
    registry files (the credential store, the kept secrets, the publisher
    signing key and the known keys) MUST be created owner-only, 0600 files
    in a 0700 directory on unix, before a byte is written, and replaced
    whole through a temporary created new: never readable by another user,
    never left half written, never written through a link. A signing key
    file is named in a diagnostic, never quoted.
  """
  risk      high
  verify unit "registry token is not included in log output"
  verify unit "diagnostic messages report credential presence not value"
  verify property "no log line contains raw token string"
  verify unit "every file in the user's registry directory is created 0600 in a 0700 directory"
  verify unit "a user's registry file is never written through a link at its temporary name"
  verify unit "a corrupt signing key file is named, never quoted"
}

invariant surface_schema_validity "Surface Schema Validity" {
  guarantee """
    Every registered extension MCP tool MUST have an input_schema that is
    a JSON object, and an output_schema, when declared, that is a JSON
    object: a tool with another value produces E055 and is not
    registered. Every command arg MUST have a CommandArgType: the type is
    closed, so a surfaces description with another one does not parse and
    fails its extension's load (E028).
  """
  risk      medium
  verify unit "a tool whose schemas are JSON objects is registered"
  verify unit "a tool whose input_schema is not a JSON object is E055 and not registered"
  verify unit "a surfaces description with an unknown arg type fails the extension's load"
}

invariant extensions_run_in_their_loading_runtime "Extensions Run in Their Loading Runtime" {
  guarantee """
    Every extension call the host makes over a project MUST run in the
    runtime that loaded the project's extensions: the environment holds
    that runtime from the moment it loads them, a compiled project, a
    project session and every project view read it there, and no
    operation over a project view takes a runtime of its own (analyze's
    passes, collect's collectors, inference gaps' scanners, an extension
    command, a check's custom verdicts and passes). An environment loaded
    without a runtime loads no extension, and an extension call over it
    is E028, not loaded. Migrate, which compiles the project itself, and
    reading a candidate's declaration, which is in no environment, are
    handed a runtime.
  """
  risk      medium
  verify unit "an environment holds the runtime it loaded its extensions in, and its session and compiled project read it there"
  verify unit "an operation over a project view calls its extensions in the runtime the view's environment loaded them in"
  verify unit "an environment assembled from its loaded parts holds the runtime they were loaded in and probes its custom rules there; one assembled without a runtime holds no extension and enables each entry as written"
}
