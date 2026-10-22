// LSP-specific types for hover, completion, and document symbols

use "types/core"

type ContentChangeEvent "Content Change Event" {
  start_line integer
  start_col  integer
  end_line   integer
  end_col    integer
  text       string
  verify unit "ContentChangeEvent schema is valid"
}

type CompletionItem "Completion Item" {
  label       string @readonly
  detail      string @optional
  kind        string
  insert_text string @optional
  verify unit "CompletionItem schema is valid"
}

type HoverContent "Hover Content" {
  entity_kind              string
  title                    string
  source_extension         string
  testable                 boolean      @optional
  /// The statement the entity's extension declares headline and normative.
  summary                  string       @optional
  /// As specforge.inspect's coverage_status; every entity has one.
  coverage_status          string
  incoming_reference_count integer
  outgoing_reference_count integer
  /// The diagnostics about the entity not already shown for the cursor.
  diagnostics              Diagnostic[] @optional
  verify unit "HoverContent schema is valid"
}

type DocumentSymbolEntry "Document Symbol Entry" {
  name     string                @readonly
  kind     SymbolKind
  range    SourceSpan
  children DocumentSymbolEntry[] @optional
  verify unit "DocumentSymbolEntry schema is valid"
}

type SemanticTokenLegendEntry "Semantic Token Legend Entry" {
  token_type       string   @readonly
  token_modifiers  string[] @optional
  source_extension string   @optional
  verify unit "SemanticTokenLegendEntry schema is valid"
}

type WorkspaceEditResult "Workspace Edit Result" {
  edits          TextEdit[]
  document_count integer
  verify unit "WorkspaceEditResult schema is valid"
}

type CodeActionKind "Code Action Kind" = QuickFix | Refactor | Source | SourceOrganizeImports

type CodeAction "Code Action" {
  title           string  @readonly
  kind            CodeActionKind
  diagnostic_code string  @optional
  edits           TextEdit[]
  is_preferred    boolean @optional
  verify unit "CodeAction schema is valid"
}

type SemanticToken "Semantic Token" {
  line       integer  @readonly
  start_col  integer  @readonly
  length     integer  @readonly
  token_type string   @readonly
  modifiers  string[] @optional
  verify unit "SemanticToken schema is valid"
}

type WorkspaceSymbolEntry "Workspace Symbol Entry" {
  name      string @readonly
  kind      SymbolKind
  location  SourceSpan
  container string @optional
  verify unit "WorkspaceSymbolEntry schema is valid"
}

type SymbolKind "Symbol Kind" = file
  | module
  | namespace
  | class
  | method
  | property
  | field
  | constructor
  | enum
  | interface
  | function
  | variable
  | constant
  | string
  | number
  | boolean
  | array
  | object
  | key
  | enum_member
  | struct
  | event
  | operator
  | type_parameter
