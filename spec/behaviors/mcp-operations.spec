// MCP operation behaviors — mutations and project management queries
//
// 11 behaviors:
//   - Mutation Tools (6): format, rename, init, add_extension, remove_extension, migrate
//   - Project Management Tools (5): extensions, providers, doctor, collect, render

use "events/compilation"
use "events/mcp"
use "invariants/core"
use "invariants/formatting"
use "invariants/mcp"
use "invariants/validation"
use "invariants/zero-entity-core"
use "ports/inbound"
use "ports/outbound"
use "types/config"
use "types/core"
use "types/diagnostics"
use "types/formatting"
use "types/graph"
use "types/mcp"
use "types/migration"
use "types/output"

// ---------------------------------------------------------------------------
// Mutation Tools
// ---------------------------------------------------------------------------

behavior provide_mcp_format_tool "Provide MCP Format Tool" {
  features   [mcp_mutation_tools]
  invariants [
    diagnostic_determinism,
    formatting_idempotency,
    mcp_structured_error_responses,
    comment_preservation,
    formatting_consistency,
    format_rule_determinism,
    dry_run_side_effect_freedom,
  ]
  category   query
  types      [McpFormatResult, FormatDiff, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi, FileSystem]
  produces   [mcp_tool_invoked, mcp_mutation_completed]
  requires {
    filesystem_available "FileSystem port is available for reading and writing spec files"
  }
  ensures {
    files_formatted            "Spec files formatted according to canonical style"
    check_mode_readonly        "In check mode, no files modified"
    mutation_completed_emitted "mcp_mutation_completed event emitted after formatting (unless check mode)"
    tool_invoked_emitted       "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.format tool that
    accepts paths?[] (optional file paths, defaults to all), check? (optional
    boolean, report only without modifying), diff? (optional boolean, return
    diffs) and write? (optional boolean: given, it decides; absent, the call
    writes unless check or diff is set). Its input schema MUST state no
    default for write, since write has none of its own. The tool MUST format
    spec files according to the canonical style.
    In check mode, the tool MUST NOT modify files. In diff mode, the tool MUST
    return FormatDiff entries for each changed file. The tool MUST run the
    same format operation as specforge format. A file that cannot be read or
    written MUST NOT stop the others from being formatted: the result MUST
    name it, and the call MUST be reported as failed, with the kind of
    failure the OS gave (permission_denied for a file it refused, file_not_found
    for one that does not exist, internal_error for another cause or for
    failures of different kinds). ok MUST be the verdict specforge format
    exits by: false when a file could not be read or written or has a region
    left unformatted, or specforge.json has a problem the call went around
    (its E069 is among the diagnostics), and in check mode when a file would
    change; true
    otherwise, also after a write or a diff that found changes. A path naming a
    directory that is no project MUST be formatted as specforge format
    formats it, with the default configuration. all_clean MUST be true
    only when every file was read and is in canonical form; a region left
    unformatted (W142) MUST be returned among the diagnostics, with its file
    and line. Diagnostics from loading the format configuration
    (.specforgefmt.toml) MUST be returned in the result.
    A specforge.json that does not say which files are the project's
    sources MUST fail the call as config_invalid (schema_mismatch) before
    any file is read, writing nothing.
  """
  verify unit "specforge.format formats spec files"
  verify unit "check mode reports without modifying files"
  verify unit "diff mode returns FormatDiff entries"
  verify unit "paths filter restricts to specified files"
  verify unit "a file that cannot be written does not stop the others, and the failed call names it"
  verify unit "a file that cannot be read fails the call, is named, and does not stop the others"
  verify unit "a file that does not exist fails the call as file_not_found, naming it"
  verify unit "a file with a region left unformatted is not reported clean, and its W142 is returned"
  verify unit "format configuration diagnostics are returned in the result"
  verify unit "the format tool advertises no default for write, and check or diff without write writes nothing"
  verify unit "ok is the verdict specforge format exits by, in every mode"
  verify unit "a directory that is no project is formatted with the defaults, as specforge format formats it"
  verify contract "Provide MCP Format Tool: MCP format tool holds — filesystem_available, files_formatted, check_mode_readonly, mutation_completed_emitted, tool_invoked_emitted"
  verify unit "a specforge.json that does not say which files are the project's sources fails the call as config_invalid and writes nothing"
  verify unit "a problem in specforge.json the call goes around is an E069 among the diagnostics and makes ok false"
}

behavior provide_mcp_rename_tool "Provide MCP Rename Tool" {
  features   [mcp_mutation_tools]
  invariants [
    entity_id_uniqueness,
    graph_traversal_integrity,
    diagnostic_determinism,
    mcp_structured_error_responses,
    dry_run_side_effect_freedom,
  ]
  category   mutation
  types      [McpRenameResult, McpRenameEdit, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi, FileSystem]
  produces   [mcp_tool_invoked, mcp_mutation_completed]
  requires {
    graph_available      "Compiled graph is available via CompilerApi"
    filesystem_available "FileSystem port is available for updating spec files"
  }
  ensures {
    references_updated         "Entity renamed and all references updated across all spec files"
    recompilation_triggered    "Recompilation triggered after successful rename with updated diagnostics returned"
    dry_run_safe               "When dry_run is true, no files modified"
    mutation_completed_emitted "mcp_mutation_completed event emitted after rename"
    tool_invoked_emitted       "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.rename tool that
    accepts entity_id (required), new_name (required), dry_run? (optional
    boolean, default false) and path? (the project root; the served project
    when omitted; another project is planned, edited and brought up to date
    on its own, the served one untouched). The rename is planned on the project as
    it is on disk. The tool MUST rename the entity and update all
    references across all spec files. The edits are exactly the entity's
    declaration name and its references, as find-references returns them;
    text in strings, comments and verify statements that mentions the ID
    is not a reference and is not edited. The response MUST include the list of
    McpRenameEdit operations applied. When dry_run is true, the tool MUST return
    the rename plan (affected files and McpRenameEdit operations) without applying
    any changes. If the entity does not exist, the tool MUST return an error.
    If new_name is not a legal entity ID (t_entity_id: 2-60 letters, digits
    and underscores, starting with a letter or underscore), the tool MUST
    return a validation error. After a successful rename, the tool MUST trigger
    recompilation and return updated diagnostics in the response; a preview
    and a failed rename return none.
  """
  verify unit "specforge.rename renames entity and all references"
  verify unit "non-existent entity returns error response"
  verify unit "invalid new_name returns validation error"
  verify unit "dry_run returns rename plan without applying changes"
  verify contract "Provide MCP Rename Tool: MCP rename tool holds — graph_available, filesystem_available, references_updated, recompilation_triggered, dry_run_safe, mutation_completed_emitted, tool_invoked_emitted"
  verify unit "rename plans on the project as it is on disk, references added since the last call included"
  verify unit "rename edits exactly the declaration and the references find_references returns"
  verify unit "rename's updated diagnostics follow a successful write only: a preview and a failure carry none"
  verify unit "a specforge.json that does not say which files are the project's sources fails the rename as config_invalid and writes nothing"
}

// MCP init creates a project at a specified path, not the current project.
// The agent is connected to an existing project's MCP server and uses it
// to scaffold a new project elsewhere. For bootstrapping the very first
// project, use the CLI: specforge init.
behavior provide_mcp_init_tool "Provide MCP Init Tool" {
  features   [mcp_mutation_tools]
  invariants [
    diagnostic_determinism,
    init_config_validity,
    mcp_structured_error_responses,
    zero_domain_knowledge_core,
    spec_root_singleton,
  ]
  category   query
  types      [McpInitResult, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi, FileSystem]
  produces   [mcp_tool_invoked, mcp_mutation_completed, project_initialized]
  requires {
    filesystem_available "FileSystem port is available for creating project directory and files"
  }
  ensures {
    project_created             "specforge.json and spec directory scaffolded at specified path"
    path_outside_current        "Target path verified to be outside the project the server serves (its root), by the call target before the operation runs"
    extensions_validated        "When extensions specified, manifests validated and added to config"
    project_initialized_emitted "project_initialized event emitted on success, with the project name, its extension count and the starter file"
    tool_invoked_emitted        "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.init tool that
    accepts path (required, target directory for the new project), name
    (required), extensions?[] (optional list of extension names to install),
    and version? (optional, defaults to 0.1.0). The tool MUST accept a path
    parameter specifying the target directory for the new project. The path
    MUST be outside the project the server serves: a path inside its root
    is refused by the call target (conflict, naming path) before anything
    is written. A directory whose starter file already exists is refused
    (conflict), as specforge init refuses it. The tool MUST create
    a new specforge.json project configuration file and scaffold the spec
    directory at the specified path. If extensions are specified, they MUST
    be added to the config and their manifests validated. MCP init is always
    non-interactive — extension selection is provided via the extensions
    parameter. Interactive extension selection (TTY prompting) is only
    available via the CLI init command. The tool MUST scaffold what
    specforge init scaffolds for the same inputs, through the same
    operation: specforge.json with $schema and spec_root, the .gitignore
    entries, and the starter file spec/hello.spec from the enabled
    extensions' templates.
  """
  verify unit "specforge.init creates specforge.json project"
  verify unit "extensions installed when specified"
  verify unit "default version is 0.1.0"
  verify unit "path inside current project returns error"
  verify unit "init inside the served project is refused by the call target as a conflict on path, before anything is written"
  verify unit "init refuses a directory whose starter file exists, writing nothing"
  verify unit "invalid project name returns error"
  verify unit "unknown extension returns error with diagnostic"
  verify unit "version parameter overrides default 0.1.0"
  verify unit "specforge.init result includes the starter file path and installed extensions"
  verify unit "init without a path is invalid_input on path"
  verify integration "MCP init followed by check produces zero errors"
  verify integration "specforge.init writes the files and config specforge init writes for the same inputs"
  verify contract "Provide MCP Init Tool: MCP init tool holds — filesystem_available, project_created, path_outside_current, extensions_validated, project_initialized_emitted, tool_invoked_emitted"
}

behavior provide_mcp_add_extension_tool "Provide MCP Add Extension Tool" {
  features   [mcp_mutation_tools]
  invariants [
    diagnostic_determinism,
    mcp_structured_error_responses,
    init_config_validity,
    dry_run_side_effect_freedom,
  ]
  category   query
  types      [McpAddExtensionResult, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi, FileSystem]
  produces   [mcp_tool_invoked, mcp_mutation_completed, extension_added]
  requires {
    filesystem_available "FileSystem port is available for updating specforge.json"
  }
  ensures {
    extension_installed     "Extension added to specforge.json with manifest validated"
    wasm_downloaded         "Wasm module downloaded if extension is remote"
    extension_added_emitted "extension_added event emitted on success"
    dry_run_safe            "When dry_run is true, no files modified and preview returned"
    tool_invoked_emitted    "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.add_extension
    tool that accepts specifiers (required, a non-empty list of
    @scope/name[@version], builtin names or .wasm paths, added as one change
    exactly as specforge add adds several: all or nothing, judged on the
    whole set, a refusal naming the specifier refused, a repeated one
    reported as a duplicate), dry_run? (optional boolean, default false) and allow_unsigned?
    (optional boolean, default false: accept a registry package with no
    publisher signature). When dry_run is true, the tool MUST
    return a preview of the changes without modifying specforge.json or
    downloading any Wasm modules. The tool MUST add
    the extension to specforge.json, download the Wasm module if remote, and
    validate the extension manifest. If the manifest is invalid or the
    extension conflicts with an existing one, the tool MUST return an error.
    If an extension is already installed, the tool MUST answer action
    already_present for it without modifying specforge.json. A call that
    succeeds emits one extension_added per specifier, in order, with
    wasDuplicate true for an extension already there or a specifier
    repeating an earlier one; a dry run emits none. Its reply is the add
    document specforge add --format json prints (McpAddExtensionResult: one
    entry per specifier under added), plus a note saying whether the server
    serves the extensions from the next call on, absent for a preview. A registry specifier with no registry configured in
    specforge.json MUST make no network call and MUST return an E063 error
    whose suggestion names the registries key. The tool MUST run the add
    specforge add runs: a builtin is enabled offline with its required
    builtin peers, and a registry package passes the same integrity,
    signature and version-diamond checks (a key change is refused, since
    no one can be asked).
  """
  verify unit "with no registry configured, add_extension makes no network call and reports how to configure one"
  verify unit "add_extension of a registry package reports what reading the registry configuration found"
  verify unit "specforge.add_extension adds extension to config"
  verify unit "specforge.add_extension adds every specifier as one change, one extension_added per specifier"
  verify unit "already-installed extension returns info without modifying config"
  verify unit "wasm module downloaded for remote extensions"
  verify unit "invalid manifest returns error"
  verify unit "dry_run returns preview without modifying files"
  verify unit "add, init and publish read a candidate's declaration in the runtime their surface passes"
  verify contract "Provide MCP Add Extension Tool: MCP add extension tool holds — filesystem_available, extension_installed, wasm_downloaded, extension_added_emitted, dry_run_safe, tool_invoked_emitted"
  verify unit "invalid specifier format returns error"
  verify integration "a builtin is enabled with no registry and no network"
  verify integration "a version diamond with a locked peer is refused with R-RES-006, as specforge add refuses it"
  verify unit "after add_extension the server serves the extension it installed"
}

behavior provide_mcp_remove_extension_tool "Provide MCP Remove Extension Tool" {
  features   [mcp_mutation_tools]
  invariants [
    diagnostic_determinism,
    mcp_structured_error_responses,
    init_config_validity,
    dry_run_side_effect_freedom,
  ]
  category   query
  types      [McpRemoveExtensionResult, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi, FileSystem]
  produces   [mcp_tool_invoked, mcp_mutation_completed]
  requires {
    filesystem_available "FileSystem port is available for updating specforge.json"
  }
  ensures {
    extension_removed          "Extension removed from specforge.json"
    stranded_entities_listed   "The entities the removal strands are listed"
    dry_run_safe               "When dry_run is true, no files modified and preview returned"
    mutation_completed_emitted "mcp_mutation_completed event emitted after removal"
    tool_invoked_emitted       "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.remove_extension
    tool that accepts name (required), force? and dry_run? (optional
    booleans, default false) and path? (as rename); dependents and stranded
    entities are those of the project the path names. When dry_run is true, the tool MUST return a
    preview of the removal (including the stranded entities) without modifying
    specforge.json. The tool MUST remove the
    extension from specforge.json. If removing the extension strands
    entities (entities of kinds only that extension defines, E024 on the
    next compile), the tool MUST list them in the response (stranded:
    entity_id, kind, in id order) but still proceed.
    Its reply is the remove document specforge remove --format json prints
    (McpRemoveExtensionResult): action removed, or planned for a dry run,
    the extension, its source and version, and the stranded entities.
    If the specified extension is not installed (not listed in specforge.json),
    the tool MUST return an isError result whose McpError code is
    "extension_not_found" and whose message names the unknown extension.
    A .wasm file entry is removed as remove_extension removes it: by the
    name its component declares or by its entry, its file left in place.
  """
  verify unit "specforge.remove_extension removes extension from config"
  verify integration "specforge.remove_extension removes a .wasm file entry by the name it declares, leaving its file in place"
  verify unit "the entities whose kind only that extension declares are listed as stranded"
  verify unit "non-installed extension returns extension_not_found error"
  verify unit "dry_run returns preview without modifying files"
  verify contract "Provide MCP Remove Extension Tool: MCP remove extension tool holds — filesystem_available, extension_removed, stranded_entities_listed, dry_run_safe, mutation_completed_emitted, tool_invoked_emitted"
}

behavior provide_mcp_migrate_tool "Provide MCP Migrate Tool" {
  features   [mcp_mutation_tools]
  invariants [diagnostic_determinism, mcp_structured_error_responses, dry_run_side_effect_freedom]
  category   mutation
  types      [McpMigrateResult, MigrationResult, MigrationSummary, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi, FileSystem]
  produces   [mcp_tool_invoked, mcp_mutation_completed]
  requires {
    filesystem_available "FileSystem port is available for reading and writing spec files"
  }
  ensures {
    migrations_applied         "Pending migrations detected and applied to spec files"
    post_migration_validated   "Post-migration validation performed with errors reported"
    dry_run_safe               "In dry_run mode, diff returned without modifying files"
    mutation_completed_emitted "mcp_mutation_completed event emitted after migration"
    tool_invoked_emitted       "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.migrate tool
    that accepts dry_run? (optional boolean, default false),
    target_version? (optional string, format "major.minor", default the
    current format version), no_backup? (optional boolean, default false:
    skip the .bak backups) and path? (the project root; the initialized
    root when omitted). A target_version that is malformed or above the
    highest supported format version is an E019 error, and modifies no
    file. The tool MUST
    detect and apply pending migrations to spec files. In dry_run mode, the
    tool MUST return the diff without modifying any files. After migration,
    the tool MUST validate the result and report any post-migration errors.
    Its reply is the migration document specforge migrate --format json
    prints (McpMigrateResult); a project with nothing pending answers it
    with pending false and nothing migrated.
    The tool MUST run the same migration as specforge migrate: extension
    migration hooks run after the files are migrated, and a migration whose
    hooks fail or whose graph changes structure is rolled back. The result
    MUST report the hooks run, the structural differences found, and
    whether the migration was rolled back. The result (and a failed call's
    data) MUST carry ok, the verdict specforge migrate exits by: false when
    a file failed to migrate, the migration was rolled back, or
    specforge.json has a problem the run went around (E069, in
    diagnostics). A failed
    migration's error names its cause: a migration hook that failed
    (internal_error), a migrated project whose graph changed (W054) or that
    does not compile (compilation_failed), or files that could not be
    migrated or a specforge.json not used as written (config_invalid,
    schema_mismatch); its data is the migration document. A path
    naming a directory that is no project
    MUST be migrated as specforge migrate migrates it. A project with nothing to
    migrate MUST be reported as already at the target version, without
    running hooks or validation.
    A specforge.json that does not say which files are the project's
    sources or which extensions it loads MUST fail the call as
    config_invalid before any file is read, writing nothing.
  """
  verify unit "specforge.migrate applies pending migrations"
  verify unit "dry_run returns diff without modifying files"
  verify unit "post-migration validation reports errors"
  verify unit "target_version selects the format version to migrate to"
  verify unit "a malformed or unsupported target_version is refused without modifying files"
  verify unit "the result reports the hooks run, the structural differences and whether the migration was rolled back"
  verify unit "ok is the verdict specforge migrate exits by, and a failed migration's kind is the operation's"
  verify unit "a failed migration's error names its cause: a hook, a changed graph, a project that does not compile, or files that could not be migrated"
  verify unit "a directory that is no project is migrated as specforge migrate migrates it"
  verify contract "Provide MCP Migrate Tool: MCP migrate tool holds — filesystem_available, migrations_applied, post_migration_validated, dry_run_safe, mutation_completed_emitted, tool_invoked_emitted"
  verify unit "a specforge.json that does not say which files are the project's sources fails the call as config_invalid and writes nothing"
}

// ---------------------------------------------------------------------------
// Project Management Tools
// ---------------------------------------------------------------------------

behavior provide_mcp_extensions_tool "Provide MCP Extensions Tool" {
  features   [mcp_project_management_tools]
  invariants [diagnostic_determinism, mcp_structured_error_responses, mcp_tool_idempotency]
  category   query
  types      [McpExtensionsResult, McpExtensionInfo, McpLockEntry, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_tool_invoked]
  requires {
    compiler_api_available "CompilerApi port is available for querying loaded extensions"
  }
  ensures {
    extensions_listed    "All installed extensions returned with name, version, entity kinds, and status"
    config_reflected     "Response reflects current specforge.json configuration"
    tool_invoked_emitted "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.extensions tool
    with no required parameters. The tool MUST return a list of all installed
    extensions including name, version, entity kinds contributed, and status.
    The response MUST reflect the current specforge.json configuration.
  """
  verify unit "specforge.extensions lists all installed extensions"
  verify unit "each entry includes name, version, entity kinds, and status"
  verify contract "Provide MCP Extensions Tool: MCP extensions tool holds — compiler_api_available, extensions_listed, config_reflected, tool_invoked_emitted"
}

behavior provide_mcp_providers_tool "Provide MCP Providers Tool" {
  features   [mcp_project_management_tools]
  invariants [diagnostic_determinism, mcp_structured_error_responses, mcp_tool_idempotency]
  category   query
  types      [McpProvidersResult, McpProviderInfo, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_tool_invoked]
  requires {
    compiler_api_available "CompilerApi port is available for querying configured providers"
  }
  ensures {
    providers_listed     "All configured providers returned with scheme, alias, extension, and status"
    tool_invoked_emitted "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.providers tool
    with no required parameters. The tool MUST return a list of all configured
    providers including scheme, alias, extension, and status. Providers supply
    external reference validation via registered schemes.
  """
  verify unit "specforge.providers lists all configured providers"
  verify unit "each entry includes scheme, alias, extension, and status"
  verify contract "Provide MCP Providers Tool: MCP providers tool holds — compiler_api_available, providers_listed, tool_invoked_emitted"
}

behavior provide_mcp_doctor_tool "Provide MCP Doctor Tool" {
  features   [mcp_project_management_tools]
  invariants [diagnostic_determinism, mcp_structured_error_responses, mcp_tool_idempotency]
  category   query
  types      [McpDoctorReport, McpToolDescriptor, McpDoctorFinding]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_tool_invoked]
  requires {
    compiler_api_available "CompilerApi port is available for project health inspection"
  }
  ensures {
    health_checked            "Project health checked: extension conflicts, stale cache, missing fields, version mismatches, unreferenced entities"
    resolution_steps_provided "Deterministic resolution steps included for each detected issue"
    tool_invoked_emitted      "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.doctor tool with
    no required parameters. The tool MUST check project health: extension
    conflicts, stale Wasm cache entries, extensions that fail to load (E028,
    E070), missing specforge.json fields, version mismatches, and unreferenced
    entities. Every diagnostic the environment reports MUST be a finding, an
    unusable specforge.json (E069) included. The response MUST be the report specforge doctor computes: its
    verdict ok and each detected issue once, with what it is about and a
    deterministic resolution step. Like specforge.validate, the tool MUST bring the
    project up to date with disk before checking it, so it sees edits made
    outside the server; with use_cached (optional boolean, default false) it
    MUST report on the project as last brought up to date instead.
  """
  verify unit "specforge.doctor detects extension conflicts"
  verify unit "response checks wasm cache integrity"
  verify unit "response provides deterministic resolution steps"
  verify unit "specforge.doctor reports an extension that fails to load (E028, E070) as an error"
  verify unit "specforge.doctor compiles the project afresh unless use_cached is set"
  verify unit "specforge.doctor reports an unusable specforge.json (E069) as a finding"
  verify contract "Provide MCP Doctor Tool: MCP doctor tool holds — compiler_api_available, health_checked, resolution_steps_provided, tool_invoked_emitted"
}

behavior provide_mcp_collect_tool "Provide MCP Collect Tool" {
  features   [mcp_project_management_tools]
  invariants [diagnostic_determinism, mcp_structured_error_responses]
  category   query
  types      [McpCollectResult, McpCollectRunner, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi, FileSystem]
  produces   [mcp_tool_invoked]
  requires {
    filesystem_available   "FileSystem port is available for reading test results and writing report"
    compiler_api_available "CompilerApi port is available for extension collector dispatch"
  }
  ensures {
    report_emitted       "specforge-report.json emitted with test-to-entity mappings"
    collector_delegated  "Collection delegated to the enabled extensions' collectors"
    never_prompts        "the tool never asks for approval: it runs only commands the user already approved"
    tool_invoked_emitted "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.collect tool
    that accepts runner? (a collector name or extension; detected from
    project files when omitted), run? (default false) and path? (the
    project root; the initialized root when omitted). It runs the same
    flow as `specforge collect`: by default it parses the runners' existing
    reports; with run=true it first runs each runner's declared command,
    but only a command the user already approved for the project with
    `specforge collect` in a terminal, and with its output discarded
    because the server owns stdio. A path that holds no project is a
    no_project refusal, as `specforge collect` refuses it. An unapproved
    command is an E059 error,
    and a missing collector or report is an error naming its code. The
    result lists each runner's counts, the W115 diagnostics and the path of
    the written specforge-report.json.
  """
  verify unit "specforge.collect parses test results and maps to entities"
  verify unit "specforge.collect refuses to run an unapproved command"
  verify unit "specforge.collect of a directory that holds no project refuses with no_project"
  verify unit "a project without a collector returns an E058 error"
  verify contract "Provide MCP Collect Tool: MCP collect tool holds — filesystem_available, compiler_api_available, report_emitted, collector_delegated, never_prompts, tool_invoked_emitted"
}

behavior provide_mcp_render_tool "Provide MCP Render Tool" {
  features   [mcp_project_management_tools]
  invariants [graph_traversal_integrity, diagnostic_determinism, mcp_structured_error_responses]
  category   query
  types      [McpRenderResult, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi, FileSystem]
  produces   [mcp_tool_invoked]
  requires {
    graph_available      "Compiled graph is available via CompilerApi"
    filesystem_available "FileSystem port is available for writing output files"
  }
  ensures {
    files_written        "Output files written to out_dir by the matching renderer"
    files_listed         "Response lists all files written"
    tool_invoked_emitted "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.render tool that
    accepts format (required, a format string matching a registered renderer),
    out_dir? (output directory path) and scope? (an entity id). The tool MUST
    invoke the matching registered renderer and write output files to out_dir;
    without out_dir it MUST return the rendering inline instead. A relative
    out_dir names a directory under the call's project root, wherever the
    server runs; with no project served a relative out_dir MUST be refused as
    invalid input on out_dir. output_files lists the absolute paths written.
    The renderers are the core graph engine's export formats (see P7
    justification in features/output.spec), named as `specforge export
    --format` names them: graph (also accepted as json; the full graph, as
    `specforge export --format graph` writes it, in graph.json), dot,
    context and brief. The result's format is the renderer's name.
    Extension renderer contributions are not dispatched by this tool.
    Renderers produce graph diagnostic artifacts: JSON serializations, DOT
    visualizations, traceability matrices, validation summaries. They MUST NOT
    produce source code, application configuration, user documentation, or any
    artifact consumed by end users or deployed to production. The distinction:
    if the artifact helps understand the graph, it belongs here; if it is
    consumed beyond the spec workflow, it belongs to an agent (vision/README.md).
    Unrecognized format strings MUST return an error listing available renderers.
    The response MUST list all files written.
  """
  verify unit "specforge.render writes output files to out_dir"
  verify unit "a relative out_dir is written under the call's project root, wherever the server runs"
  verify unit "a relative out_dir with no project served is invalid input on out_dir"
  verify unit "registered renderer invoked for matching format"
  verify unit "unrecognized format returns error listing available renderers"
  verify unit "graph and its alias json select the full graph renderer"
  verify contract "Provide MCP Render Tool: MCP render tool holds — graph_available, filesystem_available, files_written, files_listed, tool_invoked_emitted"
}
