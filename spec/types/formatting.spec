// Formatting types — data shapes for the code formatter

type FormatConfig "Format Config" {
  /// Default: 2
  indent_width    integer
  /// Default: false
  use_tabs        boolean
  /// Default: 100
  max_width       integer
  extension_rules JsonValue @optional
  verify unit "FormatConfig schema is valid"
}

type FormatRule "Format Rule" = IndentRule
  | SpacingRule
  | AlignmentRule
  | WrappingRule
  | NewlineRule
  | CommentRule
  | ImportRule
  | StringRule
  | ExtensionFormatRule

type IndentRule "Indent Rule" {
  scope string
  style string
  verify unit "IndentRule schema is valid"
}

type SpacingRule "Spacing Rule" {
  context string
  spaces  integer
  verify unit "SpacingRule schema is valid"
}

type AlignmentRule "Alignment Rule" {
  target string
  column integer @optional
  verify unit "AlignmentRule schema is valid"
}

type WrappingRule "Wrapping Rule" {
  max_width  integer
  wrap_style string
  verify unit "WrappingRule schema is valid"
}

type NewlineRule "Newline Rule" {
  context string
  count   integer
  verify unit "NewlineRule schema is valid"
}

type CommentRule "Comment Rule" {
  style           string
  preserve_inline boolean
  verify unit "CommentRule schema is valid"
}

type ImportRule "Import Rule" {
  sort_order string
  grouping   string @optional
  verify unit "ImportRule schema is valid"
}

type StringRule "String Rule" {
  quote_style      string
  multiline_string boolean @optional
  verify unit "StringRule schema is valid"
}

type ExtensionFormatRule "Extensions can register custom formatting rules via their manifest. The core formatter delegates to extension-provided rules for entity-specific formatting concerns (e.g., field ordering within extension-defined blocks). This is the P7 extension point: core formatting handles generic CST structure; ExtensionFormatRule lets extensions customize entity-specific formatting without modifying the core formatter. See features/formatting.spec P7 comment." {
  extension_id string
  rule_name    string
  config       JsonValue @optional
  verify unit "ExtensionFormatRule schema is valid"
}

type FormatDiff "Per-file formatting diff with change statistics" {
  /// The file, named from the project root (ADR 0102).
  file_path  string @readonly
  before     string
  after      string
  insertions integer
  deletions  integer
  verify unit "Per-file formatting diff with change statistics conforms to schema"
}
