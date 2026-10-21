// Migration behaviors — spec file format version migration

use "events/compilation"
use "invariants/core"
use "invariants/migration"
use "invariants/validation"
use "invariants/wasm"
use "invariants/zero-entity-core"
use "ports/inbound"
use "ports/outbound"
use "types/core"
use "types/diagnostics"
use "types/graph"
use "types/migration"
use "types/wasm"
use "types/zero-entity-core"

defaults behavior {
  features [spec_file_migration]
  types    [SpecFile, MigrationResult]
}

behavior detect_format_version_mismatch "Detect Format Version Mismatch" {
  invariants [multi_error_collection, zero_domain_knowledge_core, diagnostic_determinism]
  category   validation
  types      [SpecFile, FormatVersion]
  requires {
    spec_file_available "A .spec file is being parsed and its content is accessible"
  }
  ensures {
    version_mismatch_reported    "I007 info diagnostic emitted when detected version differs from expected version"
    unsupported_version_rejected "E019 diagnostic emitted for format versions outside the supported range"
    parsing_continues            "Parsing proceeds with best-effort compatibility regardless of version mismatch"
  }
  contract   """
    During parsing, the system MUST detect when a .spec file uses an older
    format version than the compiler expects. The detected version and the
    expected version MUST be reported as an I007 info diagnostic. The system
    MUST continue parsing with best-effort compatibility. Files without an
    explicit format version MUST be treated as the current version and
    report nothing.

    A file declares its format version with a header comment
    `// specforge-format: <major>.<minor>` as its first non-blank line. The
    version names the .spec file format the file was written against; it
    is distinct from the Graph Protocol's SchemaVersion (types/graph.spec).

    The compiler MUST support the current and previous major format versions.
    Files declaring an unsupported format version (older than previous major
    or newer than current) MUST produce an E019 diagnostic with upgrade
    guidance indicating which compiler version supports that format.

    The compile reports both as ordinary diagnostics, spanned on the header
    line, so every surface that shows a project's diagnostics (check, watch,
    the language server, MCP) shows them; `specforge migrate` reads the
    version with the same detection.
  """
  verify unit "older format version detected and reported as I007"
  verify unit "current format version produces no diagnostic"
  verify unit "missing format version treated as the current version"
  verify unit "header comment format version detected correctly"
  verify unit "unsupported format version produces E019 with upgrade guidance"
  verify contract "Detect Format Version Mismatch: format version detection holds — spec_file_available, version_mismatch_reported, unsupported_version_rejected, parsing_continues"
}

// Transform chaining strategy: when migrating across multiple format versions
// (e.g., v1 → v3), transforms are applied sequentially (v1→v2, v2→v3), not
// directly (v1→v3). Each step is a self-contained transform function. This
// ensures that each version boundary is validated independently.
behavior migrate_spec_files_in_place "Migrate Spec Files In Place" {
  invariants [
    multi_error_collection,
    migration_idempotency,
    migration_backup_safety,
    migration_atomicity,
    migration_semantic_preservation,
    zero_domain_knowledge_core,
    document_project_paths,
  ]
  category   mutation
  types      [SpecFile, MigrationResult, MigrationSummary, MigrationBackup]
  ports      [CompilerApi, FileSystem]
  produces   [migration_starting, migration_started, migration_complete]
  requires {
    files_exist          "All .spec files targeted for migration exist on disk and are readable"
    valid_target_version "Target version is a valid format version"
  }
  ensures {
    files_at_target      "All successfully migrated files are at the target version"
    summary_emitted      "migration_summary diagnostic emitted with accurate counts"
    complete_event_fired "migration_complete event fires after all files are processed"
  }
  maintains {
    semantic_preservation "Migrated files preserve all entity IDs, references, and field values from the source version"
  }
  // Temporal distinction between migration events:
  //   migration_starting — fires before any file I/O begins (intent signal).
  //   migration_started — fires after the verdict keeps the migration,
  //     when its files are staged for writing (point of no return). Backups
  //     exist only for a kept migration: a refused one writes nothing.
  contract   """
    When specforge migrate is invoked, the system MUST first capture the
    compiled graph before any file is written, then emit
    migration_starting before beginning transforms.

    The system MUST transform .spec files from the detected source version
    to the target version. Before modifying any file, the system MUST
    create a backup copy with a .bak extension unless --no-backup is
    specified. A migration MUST be written all or nothing: when any file
    cannot be read or migrated, no file is written, and the report names every
    such file with its error (a dry run reports the same). The run is one
    project change: each source is replaced by its name, keeping its
    permissions, and a source the process may not write refuses the run
    (file_unwritable), as a typed edit refuses it; a source changed since
    the migration read it refuses the run (file_changed). Each backup is a
    name the migration owns: whatever is at it (a link, or a file left by an
    earlier run) is unlinked, never its target, and the backup is created
    new with its source's permissions. Each file MUST be read once for the
    migration: its backup, its diff and the text its change is staged over
    are that one read, so a save made while the migration runs is never
    split between them. Over MCP, a source, its backup or the migration record
    that would land outside the project, by a symlink, refuses the whole
    call before anything is written.

    The header is rewritten where the parser reads it, the file's first
    non-blank line; a file with none gets one as its first line. No other
    line changes.

    The system MUST report the number of files migrated, failed, and
    skipped via MigrationSummary. Skipped files are those already at the
    target version — this ensures migration idempotency: running migrate
    on an already-current project produces zero modifications.

    The FileSystem port is required for reading source files, writing
    backups, writing temporary files, and performing atomic renames.

    Its JSON output is the migration document specforge.migrate answers
    (McpMigrateResult), listing each migrated file and each backup as
    files_written (none for a dry run).

    A migration that is kept (no file failed, its hooks and checks
    passed) and made backups MUST record each migrated file, its backup
    and the SHA-256 of the file as the migration left it in
    .specforge/migration.json as part of the same change, replacing any
    earlier record, and lists that file as written. A kept migration made with --no-backup removes
    the record.
    The project's specforge.json is read once, through the read the compile
    uses, before any file: one that does not say which files are the
    project's sources or which extensions it loads (it can't be used, or
    spec_root, exclude or extensions has the wrong type) MUST refuse the
    migration with config_invalid, dry run included, and nothing is written.
    Each other problem is one E069 in the migration document's diagnostics
    and makes ok false; the migration still runs. The preview and the
    migration rewrite the same sources: the project's sources
    (resolve_use_imports), so a nested project's files are its own
    project's to migrate.
  """
  verify integration "migrate --format json lists each migrated file and its backup in files_written"
  verify integration "the migrate document names each file and backup from the project root"
  verify integration "migrate --format json prints the migration document specforge.migrate answers, with files_written"
  verify unit "a refused migration made without backups leaves the record as it was"
  verify contract "Migrate Spec Files In Place: in-place migration holds — semantic_preservation"
  verify unit "files migrated from source to target version"
  verify unit "backup created before modification"
  verify unit "--no-backup skips backup creation"
  verify unit "a file that cannot be migrated leaves every file as it was"
  verify unit "a migrated source keeps its permissions"
  verify unit "a backup has its source's permissions"
  verify unit "a read-only source refuses the migration and nothing is written"
  verify unit "a record the migration cannot write refuses it"
  verify unit "a source changed since the migration read it refuses the migration"
  verify unit "summary reports migrated, failed, and skipped counts"
  verify unit "interrupted migration leaves no partially written files"
  verify unit "the backup and temporary file are created new, never through a planted link"
  verify unit "the graph is captured before any file is transformed"
  verify unit "files already at target version are skipped with skippedCount incremented"
  verify unit "only the project's sources are migrated: under spec_root, without excluded files"
  verify unit "a kept migration records its files in .specforge/migration.json"
  verify unit "each file is read once: its backup, its diff and the text its change is staged over are that text"
  verify unit "a specforge.json that does not say which files are the project's sources refuses the migration, dry run included, and nothing is written"
  verify unit "a problem in specforge.json the migration goes around is an E069 in its document and fails it"
  verify unit "the preview and the migration rewrite the same sources, read once"
  verify unit "a header below leading blank lines is rewritten where it stands"
  verify property "the header migrate writes is the header the parser reads"
  verify integration "migrate leaves a nested project's files alone"
}

behavior add_missing_titles "Add Missing Titles" {
  invariants [migration_idempotency, migration_semantic_preservation]
  category   mutation
  ports      [FileSystem]
  contract   """
    The titles step MUST write a title into every block header that has
    none, entity blocks and unions alike: the block's doc comment (the //
    lines directly above its header with no blank line between, the format
    header excluded), each line's text after // trimmed and the lines
    joined by one space, with \\ and " escaped, the comment lines removed.
    A block with no doc comment MUST get its id in Title Case words. The
    step MUST leave define, spec and ref blocks, titled blocks and text
    with a syntax error as they are, and MUST be idempotent. It runs on
    every file it changes, whatever the file's format version, and the
    graph comparison keeps exactly the title it added where a block had
    none.
  """
  verify unit "a doc comment directly above a block becomes its title"
  verify unit "a multi-line doc comment becomes one title"
  verify unit "a union takes its doc comment as its title"
  verify unit "a block with no doc comment is titled with its id in words"
  verify unit "a comment after a blank line is not lifted"
  verify unit "a trailing comment on the header stays"
  verify unit "the format header is never lifted"
  verify unit "quotes and backslashes in a doc comment are escaped in the title"
  verify unit "titled, define, spec and ref blocks are left alone"
  verify unit "a file with a syntax error is left alone"
  verify unit "the titles step is idempotent"
  verify unit "a migration that adds titles is kept, with no W054"
  verify unit "a dry run shows the titles and writes nothing"
  verify unit "a project already titled has nothing pending"
  verify unit "a planned text that also changes a field refuses the titles"
  verify unit "the changes beyond the steps drop only a title the step added"
}

behavior hoist_file_defaults "Hoist File Defaults" {
  invariants [migration_idempotency, migration_semantic_preservation]
  category   mutation
  ports      [FileSystem]
  contract   """
    The defaults step MUST move into a `defaults <kind>` block each reference
    list a file's blocks repeat: for a kind with two or more blocks in the
    file and no defaults block of its own, a field is hoisted when every block
    of the kind writes it exactly once, as a one-line list of bare items with
    no annotation and no comment on or directly above its line, and at least
    two blocks write the same list. The most common list becomes the default
    and is deleted from the blocks that write it; the others keep theirs. A
    block MUST be written only when it saves at least one line. It goes after
    the last import, or before the first form and its leading comments. The
    step MUST leave a text with a syntax error as it is and MUST be
    idempotent. The graph comparison proves the graph is unchanged: a hoist
    that changes it is refused with W054 and nothing is written.
  """
  verify unit "a list every block writes alike is hoisted"
  verify unit "the most common list is hoisted and the others keep theirs"
  verify unit "a kind where a block lacks the field is left as written"
  verify unit "an identifier, a quoted or a multi-line list is never hoisted"
  verify unit "a line with a comment on or above it is left as written"
  verify unit "a kind that has defaults is left as written"
  verify unit "a hoist that saves no line is not written"
  verify unit "the block goes after the imports or before the first form"
  verify unit "hoisting twice changes nothing"
  verify unit "a file with a syntax error is left alone"
  verify unit "a migration that hoists defaults keeps the graph"
  verify unit "a hoist that changes the graph is refused and nothing is written"
}

behavior generate_migration_diff "Generate Migration Diff" {
  invariants [
    diagnostic_determinism,
    migration_idempotency,
    dry_run_side_effect_freedom,
    zero_domain_knowledge_core,
  ]
  category   query
  types      [SpecFile, MigrationResult, MigrationDiff]
  ports      [CompilerApi]
  produces   [migration_diff_generated]
  requires {
    spec_files_available "All .spec files targeted for migration exist on disk and are readable"
    dry_run_flag_set     "The --dry-run flag is specified on the migrate command"
  }
  ensures {
    diff_produced                    "Unified diff of all migration changes is computed and displayed"
    no_files_modified                "No .spec files are modified on disk during dry-run"
    migration_diff_generated_emitted "migration_diff_generated event fires with the computed diff"
  }
  contract   """
    When specforge migrate --dry-run is invoked, the system MUST compute
    and display the unified diff of all changes that would be applied without
    modifying any files. The diff MUST use POSIX standard unified diff format
    (IEEE Std 1003.1 unified output format) compatible with patch(1) and
    git apply. A dry run reports the verdict the run reaches: the hooks run
    and the planned texts are compiled in memory and compared, so its ok,
    structural_differences and post_migration_errors are the run's, and it
    fails exactly when the run would be refused. Each file's diff MUST be clearly labeled with the file path
    using the `--- a/path` and `+++ b/path` header convention. A failure
    to compute the diff for a specific file MUST NOT prevent diff generation
    for other files — the system MUST report per-file outcomes using
    MigrationResult. When `--format=json` is specified, the diff MUST be
    serialized as a MigrationDiff JSON object with file-level entries, each
    containing path, before/after content hashes, and a list of changed
    line ranges.
  """
  verify unit "dry-run shows unified diff without modifying files"
  verify unit "diff format is compatible with patch(1)"
  verify unit "each file diff labeled with file path"
  verify unit "failure in one file does not block diff generation for others"
  verify unit "json format diff produces structured output with file-level entries"
  verify unit "a dry run reports the verdict the run reaches: its hooks, its W054 and its errors"
  verify unit "a dry run the run would refuse exits 1"
  verify contract "Generate Migration Diff: migration diff generation holds — spec_files_available, dry_run_flag_set, diff_produced, no_files_modified, migration_diff_generated_emitted"
}

behavior validate_post_migration_integrity "Validate Post-Migration Integrity" {
  invariants [
    multi_error_collection,
    graph_traversal_integrity,
    migration_event_ordering,
    diagnostic_determinism,
    migration_semantic_preservation,
    migration_cross_extension_stability,
    zero_domain_knowledge_core,
  ]
  category   validation
  types      [Graph, GraphDelta, DiagnosticBag]
  ports      [CompilerApi]
  // Runs exactly once after the terminal migration event
  // (extension_migration_hooks_complete).
  consumes   [extension_migration_hooks_complete]
  produces   [migration_validation_complete]
  requires {
    extension_hooks_complete_fired "extension_migration_hooks_complete event has fired, confirming all extension migration hooks have run"
  }
  ensures {
    structural_equivalence_checked        "The graph delta from the pre-migration graph to the post-migration graph is computed, source positions ignored"
    differences_reported                  "Each change the graph delta names is reported as a W054 warning"
    migration_validation_complete_emitted "migration_validation_complete event fires after validation finishes"
  }
  contract   """
    This behavior validates instance-level integrity after migration.
    A migration never changes the Graph Protocol schema: the schema is the
    registry build's (ADR 0060), the registry build is the environment's,
    and a migration changes only the project's sources (its hooks run with
    no file system), so no schema is compared.

    After all migration transforms and extension hooks have completed,
    and before any file is written, the system MUST compile the planned
    texts in memory over the project (the verdict, migrate::judge) and
    compute the GraphDelta from the pre-migration graph to the post-migration graph
    (compute_graph_delta): entity IDs, and each entity's kind, title,
    field values, methods and outgoing edges. Source positions MUST be
    ignored, since format migration inherently shifts them. The
    migration steps declare their own changes (add_missing_titles: a
    title where a node had none); the delta is taken less those. The
    migration keeps the graph exactly when that delta is empty. Each
    change the delta names MUST be reported as one W054 warning: an
    entity removed or added, an entity whose content changed (naming
    what changed, in the delta's words), and an edge removed or added.
    A non-empty delta refuses the migration: nothing is written. New
    diagnostics introduced by migration MUST be reported, and a refusal for
    errors counts only those. A dry run reports the verdict the run reaches
    and fails exactly when the run would be refused.
  """
  verify unit "post-migration check runs automatically"
  verify unit "structural equivalence verified between pre and post graphs"
  verify unit "structural differences reported as warnings"
  verify unit "new diagnostics from migration reported"
  verify unit "a planned text that changes an existing title or a kind is reported and refused"
  verify unit "each change the graph delta names is one W054 warning, and an empty delta none"
  verify unit "a refused migration's cause counts only the errors it would add"
  verify unit "a refused migration writes nothing, not even a backup"
  verify unit "under --format json a refused migration writes nothing beside its document"
  verify unit "a failed migration is told on stderr by its failure's message, naming no backup it did not make"
  verify contract "Validate Post-Migration Integrity: post-migration integrity validation holds — extension_hooks_complete_fired, structural_equivalence_checked, differences_reported, migration_validation_complete_emitted"
}

behavior rollback_failed_migration "Rollback Failed Migration" {
  invariants [migration_backup_safety, migration_atomicity, zero_domain_knowledge_core]
  category   command
  types      [MigrationResult, MigrationSummary, MigrationBackup]
  ports      [FileSystem]
  produces   [migration_rolled_back]
  requires {
    migration_recorded "For --rollback, .specforge/migration.json records the last kept migration"
  }
  ensures {
    files_restored         "Every recorded file still as the migration left it is restored from its backup"
    edited_files_kept      "A recorded file changed since the migration is left as it is, with a warning, and stays recorded"
    rollback_event_emitted "migration_rolled_back event is emitted with accurate restored, skipped, and failed counts"
  }
  maintains {
    backup_file_preservation ".bak backup files are never deleted by --rollback (preserved for user inspection)"
  }
  contract   """
    A migration whose extension hook fails, or whose validation detects
    structural differences, is refused before any file is written, so
    there is nothing to restore: a refused migration writes no source, no
    backup and no record, --no-backup included. Rollback is on demand via
    the --rollback CLI flag. --rollback MUST undo the migration recorded in
    .specforge/migration.json and nothing else: each recorded file whose
    SHA-256 is still the one recorded MUST be restored from its backup;
    a recorded file changed since MUST be left as it is with a warning
    naming it and its backup; a recorded file whose backup is missing
    MUST be skipped with a warning naming the backup. With no record,
    nothing is restored and one warning says so. The record then keeps
    only the files not restored, and is removed when none is left. Each
    file MUST be restored by replacing its name, keeping its permissions;
    a file the process may not write fails its restore. If a
    restore fails for one file, the system MUST emit an E-level
    diagnostic and continue with the remaining files — a single restore
    failure MUST NOT block others. After rollback, .bak files MUST be
    preserved (not auto-deleted) so the user can retry or inspect them.
    The system MUST report the number of files restored, skipped, and
    failed. It MUST report each file it restored as written
    (files_written under --format json). Under --format json the document
    (hook_failures, structural_differences) is the whole report of a
    refused migration: nothing is written on stderr.
    Under human output, the W054s and then the failure's message are its
    report on stderr.
  """
  verify unit "restores migrated files from .bak backups"
  verify unit "missing .bak file produces warning and skips"
  verify unit "restore is atomic per file"
  verify unit "a rollback keeps a file's permissions and refuses a read-only one"
  verify unit "rollback failure for one file does not block others"
  verify unit "summary reports restored, skipped, and failed counts"
  verify unit "a rollback reports the files it restored as written"
  verify unit "a file edited since the migration is left as it is, its backup kept"
  verify unit "a rollback with no recorded migration restores nothing"
  verify contract "Rollback Failed Migration: migration rollback holds — migration_recorded, files_restored, edited_files_kept, rollback_event_emitted, backup_file_preservation"
}

behavior invoke_extension_migration_hooks "Invoke Extension Migration Hooks" {
  // multi_error_collection applies here because extension hook failures are
  // collected into the DiagnosticBag (not fail-fast) — each hook failure is
  // an independent error that must be reported alongside others.
  category   command
  invariants [
    multi_error_collection,
    migration_idempotency,
    wasm_sandbox_integrity,
    extension_isolation,
    extension_load_order_determinism,
    migration_cross_extension_stability,
  ]
  types      [MigrationResult, MigrationInput, WasmTrapInfo, ExtensionDeclaration]
  ports      [CompilerApi, WasmRuntime]
  consumes   [migration_complete]
  produces   [extension_migration_hooks_complete]
  requires {
    migration_complete "migration_complete event MUST have fired, confirming all .spec files have been migrated"
  }
  ensures {
    all_hooks_invoked   "Every installed extension's migration hook has been invoked"
    hooks_event_emitted "extension_migration_hooks_complete event emitted"
  }
  maintains {
    extension_isolation "A failing extension hook does not prevent invocation of remaining hooks"
  }
  contract   """
    When migrating spec files, the compiler MUST invoke each installed
    extension's migration hook — a Wasm export whose name the extension's
    `__handshake` response declares in its optional `migration_hook`
    field, which the host carries onto the extension's ExtensionDeclaration. The
    host calls the export through the protocol's JSON call convention.
    The `migration_hook` field is a string naming the Wasm export; if the
    field is absent or empty, no hook is invoked for that extension.
    Extension hooks are invoked after core format migration completes but
    before post-migration validation, and before any file is written. The
    hook's input is a JSON object `{"from", "to", "files"}`: the format
    version the project is migrated from and to, and the paths of the files
    the core migration will rewrite, so the hook knows which data to
    transform. The hooks are those of the
    project the path is in, so a migration started from a sub-path runs the
    same hooks.

    Migration hook lifecycle: core migration runs first (transforming .spec
    file syntax), then extension hooks run (transforming extension-specific
    data within those files). Post-migration validation
    (validate_post_migration_integrity) runs exactly once, after BOTH core
    migration and all extension hooks have completed and before any file is
    written. This ensures
    validation sees the fully-migrated state, not an intermediate state.

    If an extension does not declare a `migration_hook` field in its
    ExtensionDeclaration, or the field value is empty, the compiler MUST skip that
    extension silently with no diagnostic. The absence of a hook is normal
    and expected for extensions that have no version-sensitive data.

    If an extension's migration hook returns an error (non-zero return code)
    or traps, the compiler MUST collect the error into the DiagnosticBag
    (including WasmTrapInfo if a trap occurred) and continue invoking
    migration hooks for the remaining extensions. A single extension hook
    failure MUST NOT prevent other extensions from running their hooks.

    The compiler MUST invoke extension hooks in the registry build's load
    order (registry_build_load_order, extension_load_order_determinism). A
    cycle among required peers is the compile's E027, not a hook failure:
    the hooks still run. An extension that failed
    to load is not loaded, so it has no hook to invoke.

    Cross-extension reference stability: extension migration hooks MUST
    NOT rename or remove entities that are referenced by other extensions.
    If a hook does so, the post-migration validation phase will detect
    the broken cross-extension references and report them as diagnostics.

    Hook timeout: each extension migration hook has a configurable timeout
    (default 30s). If a hook exceeds the timeout, it MUST be treated as
    a trap — the compiler MUST terminate the hook execution, collect a
    WasmTrapInfo diagnostic, and continue with the next extension.
  """
  verify unit "a migration hook receives the from and to format versions and the migrated files"
  verify unit "the hooks run before any file is written"
  verify unit "a migration whose extension hook fails is refused and nothing is written"
  verify unit "extension with migration_hook field has it invoked during migrate"
  verify unit "extension without migration_hook field is skipped silently"
  verify unit "extension with empty migration_hook field is skipped silently"
  verify unit "hook returning error collects diagnostic and continues"
  verify unit "the nine builtin extensions' hooks run in dependency order with no failure"
  verify unit "hook that traps collects WasmTrapInfo and continues"
  verify unit "hooks invoked in deterministic extension load order"
  verify unit "hooks run for the project a sub-path is in"
  verify unit "extension in failed lifecycle state has hook skipped"
  verify unit "hook exceeding timeout treated as trap"
  verify unit "validation runs once after both core and extension hooks complete"
  verify contract "Invoke Extension Migration Hooks: extension migration hooks holds for the declared obligations"
}
