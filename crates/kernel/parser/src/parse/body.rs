use super::*;

impl<'a> ParseContext<'a> {
    pub(super) fn parse_block_body(
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
