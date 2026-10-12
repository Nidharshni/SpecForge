use crate::ast::*;
use crate::cst::{Cst, CstError, UnclosedString};
use crate::expr::{CmpOp, Expr, ExprSpan, SpannedExpr};
use crate::format_version::{CURRENT_FORMAT_VERSION, detect_format_version};
use specforge_common::{SourceSpan, Sym, structural};
use tree_sitter::Node;
use tree_sitter_specforge::{field, kind};

/// Process escape sequences in a string literal (after quote stripping).
fn unescape(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('r') => result.push('\r'),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }
    result
}

pub fn parse(source: &str, file_path: &str) -> SpecFile {
    parse_incremental(source, file_path, None).0
}

/// Parse with an optional old tree for incremental reparsing.
/// Returns the parsed `SpecFile` and the new `tree_sitter::Tree` for reuse.
pub fn parse_incremental(
    source: &str,
    file_path: &str,
    old_tree: Option<&tree_sitter::Tree>,
) -> (SpecFile, Option<tree_sitter::Tree>) {
    let (mut file, tree) = parse_syntax(source, file_path, old_tree);
    // The header is read with the file, so every consumer of a parse (check,
    // watch, the LSP, MCP) has the version and what it reports.
    (file.format_version, file.format_diagnostics) = detect_format_version(source, file_path);
    (file, tree)
}

/// The file's entities, imports and syntax errors.
fn parse_syntax(
    source: &str,
    file_path: &str,
    old_tree: Option<&tree_sitter::Tree>,
) -> (SpecFile, Option<tree_sitter::Tree>) {
    let file_sym = Sym::new(file_path);
    let cst = match Cst::read(source, old_tree) {
        Ok(cst) => cst,
        Err(CstError::Language(e)) => {
            return (
                unparsed(file_sym, format!("failed to load specforge grammar: {e}")),
                None,
            );
        }
        Err(CstError::NoTree) => {
            return (
                unparsed(file_sym, "tree-sitter parse failed".to_string()),
                None,
            );
        }
    };
    let mut file = SpecFile {
        path: file_sym,
        imports: Vec::new(),
        entities: Vec::new(),
        format_version: CURRENT_FORMAT_VERSION,
        format_diagnostics: Vec::new(),
        errors: Vec::new(),
    };
    let pieces = cst.pieces();
    let recovered = pieces.len() > 1 || pieces[0].unclosed().is_some();
    for piece in pieces {
        let mut ctx = ParseContext::new(piece.text(), file_sym);
        ctx.row_offset = piece.row();
        ctx.limit = piece.unclosed().map(|_| piece.source_len());
        ctx.walk(piece.root());
        // Errors past the piece come from the appended delimiter, and an
        // error around the opening quote is the grammar tripping over the
        // unclosed string: its own diagnostic replaces both.
        let limit = ctx.limit_point();
        ctx.errors
            .retain(|e| (e.span.start_line, e.span.start_col) < limit);
        if let Some(u) = piece.unclosed() {
            let quote = unclosed_error(source, file_sym, &u, piece.start() + piece.source_len());
            let open = (quote.span.start_line, quote.span.start_col);
            ctx.errors.retain(|e| {
                let start = (e.span.start_line, e.span.start_col);
                let end = (e.span.end_line, e.span.end_col);
                !(start <= open && open < end)
            });
            ctx.errors.push(quote);
        }
        file.imports.append(&mut ctx.imports);
        file.entities.append(&mut ctx.entities);
        file.errors.append(&mut ctx.errors);
    }
    if recovered {
        file.errors
            .sort_by_key(|e| (e.span.start_line, e.span.start_col));
    }
    (file, Some(cst.into_tree()))
}

/// A file nothing could be read from, with the one error saying why.
fn unparsed(file_sym: Sym, message: String) -> SpecFile {
    SpecFile {
        path: file_sym,
        imports: Vec::new(),
        entities: Vec::new(),
        format_version: CURRENT_FORMAT_VERSION,
        format_diagnostics: Vec::new(),
        errors: vec![ParseError {
            message,
            span: SourceSpan {
                file: file_sym,
                start_line: 1,
                start_col: 1,
                end_line: 1,
                end_col: 1,
            },
            expected: None,
            found: None,
        }],
    }
}

/// The diagnostic for an unclosed string, at its opening delimiter; `end` is
/// the byte of `source` where the string was ended.
fn unclosed_error(source: &str, file_sym: Sym, u: &UnclosedString, end: usize) -> ParseError {
    let kind = if u.delim.len() == 3 {
        "triple-quoted string"
    } else {
        "string"
    };
    let span = position_span(source, file_sym, u.open, u.open + u.delim.len());
    let until = if end < source.len() {
        let line = source[..end].matches('\n').count() + 1;
        format!("; it was ended before the block on line {line}")
    } else {
        String::new()
    };
    ParseError {
        message: format!(
            "syntax error: unclosed {kind} — missing closing '{}'{until}",
            u.delim
        ),
        span,
        expected: Some(format!("a closing '{}'", u.delim)),
        found: Some(u.delim.to_string()),
    }
}

/// The span of `source[start..end]` (1-based lines, byte columns).
fn position_span(source: &str, file_sym: Sym, start: usize, end: usize) -> SourceSpan {
    let at = |byte: usize| {
        let before = &source[..byte];
        let line = before.matches('\n').count() + 1;
        let col = byte - before.rfind('\n').map_or(0, |nl| nl + 1) + 1;
        (line, col)
    };
    let (start_line, start_col) = at(start);
    let (end_line, end_col) = at(end);
    SourceSpan {
        file: file_sym,
        start_line,
        start_col,
        end_line,
        end_col,
    }
}

struct ParseContext<'a> {
    source: &'a str,
    file_sym: Sym,
    /// Lines before this piece of the file (recovery parses in pieces).
    row_offset: usize,
    /// Bytes of the piece that are real source; past this is the appended
    /// closing delimiter, and positions there clamp to its start.
    limit: Option<usize>,
    imports: Vec<ImportDeclaration>,
    entities: Vec<Entity>,
    errors: Vec<ParseError>,
}

impl<'a> ParseContext<'a> {
    fn new(source: &'a str, file_sym: Sym) -> Self {
        ParseContext {
            source,
            file_sym,
            row_offset: 0,
            limit: None,
            imports: Vec::new(),
            entities: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn walk(&mut self, root: Node) {
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            match child.kind() {
                _ if child.is_error() => self.push_error_node(child),
                kind::ENTITY_BLOCK => self.parse_entity_block(child),
                kind::SPEC_BLOCK => self.parse_spec_block(child),
                kind::REF_BLOCK => self.parse_ref_block(child),
                kind::DEFINE_BLOCK => self.parse_define_block(child),
                kind::UNION_BLOCK => self.parse_union_block(child),
                kind::USE_IMPORT => self.parse_use_import(child, false),
                kind::PUB_USE_IMPORT => self.parse_use_import(child, true),
                kind::COMMENT => {}
                _ => {}
            }
        }
        // The walk above reports errors at block level and inside field
        // values. Tree-sitter recovers anywhere, so report the rest too
        // (method signatures, parameters, ...): no syntax error is silent.
        self.report_unreported_errors(root);
    }

    fn text(&self, node: Node) -> &'a str {
        node.utf8_text(self.source.as_bytes()).unwrap_or("")
    }

    /// A node position as 1-based (line, column) in the whole file.
    fn position(&self, byte: usize, point: tree_sitter::Point) -> (usize, usize) {
        if let Some(limit) = self.limit
            && byte > limit
        {
            return self.limit_point();
        }
        (point.row + self.row_offset + 1, point.column + 1)
    }

    /// Where the real source of this piece ends, as 1-based (line, column).
    fn limit_point(&self) -> (usize, usize) {
        match self.limit {
            Some(limit) => {
                let before = &self.source[..limit];
                let row = before.matches('\n').count();
                let col = limit - before.rfind('\n').map_or(0, |nl| nl + 1);
                (row + self.row_offset + 1, col + 1)
            }
            None => (usize::MAX, usize::MAX),
        }
    }

    fn span(&self, node: Node) -> SourceSpan {
        let (start_line, start_col) = self.position(node.start_byte(), node.start_position());
        let (end_line, end_col) = self.position(node.end_byte(), node.end_position());
        SourceSpan {
            file: self.file_sym,
            start_line,
            start_col,
            end_line,
            end_col,
        }
    }

    fn unquote(&self, node: Node) -> String {
        let text = self.text(node);
        if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
            let inner = &text[1..text.len() - 1];
            unescape(inner)
        } else {
            text.to_string()
        }
    }

    fn extract_brace_body(&self, node: Node) -> Option<String> {
        let mut open = None;
        let mut close = None;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "{" {
                open = Some(child.end_byte());
            } else if child.kind() == "}" {
                close = Some(child.start_byte());
            }
        }
        match (open, close) {
            (Some(start), Some(end)) if end > start => Some(self.source[start..end].to_string()),
            (Some(start), Some(end)) if start == end => Some(String::new()),
            _ => None,
        }
    }

    /// Report every outermost ERROR or MISSING node under `node` that no
    /// already-reported error covers.
    fn report_unreported_errors(&mut self, node: Node) {
        if node.is_error() || node.is_missing() {
            let span = self.span(node);
            let covered = self.errors.iter().any(|e| span_within(&span, &e.span));
            if covered {
                return;
            }
            if node.is_missing() {
                self.errors.push(ParseError {
                    message: format!("syntax error: missing '{}'", node.kind()),
                    span,
                    expected: Some(format!("'{}'", node.kind())),
                    found: None,
                });
            } else {
                self.push_error_node(node);
            }
            return;
        }
        if !node.has_error() {
            return;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.report_unreported_errors(child);
        }
    }

    fn push_error_node(&mut self, node: Node) {
        let text = self.text(node);
        let trimmed = text.trim();
        let first_line = trimmed.lines().next().unwrap_or(trimmed);
        // Truncate very long error text for readability
        let display = if first_line.len() > 60 {
            format!("{}...", &first_line[..57])
        } else {
            first_line.to_string()
        };

        // Provide contextual suggestions for common mistakes
        let (message, expected) = if trimmed.contains('{') && !trimmed.contains('}') {
            (
                format!(
                    "syntax error near '{}': unclosed block — missing closing '}}'",
                    display
                ),
                Some("a closing '}'".to_string()),
            )
        } else if trimmed.starts_with('}') {
            (
                "syntax error: unexpected '}' — possible extra closing brace".to_string(),
                Some("a valid entity block (e.g., behavior name \"Title\" {{ ... }})".to_string()),
            )
        } else {
            (
                format!("syntax error: unexpected '{}'", display),
                Some("a valid block: keyword name \"Title\" {{ fields... }}".to_string()),
            )
        };

        self.errors.push(ParseError {
            message,
            span: self.span(node),
            expected,
            found: Some(display),
        });
    }

    fn parse_entity_block(&mut self, node: Node) {
        let kind = node
            .child_by_field_name(field::KIND)
            .map(|n| Sym::new(self.text(n)))
            .unwrap_or_else(|| Sym::new(""));
        let name = node
            .child_by_field_name(field::NAME)
            .map(|n| Sym::new(self.text(n)))
            .unwrap_or_else(|| Sym::new(""));
        let title = node
            .child_by_field_name(field::TITLE)
            .map(|n| self.unquote(n));

        let raw_body = self.extract_brace_body(node);
        let (fields, verify, methods) = self.parse_block_body(node);
        let mut field_map = fields;
        if !verify.is_empty() {
            field_map.push(Sym::new("verify"), FieldValue::VerifyList(verify));
        }

        self.entities.push(Entity {
            kind: EntityKind { raw: kind },
            id: EntityId { raw: name },
            title,
            fields: field_map,
            raw_body,
            span: self.span(node),
            methods,
        });
    }

    fn parse_spec_block(&mut self, node: Node) {
        let name = node
            .child_by_field_name(field::NAME)
            .map(|n| self.unquote(n))
            .unwrap_or_default();

        let raw_body = self.extract_brace_body(node);
        let (fields, verify, methods) = self.parse_block_body(node);
        let mut field_map = fields;
        if !verify.is_empty() {
            field_map.push(Sym::new("verify"), FieldValue::VerifyList(verify));
        }

        self.entities.push(Entity {
            kind: EntityKind {
                raw: Sym::new(structural::SPEC),
            },
            id: EntityId {
                raw: Sym::new(&name),
            },
            title: Some(name),
            fields: field_map,
            raw_body,
            span: self.span(node),
            methods,
        });
    }

    fn parse_ref_block(&mut self, node: Node) {
        let Some(inner) = node.child(0) else {
            self.push_error_node(node);
            return;
        };
        let id_text = inner
            .child_by_field_name(field::ID)
            .map(|n| self.text(n))
            .unwrap_or_default();
        let title = inner
            .child_by_field_name(field::TITLE)
            .map(|n| self.unquote(n));

        let mut fields = FieldMap::new();
        if let Some((scheme, kind, identifier)) = parse_ref_id(id_text) {
            fields.push(
                Sym::new(crate::ast::REF_SCHEME_FIELD),
                FieldValue::String(scheme),
            );
            fields.push(Sym::new("ref_kind"), FieldValue::String(kind));
            fields.push(Sym::new("identifier"), FieldValue::String(identifier));
        }

        if inner.kind() == kind::REF_FULL {
            let (body_fields, _, _) = self.parse_block_body(inner);
            for entry in body_fields.entries() {
                fields.push_entry(entry.clone());
            }
        }

        self.entities.push(Entity {
            kind: EntityKind {
                raw: Sym::new(structural::REF),
            },
            id: EntityId {
                raw: Sym::new(id_text),
            },
            title,
            fields,
            raw_body: None,
            span: self.span(node),
            methods: Vec::new(),
        });
    }

    fn parse_union_block(&mut self, node: Node) {
        let kind = node
            .child_by_field_name(field::KIND)
            .map(|n| Sym::new(self.text(n)))
            .unwrap_or_else(|| Sym::new(""));
        let name = node
            .child_by_field_name(field::NAME)
            .map(|n| Sym::new(self.text(n)))
            .unwrap_or_else(|| Sym::new(""));

        let mut variants = Vec::new();
        if let Some(variants_node) = node.child_by_field_name(field::VARIANTS) {
            let mut cursor = variants_node.walk();
            for child in variants_node.children(&mut cursor) {
                match child.kind() {
                    kind::IDENTIFIER => variants.push(self.text(child).to_string()),
                    kind::STRING => variants.push(self.unquote(child)),
                    kind::INTEGER | kind::NEGATIVE_INTEGER => {
                        variants.push(self.text(child).to_string())
                    }
                    _ => {}
                }
            }
        }

        let mut fields = FieldMap::new();
        fields.push(
            Sym::new(crate::ast::UNION_VARIANTS_FIELD),
            FieldValue::VariantList(variants),
        );

        self.entities.push(Entity {
            kind: EntityKind { raw: kind },
            id: EntityId { raw: name },
            title: None,
            fields,
            raw_body: None,
            span: self.span(node),
            methods: Vec::new(),
        });
    }

    fn parse_define_block(&mut self, node: Node) {
        let name = node
            .child_by_field_name(field::NAME)
            .map(|n| Sym::new(self.text(n)))
            .unwrap_or_else(|| Sym::new(""));

        let raw_body = self.extract_brace_body(node);
        let (fields, verify, methods) = self.parse_block_body(node);
        let mut field_map = fields;
        if !verify.is_empty() {
            field_map.push(Sym::new("verify"), FieldValue::VerifyList(verify));
        }

        self.entities.push(Entity {
            kind: EntityKind {
                raw: Sym::new(structural::DEFINE),
            },
            id: EntityId { raw: name },
            title: None,
            fields: field_map,
            raw_body,
            span: self.span(node),
            methods,
        });
    }

    fn parse_use_import(&mut self, node: Node, is_pub: bool) {
        let path = node
            .child_by_field_name(field::PATH)
            .map(|n| Sym::new(&self.unquote(n)))
            .unwrap_or_else(|| Sym::new(""));

        let (kind, bindings, namespace) =
            if let Some(bindings_node) = node.child_by_field_name(field::BINDINGS) {
                let mut bs = Vec::new();
                let mut cursor = bindings_node.walk();
                for child in bindings_node.children(&mut cursor) {
                    if child.kind() == kind::IMPORT_BINDING {
                        let name = child
                            .child(0)
                            .map(|n| self.text(n).to_string())
                            .unwrap_or_default();
                        let alias = child
                            .child_by_field_name(field::ALIAS)
                            .map(|n| self.text(n).to_string());
                        bs.push(ImportBinding { name, alias });
                    }
                }
                (ImportKind::Selective, Some(bs), None)
            } else if let Some(ns_node) = node.child_by_field_name(field::NAMESPACE) {
                let alias = ns_node
                    .child_by_field_name(field::ALIAS)
                    .map(|n| self.text(n).to_string())
                    .unwrap_or_default();
                (ImportKind::Namespace, None, Some(alias))
            } else {
                (ImportKind::Full, None, None)
            };

        self.imports.push(ImportDeclaration {
            path,
            kind,
            bindings,
            namespace,
            is_pub,
            span: self.span(node),
        });
    }

    fn parse_block_body(
        &mut self,
        node: Node,
    ) -> (FieldMap, Vec<VerifyStatement>, Vec<MethodDecl>) {
        let mut fields = FieldMap::new();
        let mut verify = Vec::new();

        let mut methods: Vec<MethodDecl> = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                _ if child.is_error() => self.push_error_node(child),
                kind::FIELD => {
                    if let Some(entry) = self.parse_field(child) {
                        fields.push_entry(entry);
                    }
                }
                kind::VERIFY_STATEMENT => {
                    if let Some(stmt) = self.parse_verify_statement(child) {
                        verify.push(stmt);
                    }
                }
                kind::METHOD_STATEMENT => methods.push(self.parse_method_statement(child)),
                _ => {}
            }
        }

        (fields, verify, methods)
    }

    fn parse_field(&mut self, node: Node) -> Option<FieldEntry> {
        let key = node.child_by_field_name(field::KEY)?;
        let value = node.child_by_field_name(field::VALUE)?;
        // Tree-sitter may recover from a syntax error deep inside a value
        // (e.g. a dangling operator in an expression group); surface it.
        if let Some(broken) = find_error_descendant(value) {
            let span = self.span(broken);
            self.errors.push(ParseError {
                message: "syntax error in field value".to_string(),
                span,
                expected: None,
                found: Some(self.text(broken).chars().take(40).collect()),
            });
        }
        let key_sym = Sym::new(self.text(key));
        let mut field_value = self.parse_value(value);
        // A field named "values" contains enum tags, not entity references
        if key_sym == "values"
            && let FieldValue::ReferenceList(items) = &field_value
        {
            field_value = FieldValue::VariantList(items.iter().map(|r| r.id.clone()).collect());
        }

        // Extract annotations (children with kind "annotation")
        let mut annotations = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == kind::ANNOTATION {
                annotations.push(self.parse_annotation(child));
            }
        }

        Some(FieldEntry {
            key: key_sym,
            value: field_value,
            annotations,
            value_span: Some(self.span(value)),
        })
    }

    /// Parse an annotation node. The grammar defines annotation as:
    ///   `TOKEN("@" + identifier_pattern) string?`
    ///
    /// The `@name` portion is an opaque TOKEN — tree-sitter does not create a
    /// child node for it. Instead the `@name` text is the beginning of the
    /// annotation node's own source text. The only possible named child is
    /// an optional `string` node carrying the annotation value.
    fn parse_annotation(&self, node: Node) -> Annotation {
        // The annotation node's full text starts with "@name" followed by
        // optional whitespace and a quoted string value.
        // Extract the name by taking the first whitespace-delimited token
        // and stripping the leading '@'.
        let full_text = self.text(node).trim();
        let name = full_text
            .split_whitespace()
            .next()
            .unwrap_or(full_text)
            .strip_prefix('@')
            .unwrap_or(full_text)
            .to_string();

        // Look for an optional string child (the annotation value).
        // The grammar does not use field names on annotation children,
        // so we search by node kind.
        let value = {
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .find(|c| c.kind() == kind::STRING)
                .map(|n| self.unquote(n))
        };

        Annotation { name, value }
    }

    fn parse_value(&mut self, node: Node) -> FieldValue {
        match node.kind() {
            kind::STRING => FieldValue::String(self.unquote(node)),
            kind::TRIPLE_QUOTED_STRING => FieldValue::String(self.parse_triple_quoted(node)),
            kind::INTEGER | kind::NEGATIVE_INTEGER => {
                let text = self.text(node);
                match text.parse::<i64>() {
                    Ok(val) => FieldValue::Integer(val),
                    Err(_) => {
                        self.errors.push(ParseError {
                            message: format!(
                                "integer value '{}' is too large (overflows i64)",
                                text
                            ),
                            span: self.span(node),
                            expected: Some("integer within i64 range".to_string()),
                            found: Some(text.to_string()),
                        });
                        FieldValue::Integer(0)
                    }
                }
            }
            kind::BOOLEAN => FieldValue::Boolean(self.text(node) == "true"),
            kind::DATE_LITERAL => FieldValue::Date(self.text(node).to_string()),
            kind::IDENTIFIER => FieldValue::Identifier(self.text(node).to_string()),
            kind::ARRAY_TYPE => FieldValue::Identifier(self.text(node).to_string()),
            kind::EXPR_GROUP => FieldValue::Expression(self.parse_expr_group(node)),
            kind::TYPE_UNION => FieldValue::TypeUnion(self.parse_type_union(node)),
            kind::LIST => self.parse_list(node),
            kind::NESTED_BLOCK => self.parse_nested_block(node),
            _ => FieldValue::String(self.text(node).to_string()),
        }
    }

    fn parse_triple_quoted(&self, node: Node) -> String {
        let text = self.text(node);
        let inner = &text[3..text.len() - 3];
        dedent(inner)
    }

    fn parse_list(&mut self, node: Node) -> FieldValue {
        let mut has_string = false;
        let mut has_identifier = false;
        let mut has_integer = false;
        let mut has_boolean = false;

        // Collect typed items for mixed-type detection
        let mut typed_items: Vec<FieldValue> = Vec::new();
        // Parallel flat string items for homogeneous lists, with each
        // item's source span so references can be diagnosed token-exactly.
        let mut flat_items: Vec<String> = Vec::new();
        let mut item_spans: Vec<SourceSpan> = Vec::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                kind::IDENTIFIER => {
                    let text = self.text(child).to_string();
                    let span = self.span(child);
                    // Detect boolean literals that tree-sitter parses
                    // as identifiers inside list context.
                    if text == "true" || text == "false" {
                        has_boolean = true;
                        typed_items.push(FieldValue::Boolean(text == "true"));
                    } else {
                        has_identifier = true;
                        typed_items.push(FieldValue::Identifier(text.clone()));
                    }
                    flat_items.push(text);
                    item_spans.push(span);
                }
                kind::STRING => {
                    let text = self.unquote(child);
                    has_string = true;
                    typed_items.push(FieldValue::String(text.clone()));
                    flat_items.push(text);
                    item_spans.push(self.span(child));
                }
                kind::SCHEME_REF_ID => {
                    let text = self.text(child).to_string();
                    has_identifier = true;
                    typed_items.push(FieldValue::Identifier(text.clone()));
                    flat_items.push(text);
                    item_spans.push(self.span(child));
                }
                kind::INTEGER => {
                    let text = self.text(child);
                    has_integer = true;
                    let val = text.parse::<i64>().unwrap_or(0);
                    typed_items.push(FieldValue::Integer(val));
                    flat_items.push(text.to_string());
                    item_spans.push(self.span(child));
                }
                kind::BOOLEAN => {
                    let text = self.text(child);
                    has_boolean = true;
                    typed_items.push(FieldValue::Boolean(text == "true"));
                    flat_items.push(text.to_string());
                    item_spans.push(self.span(child));
                }
                _ => {}
            }
        }

        // Determine how many distinct type categories are present
        let type_count = [has_string, has_identifier, has_integer, has_boolean]
            .iter()
            .filter(|&&b| b)
            .count();

        if type_count > 1 {
            // Mixed types detected — emit warning and preserve per-item types
            if has_string && has_identifier {
                self.errors.push(ParseError {
                    message: "mixed list contains both quoted strings and bare identifiers"
                        .to_string(),
                    span: self.span(node),
                    expected: Some("either all quoted strings or all bare identifiers".to_string()),
                    found: None,
                });
            }
            return FieldValue::MixedList(typed_items);
        }

        // Homogeneous list — use flat string representation
        if has_string {
            FieldValue::StringList(flat_items)
        } else {
            FieldValue::ReferenceList(
                flat_items
                    .into_iter()
                    .zip(item_spans)
                    .map(|(id, span)| SpannedRef { id, span })
                    .collect(),
            )
        }
    }

    fn parse_nested_block(&mut self, node: Node) -> FieldValue {
        let mut fields = FieldMap::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == kind::FIELD
                && let Some(entry) = self.parse_field(child)
            {
                fields.push_entry(entry);
            }
        }
        FieldValue::Block(fields)
    }

    fn parse_method_statement(&self, node: Node) -> MethodDecl {
        let name = node
            .child_by_field_name(field::NAME)
            .map(|n| self.text(n).to_string())
            .unwrap_or_default();
        let returns = node
            .child_by_field_name(field::RETURNS)
            .map(|n| self.text(n).trim().to_string());
        let mut params = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() != kind::PARAMETER {
                continue;
            }
            let pname = child
                .child_by_field_name(field::NAME)
                .map(|n| self.text(n).to_string())
                .unwrap_or_default();
            let pty = child
                .child_by_field_name(field::TYPE)
                .map(|n| self.text(n).trim().to_string())
                .unwrap_or_default();
            let mut annotations = Vec::new();
            let mut param_cursor = child.walk();
            for part in child.children(&mut param_cursor) {
                if part.kind() == kind::ANNOTATION {
                    annotations.push(self.parse_annotation(part));
                }
            }
            params.push(Parameter {
                name: pname,
                ty: pty,
                optional: child.child_by_field_name(field::OPTIONAL).is_some(),
                annotations,
            });
        }
        MethodDecl {
            name,
            params,
            returns,
            span: self.span(node),
        }
    }

    fn parse_verify_statement(&self, node: Node) -> Option<VerifyStatement> {
        let desc = node.child_by_field_name(field::DESCRIPTION)?;
        let kind = node
            .child_by_field_name(field::KIND)
            .map(|n| self.text(n).to_string())
            .unwrap_or_default();
        Some(VerifyStatement {
            kind,
            description: self.unquote(desc),
        })
    }
    // --- Formal expressions (`metric expr { ... }`) ---------------------

    fn parse_expr_group(&self, node: Node<'a>) -> Vec<SpannedExpr> {
        let mut cursor = node.walk();
        node.children(&mut cursor)
            .filter(|c| c.kind() == kind::EXPR_OR)
            .map(|c| self.convert_expr(c))
            .collect()
    }

    /// Collect the declared types of a union-typed field value.
    fn parse_type_union(&self, node: Node<'a>) -> Vec<String> {
        let mut types = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                kind::IDENTIFIER
                | kind::ARRAY_TYPE
                | kind::TYPE_GENERIC
                | kind::UNIT_TYPE
                | kind::FUNCTION_TYPE
                | kind::STRING => {
                    types.push(self.text(child).trim().to_string());
                }
                _ => {}
            }
        }
        types
    }

    fn expr_span(&self, node: Node<'a>) -> ExprSpan {
        let span = self.span(node);
        ExprSpan {
            start_line: span.start_line,
            start_col: span.start_col,
            end_line: span.end_line,
            end_col: span.end_col,
        }
    }

    /// Map an expression CST node onto the typed [`SpannedExpr`] AST.
    /// The grammar guarantees node shapes, so degenerate arms only occur
    /// under ERROR recovery and yield neutral placeholders.
    fn convert_expr(&self, node: Node<'a>) -> SpannedExpr {
        let span = self.expr_span(node);
        match node.kind() {
            kind::EXPR_OR | kind::EXPR_AND => {
                let is_or = node.kind() == kind::EXPR_OR;
                let mut cursor = node.walk();
                let mut parts = node
                    .children(&mut cursor)
                    .filter(|c| c.is_named())
                    .map(|c| self.convert_expr(c));
                let mut acc = parts.next().unwrap_or_else(|| fallback_var(span));
                for rhs in parts {
                    let joined = join_span(&acc.span, &rhs.span);
                    let expr = if is_or {
                        Expr::Or(Box::new(acc), Box::new(rhs))
                    } else {
                        Expr::And(Box::new(acc), Box::new(rhs))
                    };
                    acc = SpannedExpr { expr, span: joined };
                }
                acc
            }
            kind::EXPR_CMP => {
                let mut lhs: Option<SpannedExpr> = None;
                let mut op: Option<CmpOp> = None;
                let mut rhs: Option<SpannedExpr> = None;
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        kind::EXPR_ADD => {
                            if lhs.is_none() {
                                lhs = Some(self.convert_expr(child));
                            } else {
                                rhs = Some(self.convert_expr(child));
                            }
                        }
                        "<" => op = Some(CmpOp::Lt),
                        "<=" => op = Some(CmpOp::Le),
                        ">" => op = Some(CmpOp::Gt),
                        ">=" => op = Some(CmpOp::Ge),
                        "==" => op = Some(CmpOp::Eq),
                        "!=" => op = Some(CmpOp::Ne),
                        _ => {}
                    }
                }
                match (lhs, op, rhs) {
                    (Some(l), Some(op), Some(r)) => SpannedExpr {
                        expr: Expr::Cmp(op, Box::new(l), Box::new(r)),
                        span,
                    },
                    (Some(l), _, _) => l,
                    _ => fallback_var(span),
                }
            }
            kind::EXPR_ADD => {
                let mut acc: Option<SpannedExpr> = None;
                let mut pending_sub = false;
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        kind::EXPR_ATOM => {
                            let operand = self.convert_expr(child);
                            match acc.take() {
                                None => acc = Some(operand),
                                Some(lhs) => {
                                    let joined = join_span(&lhs.span, &operand.span);
                                    let expr = if pending_sub {
                                        Expr::Sub(Box::new(lhs), Box::new(operand))
                                    } else {
                                        Expr::Add(Box::new(lhs), Box::new(operand))
                                    };
                                    acc = Some(SpannedExpr { expr, span: joined });
                                }
                            }
                        }
                        "-" => pending_sub = true,
                        "+" => pending_sub = false,
                        _ => {}
                    }
                }
                acc.unwrap_or_else(|| fallback_var(span))
            }
            kind::EXPR_ATOM => {
                let mut cursor = node.walk();
                let children: Vec<Node<'a>> = node.children(&mut cursor).collect();
                let prefix = children.iter().find_map(|c| match c.kind() {
                    "-" | "not" => Some(c.kind()),
                    _ => None,
                });
                let operand = children.iter().find(|c| {
                    matches!(
                        c.kind(),
                        kind::NUMBER_WITH_UNIT | kind::IDENTIFIER | kind::EXPR_OR | kind::EXPR_ATOM
                    )
                });
                let inner = match operand {
                    Some(c) if c.kind() == kind::NUMBER_WITH_UNIT => {
                        let text = self.text(*c);
                        let unit_start = text.trim_end_matches(char::is_alphabetic).len();
                        let value = text[..unit_start].parse::<f64>().unwrap_or(0.0);
                        SpannedExpr {
                            expr: Expr::Num(value, text[unit_start..].to_string()),
                            span,
                        }
                    }
                    Some(c) if c.kind() == kind::IDENTIFIER => SpannedExpr {
                        expr: Expr::Var(self.text(*c).to_string()),
                        span,
                    },
                    // parenthesized group: keep the group's value, span covers parens
                    Some(c) => {
                        let converted = self.convert_expr(*c);
                        SpannedExpr {
                            expr: converted.expr,
                            span,
                        }
                    }
                    None => fallback_var(span),
                };
                match prefix {
                    Some("not") => SpannedExpr {
                        expr: Expr::Not(Box::new(inner)),
                        span,
                    },
                    Some(_) => SpannedExpr {
                        expr: Expr::Neg(Box::new(inner)),
                        span,
                    },
                    None => inner,
                }
            }
            _ => fallback_var(span),
        }
    }
}
fn join_span(a: &ExprSpan, b: &ExprSpan) -> ExprSpan {
    ExprSpan {
        start_line: a.start_line.min(b.start_line),
        start_col: a.start_col.min(b.start_col),
        end_line: a.end_line.max(b.end_line),
        end_col: a.end_col.max(b.end_col),
    }
}

fn fallback_var(span: ExprSpan) -> SpannedExpr {
    SpannedExpr {
        expr: Expr::Var(String::new()),
        span,
    }
}

fn parse_ref_id(id: &str) -> Option<(String, String, String)> {
    let dot_pos = id.find('.')?;
    let colon_pos = id.find(':')?;
    if colon_pos <= dot_pos {
        return None;
    }
    let scheme = id[..dot_pos].to_string();
    let kind = id[dot_pos + 1..colon_pos].to_string();
    let identifier = id[colon_pos + 1..].to_string();
    Some((scheme, kind, identifier))
}

fn dedent(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();

    let min_indent = lines
        .iter()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    let mut result: Vec<&str> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                result.push(trimmed);
            }
        } else if line.trim().is_empty() {
            result.push("");
        } else if line.len() >= min_indent {
            result.push(&line[min_indent..]);
        } else {
            result.push(line);
        }
    }

    while result.last() == Some(&"") {
        result.pop();
    }

    result.join("\n")
}

/// Whether `inner` lies inside `outer` (same file assumed).
fn span_within(inner: &SourceSpan, outer: &SourceSpan) -> bool {
    (outer.start_line, outer.start_col) <= (inner.start_line, inner.start_col)
        && (inner.end_line, inner.end_col) <= (outer.end_line, outer.end_col)
}

/// Depth-first search for an ERROR or MISSING node produced by the
/// parser's recovery inside a value subtree.
fn find_error_descendant<'a>(node: Node<'a>) -> Option<Node<'a>> {
    if node.is_error() || node.is_missing() {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_error_descendant(child) {
            return Some(found);
        }
    }
    None
}
