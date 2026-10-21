// Core domain types — fundamental data shapes

type SpecFile "Spec File" {
  path     string       @readonly
  imports  ImportDeclaration[]
  entities Entity[]
  errors   ParseError[] @optional
  verify unit "SpecFile schema is valid"
}

type Entity "Entity" {
  id          EntityId   @readonly @unique
  kind        EntityKind
  // title is a grammar-level positional element (the string after keyword + id),
  // NOT a FieldRegistry entry. It is parsed by the generic_entity_block rule
  // and stored here for display, but does not participate in field validation.
  title       string
  fields      FieldMap
  obligations VerifyStatement[]
  source_span SourceSpan @readonly
  // name_span is where the parse read the name (a spec block's string, quotes included)
  name_span   SourceSpan @readonly
  verify unit "Entity schema is valid"
}

type EntityId "EntityId uniqueness is on the raw string alone — two entities with different kinds but the same raw ID are forbidden. See entity_id_uniqueness invariant in invariants/core.spec." {
  kind EntityKind @readonly
  raw  string     @readonly @unique
  verify unit "EntityId schema is valid"
}

type EntityKind "EntityKind is an open string — not a closed enum. All domain keywords come from extensions via KindRegistry. Grammar-level constructs (use, define) are not entity kinds. Structural node types (parsed by dedicated grammar rules, not KindRegistry): - spec: singleton project root, creates a graph node - ref: external reference, creates a graph node for unreferenced detection (W012) These are NOT in the KindRegistry — the KindRegistry is exclusively for extension-defined domain entity kinds. Structural node types have hardcoded parsing and validation in core. See zero-entity-core architecture." {
  raw string @readonly
  verify unit "EntityKind schema is valid"
}

type FieldMap "Field Map" {
  entries FieldEntry[]
  verify unit "FieldMap schema is valid"
}

type FieldEntry "Field Entry" {
  key   string
  value FieldValue
  verify unit "FieldEntry schema is valid"
}

type FieldValue "Note: integer, bool, and enum field values are parsed as StringValue by the generic tree-sitter grammar. Type coercion and validation happen in the semantic phase using FieldRegistry type declarations." = StringValue
  | ListValue
  | Block

type StringValue "String Value" {
  _tag    "StringValue" @literal
  content string
  verify unit "StringValue schema is valid"
}

type ListValue "A list as written (ADR 0093): what it references is its field's declared type, never the list's own shape." {
  _tag  "ListValue" @literal
  items ListItem[]
  verify unit "ListValue schema is valid"
}

type ListItem "List Item" {
  text    string
  // bare, scheme_ref, quoted or literal
  written string
  verify unit "ListItem schema is valid"
}

type Block "Block" {
  _tag    "Block" @literal
  entries FieldEntry[]
  verify unit "Block schema is valid"
}

type VerifyStatement "Verify Statement" {
  kind        VerifyKind
  description string
  verify unit "VerifyStatement schema is valid"
}

type VerifyKind "VerifyKind is an open string — not a closed enum. Extensions declare allowedVerifyKinds per entity kind in their manifest. The core compiler does not define any built-in verify kinds." {
  raw string @readonly
  verify unit "VerifyKind schema is valid"
}

type JsonValue "Opaque JSON value for Wasm host function I/O. Serialized as serde_json::Value in Rust." {
  _tag "JsonValue" @literal
  raw  string
  verify unit "JsonValue schema is valid"
}

type SourceSpan "Source Span" {
  file       string  @readonly
  start_line integer @readonly
  start_col  integer @readonly
  end_line   integer @readonly
  end_col    integer @readonly
  verify unit "SourceSpan schema is valid"
}

type TextEdit "Text Edit" {
  file_path string @readonly
  range     SourceSpan
  new_text  string
  verify unit "TextEdit schema is valid"
}

type ImportDeclaration "Import Declaration" {
  path         string
  selected_ids EntityId[] @optional
  verify unit "ImportDeclaration schema is valid"
}

type timestamp "Timestamp" = string

type JsonObject "Json Object" = string // Serialized JSON object; used for MCP arguments
