// Formatting behaviors — the code formatter pipeline

use "events/compilation"
use "invariants/core"
use "invariants/formatting"
use "ports/inbound"
use "ports/outbound"
use "types/config"
use "types/core"
use "types/formatting"

behavior format_spec_files "Format Spec Files" {
  features   [code_formatting]
  invariants [
    formatting_idempotency,
    formatting_consistency,
    comment_preservation,
    format_rule_determinism,
    formatting_semantic_preservation,
  ]
  category   query
  types      [FormatConfig, FormatDiff]
  ports      [FileSystem, CompilerApi]
  produces   [format_complete]
  requires {
    spec_files_available "target .spec files exist on disk and are readable"
    format_config_loaded "FormatConfig has been resolved (from .specforgefmt.toml or defaults)"
  }
  ensures {
    formatted_output_written  "formatted output is written back to disk for files that changed"
    unchanged_files_preserved "files already in canonical format are not rewritten"
    format_complete_emitted   "format_complete event is produced after successful formatting"
    summary_printed           "names of changed files and a summary count are printed"
    exit_code_reported        "exit code is 1 when a file cannot be read or written, or has a region left unformatted; 0 otherwise"
  }
  contract   """
    When specforge format is invoked with file paths or a project directory,
    the system MUST parse each .spec file into a CST using tree-sitter,
    apply formatting rules, and write the formatted output back to disk.
    Files that are already correctly formatted MUST NOT be rewritten.
    The command MUST print the names of changed files and a summary count.
    A file that cannot be read or written, or that has a region left
    unformatted (format_with_parse_errors), MUST make the command exit with
    code 1; every other file is still formatted and written.
  """
  verify unit "files matching the canonical format are not rewritten"
  verify unit "changed files are printed to stdout"
  verify unit "summary count reflects actual changes"
  verify integration "formatting all files in spec/ directory succeeds"
  verify unit "a region left unformatted makes the run exit 1, the rest of the file still written"
  verify contract "Format Spec Files: spec file formatting holds — spec_files_available, format_config_loaded, formatted_output_written, unchanged_files_preserved, format_complete_emitted, summary_printed"
}

behavior preserve_comments "Preserve Comments During Formatting" {
  features   [code_formatting]
  invariants [comment_preservation]
  category   command
  types      [FormatConfig]
  requires {
    cst_available "the .spec file has been parsed into a CST with comment nodes present"
  }
  ensures {
    all_comments_attached "every comment is attached to the correct CST node per the attachment algorithm"
    no_comments_lost      "no comments are dropped or reordered during formatting"
  }
  contract   """
    The formatter MUST attach every comment to the correct CST node using
    the comment attachment algorithm: leading comments attach to the following
    node, trailing comments attach to the preceding node on the same line,
    section header comments attach to the next block group, and standalone
    comment blocks separated by blank lines are preserved as-is. Comments
    inside blocks stay between the statements they sat between, in every
    block kind (spec, define, ref and entity blocks). A comment's text is
    kept as written — `///`, `//!` and indentation after `//` included —
    except that `//text` gains one space.
    A statement the formatter rebuilds on one line (an import, an inline
    ref, a union block, a field, a verify statement, a method) that holds a
    comment between its tokens is kept as written instead, its first line
    at the body's indentation, so no comment is dropped and none swallows
    the code after it.
  """
  verify unit "leading comment attaches to following node"
  verify unit "trailing comment attaches to preceding node on same line"
  verify unit "section header comment attaches to next block group"
  verify unit "standalone comment block between blocks is preserved"
  verify unit "a statement holding a comment between its tokens is kept as written"
  verify property "no comments are lost after formatting"
  verify contract "Preserve Comments During Formatting: comment preservation holds — cst_available, all_comments_attached, no_comments_lost"
}

behavior check_formatting "Check Formatting Without Modifying Files" {
  features   [code_formatting]
  // Dry-run mode: does not emit format_complete (no files modified)
  category   validation
  invariants [
    formatting_idempotency,
    formatting_consistency,
    comment_preservation,
    format_rule_determinism,
    dry_run_side_effect_freedom,
    formatting_semantic_preservation,
  ]
  types      [FormatConfig, FormatDiff]
  ports      [FileSystem]
  requires {
    spec_files_available "target .spec files exist on disk and are readable"
    format_config_loaded "FormatConfig has been resolved (from .specforgefmt.toml or defaults)"
  }
  ensures {
    no_files_written          "no files are written to disk in check mode"
    exit_code_correct         "exit code is 0 when every file is read and in canonical form; 1 when any would change, cannot be read, or has a region left unformatted"
    unformatted_paths_printed "file paths of unformatted files are printed to stdout"
  }
  contract   """
    When specforge format --check is invoked, the system MUST compare
    what would be formatted against existing files on disk. If any file
    would change, the command MUST exit with code 1 and print the file
    paths. The system MUST NOT write any files in check mode. A file that
    cannot be read, or that has a region left unformatted
    (format_with_parse_errors), MUST also make the check exit with code 1.
  """
  verify unit "already formatted files exit with code 0"
  verify unit "unformatted files exit with code 1"
  verify unit "check mode writes no files to disk"
  verify unit "a file that cannot be read makes the check fail"
  verify unit "a region left unformatted makes the check fail"
  verify contract "Check Formatting Without Modifying Files: formatting check holds — spec_files_available, format_config_loaded, no_files_written, exit_code_correct, unformatted_paths_printed"
}

behavior show_formatting_diff "Show Formatting Diff" {
  features   [code_formatting]
  // Dry-run mode: does not emit format_complete (no files modified)
  category   query
  invariants [
    formatting_idempotency,
    formatting_consistency,
    comment_preservation,
    format_rule_determinism,
    dry_run_side_effect_freedom,
    formatting_semantic_preservation,
  ]
  types      [FormatDiff]
  ports      [FileSystem]
  requires {
    spec_files_available "target .spec files exist on disk and are readable"
    format_config_loaded "FormatConfig has been resolved (from .specforgefmt.toml or defaults)"
  }
  ensures {
    no_files_written      "no files are written to disk in diff mode"
    unified_diff_produced "a unified diff with --- and +++ headers is produced for each file that would change"
  }
  contract   """
    When specforge format --diff is invoked, the system MUST produce
    a unified diff of formatting changes for each file that would be
    modified. The diff MUST use standard unified format with --- and +++
    headers. The system MUST NOT write any files in diff mode.
  """
  verify unit "diff output uses unified format"
  verify unit "diff mode writes no files to disk"
  verify unit "unchanged files produce no diff output"
  verify unit "an unchanged line is shown as context, never as removed and added"
  verify contract "Show Formatting Diff: formatting diff holds — spec_files_available, format_config_loaded, no_files_written, unified_diff_produced"
}

behavior format_from_stdin "Format from Standard Input" {
  features   [code_formatting]
  invariants [
    formatting_idempotency,
    formatting_consistency,
    comment_preservation,
    format_rule_determinism,
    formatting_semantic_preservation,
  ]
  category   query
  types      [FormatConfig]
  produces   [format_complete]
  requires {
    stdin_available      "standard input contains valid .spec content"
    format_config_loaded "FormatConfig has been resolved (from .specforgefmt.toml or defaults)"
  }
  ensures {
    stdout_produced         "formatted output is written to standard output"
    no_files_touched        "no files are read from or written to disk"
    format_complete_emitted "format_complete event is produced after successful formatting"
  }
  contract   """
    When specforge format --stdin is invoked, the system MUST read
    .spec content from standard input, format it, and write the
    formatted output to standard output. This enables editor integrations
    that pipe buffer contents through the formatter. The same formatting
    guarantees (idempotency, consistency, comment preservation) apply as
    for file-based formatting. In stdin mode, the format_complete event
    MUST set filesChecked=1 and filesChanged to 0 (input already canonical)
    or 1 (formatting applied). When the input has a region left
    unformatted, the formatted text MUST still be printed and the command
    MUST exit with code 1.
  """
  verify unit "stdin content is formatted and written to stdout"
  verify unit "stdin with a region left unformatted prints the formatted text and exits 1"
  verify unit "stdin mode does not read or write files"
  verify property "stdin formatting is idempotent"
  verify property "stdin formatting converges to canonical form"
  verify contract "Format from Standard Input: stdin formatting holds — stdin_available, format_config_loaded, stdout_produced, no_files_touched, format_complete_emitted"
}

behavior load_format_config "Load Format Configuration" {
  features   [code_formatting]
  invariants [config_defaults_valid]
  category   query
  types      [FormatConfig]
  ports      [FileSystem]
  requires {
    project_root_available "project root directory (containing specforge.json) is identifiable"
    filesystem_accessible  "FileSystem port is available for reading configuration files"
  }
  ensures {
    config_resolved          "FormatConfig is resolved from .specforgefmt.toml or defaults"
    walk_bounded             "configuration discovery does not continue beyond the project root"
    invalid_values_diagnosed "invalid configuration values produce diagnostics and fall back to defaults"
  }
  contract   """
    The formatter MUST discover configuration by walking up from the
    formatted file's directory toward the project root (the directory
    containing specforge.json). The walk MUST stop at the project root
    and MUST NOT continue beyond it. Configuration files outside the
    project root MUST NOT be discovered. If .specforgefmt.toml is found,
    it MUST be parsed and validated. Invalid values MUST produce
    diagnostics and fall back to defaults. If no config file is found
    within the project root boundary, defaults MUST be used. The walk
    stops at the file's own project root, the nearest directory holding
    specforge.json, whichever project the run started in.
  """
  verify unit "config file in project root is loaded"
  verify unit "config file in parent directory is discovered"
  verify unit "config discovery walks from formatted file directory up to specforge.json parent then stops"
  verify unit "config outside project root is not discovered"
  verify unit "invalid indent_width produces diagnostic and uses default"
  verify unit "missing config file uses defaults"
  verify unit "a file's configuration does not depend on where format runs"
  verify unit "a file is formatted with the configuration of its own project"
  verify contract "Load Format Configuration: format config loading holds — project_root_available, filesystem_accessible, config_resolved, walk_bounded, invalid_values_diagnosed"
}

behavior apply_format_rules "Apply Format Rules" {
  features   [code_formatting]
  // Extension format rules are discovered via the contribution registry at format time
  category   query
  invariants [
    formatting_idempotency,
    formatting_consistency,
    comment_preservation,
    format_rule_determinism,
    format_rule_priority,
    formatting_semantic_preservation,
    cst_vocabulary_grammar_consistency,
  ]
  types      [
    FormatConfig,
    FormatRule,
    IndentRule,
    SpacingRule,
    AlignmentRule,
    WrappingRule,
    NewlineRule,
    CommentRule,
    ImportRule,
    StringRule,
    ExtensionFormatRule,
  ]
  ports      [WasmRuntime]
  requires {
    cst_available                   "the .spec file has been parsed into a CST for rule engine traversal"
    format_config_loaded            "FormatConfig has been resolved with indent style and width settings"
    contribution_registry_available "in-memory contribution registry is populated with extension format rules"
  }
  ensures {
    deterministic_output    "all rules produce deterministic output for the same input and configuration"
    no_domain_logic         "rules operate on generic keyword blocks and fields with no extension-specific entity kind logic"
    extension_rules_applied "extension-contributed format rules are executed at priority level 9 via WasmRuntime"
  }
  contract   """
    The formatting rule engine MUST walk the CST and emit formatting
    decisions for each whitespace region: keep, replace, insert, or remove.
    Rules cover indentation, spacing, alignment, wrapping, blank lines,
    comments and imports. String literals, triple-quoted ones included, are
    kept byte-for-byte: their text is a field's value. Statements keep their source
    order; only runs of imports are sorted. Field keys align to the longest
    key plus one; annotations of single-line values align in one column;
    `verify [kind] "..."` statements are single-spaced. A list that does
    not fit wraps one item per line, splitting only between items. All rules MUST produce
    deterministic output for the same input and configuration. Rules
    operate on generic keyword blocks and fields — they MUST NOT contain
    logic specific to any extension-defined entity kind.

    Extension-contributed format rules MUST be discovered from the
    in-memory contribution registry (populated during extension loading)
    and executed via the WasmRuntime port. Extension rules run at priority
    level 9 (after all 8 core rules). When multiple extensions contribute
    format rules, they MUST be applied in extension load order.
  """
  verify unit "indentation rules normalize to configured indent style"
  verify unit "spacing rules normalize single spaces between tokens"
  verify unit "alignment rules align field values within blocks"
  verify unit "statements keep their source order"
  verify unit "a union that does not fit wraps one variant per line"
  verify unit "wrapping rules break long reference lists to multi-line"
  verify unit "import sorting produces alphabetical order"
  verify unit "blank line rules enforce exactly one between blocks"
  verify unit "comment rules normalize spacing around inline comments"
  verify unit "multiline string literals are kept byte-for-byte"
  verify property "two files differing only in whitespace produce identical output after formatting"
  verify contract "Apply Format Rules: format rule application holds — cst_available, format_config_loaded, contribution_registry_available, deterministic_output, no_domain_logic, extension_rules_applied"
}

behavior maintain_format_idempotency "Maintain Format Idempotency" {
  features   [code_formatting]
  invariants [formatting_idempotency, formatting_semantic_preservation]
  category   query
  types      [FormatConfig]
  requires {
    format_rules_available "all format rules (core and extension) are loaded and ready"
  }
  ensures {
    idempotency_holds "format(format(x)) == format(x) for all valid inputs"
    no_oscillation    "alignment and wrapping decisions are stable across consecutive runs"
  }
  contract   """
    The formatter MUST satisfy the idempotency property: applying the
    formatter twice produces the same output as applying it once.
    This MUST be verified by property-based tests that generate random
    valid .spec files and check format(format(x)) == format(x).
    Any idempotency violation is treated as a P0 bug.
  """
  verify property "format(format(x)) == format(x) for random valid inputs"
  verify unit "alignment rules do not oscillate between runs"
  verify unit "wrapping decisions are stable across runs"
  verify contract "Maintain Format Idempotency: format idempotency holds — format_rules_available, idempotency_holds, no_oscillation"
}

behavior lsp_format_document "LSP Format Document" {
  features   [lsp_formatting]
  invariants [
    formatting_idempotency,
    formatting_consistency,
    comment_preservation,
    format_rule_determinism,
    formatting_semantic_preservation,
  ]
  category   query
  types      [FormatConfig, TextEdit]
  refs       [format_with_parse_errors]
  ports      [LspProtocol]
  produces   [format_complete]
  requires {
    document_open        "the document is registered in the LSP open document set"
    format_config_loaded "FormatConfig has been resolved for the document's project"
  }
  ensures {
    textedit_list_returned  "a list of non-overlapping TextEdit operations is returned"
    cli_parity_enforced     "the result is identical to running specforge format on the same file"
    format_complete_emitted "format_complete event is produced after successful formatting"
  }
  contract   """
    When the LSP server receives a textDocument/formatting request,
    it MUST format the full document using the formatting engine and
    return a list of TextEdit operations. The result MUST be identical
    to running specforge format on the same file.
    The configuration is the one specforge format uses for that file: the
    .specforgefmt.toml nearest the document's directory within its project,
    else the defaults (see lsp_respect_editor_config). TextEdit coordinates
    use 0-indexed lines and columns (LSP standard). TextEdit operations
    in a response MUST NOT overlap. When the document contains parse
    errors, the server MUST format well-formed regions and leave error
    regions unchanged, consistent with format_with_parse_errors.
    Diagnostics the formatter reports MUST be published alongside the
    document's compile diagnostics, never in place of them.
  """
  verify unit "formatting request returns TextEdit list"
  verify integration "formatting keeps the document's compile diagnostics published"
  verify unit "TextEdit coordinates are 0-indexed lines and columns"
  verify unit "TextEdit operations in a response do not overlap"
  verify integration "LSP format produces same result as CLI format"
  verify integration "parse errors in document trigger format_with_parse_errors delegation"
  verify performance "formats document within 50ms for files under 1000 lines"
  verify contract "LSP Format Document: LSP document formatting holds — document_open, format_config_loaded, textedit_list_returned, cli_parity_enforced, format_complete_emitted"
}

behavior lsp_format_range "LSP Format Range" {
  features   [lsp_formatting]
  invariants [
    formatting_idempotency,
    formatting_consistency,
    comment_preservation,
    format_rule_determinism,
    formatting_semantic_preservation,
  ]
  category   query
  types      [FormatConfig, TextEdit]
  ports      [LspProtocol]
  produces   [format_complete]
  requires {
    document_open        "the document is registered in the LSP open document set"
    format_config_loaded "FormatConfig has been resolved for the document's project"
  }
  ensures {
    range_expanded          "the range is expanded to complete block boundaries before formatting"
    textedit_list_returned  "a list of non-overlapping TextEdit operations for the affected region is returned"
    full_format_parity      "the formatted range produces the same result as full-document formatting for affected blocks"
    format_complete_emitted "format_complete event is produced after successful formatting"
  }
  contract   """
    When the LSP server receives a textDocument/rangeFormatting request,
    it MUST expand the range to complete block boundaries, format the
    expanded range, and return TextEdit operations for the affected region.
    TextEdit coordinates use 0-indexed lines and columns (LSP standard).
    TextEdit operations in a response MUST NOT overlap. The formatted
    range MUST produce the same result as full-document formatting for
    the affected blocks. When the selected range contains parse errors,
    error regions within the range MUST be left unchanged.
  """
  verify unit "range is expanded to block boundaries"
  verify unit "range formatting matches full formatting for affected blocks"
  verify integration "parse errors within range are left unchanged per format_with_parse_errors"
  verify unit "a region left unformatted is reported at its document lines"
  verify performance "formats range within 20ms for ranges under 200 lines"
  verify contract "LSP Format Range: LSP range formatting holds — document_open, format_config_loaded, range_expanded, textedit_list_returned, full_format_parity, format_complete_emitted"
}

behavior lsp_respect_editor_config "LSP Respect Editor Config" {
  features   [lsp_formatting]
  invariants [config_defaults_valid, format_rule_determinism]
  category   command
  types      [FormatConfig]
  ports      [LspProtocol]
  requires {
    lsp_initialized_fired "LSP server has been initialized and editor settings are available"
  }
  ensures {
    config_precedence_enforced "inside a project, the project's format configuration (its .specforgefmt.toml, else the defaults) is used and editor settings are ignored"
    editor_fallback_applied    "outside any project, editor-level tab size and insert-spaces are used"
  }
  contract   """
    Inside a project (an ancestor directory holds specforge.json), LSP
    formatting MUST use the configuration specforge format uses for the
    file: the nearest .specforgefmt.toml, else the defaults. Editor settings
    MUST NOT change it, so a file the editor formats passes
    specforge format --check. Outside any project (an unsaved buffer, a file
    with no specforge.json above it), LSP formatting MUST use the editor's
    tab size and insert-spaces. When it ignores editor settings that differ
    from the project's configuration, the server MUST say so once per
    session and configuration, as a log message naming the configuration
    it used.
  """
  verify unit "editor settings are used for a document outside any project"
  verify unit "config file takes precedence over editor settings"
  verify unit "a project without a config file formats with the defaults, not the editor's settings"
  verify integration "the editor formats a project file as specforge format --check expects"
  verify integration "the editor is told once when the project's configuration overrides its settings"
  verify contract "LSP Respect Editor Config: editor config respect holds — lsp_initialized_fired, config_precedence_enforced, editor_fallback_applied"
}

behavior format_with_parse_errors "Format Files with Parse Errors" {
  features   [code_formatting]
  // formatting_consistency applies to well-formed regions only; error regions
  // are preserved verbatim and do not participate in consistency checks.
  category   query
  invariants [
    comment_preservation,
    formatting_idempotency,
    formatting_consistency,
    format_rule_determinism,
    formatting_semantic_preservation,
  ]
  types      [FormatConfig, FormatDiff]
  ports      [FileSystem, LspProtocol]
  requires {
    cst_with_errors "the .spec file has been parsed into a CST that contains tree-sitter ERROR or MISSING nodes"
  }
  ensures {
    no_crash                      "the formatter does not crash or produce corrupted output"
    well_formed_regions_formatted "well-formed regions of the file are formatted normally"
    error_regions_preserved       "error regions are preserved verbatim with original whitespace byte-for-byte"
    parse_error_diagnosed         "a diagnostic is emitted listing each file with parse errors and error line ranges"
  }
  contract   """
    When the formatter encounters a .spec file that contains parse errors,
    it MUST NOT crash or produce corrupted output. The formatter MUST use
    tree-sitter error recovery to format well-formed regions of the file
    and leave error regions unchanged. An error region begins at the first
    unparseable token (as identified by a tree-sitter ERROR or MISSING node)
    and extends forward until the next token that begins a parseable
    top-level statement (use directive, entity block, or comment). All
    original whitespace within an error region MUST be preserved byte-for-byte.
    A diagnostic MUST be emitted listing each file that could not be fully
    formatted due to parse errors, including the line range of each error region.
    The diagnostic MUST span the region it kept, from its first line's start
    to its last line's end.
  """
  verify unit "file with syntax error is partially formatted without crash"
  verify unit "well-formed blocks in a file with errors are still formatted"
  verify unit "error regions are preserved verbatim in output"
  verify unit "error region starts at first unparseable token"
  verify unit "error region ends before next parseable top-level statement"
  verify unit "whitespace within error regions is preserved byte-for-byte"
  verify unit "diagnostic lists files with parse errors and error line ranges"
  verify contract "Format Files with Parse Errors: formatting with parse errors holds — cst_with_errors, no_crash, well_formed_regions_formatted, error_regions_preserved, parse_error_diagnosed"
}

behavior discover_format_targets "Discover Format Targets" {
  features   [code_formatting]
  invariants [discover_completeness]
  category   query
  types      [FormatConfig, CompilerConfig]
  ports      [FileSystem]
  requires {
    project_root_available "project root directory (containing specforge.json) is identifiable for spec_root resolution"
    filesystem_accessible  "FileSystem port is available for directory traversal"
  }
  ensures {
    all_spec_files_discovered "all .spec files under spec_root are discovered when no explicit paths are given"
    exclusions_applied        "files the project's exclude entries leave out are not discovered"
    non_spec_skipped          "non-.spec files are skipped without error"
  }
  contract   """
    When specforge format is invoked without explicit file paths, the
    system MUST discover all .spec files under the spec_root defined in
    specforge.json (the project root when unset): the files a compile
    reads. Files the project's exclude entries leave out MUST NOT be
    discovered; a file named explicitly is formatted all the same. A
    directory named explicitly that holds specforge.json MUST be read as
    that project's sources; walking any other named directory MUST take a
    nested project's sources where it reaches that project's
    specforge.json, never the rest of its files. When explicit paths are
    provided, only those paths MUST be formatted. Directories provided as
    arguments MUST be recursively searched for .spec files.
    A directory is walked as discovery walks (no skipped directory
    below it, no symlink, files only). Non-.spec
    files MUST be skipped without error.
  """
  verify unit "no arguments formats all .spec files under spec_root"
  verify unit "files the project's exclude entries leave out are not formatted"
  verify unit "a named directory that is a project formats that project's sources"
  verify unit "explicit file paths format only those files"
  verify unit "directory argument recursively discovers .spec files"
  verify unit "non-.spec files are skipped with no error"
  verify unit "a directory named like a .spec file is no target, walked or discovered"
  verify contract "Discover Format Targets: format target discovery holds — project_root_available, filesystem_accessible, all_spec_files_discovered, exclusions_applied, non_spec_skipped"
}
