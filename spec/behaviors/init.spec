// Initialization behaviors — project scaffolding

use "behaviors/wasm-extensions"
use "events/compilation"
use "invariants/core"
use "invariants/extensions"
use "invariants/wasm"
use "invariants/zero-entity-core"
use "ports/outbound"
use "types/config"

defaults behavior {
  features [project_initialization]
  types    [CompilerConfig, InitError]
  ports    [FileSystem]
}

// L5: project_initialized event boundary — scaffold_new_project fires
// project_initialized when extensions are selected; graceful_zero_extension_init
// fires it when zero extensions are selected. These are mutually exclusive paths
// in interactive mode (exactly one fires per init invocation).
behavior scaffold_new_project "Scaffold New Project" {
  category   command
  invariants [
    spec_root_singleton,
    init_config_validity,
    zero_domain_knowledge_core,
    authentication_never_gates_core_use,
  ]
  types      [CompilerConfig, InitConfig, InitError, ProjectConfig]
  produces   [project_initialized]
  requires {
    filesystem_available "FileSystem port is available for writing project files"
    no_existing_project  "No specforge.json exists in the directory being initialized"
  }
  ensures {
    valid_config_created        "A syntactically valid specforge.json is created with the user-provided project name and version"
    schema_field_included       "Generated config includes a $schema field for IDE autocomplete"
    project_initialized_emitted "project_initialized event is emitted after successful scaffolding"
  }
  contract   """
    When specforge init is invoked in an empty directory,
    the system MUST create a specforge.json file with the user-provided
    project name and version. The generated config MUST include the
    $schema field pointing to the SpecForge JSON schema URL to enable
    IDE autocomplete. The generated config MUST be syntactically valid
    and parseable by the compiler. If a specforge.json
    already exists in the directory being initialized, the system MUST
    reject the operation with an error message and exit code 1. Init only
    adds: a directory whose starter file (spec/hello.spec) already exists
    is rejected the same way, and an init that fails after writing leaves
    the directory as it was, files that were already there (specforge.lock,
    .specforge/) included. A project
    in an ancestor directory MUST NOT block init: the new project is
    separate, and commands run inside it resolve to it because the
    nearest project wins. Init notes the enclosing project on stderr. The
    full init-check-export cycle MUST complete in under 60 seconds on commodity hardware, enforcing
    Principle 8 (seconds to value). In interactive mode, the project name
    MUST default to the directory name and prompt the user for confirmation
    or override. The command lists every file it wrote (.gitignore when it
    changed it, the config, the starter file, and each installed module and
    lock), in its human output and as files_written in its JSON output.
  """
  verify unit "scaffold creates valid specforge.json"
  verify unit "scaffold includes $schema field in generated config"
  verify unit "scaffold rejects when specforge.json already exists"
  verify unit "init refuses a directory whose starter file exists, writing nothing"
  verify unit "a failed init leaves the directory as it was, files that were there included"
  verify unit "scaffold inside another project creates a separate project"
  verify performance "full init-check-export cycle completes in under 60 seconds"
  verify integration "scaffold in non-empty directory preserves existing files"
  verify integration "scaffolded project passes init-check-export cycle"
  verify unit "init adds the generated report files to .gitignore without duplicating entries"
  verify integration "init lists every file it wrote, in its human and JSON output"
  verify contract "Scaffold New Project: new project scaffolding holds — filesystem_available, no_existing_project, valid_config_created, schema_field_included, project_initialized_emitted"
}

// Sub-step of scaffold_new_project — not an independent entry point
behavior scaffold_starter_spec_file "Scaffold Starter Spec File" {
  category   command
  invariants [zero_domain_knowledge_core, init_config_validity]
  // Deliberately passive: declared as data (C12-13) so the event-graph
  // lint can enforce the absence, not just a comment.
  produces   []
  requires {
    config_created       "specforge.json has been created by the parent scaffold_new_project step"
    filesystem_available "FileSystem port is available for writing the starter spec file"
  }
  ensures {
    starter_file_created   "A starter .spec file is created alongside specforge.json"
    structural_syntax_only "Starter file uses only structural syntax when no extensions contribute templates"
    zero_diagnostic_pass   "Starter file passes specforge check with zero diagnostics regardless of installed extensions"
  }
  contract   """
    During specforge init, after creating specforge.json, the system
    MUST create a starter .spec file (e.g., hello.spec) demonstrating
    the basic DSL syntax using only structural syntax: generic entity
    blocks with string fields and reference lists. The core init command
    MUST NOT generate domain-specific content or reference extension
    keywords — the core compiler has zero domain knowledge (Principle 2).
    Extensions MAY contribute starter templates via their manifest
    metadata; if an installed extension declares a starter template,
    init MAY use it, but this is extension-provided content, not
    core-generated content. The starter file MUST be valid and pass
    specforge check with zero diagnostics regardless of which extensions
    are installed. This behavior delivers on Principle 8 (seconds to
    value): the user can run specforge check immediately after init.
    The starter spec's spec block MUST state the project's version, the
    one specforge.json records: an extension's template writes it as
    {version} (as {project} stands for the project id), which init fills
    in. Init MUST write the starter as the
    formatter writes it, whatever the extensions contribute, so a freshly
    initialised project passes specforge format --check.
  """
  verify unit "starter spec file is created alongside specforge.json"
  verify unit "starter spec file passes specforge check with zero errors"
  verify unit "starter file uses only structural syntax when no extensions contribute templates"
  verify unit "starter file contains no domain-specific keywords from extensions"
  verify unit "starter file content is deterministic for same extension set"
  verify integration "extension-contributed starter templates are used when available"
  verify integration "when several enabled extensions contribute starter templates, the one listed first in specforge.json is used"
  verify integration "extension-contributed starter file passes specforge check with zero errors"
  verify integration "the software starter passes specforge check with no warnings"
  verify unit "the starter spec's version is the project's"
  verify integration "a freshly initialised project passes format --check and check"
  verify contract "Scaffold Starter Spec File: starter spec file scaffolding holds — config_created, filesystem_available, starter_file_created, structural_syntax_only, zero_diagnostic_pass"
}

// Sub-step of scaffold_new_project — does not produce an independent event.
// The project_initialized event is produced by the parent orchestrator.
behavior interactive_extension_selection "Interactive Extension Selection" {
  category   command
  invariants [spec_root_singleton, init_config_validity, zero_domain_knowledge_core]
  types      [CompilerConfig, BundledExtensionCatalog, BundledExtensionEntry]
  ports      [FileSystem, RegistryClient]
  requires {
    tty_or_fallback_ready    "Either a TTY is available for interactive prompts or non-TTY fallback path is ready"
    catalog_source_available "At least one extension catalog source is reachable (registry, local cache, or bundled index), or graceful degradation to zero extensions is prepared"
  }
  ensures {
    selected_in_config      "Selected extensions appear in the generated specforge.json extensions list"
    unselected_excluded     "Unselected extensions are absent from the generated config"
    no_default_preselection "No extensions are pre-selected by default"
    non_tty_graceful        "When no TTY is available, interactive prompts are skipped and zero extensions are selected"
  }
  contract   """
    During specforge init, the system MUST discover available extensions
    (from registry, local cache, or bundled index) and present them for
    interactive selection. The system SHOULD indicate commonly-used
    extensions as determined by registry metadata (e.g., download counts,
    curated lists) but MUST NOT pre-select
    any extension by default. Selected extensions MUST be added to the
    extensions list in the generated specforge.json. Unselected extensions
    MUST NOT appear. When no TTY is available (e.g., piped input in CI),
    the system MUST skip interactive prompts and proceed with zero
    extensions selected.
    If no registry is reachable, the system MUST fall back to a bundled
    extension index. The init flow MUST NOT fail due to registry
    unavailability. If both the registry and the bundled index are
    unavailable, the system MUST proceed with zero extensions and emit
    an I-level diagnostic explaining that no extension catalog was available.
  """
  verify unit "selected extensions appear in generated config"
  verify unit "unselected extensions are absent from generated config"
  verify unit "no extensions are pre-selected by default"
  verify unit "interactive prompts are skipped when no TTY is available"
  verify unit "registry unavailability falls back to bundled extension index"
  verify performance "bundled index fallback completes within 10 seconds"
  verify unit "init does not fail when registry is unreachable"
  verify unit "missing registry and bundled index proceeds with zero extensions and info diagnostic"
  verify contract "Interactive Extension Selection: interactive extension selection holds — tty_or_fallback_ready, catalog_source_available, selected_in_config, unselected_excluded, no_default_preselection, non_tty_graceful"
}

behavior non_interactive_init "Non-Interactive Init" {
  category   command
  invariants [spec_root_singleton, init_config_validity, zero_domain_knowledge_core]
  types      [CompilerConfig, InitConfig, McpInitResult, InitError, BundledExtensionCatalog]
  ports      [FileSystem, RegistryClient]
  produces   [project_initialized]
  requires {
    name_flag_provided   "--name flag is provided with a valid project name"
    filesystem_available "FileSystem port is available for writing project files"
    no_existing_project  "No specforge.json exists in the directory being initialized"
  }
  ensures {
    config_identical_to_interactive "Generated specforge.json is structurally identical to one created interactively with the same inputs"
    all_prompts_skipped             "All interactive prompts are skipped"
    json_output_supported           "When --format=json is specified, output is the init document specforge.init answers: project_root, name, version, config_file, starter_file, extensions_installed and files_written"
    project_initialized_emitted     "project_initialized event is emitted after successful non-interactive init"
  }
  contract   """
    When specforge init is invoked with --name and optional --extensions
    flags, the system MUST skip all interactive prompts and create the
    project non-interactively. This enables CI pipelines, scripts, and
    automated tooling to scaffold projects without user interaction.
    The generated specforge.json MUST be identical in structure to
    one created interactively with the same inputs. When --format=json
    is specified, output MUST be the init document specforge.init answers
    (McpInitResult): { project_root (absolute), name, version, config_file
    and starter_file (relative to project_root), extensions_installed[] },
    with files_written.
    When --version is specified, it MUST override the default version
    (0.1.0) in the generated specforge.json.
    --extensions takes builtins and local .wasm files; the command line
    accepts several to a flag, separated by commas (its spelling of the
    list), each read as add reads a specifier
    (parse_extension_specifier: E054, R-RES-003, E040, the same messages
    and suggestions): init enables, before each extension it enables or
    installs, the builtins that extension requires (its non-optional peers
    that are builtins, and theirs), as add does. Two specifiers naming one
    extension are claimed as add claims them: the same way, it is enabled
    once; differently (other bytes, another version), the init is refused
    with extension_conflict naming both, before anything is written.
    A relative local path is read from the directory init creates, its .. steps
    resolved as the file system resolves them where the path exists and by
    name beyond, as add resolves them.
    Init is one lock-plan change (ADR 0041): the project's peer set is
    judged once, before any file is written, so it never writes an entry
    specforge check cannot load or a project whose check reports an E027
    for an extension it enabled; then its modules, specforge.lock, specforge.json, .gitignore
    and starter are written as one change, put back together on failure.
    A registry package MUST be rejected with E063 (a new project has no
    registry configured; the suggestion names the specforge add to run
    once one is), a git source with E064, both with exit code 1 and before
    anything is written. A project name whose starter spec ID would break
    the identifier contract (2-60 characters) MUST be rejected the same
    way. Everything is validated before any file is written.
  """
  verify unit "non-interactive init creates valid specforge.json"
  verify unit "non-interactive init skips all prompts"
  verify unit "non-interactive init with --extensions populates extensions list"
  verify unit "init enables a builtin after the builtins it requires, as add does"
  verify unit "init refuses local extensions the lock plan refuses before it writes anything"
  verify unit "init enables an extension two of its specifiers name the same way once, and refuses two that add it differently with extension_conflict, as add does"
  verify unit "init reads its extension specifiers as add reads them, with add's codes"
  verify unit "init resolves a .. in a relative local path as the file system does, as add resolves it"
  verify unit "init enables the builtins a local file requires before it"
  verify integration "init with a builtin that requires another passes check"
  verify unit "non-interactive init with unknown extension rejects with diagnostic and exit code 1"
  verify unit "invalid project name is rejected with InitError::invalid_name"
  verify integration "--extensions splits a comma-separated list into its extensions"
  verify integration "a one-character project name is rejected before its starter can fail E014"
  verify integration "non-interactive output matches interactive output for same inputs"
  verify unit "non-interactive init with --format=json prints the init document specforge.init answers"
  verify unit "non-interactive init --format=json includes project_root, name, version, config_file, starter_file and extensions_installed"
  verify unit "non-interactive init with --version overrides default version in specforge.json"
  verify integration "non_interactive_init completes full init-check-export cycle in under 60 seconds"
  verify contract "Non-Interactive Init: non-interactive init holds — name_flag_provided, filesystem_available, no_existing_project, config_identical_to_interactive, all_prompts_skipped, json_output_supported, project_initialized_emitted"
}

// L7: Lock file interaction (download, integrity checks, version pinning) is
// handled by the write_lock_file behavior — see behaviors/wasm-extensions.spec.
behavior add_extension_to_existing_project "Add Extension to Existing Project" {
  category   command
  invariants [
    spec_root_singleton,
    init_config_validity,
    peer_dependency_satisfaction,
    zero_domain_knowledge_core,
  ]
  ports      [FileSystem, RegistryClient]
  produces   [extension_added]
  requires {
    existing_project_found "A specforge.json exists at or above the path given (locate_project_root)"
    extension_resolvable   "Extension specifier can be resolved via registry, bundled index, or local cache"
  }
  ensures {
    extension_appended      "Each extension is added to the extensions list in specforge.json, all of them as one change or none"
    no_duplicate_added      "Already-installed extensions are not duplicated"
    other_fields_preserved  "No other fields in specforge.json are modified"
    peer_deps_satisfied     "An add that would leave a peer it touches unsatisfied in the project's peer set (the extensions its entries load: builtins, installed extensions and .wasm file entries), or whose peer range can't be read, is refused before anything is written; the builtins an install requires are enabled with it"
    extension_added_emitted "extension_added event is emitted after successful addition"
  }
  contract   """
    When specforge add <extension-specifier>... is invoked on an existing
    project, the system MUST add each extension to the extensions list in
    specforge.json. It takes one or more specifiers and adds them as one
    change: every install is staged in one lock plan and judged once, on the
    whole set (ADR 0041), so the order of the specifiers decides nothing;
    the modules, specforge.lock (written once) and specforge.json are
    written together. The add is all or nothing: if any specifier is
    refused, nothing is written, and with several specifiers the refusal
    names the one refused ("adding '<specifier>': ..."). A specifier naming
    an extension an earlier specifier of the same add names, the same way,
    is reported as a duplicate of it (added once); two specifiers adding
    one extension differently (another version, other bytes, another
    source) are refused with extension_conflict, naming both.
    The extension specifier MUST accept @scope/name@version syntax; version
    resolution is delegated to parse_extension_specifier
    (specforge_ops::extension::parse over the package module).
    If no version is specified, the system MUST resolve to the
    latest compatible version.
    An installed extension MUST be enabled by its bare name; specforge.lock
    records the version the extension itself declares (its handshake), with
    source "registry", or "local:<path>" for a local .wasm file, so peer
    and diamond checks compare real versions and update never replaces a
    local build from a registry.
    A local path may name an extension's crate directory, with or without ./: the component its
    crate builds (target/wasm32-wasip2/release/<crate>.wasm) is installed
    and locked by its own path, as publish reads it; a directory with no
    built component is E040.
    The system MUST NOT duplicate an already-installed extension.
    An extension a .wasm file entry of specforge.json loads (its component declares that name and
    the extension load admits it, an installed extension of that name whose module is missing or
    changed included) MUST NOT be installed or enabled beside it: the add is refused
    with extension_conflict, naming the entry's file, and writes nothing, a dry run included.
    Only a named entry (name, or name@version as older adds wrote it) makes an extension already
    enabled or already present.
    A dry run is the add's lock plan judged and not committed: it answers
    what the add would answer for each specifier (planned, with the
    builtins it would enable as peers_enabled), or the refusal the add
    would give, and writes, pins and downloads nothing. A registry
    package is judged by the declaration it was published with (its
    manifest, read without its binary), which is what the add's peer
    check judges; what only the binary can fail (integrity, the publisher
    signature, its load) is not checked. An extension already present
    answers as the add would, whatever its source. specforge add --dry-run
    is that dry run on the command line.
    The system MUST NOT modify any other field in specforge.json.
    An add writes the entries its lock plan judged into the specforge.json
    it read; when the file changed since, the add MUST be refused
    (file_changed) and write nothing.
    When adding an extension, the system MUST judge, by the one peer rule
    (ADR 0041) and before anything is installed, the project's peer set as
    the add leaves it: the extensions its specforge.json entries load,
    resolved as the extension load resolves them (a builtin by the
    declaration this specforge embeds, an installed extension by its lock
    entry, a .wasm file entry by its file's declaration). A locked extension
    no entry enables is not judged. Every declared peer whose dependent or
    peer the add changes is judged: a peer range that is not a SemVer
    requirement is refused with E073; a required peer no extension of the
    set provides is refused with E027 (the message and suggestion specforge
    check gives). A peer installed from a registry at a version outside a
    requirer's range is a version diamond (ADR 0001): from a registry the
    operation MUST be rejected with R-RES-006 naming the version that would
    satisfy every requirer of the set, or R-RES-005 when no published
    version does (R-RES-002 when the registry publishes no version of the
    peer, as an add of the peer is refused); a local install, with no registry to consult, MUST be
    rejected with E027. A builtin or a .wasm file entry at a version outside
    a range is E027 whatever the source: no install changes its version.
    Installing or enabling an extension at a version an extension of the
    set does not accept MUST be rejected the same way, its message naming
    the extension it would break. The peers are judged against the lock as
    the add leaves it: a package already locked at another version is
    judged by its new declaration alone, and a version diamond names each
    requirer once. Before it is judged, an add enables the builtins it
    requires, whatever its source: each non-optional peer of what it adds
    that is a builtin no entry enables, and theirs, dependencies first, so
    specforge check never reports an E027 the add could have seen. A builtin
    whose declaration cannot be read refuses the operation (E028) before
    anything is written.
    If no specforge.json exists at or above the path given (the project
    it is in, locate_project_root), the system MUST reject
    the operation with an error message and exit code 1. If the extension
    specifier cannot be resolved, the system MUST reject the operation with
    a diagnostic naming the unresolvable extension. A registry specifier
    with no registry configured in specforge.json MUST make no network call
    and MUST fail with E063, whose suggestion names the registries key.
    Its JSON output is the add document specforge.add_extension answers
    (McpAddExtensionResult: dry_run, added, one entry per specifier in the order
    given, each naming its specifier, its action enabled, installed,
    already_present, planned or duplicate; no note; an enabled builtin,
    an installed package or a planned one lists the required builtins the
    add enabled with it as peers_enabled), with the files it
    wrote as files_written (empty when every extension was already
    enabled; absent from a dry run). A registry package's source is registry, and its key_id is
    present only when the package is signed.
  """
  verify unit "with no registry configured, add makes no network call and reports how to configure one"
  verify unit "add extension appends to extensions list"
  verify unit "an add over a specforge.json changed since it was read writes nothing"
  verify unit "a .wasm file entry never makes an install already present"
  verify integration "add --format json lists the files it wrote in files_written"
  verify integration "specforge add with several specifiers prints one entry per specifier and writes them as one change, or nothing"
  verify unit "the add document lists what the add did for each specifier by action, with source always and key_id only for a signed package"
  verify unit "add takes several specifiers and adds them as one change"
  verify unit "an add of several specifiers that refuses one writes nothing, its refusal naming that specifier"
  verify unit "the specifiers of an add are read in order, the first unreadable one refused and named among several"
  verify unit "an add of several specifiers judges their peers on the whole set, whatever their order"
  verify unit "a specifier naming an extension an earlier one adds is a duplicate, and two adding it differently are refused"
  verify unit "a dry run of several specifiers previews each and writes nothing"
  verify unit "a dry run answers what the add answers, refusals and the builtins it enables included, and writes nothing"
  verify unit "a dry run judges a registry package by the declaration it was published with, downloading nothing"
  verify unit "add enables a builtin's required peers but not its optional ones"
  verify unit "add enables the builtins a required builtin peer requires, dependencies first"
  verify unit "a builtin whose declaration cannot be read is refused before anything is written"
  verify unit "a required builtin peer the embedded builtin does not satisfy is refused before anything is written"
  verify unit "add duplicate extension is a no-op with info message"
  verify unit "a dry run of an extension already present answers as the add would, whatever its source"
  verify unit "add extension with no specforge.json rejects with error and exit code 1"
  verify unit "add unresolvable extension rejects with diagnostic"
  verify unit "add extension with @scope/name@version resolves version via parse_extension_specifier"
  verify unit "add extension without version resolves to latest compatible version"
  verify unit "an extension whose peer range is not SemVer is refused with E073 before anything is installed"
  verify unit "a local install whose peer is installed outside its range is refused with E027"
  verify integration "an install that leaves a locked extension's peer unsatisfied is refused before anything is written, local or from a registry"
  verify unit "a peer locked outside the new extension's range fails R-RES-006 naming a version that satisfies every requirer"
  verify unit "a peer no single version satisfies for every requirer fails R-RES-005"
  verify unit "a peer its registry publishes no version of fails R-RES-002, as an add of that peer does"
  verify unit "an add that upgrades a locked package judges its peers against the lock as the add leaves it"
  verify unit "a version diamond names each requirer once"
  verify unit "add judges the project's peer set as the add leaves it: its builtins, installed extensions and .wasm file entries"
  verify unit "an install whose required peer is not installed is refused with E027 before anything is written"
  verify unit "an install whose required peer is installed but whose module is missing is refused with E027 before anything is written"
  verify unit "enabling a builtin that an enabled extension's peer range does not accept is refused with E027"
  verify unit "an install that breaks a .wasm file entry, or that a .wasm file entry does not satisfy, is refused with E027"
  verify unit "an install enables the builtins it requires first, whatever its source"
  verify unit "an extension no specforge.json entry enables is not judged"
  verify unit "an add of an extension a .wasm file entry loads is refused, naming the entry, and writes nothing"
  verify unit "an add of an extension a .wasm file entry loads in place of a broken install is refused, naming the entry, and writes nothing"
  verify integration "add and check agree: an add check would report E027 or E073 for is refused, and an add that passes leaves none"
  verify integration "a local .wasm install is locked at its declared version with source local:<path> and enabled by its bare name"
  verify unit "a local crate directory installs the component its crate builds"
  verify integration "add extension preserves all other config fields"
  verify contract "Add Extension to Existing Project: adding extension to existing project holds — existing_project_found, extension_resolvable, extension_appended, no_duplicate_added, other_fields_preserved, peer_deps_satisfied, extension_added_emitted"
}

behavior graceful_zero_extension_init "Graceful Zero-Extension Init" {
  category   command
  invariants [spec_root_singleton, init_config_validity, zero_domain_knowledge_core]
  types      [CompilerConfig]
  produces   [project_initialized]
  requires {
    zero_extensions_selected "Init completed with zero extensions selected (either by user choice or non-TTY fallback)"
    filesystem_available     "FileSystem port is available for writing project files"
  }
  ensures {
    empty_extensions_list       "Generated specforge.json has an empty extensions list"
    structural_starter_valid    "Generated starter spec file passes specforge check with zero errors"
    valid_graph_exportable      "specforge export produces a valid Graph Protocol JSON with structural entities"
    project_initialized_emitted "project_initialized event is emitted after successful zero-extension init"
  }
  contract   """
    When specforge init completes with zero extensions selected,
    the system MUST still produce a valid specforge.json with an
    empty extensions list. The generated starter .spec file MUST
    use only structural syntax (entity blocks with string fields
    and reference lists) that the core compiler can parse without
    any extensions. Running specforge check on the resulting
    project MUST produce zero errors. Running specforge export
    MUST produce a valid Graph Protocol JSON with the structural
    entities. This ensures Principle 1 (structure is a spectrum)
    and Principle 8 (seconds to value): even a project with zero
    domain extensions provides value.
  """
  verify unit "zero-extension init creates valid specforge.json with empty extensions"
  verify unit "zero-extension starter file passes specforge check"
  verify integration "zero-extension project produces valid graph via specforge export"
  verify integration "graceful_zero_extension_init completes full init-check-export cycle in under 60 seconds"
  verify unit "zero-extension config produces empty extensions array []"
  verify contract "Graceful Zero-Extension Init: zero-extension init holds — zero_extensions_selected, filesystem_available, empty_extensions_list, structural_starter_valid, valid_graph_exportable, project_initialized_emitted"
}

behavior locate_project_root "Locate the Project a Path Is In" {
  category   internal
  invariants [spec_root_singleton]
  types      [CompilerConfig]
  requires {
    filesystem_available "FileSystem port is available for directory traversal and symlink resolution"
  }
  ensures {
    closest_wins_enforced "The first directory at or above the path holding specforge.json is the project root"
    one_marker            "Only specforge.json marks a project; a specforge.spec file is a source like any other"
    loose_root            "With no specforge.json at or above the path, the path's directory is a loose root read with the default configuration"
    missing_path_refused  "A path that does not exist is refused as file_not_found, never read as an empty project"
    symlinks_resolved     "The walk follows the path's resolved directories, and the root is spelled as the path spells it where that spelling reaches it"
    every_surface_agrees  "Every command, watch, the LSP and MCP open the project a path is in by this rule; only init takes its directory as given"
    one_option            "On the command line a project is named by --path on every command that reads one, never by a positional argument"
  }
  contract   """
    Any path inside a project (the directory itself, a subdirectory, a
    file) names that project. The system MUST locate its root by walking
    from the path's directory (a file's parent) upward, symlinks
    resolved: the first directory holding specforge.json is the project
    root, the nearest winning. Only specforge.json marks a project; a
    specforge.spec file marks nothing and is a source like any other.
    With no specforge.json at or above the path, the path's own directory
    is a loose root: a project with the default configuration and no
    extension, whose I002 says no project was found and suggests
    specforge init. A path that does not exist MUST be refused as
    file_not_found ("path not found: <path>"), never read as an empty
    project. The root is spelled as the path spells it where that
    spelling reaches it, else canonically; two roots are the same when
    they are the same directory. Every core command that takes a path,
    every extension command (--path), specforge watch, the LSP's
    workspace root and MCP (its root and a call's path) MUST open the
    project their path is in by this one rule; the project's spec root
    is then read from its specforge.json (spec_root), and is never the
    path given. A path inside a directory below a project's spec root
    that holds its own specforge.json is in that nested project, and only
    there: the outer project's sources end at it (resolve_use_imports), so
    no command, watch, the LSP or MCP reads its files as the outer
    project's. On the command line every command that reads a project
    (every core command but init, new, explain, mcp and extension
    init|build|validate, and every extension command) names it with
    --path PATH, default ., whose help says it is a path inside the
    project; no command takes a project path as a positional argument,
    and no other argument is called --path (an extension crate's
    directory is --dir; specforge mcp's served root is its positional
    ROOT). Only init takes its directory as given: it creates the
    project there. A session keeps the root it was opened at; an
    environment reload does not locate it again.
  """
  verify unit "specforge.json found in the path's own directory"
  verify unit "specforge.json found in an ancestor directory"
  verify unit "closest directory wins over ancestor directory"
  verify unit "a specforge.spec file marks no project"
  verify unit "a path with no specforge.json at or above it is a loose root, its own directory"
  verify unit "a file path is in the project of its directory"
  verify unit "a path that does not exist is refused as file_not_found"
  verify unit "symlinks are resolved before path comparison"
  verify unit "the root is spelled as the path spells it"
  verify unit "circular symlink chain does not cause infinite loop"
  verify performance "directory traversal completes in under 100ms for 20-level deep hierarchy"
  verify integration "every command run from a subdirectory of a project acts on that project"
  verify integration "a command given a path that does not exist refuses it as file_not_found"
  verify integration "an extension command run from a subdirectory is routed over the project it is in"
  verify integration "watch run from a subdirectory watches the project it is in"
  verify integration "every command given --path acts on the project the path is in, from any directory"
  verify integration "a project path given as a positional argument is refused as a usage error"
  verify unit "every command that reads a project takes the one --path option, and none takes a project path as a positional argument"
  verify unit "a server started in a spec root holding specforge.spec serves the project around it"
  verify unit "a workspace root in a spec root holding specforge.spec opens the project around it"
  verify unit "a loose root's I002 says no project was found and suggests init"
  verify integration "a file in a nested project is checked by its own project and by no project above it"
  verify contract "Locate the Project a Path Is In: project root location holds — filesystem_available, closest_wins_enforced, one_marker, loose_root, missing_path_refused, symlinks_resolved, every_surface_agrees, one_option"
}
