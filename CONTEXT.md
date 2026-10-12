# SpecForge domain terms

Terms the code, the specs and the docs use with one meaning. Architecture decisions are in
`docs/adr/`; ADR 0004 records the one-project migration these terms come from.

- **Environment**: everything derived from `specforge.json` and the loaded extensions before any
  `.spec` file is read: config, spec root, registries, rules, surfaces, load diagnostics, and the
  runtime the extensions were loaded in (`runtime()`; none when it was loaded without one, so nothing
  loaded). Every extension call an operation makes over the project, a check's included, runs in
  that runtime (`specforge_project::Environment`, ADR 0015 "The runtime travels with the
  environment"). A `specforge.json` that is there and can't be used is the
  default config (for the unusable file or key), with each reason kept (`config_problems()`) and
  reported as the error E069. It also holds the project's **installed extensions** (`installed()`:
  what `specforge.lock` held when it was read, absent, read or unreadable, once, for every
  operation over the project) and what each `extensions` entry enabled, as the **extension load**
  left it. An environment is assembled from those parts (`Environment::assemble`: the config read,
  the installed extensions, whose root is the project root, and the extension load), and the assembly
  derives the rest: the spec root, the registry build, the custom rules' probes, the providers'
  registration and I002. `Environment::from_read` is its disk adapter: it reads the lock and runs
  the extension load. Its fields are private; it is read through accessors (ADR 0047). A session
  reads `specforge.json` once per load and builds its extension runtime and, holding it, its
  environment from that read, after stamping every environment input (ADR 0030). It
  opens in two steps (`ProjectSession::begin_open`, then `OpeningProject::finish`), so an editor
  answers what needs only the environment (keyword completion) while the sources are still being
  read.
- **Compiled project**: an environment plus the sources it read, their graph build, what resolving their
  imports and running the checks reported, and the memo of its entity snapshot and coverage. Its
  diagnostics are, by definition, what `specforge check` reports under the default policy, in the one
  report order: the environment's, the imports', the graph build's, the checks', then surface conflicts
  (`specforge_project::CompiledProject`). A one-shot compile (every CLI command) is one and keeps no
  stamp; a project session holds one and keeps it current (`ProjectSession::project`, ADR 0047); it is
  compiled from a project root (`CompiledProject::compile(&ProjectRoot, …)`).
- **Project root**: the directory holding the `specforge.json` a path is in, located from any path inside the
  project (a subdirectory, a file) by walking up, the nearest winning
  (`specforge_common::ProjectRoot::of`, ADR 0053). Only `specforge.json` marks a project. With none at or
  above the path, the path's own directory is a **loose root**: a project with the default configuration and
  no extension (I002 says so). A path that does not exist is `file_not_found`. The compile, the session open,
  the environment load and every operation that reads a project from disk take a project root, never a raw
  path; only `init` takes its directory as given. Distinct from the **spec root**, where the project's
  `.spec` files are discovered (`spec_root` in its `specforge.json`, else the project root): a path inside
  the spec root names the project, never a project of its own.
- **Format version**: the `.spec` file format a file was written against, `MAJOR.MINOR`, declared by
  the `// specforge-format:` header on its first non-blank line; a file with no header is at the
  current version and reports nothing. The parser reads it with the file
  (`specforge_parser::detect_format_version`, `SpecFile::format_version`) and the compile reports
  I007 (older) and E019 (newer, or a header that is not `MAJOR.MINOR`) on the header line, like any
  parser diagnostic; `specforge migrate` reads the version through the same function. It is
  distinct from the Graph Protocol's schema version.
- **Migration record**: `<root>/.specforge/migration.json`, what `specforge migrate --rollback` undoes: the
  last kept migration made with backups, each file with its backup and the SHA-256 of the file as the migration
  left it (`specforge_migrate::MigrationRecord`). A rollback restores only a recorded file that still holds that
  text; an automatic rollback restores the texts its own run read. Backups are never removed.
- **Project session**: a compiled project kept current, plus what it is built from: its **session
  inputs**, the stamps of what it last read, and where each environment load gets its extension runtime
  (ADR 0047). It classifies any changed path through them, applies changes as an
  update, an environment reload or a re-check, and can bring itself up to date with disk without a
  watcher (`ensure_fresh`). A session is opened from disk or detached (no project: what the LSP
  holds with no workspace folder and MCP while nothing is served). Watch, the LSP and MCP each hold
  one (`specforge_project::ProjectSession`; MCP's served one is always opened from disk, ADR 0025);
  watch and the LSP feed it watcher events and follow every update that changes its inputs
  (`Update::inputs_changed`) by watching them anew and then bringing the session up to date for what
  changed meanwhile, MCP asks it to be fresh before every request that reads the project (ADR 0014,
  ADR 0030, ADR 0035). In a debug build it checks every update against a cold rebuild, whichever
  surface holds it, and each surface reports a divergence where it reports (ADR 0035). The LSP gives it its open documents as **held buffers**, each batch of edits as one update
  (`SourceChange::Hold`), and releases a closed document's buffer (`ProjectSession::release`); what an open
  buffer is to its file is the session's to say (ADR 0023, ADR 0046). After every update that runs the
  checks, its compiled project reports exactly what a fresh compile of the same sources reports, in the
  same order (an editor update that skips the checks while a file does not parse is the one exception).
- **Session inputs**: everything a project session depends on besides its sources' text: where its
  sources are discovered (the spec root and `exclude`), its **environment inputs**
  (`specforge.json`, `specforge.lock`, the extension modules it loaded) and its **check inputs**
  (`specforge-cache.json` when check-phase passes read it, each file a `file_reference` field or a
  `file_exists` rule names, and the directory of each such file that is missing). One value per
  environment load, renewed each time the checks run (`specforge_project::SessionInputs`,
  `ProjectSession::inputs`). What a changed path is, which directories watch watches, which files
  the LSP asks its client to report and what the session stamps for freshness are all read from it;
  a detached session's inputs are empty (ADR 0030).
- **Held buffer**: an editor buffer a project session holds (`specforge_project::Buffer`: the file's path, the
  editor's text and version). While held it is the truth for its file, whatever happens to the file on disk:
  the session's freshness and classification leave it out, and an open or an environment reload builds it in
  place of its file. Releasing it reads the file through the session's one read. Diagnostics computed from it
  are published with its version. Only the LSP holds buffers (ADR 0046).
- **Call target**: the project one MCP call acts on, resolved from the call's optional `path` and its
  entry's target before the handler runs: the served session (brought up to date unless `use_cached`),
  another project opened as a session for that call only and brought up to date after the call writes, as the served one is, the directory `init` creates, or, with nothing served and
  no project named, the empty session for an entry that reads only the project view
  (`specforge_mcp::target::CallTarget`).
  Only a project (a directory holding `specforge.json`) is ever served: the server
  serves the nearest one at or above its root, else nothing, and a `path` to a directory that is no
  project is read for its call only (ADR 0014 D6 as amended).
  An entry's target is derived from what its handler is given
  (nothing, the project view, the project, the new directory), never declared beside it: an entry
  whose handler acts on the project is refused as no project before it runs (ADR 0014, ADR 0024).
- **Update**: one change applied to a project session. It re-reads and re-parses exactly the changed files (an
  importer parses the same, since references resolve without `use`), applies them to the session's graph
  build, resolves every file's imports again and re-runs the checks (`specforge_project::Update`, ADR 0006,
  ADR 0032). An update says whether the session's inputs changed (`inputs_changed`) and, when it was
  verified, how it differs from a cold rebuild (`divergence`). An update that changes no file (a held buffer
  whose text is already its file's) runs no check.
- **Graph delta**: what an update or a reload changed in the graph: added, removed and modified
  nodes (source positions ignored) and edges. Watch prints it and MCP notifies it
  (`specforge_graph::GraphDelta`, re-exported as `specforge_project::GraphDelta`). A graph build computes it.
- **Debounce rule**: changes that arrive less than 50 ms apart are one batch, due 50 ms after the last
  of them, each change once. Watch batches the file changes under every directory it watches as one
  stream, and the LSP batches edited documents, by the same rule (`specforge_watch::Coalescer`,
  ADR 0035).
- **Graph build**: the graph of a set of parsed `.spec` files and what building it reported (parse errors,
  duplicates, define blocks, unknown ref schemes, unresolved references, reference cycles), kept current one
  whole file at a time (`specforge_graph::GraphBuild`). Files are taken in path order; each entity ID is the
  node of its first declaration that is not a `define` block. A compile applies every file at once; a session
  applies each update's files to its own, and is then exactly the build of the same files at once; with
  verification on (every debug build) each update is checked against that (ADR 0032).
- **Extension declaration**: everything one extension declares — its handshake and every describe
  category — as the protocol types (`specforge_protocol_types::ExtensionDeclaration`). The SDK
  builds it, the guest serves it, the host loads it once, the Registry build reads it, a package
  registry stores it (ADR 0012). What it contributes is what it declares (`ExtensionDeclaration::declares`
  over the ten describe categories); its handshake's `providers` says it implements configured providers
  (ADR 0050).
- **Guest wire**: what the host and an extension's guest both compile (`specforge-protocol-types`): the
  extension declaration, the operation payloads, the extension vocabulary and the command-arg rule. Every
  builtin blob is built from it, so it holds nothing only the host reads: the host's reading of package names,
  version requirements and peer ranges is `specforge-package` (ADR 0050).
- **Installed extension**: an extension placed in the project under
  `.specforge/extensions/<name>/extension.wasm` and pinned by its `specforge.lock` entry (version,
  source `registry` or `local:<path>`, the SHA-256 of the binary and, for a signed registry package, its
  publisher key id). It loads only while its binary is
  the one its entry pins (E070 otherwise; W149 when the entry pins no hash). A project's installed
  extensions are one value, its lock read once (`specforge_installed::Installed`); installing,
  updating and removing them is one **change** (`Installed::change`), written all at once or not at
  all, `specforge.json` included (ADR 0028); `add`, `update` and `init` install through the
  **lock plan**.
- **Extension load**: turning a project's `extensions` entries into loaded extensions and their
  declarations, once per environment load (`Installed::load`, over the `WasmRuntime` port): a
  builtin from its embedded binary, an installed extension from its pinned module, a `.wasm` file
  entry from its file under the name it declares. Its declarations come in entry order; the registry
  build puts them in load order. What does not load is a typed `LoadFailure` on its
  entry with one diagnostic; the runtime keeps none. Without a runtime nothing loads, and each entry
  enables what it names (`Loaded::unloaded`); what an environment is assembled from says which
  (`specforge_project::ExtensionLoad`: `NoRuntime`, or what was loaded `In` a runtime).
- **Extension inventory**: every extension a project has, one row per name: enabled by a
  `specforge.json` entry, pinned in the `specforge.lock` the compile read, or loaded. Each row has its
  origin (`file:<path>` when only `.wasm` file entries enable it, `builtin`, the lock entry's source,
  else `unknown`: the order the extension load tries them), its status (loaded, not loaded, not
  configured), the entries that enable it, its declaration, its lock entry and why it did not load
  (`specforge_ops::extension::inventory`, over the project view). The extensions listing, doctor and
  remove read it, so they cannot disagree about which extensions a project has, where one comes from
  or whether it loaded (ADR 0028).
- **Candidate**: an extension a project does not load yet — a builtin `add` or `init` enables, a
  binary `add`, `update` or `init` installs, a binary `publish` uploads — and the extension
  declaration read from its binary once, through the `WasmRuntime` the operation's caller passes (the
  CLI's component runtime with the per-user compile cache, the MCP call's runtime, a test's in-process
  runtime), never through one the operation builds (`specforge_ops::extension::candidate`, ADR 0028
  D7). A binary to install must load (E028) and must not declare a builtin's name; enabling a builtin
  enables the builtins it requires first, in `add` and `init` alike, and a builtin that does not load
  refuses the operation.
- **Lock plan**: the installs one operation makes (`add` one, `update` every upgrade, `init` its
  local files), staged into the lock as the operation leaves it: each package's entry replaces
  its previous one, never sits beside it (`specforge_ops::extension::lock_plan::LockPlan`, ADR
  0041). The peer rule judges it once, after every install is staged: each staged extension's
  own peers, and every locked extension's peers on a staged one, unified against the registry an
  install came from (R-RES-006/005, ADR 0001) or, for a local file, refused with E027; a peer that
  is not locked is `check`'s. Lock order decides nothing. A registry install's binary loads only
  after that and must declare what was published; then the plan is written as one change, whose last
  step pins the publisher keys its fetches accepted (`Registry::pin`): a refused or failed plan pins
  nothing (ADR 0059).
- **Registry build**: the pure result of putting extension declarations in load order and turning
  them into kind, field and edge registries, the rule set, pass order, the graph inputs
  (`RegistryBuild::graph_inputs`) and the diagnostics of those declarations, in one order
  (`RegistryBuild::diagnostics`: the declarations' own, the registries', the custom rules' probes,
  the surfaces') (`specforge_registry::build_registries`). The custom rules' probe is the one call it
  makes out, through the `CustomVerdicts` port, when the environment probes it in the runtime its
  extensions loaded in (`RegistryBuild::probe`). It also runs every check over a built graph's entity
  records, in one order behind one gate: the structural checks, then the rule set
  (`RegistryBuild::check`), and says which files those checks read (`RegistryBuild::files`). Its
  outcomes are the `registry_build_*` behaviors and `check_entities_in_one_order`. Tests and every
  caller reach it only through `build_registries` and that build's methods; its steps, its checks and
  its fields are private (ADR 0031).
- **Graph inputs**: what the graph build reads of the loaded extensions (`specforge_graph::GraphInputs`):
  the kinds whose bodies an extension parses, the single and the absent-target reference fields, the
  inverse field pairs, each field's value coercion and the derived reference fields. Only the registry
  build derives them (`RegistryBuild::graph_inputs`); with the provider schemes the environment
  registers they are every graph's `GraphConfig` (`Environment::graph_config`, ADR 0032).
- **Rule set**: the extensions' declared validation rules plus the host's E006 rules for required
  fields, each turned once per registry build into a typed rule that carries only what its check
  reads, resolved against the registries (a compiled regex, an edge rule's peer kind, the fields an
  edge type is written as). It runs itself over the entity snapshot's records (`RuleInput`, ADR
  0019), cycles included, and answers which rule applies to a kind, which the snapshot's standing
  reads (`specforge_registry::rules::Rules`, ADR 0020). A declared rule that cannot work is W112; a
  property its check does not read is W147.
- **Structural checks**: the host's own checks over the entity snapshot's records, run by the
  registry build before the rule set, in this order: W012 (a `ref` nothing references), E016 (a
  path a `file_reference` field of the entity's kind names that does not exist under the spec
  root), then, unless the project is structural-only, E024, E013, E014, W020, E022 and E061
  (`specforge_registry`'s `checks`, ADR 0031). None reads a graph node. Whether every reference
  became an edge is the linker's own debug assertion, not a check.
- **Structural-only**: no loaded extension declares an entity kind
  (`RegistryBuild::structural_only`). The checks that read kinds, fields and identifiers do not
  run; with no extension loaded I002 says so, with extensions loaded W151 names the entities left
  unchecked.
- **Custom verdict**: an extension's answer, through its `wasm_function`, on one entity for a
  `check: "custom"` rule: pass, or fail naming a field and value. The rule set asks for it through
  the `CustomVerdicts` port; the project's adapter calls the extension (`ExtensionCalls::validate`),
  tests answer with a closure. A function that cannot answer is W112 at load, W148 at check.
- **Entity snapshot**: every entity of one built graph as every check after the build reads it, taken
  once per compile and per session check (`specforge_project::snapshot::EntitySnapshot`, ADR 0019).
  Each entity's record holds:
  - what it writes, as field text, with each value's shape (quoted, bare, a number, a list of
    strings or references, …) and span beside it for the host's own checks
    (`specforge_registry::entity::ValueShape`);
  - its references, obligations and methods;
  - its edge counts by peer kind;
  - what exempts it, if anything (a union body, an exempting flag, a kind that accepts no `verify`).

  The registry checks and the rules read the records (`specforge_registry::entity::EntityRecord`).
  The pass input and a custom validator's context are adapters over it; the coverage rule reads the pass
  input's entities (`specforge_coverage::Entity::from`), on the host as in every extension pass.
  Never read a graph node to answer what an entity says or owes.
- **Field text**: the one string a field value is to a rule, a custom validator and a compiler pass
  (ADR 0019): scalars as written; lists of strings or references and mixed lists joined by `", "`;
  variant lists and type unions by `" | "`; expressions by `", "`; verify statements by `"; "`; a
  block's keys by `", "`. An empty value is `""`. Every written field has one, and none is null.
- **Field type**: how the host reads a field's value, one of eight names an extension declares:
  `string`, `integer`, `bool`, `enum` (its values are the declaration's `enum_values`), `string_list`,
  `reference`, `reference_list`, `block` (`specforge_protocol_types::FieldType`). The registry build
  reads a declared type once, into the field's registry entry (`FieldRegistryEntry::new`, the only way
  to make one); every output names it by that one name — the schema and exports, the published JSON
  Schema, the model, the hover and E061 (ADR 0034). The older spellings (`_type` suffixes, `boolean`)
  are read, never written; any other name is W019.
- **Standing**: how the obligation rule sees an entity (`specforge_project::snapshot::Standing`):
  - whether its kind is testable;
  - which `no_verify_statements` rule requires its kind to declare obligations (a rule without a
    target kind applies to every kind);
  - what exempts it (a union body, an exempting flag, a kind that accepts no `verify` statements);
  - how many obligations it declares.

  It **owes** obligations when a rule applies and nothing exempts it. It **counts** toward coverage
  when testable and owing or declaring, and is **exempt** when testable and neither: the coverage
  rule's one counting rule (`specforge_coverage::Counting`), read with the standing's facts
  (`Standing::counting`). W004, stats, plan validation and the verify-stub fix read it. The pass
  input's `owes_none` is not this word: it says the entity owes none, and such an entity still counts
  when it declares obligations.
- **Package registry client**: what talks to a package registry: search, versions, metadata, download and
  publish over HTTP, credentials in the OS keyring, publisher trust and package signing
  (`specforge-registry-client`). Not the Registry build, which is pure and needs none of it.
  Operations reach it only through the `Registry` port, which lists a package's versions, fetches
  one, pins the publisher keys a committed install accepted, and publishes one, by package name and
  version (ADR 0036, 0045, 0059); its adapter,
  `specforge_ops_registry::ConfiguredRegistry`, reads the project's registries, asks the one that
  serves a name and runs the fetch policy over the `RegistryClient` seam (ADR 0044). It is linked by
  the CLI and MCP, never the LSP (ADR 0010), and holds what needs the user's `~/.specforge`: the
  credential, the signing key, the known keys. Publish derives the stored declaration from the
  binary; `add` checks the binary declares what was published (ADR 0012). Each seam has a second
  adapter for tests, held with the first to one contract: `MemoryRegistry` beside the port, `MemoryClient`
  beside the client (`assert_registry_contract`, `assert_client_contract`). A rate-limited call is sent again
  by the spec's backoff (`Retrying`), and every request carries the credential the user keeps for that
  registry (ADR 0044 as amended).
- **Registry credential**: the token a request to one registry carries, kept per registry alias in
  `~/.specforge/credentials.json`, never in `specforge.json`: a secret `specforge login --token` stored in the OS
  keyring (a 0600 file without one), or a reference to an environment variable (`--token-env`) or a file
  (`--token-file`), resolved before any request (`CredentialStore::credential`; R010, R011). Every read carries
  it; a publish takes `SPECFORGE_REGISTRY_TOKEN` first (ADR 0045 as amended).
- **Fetch policy**: what a package passes before an operation sees it: the registry's reply reads as the protocol's JSON (R009 else;
  a malformed signature is that) and names the package and version asked for, the binary hashes to the served SHA-256, the served manifest reads as that
  package's declaration (one from before ADR 0012 is refused), and the publisher signature verifies with the
  expected key: the user's **key pin** for the name, else the key the project's `specforge.lock` records, else
  any (trust on first use) (`ConfiguredRegistry::fetch`; ADR 0010, 0012, 0044, 0059). The fetch decides and
  writes nothing: the decision is the package's `Publisher`, and the key is pinned when the lock plan that
  fetched it commits. Accepting an unsigned package (`--allow-unsigned`) is reported as W155 at fetch, a
  re-pin as W156 once it is saved. A known-keys store that can't be read refuses the fetch (R-TRUST-006). The
  check writes nothing to the terminal: a key change is asked of the surface that can ask.
- **Key pin**: the publisher key id a package name is held to, kept per user in `~/.specforge/known-keys.json`
  (`pins`, beside the `trusted_keys` allowlist and the `denied_keys` deny list) and per project as the lock
  entry's `key_id`. The user's pin wins; the lock's stands in when the user has none. Pins are written only by
  a committed lock plan, through `Registry::pin`, after the binaries and the lock
  (`specforge_ops::registry::KeyPin`, ADR 0059).
- **Package signature**: a publisher's Ed25519 signature over a package's signed content (its name and version as
  declared, and the SHA-256 of the exact binary and manifest uploaded), with the public key and the time that verify it
  offline (`specforge_registry_wire::PackageSignature`, JSON `{sig, public_key, signed_at}`). Its key id is the first
  16 hex of the public key's SHA-256. Only a well-formed one exists. `specforge publish` signs with the user's key; the
  registry refuses an upload whose signature isn't one or doesn't verify over it (INVALID_SIGNATURE), before storing
  anything, so the version stays publishable; `add` verifies what it downloads again, because the registry is not the
  trust anchor: which keys to trust is the user's pins (ADR 0061).
- **Package registry contract**: what a package registry and its client exchange over HTTP: the paths, the JSON
  bodies (the package signature among them), the publish form and the error codes, each defined once in
  `specforge-registry-wire`, which `specforge-registry-server` and `specforge-registry-client` both compile against
  (ADR 0044, ADR 0061). A registry's refusal (HTTP 400) carries a typed code and reaches the user as R008; an
  answer that isn't the call's wire type is R009. A test reaches a registry through the real server in process or
  an in-memory client, never through JSON written by hand.
- **Registry for a package**: the one configured registry that serves a package name: the first
  `registries` entry whose `scope_filter` is the name's scope, else the first marked
  `default_registry`; with neither, none does, and the operation refuses with R-OPS-001 before any
  request (`specforge_ops_registry::Configured::registry_for`, ADR 0045). `add`, `update` and
  `publish` ask that one registry; `search` asks every entry. A registry credential is kept under
  the registry's alias (`login --registry`, else the default registry's).
- **Package name**: what an extension package is called, `@scope/name` (a registry holds only
  these) or `name` alone (a local module); each part `a-z 0-9 . _ -`, starting with a letter or
  digit, so it is always a relative path inside the directory it is joined to and one URL segment
  (`specforge_package::PackageName`, ADR 0036, 0050). A declaration's `name` is checked as one where
  `add` installs and `publish` uploads it; a `specforge.json` entry and a lock entry are read as one.
- **Version requirement**: which version of a package is asked for: `latest` (the highest release,
  a pre-release only when there is none), one full version, or a SemVer requirement read as Cargo
  reads one (`VersionRequirement`); `pick` is the one rule that chooses among published versions:
  ops runs it for `add`, and every registry's search lists a package at what it picks for `latest`
  (`pick_from`). The `Registry` port lists versions and fetches one; it does not resolve.
- **Upgrade target**: what `update` moves a locked registry extension to: the highest published version
  newer than the locked one that the update's reach admits, `^locked` (`Reach::Compatible`) or any
  (`Reach::Major`, `--major`), a pre-release only on the locked pre-release's own `MAJOR.MINOR.PATCH`. None
  means up to date, never a lower version; a lock version that is not SemVer moves to what `latest` picks
  (`specforge_package::upgrade_target`, ADR 0036 as amended).
- **Peer requirement**: what an extension declares it needs from another extension: the peer's name,
  a SemVer range read as Cargo reads one, and whether the peer may be absent
  (`specforge_package::peers::PeerRequirement`, ADR 0050). One rule judges it (`peers::verdict`): an
  unreadable range satisfies nothing (E073); a required peer must be installed; an installed one must
  be at a version the range accepts (E027 otherwise). The registry build, `doctor` and the lock plan
  read that rule and nothing else (ADR 0041).
- **Load order**: the order a project's extensions load in, which every first-wins rule and in-order
  list follows: entry order, except that an extension comes after the peers it declares (required
  ones always, optional ones unless that would close a cycle). The registry build produces it
  (`Peers::load_order`); a cycle among required peers is E027 and its extensions load together in
  entry order (ADR 0041).
- **Extension specifier**: an `add` argument (also each of `specforge.add_extension`'s
  `specifiers` and `init --extensions`'): a builtin's name, a local path, a `git+` URL, or a package
  reference `@scope/name[@requirement]` (`PackageRef`), read once by
  `specforge_ops::extension::parse`. An add takes one or more and adds them as one change, judged on
  the whole set (one lock plan, ADR 0041); a specifier naming an extension an earlier one of the same
  add names is a *duplicate* of it.
- **Project view**: the compiled project as one surface sees it, borrowed: the graph, the
  environment it was compiled in (config, what each `extensions` entry enabled, the registry build:
  kinds, fields, edges, rules, the extension declarations and their ordered passes), the root it was
  compiled from, and what the surface reports for it; the runtime its environment's extensions were
  loaded in (`ProjectView::runtime`), the one every operation that calls an extension over the view
  calls it in (analyze's passes, collect's collectors, inference gaps' scanners, an extension
  command), so none takes a runtime of its own; its entity snapshot, through the per-compile
  memo (`ProjectView::entities`) (`specforge_ops::view::ProjectView`). It owns
  the project's recorded test report and its versioned schema, both read at that root and never an
  ancestor's. The CLI builds one from its compiled project and the LSP from its session's
  (`ProjectView::of`); MCP from its call target (`ProjectRef::view`: the served session's compiled
  project with its I017 notices, or that of a session opened for one call) (ADR 0015, ADR 0047).
- **Read view**: an operation that only reads the project view: stats, trace, the coverage view, the
  model and outline diagrams, the versioned schema, inspect, query, list and search (the entities a
  selection over the view returns, `specforge_ops::query`), the exploration and the review
  (`specforge_ops::{explore, review}`), and the inference guide and plan
  (`specforge_ops::infer::{guide, kind_guide, inference_plan}`). Each returns a typed outcome; the CLI,
  MCP (its tools and its prompts) and the LSP only render it.
- **Entity facts**: what inspect returns for one entity: its node and kind entry, headline
  statement, standing and obligations (the snapshot's own, borrowed), references in both directions,
  coverage, and the reported diagnostics about it (`specforge_ops::inspect::EntityFacts`). MCP
  `specforge.inspect` renders it as JSON and the LSP hover as markdown, so the two cannot disagree.
- **Known kind**: a kind a loaded extension declares, or that an entity is written with (an
  undeclared one is E024's). A kind filter (over entities, or the model's `kinds`) knows both and
  reports any other kind as I020; an argument that needs a kind's declaration (a schema entry, an
  inference guide, the model's root) knows only the declared ones and refuses others with
  `unknown_kind`. Names are exact; both name the closest kind, a kind equal but for case first
  (`specforge_ops::view::KnownKinds`, `ProjectView::kinds`).
- **Unconnected entity**: an entity no edge links to another entity, in either direction
  (`ProjectView::connectivity`, `Degree::is_unconnected`). A reference that does not resolve is no edge
  (E003 or I004 reports it), and an edge from an entity to itself links it to nothing else. Stats counts
  them, the exploration lists them and the review flags those that count toward coverage. Not the same as
  unreferenced.
- **Unreferenced entity**: an entity no edge points at: W012 for a `ref`, an extension's
  `no_incoming_edges` rule for its kinds, the coverage summary's `invariant_unreferenced`. No output calls one
  an orphan: the catalog titles, product `health`'s `unreferenced_counts` and the coverage summary's
  `invariant_unreferenced` all say unreferenced.
- **Exploration**: where to start reading a project's graph (`specforge_ops::explore`): a selection (the
  entities one entity reaches within a depth, or every entity; of one kind when given), the paths from that
  entity, the starting points (the selected connected entities that reference more than they are referenced),
  the most connected and the unconnected ones. The explore prompt and `specforge explore` render it.
- **Review**: the coverage gaps of a neighbourhood (`specforge_ops::review`): the coverage view's rows of
  the entities that count toward coverage within some hops of one entity, or of the whole project, each with
  a finding when it declares no obligation or is unconnected. The review prompt and `specforge review`
  render it.
- **Inference guide**: what an agent is told to look for to infer a kind's entities from code
  (`specforge_ops::infer::KindGuide`): the kind's description and extension, every field the registry build
  registered on it, the extension's `inference_guide` followed by the project's own guide for the kind
  (`inference.<kind>` in `specforge.json`, under **Project-specific:**), an example entity writing each field
  the way its type is written, and the kind's entities. A project's guide is every declared kind's, its
  global conventions (`inference.global`) and its spec directory. The infer prompt, `specforge infer-guide`
  and the LSP's keyword completion render it.
- **Inference plan**: the infer prompt's plan (`specforge_ops::infer::inference_plan`): the inference
  progress, the unanalyzed and stale files paged from a cursor, the target spec directory (the spec root by
  default) and the kind priorities: kinds with no entity first, each after the kinds its reference fields
  target.
- **Configured providers**: the `providers` `specforge.json` lists (scheme, alias, extension,
  settings), registered once per environment against the loaded declarations, each with its
  status (registered, extension not loaded, not a provider (its handshake does not say `providers`), scheme taken) and the W118/E057 the
  registration reported (`specforge_project::providers::Providers`). The compile's I005 and the
  providers listing read this one registration.
- **Management operation**: an operation about a project's setup and tooling rather than its
  graph: the extensions and providers listings, doctor, remove, collect, inference progress and
  gaps, and the inference session steps. Like a read view it takes the project view and a request
  and returns a typed outcome; unlike one it also reads what the view does not own (installed
  binaries, source files, the inference manifest), and remove, collect and the session steps write,
  at the view's root only. `add`, `update`, `init` and `migrate` are operations but not over a view:
  they run before or instead of a compile (ADR 0015), in the runtime their surface passes (ADR 0028
  D7); `add` and `update` read `specforge.json`
  through the compile's own reader and the installed extensions through `Installed::at`, and refuse
  an unusable `specforge.json` with the refusal `remove` gives.
- **Stranded entity**: an entity whose kind only the extension being removed declares: `remove` lists it
  (`specforge_ops::extension::StrandedEntity`), still removes, and the next compile reports it E024.
- **Recorded test report**: `<root>/specforge-report.json`, what `specforge collect` last wrote. The
  project view reads it once per compile and per content
  (`specforge_project::coverage::RecordedCoverage`).
- **Stray test record**: a record of the recorded test report naming an entity the graph does not have
  (W097): `analyze` lists it apart from the passes with the closest id, never promotes it under strict,
  and it proves nothing (`specforge_ops::analyze::StrayRecord`).
- **Inference manifest**: `<root>/specforge-infer.json`, what inference has recorded: the source
  roots, each analyzed source file (root-relative path, content hash, the entities produced) and
  the inference sessions (`specforge_ops::infer::InferenceManifest`). One reader and one writer in
  `specforge_ops::infer`: a file that cannot be used is E071 for every reader, never an empty
  manifest, and a write keeps the keys it does not define. Not a project input: the compile and the
  session never read it.
- **Inference session**: one agent's run of inference, recorded in the manifest: started, then
  ended as completed or paused; at most one is active (`SessionStatus`). Its steps (start, mark a
  source file analyzed, end) are one management operation, `specforge_ops::infer::session`; MCP's
  `specforge.infer_session` is its adapter.
- **Anchors manifest**: `<root>/specforge-anchors.json`, which source item each entity is anchored
  to. Navigation reads it (`specforge_ops::navigate::source_anchors`): the anchors of a source file
  and of an entity; E071 when it cannot be used.
- **Obligation**: one `verify` statement on an entity. **Proven** when a passing test names its
  exact text, or a formal claim discharges it. Who owes obligations is the entity's standing.
- **Unverified**: an entity that counts toward coverage and is not proven
  (its standing counts and its verdict is not proven; `ProjectCoverage::is_unverified`).
- **Delivery evidence**: what the recorded test report proves of a feature, beside the `status`
  its author declares (a claim): the behaviors that implement it (name it in `features`), how many
  the coverage rule counts proven, their obligations and failing tests. A feature is **proven** when
  at least one behavior implements it and every one is proven. The host passes each entity's score
  to an extension command (`CommandInput.evidence`: none, unreadable, or recorded;
  computed by `specforge_ops::command::run` from the project view's recorded report); `@specforge/product` aggregates it per feature
  (`milestone-completion`'s `proven_count`) and its `delivery_evidence` pass reports a done feature
  that is not proven (I071). Status stays the input of every status query (ADR 0039).
- **Lifecycle consistency**: what the product kinds' statuses claim across entities, checked by
  `@specforge/product`'s `lifecycle` pass with every compile: a completed milestone with a feature
  neither done nor deprecated (W154), a done feature depending on an unfinished one (I063), a
  milestone due before its dependency (I064), a shipped deliverable tracking an incomplete milestone
  (I065). Its declarative rules check one entity at a time (ADR 0039).
- **Missing link**: an expected edge, from the registries, that a traced entity lacks
  (`MissingLink`). The only gap a trace reports.
- **Plan gap**: how an agent plan falls short of the graph: an unresolved entry, a missing entry for
  a testable entity with obligations, or an entry ordered before what it depends on (`PlanGap`). An
  edge to an entity that does not exist is neither: it is E003.
- **Verdict**: an entity's obligations, the proven ones, and the tests that bear on them. It gives
  both "proven" (the gate) and the covered/partial/uncovered status (the MCP view)
  (`specforge_coverage::Verdict`).
- **Operation**: one user-level command (init, add, remove, …) as a typed request and outcome,
  independent of surface. An operation that writes names the files it changed on disk in its
  outcome (`specforge_ops::Writes`), recorded where it wrote. It fails with an `OpError` whose kind
  (`OpErrorKind`: invalid input, not found, conflict, schema mismatch, compilation failed, …) is
  decided in ops where it is raised — a recorded test report that cannot be used included
  (`specforge_ops::report`) — and never by a surface; its code is what the CLI prints. Its outcome prints as one outcome document on
  every surface (ADR 0052). The CLI and MCP
  are adapters over it (`specforge-ops`); the CLI ends every core command through one renderer
  (`specforge_cli::outcome`: the refusal, the exit code) and writes it through its **Terminal**
  (ADR 0029).
- **Mutation outcome**: what one MCP mutation call produced, typed: a run (`specforge_mcp::mutation::Mutated`:
  its reply and what it wrote — the files its operation changed, the entities it changed and the domain
  event it produces, `Written`) or a failure (`mutation::Failed`: its McpError and the files it left
  written). Whether the call only previews is read from its tool arguments before it runs
  (`MutationArgs::previews`); a preview is no mutation, whatever its operation answered. The request
  pipeline's tools adapter alone (`mutation::complete`) turns it into the call target's refresh, the reply
  sent once (`WrittenReply`: the tool reply, `files_written` — the one place a client learns which files
  the call wrote — and what its reply type adds after the write, rename's fresh diagnostics), the domain
  event and `mcp_mutation_completed` (ADR 0022).
- **Outcome document**: the one typed JSON document of an operation's outcome, defined in ops beside
  the operation and deriving `Serialize` and `Shape`: the outcome itself when it is plain data
  (`stats::Stats`), else its `document()` (`AddDocument`, `RemoveDocument`, `UpdateDocument`,
  `InitDocument`, `ExtensionsDocument`, `MigrateDocument`, `AnalyzeDocument`, `DoctorDocument`, …).
  The CLI prints it under `--format json` and MCP answers it as the tool reply; a surface adds only
  what it alone knows, typed and flattened beside it (MCP's `note` on an add, a mutation's
  `files_written`, the CLI's credential health on doctor), and never renames, drops or re-derives a
  key. It carries no `null`. Its spec type is the tool's `Mcp*` reply type (ADR 0052).
- **Project sources**: the `.spec` files under the spec root (`spec_root`, else the project root) that
  discovery keeps — no skipped directory, no `exclude` entry. What a compile reads, and what format
  and migrate rewrite (`specforge_common::ProjectConfig::spec_files`).
  The walk is `specforge_common::walk_spec_files`: the spec root is read whatever its own name, no
  skipped directory below it is entered, no symlink is followed, and only files are sources. Format's
  directory walk is the same walk, stopping at nested projects.
  A discovered source that can't be read (not UTF-8, no permission) is E025 naming it and stays out of the
  graph, in a compile and after every update alike.
- **Format configuration**: how one `.spec` document is laid out: inside a project, the nearest
  `.specforgefmt.toml` from the document's directory up to its own project root, else the defaults;
  outside any project, the editor's tab settings. `specforge format`, MCP `specforge.format` and the
  LSP all format through `specforge_ops::format::document` (ADR 0021).
- **Option table**: one enumerated argument an operation takes — its listed names in order, the
  aliases it also accepts, a one-line help per name, and its default — beside that operation
  (`specforge_ops::options::OptionTable`; `export::FORMAT`, `model::MODEL_FORMAT`, …). The CLI's
  possible values and MCP's input-schema `enum`/`default` are built from it and both parse with it
  (ADR 0027). A set the project decides (analysis passes, entity kinds) is not one.
- **Check**: the operation that turns what a compile reported into what a surface reports: the
  diagnostic policy (lint profiles, then strict), the verdict (no error among everything reported),
  the severity filter (what is shown, never the verdict) and the opt-in build-cache record
  (`specforge_ops::check`). `specforge check` and MCP `specforge.validate` are its adapters; watch and
  the LSP report a compile's diagnostics without a policy (ADR 0018).
- **Run verdict**: whether an operation that judges the project passed, failed, or could not judge it,
  computed by the operation (`ok()`, or `verdict()` → `specforge_ops::RunVerdict` where it can be
  unjudged): check (no error reported), analyze (no error finding, and with a minimum the coverage
  gate met; unjudged when the gate has no figure), format (every target read and written, nothing
  left unformatted, and under `--check` nothing that would change), migrate (no file failed, nothing
  rolled back), doctor (no error-level finding). The CLI exits 0, 1 or 2 by it (`specforge_cli::outcome::Exit`, the only value a core
  command ends with); MCP returns it as `ok`
  (`specforge.validate`: `_meta["specforge/check"].ok`) and keeps `isError` for refusals (ADR 0004
  D4-a). A refusal (`OpError`) is not a verdict; a measuring command's refusal exits 2. Not to be
  confused with an entity's coverage **Verdict** (ADR 0029).
- **Terminal**: where a core CLI command writes and whom it asks: its stdout, its stderr, the input a
  prompt reads, whether each stream is coloured (`NO_COLOR`, then `CLICOLOR_FORCE`, then whether it is
  a terminal) and whether anyone can answer, fixed when `main` makes it from the process
  (`specforge_cli::terminal::Terminal`); a test captures one. A publisher key change and a collector's
  command are asked of it, nobody to ask being no; a reader that leaves stdout ends the output
  quietly (ADR 0029 D4b).
- **Diagnostic policy**: lint profiles (a closed set: `inferred`, `pedantic`) and strict promotion
  (`specforge_project::DiagnosticPolicy`). It is the only thing that changes a diagnostic's severity
  after the diagnostic is built. A profile's diagnostics come from the check that applies it
  (`inferred`: `specforge_ops::infer::lint`, I200/I202 or E071); the policy reads no file.
- **Extension command**: a CLI command an extension declares in its surfaces (with the SDK, together
  with its handler: `ContributionsBuilder::command`), answered by its `cmd__` export over the graph
  the host passes (`specforge_protocol_types::CommandInput`: args, project root, graph, the
  command format and today's date, UTC). The CLI runs it as `specforge <short> <command>`, MCP as
  the auto-promoted tool `specforge.<short>.<id>`, `short` being the declaration's (`ext_short`,
  else its name's last segment); neither knows any command (ADR 0008). One derivation serves both
  surfaces (`specforge_ops::command::ExtensionCommand`): its CLI name, its tool name, its args'
  command-line shapes, its input schema and the args both send, normalized by the one arg rule the
  SDK also runs (`specforge_protocol_types::command_args`): declared defaults applied by the host,
  an unset flag `false`, each value its declared type (ADR 0017). Both surfaces run it through one
  operation over the project view (`specforge_ops::command::run`): it normalizes the given args,
  sends the project root (absolute and canonical), the evidence and the date, and calls the export
  in the view's runtime; a surface only reads its arguments in and renders the output or the
  failure (`RunError`: refused args, no project, or an export that did not answer) (ADR 0011, "One
  operation runs a command").
- **Extension surface table**: what MCP serves from the project's extensions, built once per
  reload from their declarations: each tool once (an explicit `mcp__` tool, or an extension
  command), each resource with its URI template; listings are the core tables plus it, and a call
  is one lookup in it. Its entries are run by MCP's extension dispatch: an explicit tool's `mcp__`
  export, a command's `cmd__` export or a resource's `mcp__` export, one call each over the
  `WasmRuntime` port (`specforge_mcp`'s `extension_dispatch`). A contribution it does not serve is
  reported with I017 (`specforge_mcp`'s `ExtensionSurfaceTable`, ADR 0017).
- **Extension call**: one typed operation the host performs on a loaded extension — handshake,
  describe, command, MCP tool, MCP resource, compiler pass, collector, custom validator, scanner,
  migration hook — over the `WasmRuntime` port. Its input and answer are protocol types
  (`specforge_protocol_types`) the SDK shares; every failure is one `CallError`, E028, naming the
  operation, the export and the extension (`specforge_wasm::calls::ExtensionCalls`, ADR 0013).
- **Sandbox**: what the host holds an extension to. It is granted no capability (no preopened
  directory, environment, arguments, stdin, socket or name lookup), and it is held to two limits its
  handshake's `sandbox_policy` may declare, each at most the host's ceiling (30 000 ms per call,
  512 MB of linear memory) and the ceiling when undeclared; every call also gets the whole fuel
  budget. The loader applies them once it has read the handshake; the component runtime enforces them, and a call that
  crosses one traps with the limit's kind (E028). What a declaration asks for that the host does not
  give is W153 (`specforge_wasm::sandbox`, ADR 0037).
- **In-process runtime**: the test adapter of the `WasmRuntime` port that runs an SDK-declared
  extension in the host process through the guest's own routing (`Served`), unsandboxed
  (it records the limits the host applies and enforces none;
  `specforge_wasm::testing::InProcessRuntime`). It serves a binary's bytes under the name they are
  loaded as (`binary`), so the extension load and a candidate's read (add, init, update, publish)
  run in process (`specforge_ops::testing` serves `@sdk/greet` and builtin look-alikes that way); a
  test that serves an extension
  installs it (`specforge_installed::testing::install`) and the project loads it through the
  production path. Host tests declare their extensions with it; the
  component runtime is the production adapter, and both keep one contract
  (`assert_runtime_contract`). MCP's tests serve every project from a temporary directory through
  it (`tests/support`): no test writes a registry, a graph or a diagnostic into a server (ADR 0025).
  An extension's own command tests call its commands through it as well, from source
  (`@specforge/product`'s `extensions/product/src/tests/`, with `ExtensionCalls::run_command`); the
  component runtime tests a builtin only for what its vendored bytes prove: its declaration snapshots,
  that each command answers through it, and what a native test cannot hold it to (ADR 0013 D8).
- **Command format**: the output an extension command is asked for, `human` (the CLI default) or
  `json` (always, over MCP). The host owns the `--format` flag; the extension renders both, since
  only it knows its payloads (ADR 0011).
- **Tool spec**: the single definition of a core MCP tool: its own module under `specforge_mcp::tools`,
  implementing `CoreTool` once: its name, description and effect, the spec type its reply is, the tool
  arguments it reads, what its handler is given (nothing, the project view, the project, or the
  directory init creates, with how it reaches its project) and the tool reply it answers (typed, text,
  or a mutation's). Its category, annotations, call target, descriptor (input and output schemas
  included) and dispatch derive from it; the core tool table lists each once (`CORE_TOOLS`, each seen
  as a `ToolSpec`), and a declaration that contradicts its handler does not build. A prompt names a
  tool by its typed name (ADR 0024, ADR 0049).
- **Tool effect**: what a tool does to its environment, declared once on its tool spec
  (`specforge_mcp::tool::Effect`): reads, writes output artifacts (collect, render), or mutates the
  target's project files (a mutation, ADR 0022), with how a writing tool writes (destructive,
  idempotent, open world) as MCP's annotations say it. An extension tool only reads: the host grants
  an extension no capability (ADR 0037).
- **Prompt spec**: the single definition of an MCP prompt, from which its descriptor (its tool arguments'
  names, descriptions and required) and reply are derived; it renders the read view its prompt is about
  over the project view (the empty session's when nothing is served), adding only its instruction (tools named as the tool table names them), and refuses
  with an McpError, sent as a JSON-RPC error's data since prompts have no isError (`specforge_mcp`'s
  `PromptSpec` table).
- **Tool arguments**: the one typed struct a tool or prompt reads its call's arguments into
  (`#[derive(Arguments)]`, `specforge_mcp::args`, ADR 0033). Each field is an argument: its name, its
  doc comment as description, its type (how a value is read, and the JSON type listed), its default,
  and its option table or name list. The input schema (or the prompt's listed arguments) and the
  reading both derive from it: absent is the default, a value is read by its type under the arg rule
  extension commands follow (a boolean or a count may come as a string), and an argument neither it
  nor the call target declares is refused.
- **Tool reply**: the one typed definition of what a core tool answers (`Reply` beside its handler,
  or the outcome document both surfaces print, `pub use … as Reply`, with MCP's own extras
  flattened beside it), deriving `Serialize` and `Shape`
  (`specforge_common::shape`, ADR 0048). Its outputSchema is derived from it as one document, built
  once per process (every object closed, every array typed, the root an object; a named shape it
  holds defined once under `$defs`; content an extension defines is an open value), its structured
  result is it
  serialized, and a result its schema refuses is never sent as one (`schema_mismatch`). An optional
  value is absent, never `null`. `spec/types/mcp.spec`'s `Mcp*` types state the same shapes, and a
  test holds them to the derived schemas. A tool answering a document in text (validate, export,
  model, outline_extensions) has no outputSchema.
  A mutation's reply is sent as `WrittenReply`: its tool reply with what the mutation pipeline adds,
  declared beside it (`files_written`; `diagnostics` when its type says so, rename), never by the tool
  reply itself.
- **Named shape**: a type whose JSON Schema a document defines once, under its root's `$defs`, and
  refers to as `{"$ref": "#/$defs/<name>"}` wherever it appears (`#[shape(define)]`,
  `specforge_common::shape`, ADR 0051): a domain type a reply embeds from another spec file
  (`Diagnostic`, `MigrationResult`, ...) or one a reply holds more than once (`SourceSpan`,
  `GraphProtocolSchema`, ...). Each MCP tool's schema is its own document, so a named shape is defined
  once per tool, never once per listing.
- **Surface call**: one MCP request that invokes a named tool, resource or prompt (`tools/call`,
  `resources/read`, `prompts/get`), run through one pipeline: read the request, find the entry (the
  core table, then, with the served project brought up to date, the extension surface table), record
  the invocation, resolve the call target, run the handler (which records its own events), record the
  completion, answer with the kind's envelope; every record goes through the server's event sink. Each kind is one adapter; a refusal without `isError` (resources, prompts) is a JSON-RPC
  error whose data is its McpError (`specforge_mcp::surface_call`, ADR 0024).
- **Resource spec**: the single definition of a core MCP resource: its URI or template, descriptor
  and its read of the project view. A graph view (`graph`, `context`, `brief`, scoped or not, and `graph/{entity_id}`)
  is `specforge export` over the call's project view; the entity list and the diagnostics read what
  `specforge.list` and `specforge.validate` read (`specforge_mcp`'s `ResourceSpec` table). A URI names a resource, core or extension, by one
  rule (`specforge_mcp`'s `uri_template`): a template names every URI that starts with its text before
  the first placeholder, a URI without one names itself, alone or with a query; a placeholder's value
  and a query are the reader's to read or refuse (ADR 0049).
- **Stateless request**: an MCP request whose `_meta` names its protocol version (MCP 2026-07-28),
  answered on its own without `initialize`; every other request follows the revision `initialize`
  negotiated (`specforge_mcp::modern`). Every request is served under its revision, which travels
  with it from the router to the reply (`specforge_mcp::lifecycle::Revision`); the server state
  keeps only the negotiated one.
- **Subscription**: a client's interest in one resource the MCP server serves, made with
  `resources/subscribe` (the handshake revisions) or named by a `subscriptions/listen` stream
  (2026-07-28). A resource changes with the graph or the environment (the graph views, an
  extension's resources), with the environment alone (`specforge://schema`) or with the diagnostics
  (`specforge://diagnostics`). After an update that changed it, each subscription hears
  `notifications/resources/updated` once, and a handshake subscriber of a graph view or of the
  diagnostics then hears the delta (`specforge/graphChanged`, `specforge/diagnosticsChanged`).
  Subscriptions belong to the connection and end with it or at shutdown
  (`specforge_mcp::subscriptions::Subscriptions`, ADR 0024 D6).
- **MCP event**: what an MCP server records it did — a tool, prompt or listing invoked, a resource
  read, a mutation completed, a subscription made or ended, a delta notified, an extension surface
  dispatched, the server initialized or shut down, a protocol error answered, a cancel acknowledged —
  one typed value per event the spec declares for it, its payload the spec's field for field
  (`specforge_mcp::events::McpEvent`; `spec/events/mcp.spec`, `surface-contributions.spec`, and the
  `extension_added` and `project_initialized` a mutation produces). Every module records through the
  server's one **event sink** (`EventSink`): a production server's keeps nothing (`Discard`), so what
  a server holds never grows with the requests it answered; a test's keeps every event in order,
  stamped with the time it was recorded (`events::testing::EventLog`, given with
  `McpServer::with_events`). Events never leave the process: a client learns what a call wrote from
  its reply (ADR 0022 D7, ADR 0057). The CLI, the LSP and the operations keep no event.
- **Diagnostic catalog**: the one table of diagnostic codes (`specforge_diagnostics`'s `catalog!`):
  each code's title, owner, level and explanation. It generates `CATALOG`, which `specforge explain`,
  MCP `specforge.explain`, diagnostics JSON titles, doctor and the LSP hover read and from which
  `docs/diagnostics.md` is generated, and a **code constant** for every core code.
- **Code constant**: a core diagnostic code as a typed value, `specforge_diagnostics::codes::W112`
  (`Code`, at a fixed level; `GradedCode` for an `A###` finding whose pass sets the severity). Its
  level is the catalog's: a table entry whose `E`/`W`/`I`/`A` prefix contradicts its level does not
  compile. The host builds a diagnostic from one (`specforge_common::Diagnostic::new`, or
  `Diagnostic::graded` with the pass's severity), so its severity is the code's catalog level. Codes
  an extension reports cross the protocol as text and have no constant (`Diagnostic::from_extension`); they
  are checked against the catalog where they enter the host (`check_extension_code`, W150).
- **Diagnostic origin**: the extension that reported a diagnostic (`Diagnostic::origin`, JSON `origin`;
  a rule's declaring extension, a pass's extension; absent for the host's own). The catalog describes
  a diagnostic (title, explanation, docs link) only for the host's own or for the code's owner
  (`specforge_diagnostics::describes`), so an extension's finding with a code it may not use (W150) is
  kept but never presented as that code's owner's.
- **Diagnostic data**: a diagnostic's optional typed payload, the values its message names
  (`specforge_common::DiagnosticData`, e.g. an E003's unresolved target, a W061's cycle, the entity
  an extension pass named). Consumers that act on a diagnostic (the LSP's quick fixes, MCP's
  suggest_fixes, and navigation's attribution of a diagnostic to the entities it is about) read it;
  none parses the message, which is presentation.
- **Reference**: an occurrence of an entity's ID in another entity's field that resolves to it (one
  edge, written where its token is). The references *to* an entity are incoming; what an entity
  *refers to* are its outgoing references. "Find references" means incoming, with the declaration
  only on request (`specforge_ops::navigate`).
- **Navigation**: where an entity is declared, its references, entity lookup and ranking, which
  entities a diagnostic is about, and the fixes a diagnostic's data names. The LSP and MCP answer
  from one module (`specforge_ops::navigate`) in source spans. A span is a position in the text the
  project was compiled from (`ProjectSession::source_text`), not in the buffer typed since nor the disk
  now. The LSP converts spans to UTF-16 ranges through that text's line index (a file the compile holds
  no text of has no range), MCP renders them as JSON (ADR 0016, ADR 0023).
- **Lexeme**: one token of a `.spec` text as the language's one lexer reads it without a parse
  (`specforge_parser::lex`): an identifier, a scheme ref ID (one lexeme), a number, a string (read as
  the grammar reads it, across lines; one the grammar would not close is marked unclosed and, if a
  `"…"`, ended at its line's end, if a `"""…"""`, before the next form start), a comment or a punctuation character. Navigation, the LSP's
  document, the parser's recovery from unclosed strings and the formatter read text through it, and
  where a top-level form starts is read from lexemes too (form start); a test holds it to the grammar
  on the repository's spec. The prove pass's expression tokenizer is the one
  other reader (ADR 0023, ADR 0038).
- **Form start**: a line whose first lexemes open a top-level form of the grammar (an import, a spec,
  ref, define, union or entity block), indented no deeper than the top-level line before it
  (`specforge_parser::lex::starts_top_level_form`). The parser's recovery ends an unclosed string before
  one, and the lexer an unclosed `"""…"""` (ADR 0038 D7); the LSP's document closes every body, block and list left open at one, and the grammar's scanner
  ends an unclosed body there (its MISSING `}`), so a block being typed never takes the forms after it.
  A test holds it to the grammar's top-level nodes on the repository's spec, both ways, and another holds
  the scanner's copy to it (ADR 0038 D6, D8).
- **Cursor**: what the LSP knows about a position in an open document, read from the document's lexemes
  (`specforge_parser::lex`) and their block structure, never from the graph: the word under it, whether
  it is in code, a string or a comment, the entity block, field and reference list around it, and the
  `use` statement it is on. The entity a cursor names is the declaration or reference token under it,
  else an identifier at a reference position (a header's name, a value or item of a field the
  registry does not type as a non-reference, a `use` binding's imported name) that names an entity;
  hover, definition, references and rename all ask the cursor, completion asks it what completes
  there, and semantic tokens mark the same reference positions (`specforge_lsp::document`, ADR 0023).
  While the document is not the text the project was compiled from, the cursor never asks navigation
  about its position (a token of the compiled text): it names what its own word names, and
  prepareRename and rename wait for the compile. Every request's answer is decided synchronously over
  the LSP state (`specforge_lsp::answers`); the backend only carries requests.
- **LSP reaction**: what the LSP does after the client reports a change (a workspace opened, documents
  opened, edited or closed, files changed on disk): apply it to the project session as one update,
  publish what the project reports (a closed document's file always, as the project reports it),
  announce a reload, follow the session's inputs and catch up, ask the editor to refresh its
  highlighting. It runs one change at a time, synchronously, and tells the editor everything through
  one port (`specforge_lsp::reaction::Reaction` over `specforge_lsp::editor::Editor`; ADR 0035,
  ADR 0043).
- **Proof role**: what a field's value is to the prove pass, declared by its extension
  (`proof_role`): a **bound** the solver assumes (bounds must be consistent, E046) or a **claim**
  that must follow from the bounds (W139 when not; an entailed claim is a proved claim). A field
  with no role is not read by the prove pass (ADR 0009).
- **Risk grading**: the coverage owner's policy for one kind: its entities' risk is tallied, and
  one with no obligations is A002, an error at the grading's error level
  (`specforge_coverage::RiskGrading`, supplied by `@specforge/testing`; ADR 0009).
- **Lifecycle field**: the one field of a kind that holds its entities' lifecycle state
  (`lifecycle_field` on the kind). The build cache records its value so check-phase passes can
  compare against the previous build (ADR 0009).
