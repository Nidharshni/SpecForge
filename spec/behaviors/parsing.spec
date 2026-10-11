// Parsing behaviors — Tree-sitter grammar and AST construction

use "events/compilation"
use "invariants/core"
use "invariants/wasm"
use "invariants/zero-entity-core"
use "ports/outbound"
use "types/core"
use "types/errors"
use "types/wasm"

behavior parse_spec_file_to_ast "Parse Spec File to AST" {
  features   [spec_file_parsing]
  invariants [
    multi_error_collection,
    string_interning_consistency,
    zero_domain_knowledge_core,
    source_span_completeness,
  ]
  category   command
  types      [SpecFile, ParseError, SourceSpan]
  ports      [SourceParser]
  produces   [file_parsed, all_files_parsed]
  requires {
    source_parser_available "SourceParser port is initialized and ready to accept input"
    valid_utf8_input        "Input buffer is valid UTF-8"
  }
  ensures {
    ast_produced          "A complete AST is produced containing all declared entities with fields, source spans, and use imports"
    source_spans_complete "Every token in the AST has an accurate source location"
    file_parsed_emitted   "file_parsed event is emitted for each successfully parsed file"
  }
  contract   """
    As the first stage of the compiler pipeline, given a syntactically
    valid .spec file, the parser MUST produce an AST containing all
    declared entities with their fields, source spans, and use imports.
    The AST MUST preserve source locations for every token.
  """
  verify unit "parse valid file produces complete AST"
  verify unit "AST source spans match original token positions"
  verify contract "Parse Spec File to AST: spec file parsing holds — source_parser_available, valid_utf8_input, ast_produced, source_spans_complete, file_parsed_emitted"
}

// The following behaviors execute as part of parse_spec_file_to_ast
// and contribute to the file_parsed and all_files_parsed events.
// They do not produce events independently.

behavior recover_from_syntax_errors "Recover From Syntax Errors" {
  features   [error_recovery_during_parsing]
  invariants [multi_error_collection, zero_domain_knowledge_core, source_span_completeness]
  category   command
  types      [SpecFile, ParseError]
  ports      [SourceParser]
  contract   """
    When a .spec file contains syntax errors, the parser MUST recover
    and continue parsing subsequent blocks. The parser MUST collect all
    parse errors with source locations. Syntactically valid blocks
    after an error MUST still appear in the AST.
    A string left unclosed is ended before the next line that starts a
    top-level form of the grammar (an import, a spec, ref, define, union or
    entity block), as the lexer's form-start rule reads it; the text is read
    through the lexer (lex_spec_text), so a
    backslash before a line break leaves a string unclosed, as the grammar
    reads it. A body left unclosed ends, missing its closing brace, where
    the next line starts a top-level form indented no deeper than the form
    the body opened (the same form-start rule, read by the grammar's
    scanner), so the blocks after it are parsed.
  """
  requires {
    error_recovery_enabled "SourceParser is initialized with error-recovery mode enabled"
    valid_utf8_input       "Input buffer is valid UTF-8"
  }
  ensures {
    valid_blocks_preserved "All syntactically valid blocks following an error site appear in the AST"
    errors_collected       "ParseError list is populated with one entry per recovery point"
  }
  verify unit "parser collects multiple errors from one file"
  verify unit "valid blocks after syntax error are still parsed"
  verify unit "a closed multi-line string that contains a block-like line stays whole"
  verify unit "recovery resumes at every top-level form the grammar reads, a union block included"
  verify unit "a backslash before a line break leaves a string unclosed, as the grammar reads it"
  verify unit "the blocks after an unclosed body are parsed, and its missing closing brace is reported"
  verify unit "an unclosed body ends where the next top-level form starts, missing its closing brace"
  verify unit "an unclosed body ends at a form indented no deeper than the form it opened"
  verify unit "the grammar resumes at every top-level form of the repository's spec after an unclosed body"
  verify unit "completely invalid syntax produces error with location"
  verify unit "missing opening brace produces a parse error"
  verify contract "Recover From Syntax Errors: syntax error recovery holds — error_recovery_enabled, valid_utf8_input, valid_blocks_preserved, errors_collected"
}

behavior parse_use_imports "Parse Use Imports" {
  features   [spec_file_parsing]
  invariants [import_dag, zero_domain_knowledge_core, string_interning_consistency]
  category   command
  types      [SpecFile, ImportDeclaration, SourceSpan]
  ports      [SourceParser]
  requires {
    source_parser_available "SourceParser port is initialized and ready to accept input"
  }
  ensures {
    imports_extracted "All use directives are parsed into ImportDeclaration entries with paths and optional selective IDs"
  }
  contract   """
    The parser MUST recognize use directives at the top of .spec files.
    Both full imports (use path/to/file) and selective imports
    (use path/to/file { ID-1 }) MUST be parsed. The .spec extension is
    implicit; a path that spells it out resolves to the same file.
  """
  verify unit "parse full use import"
  verify unit "parse selective use import with braces"
  verify contract "Parse Use Imports: use import parsing holds — source_parser_available, imports_extracted"
}

behavior parse_all_block_types "Parse All Block Types" {
  features   [spec_file_parsing]
  invariants [
    multi_error_collection,
    zero_domain_knowledge_core,
    source_span_completeness,
    string_interning_consistency,
    cst_vocabulary_grammar_consistency,
  ]
  category   command
  types      [
    Entity,
    EntityKind,
    FieldMap,
    FieldEntry,
    FieldValue,
    StringValue,
    ReferenceList,
    StringList,
    Block,
    VerifyList,
    VerifyStatement,
    VerifyKind,
    SourceSpan,
  ]
  ports      [SourceParser]
  requires {
    source_parser_available "SourceParser port is initialized and ready to accept input"
  }
  ensures {
    generic_blocks_parsed     "All keyword blocks are parsed as generic entity_block AST nodes regardless of keyword"
    unknown_keywords_accepted "Unknown keywords are parsed without error — rejection deferred to semantic phase"
    raw_body_preserved        "Raw body text of each entity block is preserved verbatim before field parsing"
  }
  contract   """
    The parser MUST use a single generic entity_block rule that parses
    any keyword name [title] { fields } structure. Only spec, ref, use,
    and define have dedicated grammar rules due to unique structural
    syntax (ref uses scheme:identifier format). All other keywords MUST
    be parsed generically — the parser MUST NOT reject unknown keywords.
    Keyword validation happens in the semantic phase after extensions
    populate the KindRegistry. The parser MUST capture the raw body text
    of each entity block.
    Raw body text MUST be preserved verbatim before any field parsing
    occurs.
    A field named `kind` inside an entity body is an ordinary field of
    that entity, never the block's keyword.
  """
  verify unit "parse any keyword as generic entity_block"
  verify unit "spec block uses dedicated grammar rule"
  verify unit "ref block uses dedicated grammar rule"
  verify unit "define block uses dedicated grammar rule"
  verify unit "unknown keyword parsed without error"
  verify unit "optional and annotated parameters, nested arrays, unit and function types, and string-literal unions parse"
  verify unit "a type may declare a field named verify next to verify statements"
  verify unit "every spec in the repository parses without a syntax error"
  verify unit "a syntax error inside a method signature is reported"
  verify unit "any keyword produces generic entity_block AST node"
  verify unit "generic block preserves kind, name, title, and fields"
  verify unit "parse string field values correctly"
  verify unit "a type field named kind is an ordinary field"
  verify contract "Parse All Block Types: block type parsing holds — source_parser_available, generic_blocks_parsed, unknown_keywords_accepted, raw_body_preserved"
  verify unit "field annotations are extracted into FieldEntry"
  verify unit "homogeneous reference list is not MixedList"
  verify unit "homogeneous string list is not MixedList"
  verify unit "integer overflow produces parse error instead of silent 0"
  verify unit "mixed list with strings and integers preserves both"
  verify unit "mixed-type list preserves per-item types"
  verify unit "multiple annotations on a single field are all extracted"
  verify unit "negative integer parsed as field value"
  verify unit "negative integer parsed as field value with larger magnitude"
  verify unit "valid integer parses correctly"
}

behavior parse_triple_quoted_strings "Parse Triple-Quoted Strings" {
  features   [spec_file_parsing]
  invariants [multi_error_collection, string_interning_consistency, zero_domain_knowledge_core]
  category   command
  types      [SpecFile, StringValue]
  ports      [SourceParser]
  requires {
    source_parser_available "SourceParser port is initialized and ready to accept input"
  }
  ensures {
    newlines_preserved   "Internal newlines within triple-quoted strings are preserved"
    dedent_applied       "Common leading whitespace is stripped from all lines"
    relative_indent_kept "Relative indentation between lines is preserved after dedent"
  }
  contract   """
    The parser MUST handle triple-quoted strings (triple double-quotes).
    Leading whitespace common to all lines MUST be stripped (dedent).
    The content between delimiters MUST preserve internal newlines and
    relative indentation.
  """
  verify unit "triple-quoted string preserves newlines"
  verify unit "common leading whitespace is stripped"
  verify unit "relative indentation is preserved"
  verify unit "recover from unclosed triple-quoted string with diagnostic"
  verify contract "Parse Triple-Quoted Strings: triple-quoted string parsing holds — source_parser_available, newlines_preserved, dedent_applied, relative_indent_kept"
}

behavior provide_syntax_highlighting_queries "Provide Syntax Highlighting Queries" {
  features   [editor_query_files]
  // Query file behaviors describe static .scm artifacts shipped with the grammar — no runtime types or events needed
  category   query
  invariants [zero_domain_knowledge_core, query_file_grammar_consistency]
  contract   """
    The grammar MUST ship a highlights.scm query file that maps all
    node types to standard Tree-sitter capture names. Keywords MUST
    map to @keyword, strings to @string, entity IDs to @constant,
    types to @type, and comments to @comment. Generic entity blocks
    MUST be captured: the kind field as @keyword and the name field
    as @constant. The file MUST be loadable by any Tree-sitter-aware
    editor without an LSP server.
  """
  verify unit "highlights.scm captures all block keywords as @keyword"
  verify unit "highlights.scm captures strings and triple-quoted strings as @string"
  verify unit "highlights.scm captures entity IDs as @constant"
  verify unit "highlights.scm captures generic_entity_block kind as @keyword"
  verify integration "highlights.scm loads in Tree-sitter runtime and matches expected captures"
}

behavior provide_code_folding_queries "Provide Code Folding Queries" {
  features   [editor_query_files]
  invariants [zero_domain_knowledge_core, query_file_grammar_consistency]
  category   query
  contract   """
    The grammar MUST ship a folds.scm query file that marks all
    brace-delimited blocks as foldable regions. Every block type,
    sub-block, nested block, and generic_entity_block MUST be
    foldable. The file MUST be loadable by any Tree-sitter-aware
    editor without an LSP server.
  """
  verify unit "folds.scm marks generic entity_block as @fold"
  verify unit "folds.scm marks all brace-delimited sub-blocks within spec and define blocks as @fold"
  verify unit "folds.scm marks spec and define blocks as @fold"
  verify unit "folds.scm marks ref blocks as collapsible regions"
  verify integration "folds.scm loads in Tree-sitter runtime and produces expected fold regions"
}

// Phase 1 (parsing) treats all field values as raw strings; Phase 2 (semantic
// validation) applies type coercion rules. See types/core.spec for the
// canonical FieldValue type and coercion documentation.
behavior parse_verify_statements "Parse Verify Statements" {
  features   [spec_file_parsing]
  invariants [
    multi_error_collection,
    zero_domain_knowledge_core,
    source_span_completeness,
    string_interning_consistency,
  ]
  category   validation
  types      [SpecFile, VerifyList, VerifyStatement, VerifyKind, SourceSpan]
  ports      [SourceParser]
  requires {
    source_parser_available "SourceParser port is initialized and ready to accept input"
  }
  ensures {
    verify_statements_extracted  "Each verify statement is parsed into a VerifyStatement with kind and description fields"
    all_block_types_supported    "Verify statements are parsed in entity blocks, spec blocks, and define blocks"
    semantic_validation_deferred "Verify kind validation is deferred to Phase 2 — no kind rejection during parsing"
  }
  contract   """
    The core grammar MUST recognize verify statements with the syntax
    verify <kind> "<description>" within any entity block, spec block,
    and define block. Verify is a structural grammar construct — the
    parser MUST parse it in all block types without knowledge of which
    entity kinds support verify semantically. Semantic validation of
    whether the entity kind is testable and the verify kind is allowed
    is deferred to the semantic phase using the KindRegistry. Each
    verify statement MUST be parsed into a VerifyStatement entry with
    the kind and description fields.
    The verify kind token (e.g., unit, property) is parsed as a raw string in Phase 1. Validation against registered verify kinds occurs in Phase 2 semantic validation.
  """
  verify unit "parse verify statement in any entity block"
  verify unit "parse multiple verify statements in same entity"
  verify unit "verify parsed in spec block"
  verify unit "verify parsed in define block"
  verify unit "verify kind and description extracted correctly"
  verify contract "Parse Verify Statements: verify statement parsing holds — source_parser_available, verify_statements_extracted, all_block_types_supported, semantic_validation_deferred"
}

behavior parse_ref_blocks "Parse Ref Blocks" {
  features   [spec_file_parsing]
  invariants [
    multi_error_collection,
    zero_domain_knowledge_core,
    source_span_completeness,
    string_interning_consistency,
  ]
  category   command
  types      [SpecFile, ParseError, SourceSpan]
  ports      [SourceParser]
  // Ref components stored as FieldEntry in FieldMap — no dedicated ref type needed
  requires {
    source_parser_available "SourceParser port is initialized and ready to accept input"
  }
  ensures {
    ref_components_extracted "Scheme, kind, and identifier are extracted from compound ID format"
    both_forms_handled       "Both block and one-line ref syntax produce consistent AST nodes"
    malformed_refs_rejected  "Ref blocks with missing scheme or identifier are rejected with a diagnostic"
  }
  contract   """
    The core grammar MUST recognize ref blocks with the syntax
    ref <scheme>.<kind>:<identifier> [title] { fields } as a dedicated
    grammar rule. The parser MUST extract the scheme, kind, and identifier
    components from the compound ID format. Ref blocks also support the
    one-line syntax ref <scheme>.<kind>:<identifier> "title". The parser
    MUST handle both forms and produce consistent AST nodes.
  """
  verify unit "parse ref block with scheme.kind:identifier format"
  verify unit "parse one-line ref syntax"
  verify unit "ref block extracts scheme, kind, and identifier components"
  verify unit "ref block supports optional title and body fields"
  verify unit "reject ref block with missing scheme or identifier"
  verify contract "Parse Ref Blocks: ref block parsing holds — source_parser_available, ref_components_extracted, both_forms_handled, malformed_refs_rejected"
}

behavior lex_spec_text "Lex Spec Text" {
  features   [spec_file_parsing]
  invariants [zero_domain_knowledge_core, source_span_completeness]
  category   query
  types      [SourceSpan]
  ports      [SourceParser]
  requires {
    valid_utf8_input "Input buffer is valid UTF-8"
  }
  ensures {
    lexemes_match_grammar  "every identifier, scheme ref ID, number, string and comment the grammar reads is one lexeme with the same bytes"
    strings_as_the_grammar "a string spanning lines is one lexeme, as the grammar reads it; one the grammar would not close is marked unclosed"
    half_typed_text_lexes  "text the grammar rejects still lexes, an unclosed regular string ending at its line's end and an unclosed triple-quoted one before the next line that starts a top-level form"
    form_starts_as_grammar "a line starts a top-level form of the grammar exactly where the grammar reads one, whether its lexemes are read alone or in their document"
  }
  contract   """
    The lexer MUST read a .spec text, complete or half-typed, into the
    lexemes the grammar tokenizes: identifiers, scheme ref IDs (one lexeme),
    numbers, strings, comments and punctuation, without a parse.
    Navigation, the LSP, the parser's recovery from unclosed strings and
    the formatter read text through it and through no scanner of their own
    (ADR 0023, ADR 0038). A string is read as the grammar reads it, across
    lines; a regular string the grammar would not close (no closing quote,
    a backslash before a line break, or a closing quote that runs straight
    into text) ends at its line's end and is marked unclosed, and a
    triple-quoted string the grammar would not close (no closing quotes,
    or, spanning lines, closing quotes that run straight into text) ends
    before the next line
    that starts a top-level form, where the parser's recovery resumes, so an
    unclosed quote never swallows the rest of a document being typed.
    Where a top-level form starts (an import, a spec, ref, define, union or
    entity block) is read from lexemes by one rule: the parser's recovery
    ends an unclosed string before such a line and the LSP's document closes
    every body left open at one (ADR 0038 D6). The expression
    tokenizer of the prove pass (parse_expression) reads a sub-language with
    lexical rules of its own and is not built on the lexer; it MUST cut the
    expressions of the repository's spec into the same tokens (the
    two-character operators joined) and read an expr group as the grammar did.
  """
  verify unit "the lexer agrees with the grammar on every spec file of the repository"
  verify unit "the expression tokenizer, the lexer and the grammar agree on every expression of the repository's spec"
  verify unit "a scheme ref ID is one lexeme"
  verify unit "a string spanning lines is one lexeme, and one the grammar would not close ends at its line's end"
  verify unit "strings and comments are lexemes of their own and hold no others"
  verify unit "a line starts a top-level form exactly where the grammar reads one, on every spec file of the repository"
  verify unit "an unclosed triple-quoted string ends before the next line that starts a top-level form"
  verify unit "the grammar ends an unclosed body at exactly the lines the form-start rule reads as form starts"
}

behavior parse_define_blocks "Parse Define Blocks" {
  features   [spec_file_parsing]
  invariants [
    multi_error_collection,
    zero_domain_knowledge_core,
    source_span_completeness,
    string_interning_consistency,
  ]
  category   command
  types      [SpecFile, ParseError, SourceSpan, FieldMap, FieldEntry, FieldValue]
  ports      [SourceParser]
  requires {
    source_parser_available "SourceParser port is initialized and ready to accept input"
  }
  ensures {
    define_block_parsed             "Define blocks are parsed with name and body using standard field syntax"
    no_extension_knowledge_required "Define blocks are parsed as core grammar constructs without extension input"
  }
  contract   """
    The core grammar MUST recognize define blocks with the syntax
    define <name> { fields } as a dedicated grammar rule. Define blocks
    are not supported (the compiler reports each with W143, see
    report_define_blocks); they are parsed so that the report can name
    them. The parser MUST parse define blocks identically to other
    structural blocks — they are core grammar constructs, not extension-
    contributed. Define block fields MUST support the same field syntax
    as all other blocks.
  """
  verify unit "parse define block with name and body"
  verify unit "define block supports standard field syntax"
  verify unit "define block parsed without extension knowledge"
  verify contract "Parse Define Blocks: define block parsing holds — source_parser_available, define_block_parsed, no_extension_knowledge_required"
}

behavior provide_indentation_queries "Provide Indentation Queries" {
  features   [editor_query_files]
  invariants [zero_domain_knowledge_core, query_file_grammar_consistency]
  category   query
  contract   """
    The grammar MUST ship an indents.scm query file that provides
    automatic indentation for brace-delimited and bracket-delimited
    blocks. Opening braces and brackets MUST trigger @indent,
    closing braces and brackets MUST trigger @dedent.
  """
  verify unit "indents.scm indents after opening brace"
  verify unit "indents.scm dedents on closing brace"
  verify unit "indents.scm indents after opening bracket"
  verify unit "indents.scm dedents on closing bracket"
  verify integration "indents.scm loads in Tree-sitter runtime and produces expected indent/dedent"
}

// -- Extension Body Parsing ---------------------------------------------------

behavior extension_owned_body_syntax "Extension-Owned Body Syntax" {
  features   [extension_body_parsing]
  invariants [zero_domain_knowledge_core]
  category   validation
  types      [Entity, KindRegistryEntry, Diagnostic]
  requires {
    all_files_parsed_ready "All entity blocks have been parsed"
    kinds_registered       "The loaded extensions' entity kinds are registered, with their has_body_parser flags"
  }
  ensures {
    body_errors_suppressed "E001 parse errors inside an entity whose kind declares has_body_parser are not reported"
    other_errors_reported  "Every other E001 is reported unchanged"
  }
  contract   """
    An extension may declare that it owns an entity kind's body syntax
    (the kind's has_body_parser flag): software's type and port bodies
    hold field types and method signatures the core grammar does not
    parse. An E001 parse error that starts inside an entity of such a
    kind, in the same file, MUST NOT be reported, on a full build and
    on every incremental rebuild (which uses the file's current
    entities). The entity's other fields are parsed by the core field
    parser as usual; no extension code runs on the body. Extensions
    cannot contribute body parsers or grammars (ADR 0004 D5-a, ADR 0050).
  """
  verify integration "extension-owned body syntax does not surface E001 parse errors"
}
