use super::*;

/// The diagnostic for an unclosed string, at its opening delimiter; `end` is
/// the byte of `source` where the string was ended.
pub(super) fn unclosed_error(
    source: &str,
    file_sym: Sym,
    u: &UnclosedString,
    end: usize,
) -> ParseError {
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
    }
}

/// The span of `source[start..end]` (1-based lines, byte columns).
pub(super) fn position_span(source: &str, file_sym: Sym, start: usize, end: usize) -> SourceSpan {
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

/// The top-level block `missing` is the closing `}` of: an entity, spec or
/// define block, or a full ref, that is a child of the source file.
fn top_level_body(missing: Node<'_>) -> Option<Node<'_>> {
    let parent = missing.parent()?;
    let top = match parent.kind() {
        kind::ENTITY_BLOCK | kind::SPEC_BLOCK | kind::DEFINE_BLOCK => parent,
        kind::REF_FULL => parent.parent()?,
        _ => return None,
    };
    (top.parent()?.kind() == kind::SOURCE_FILE).then_some(parent)
}

/// Whether `inner` lies inside `outer` (same file assumed).
fn span_within(inner: &SourceSpan, outer: &SourceSpan) -> bool {
    (outer.start_line, outer.start_col) <= (inner.start_line, inner.start_col)
        && (inner.end_line, inner.end_col) <= (outer.end_line, outer.end_col)
}

impl<'a> ParseContext<'a> {
    /// The error for a top-level body left unclosed: at its `{`, naming the
    /// line of the form where it was ended, if one follows.
    fn unclosed_body_error(&self, block: Node) -> ParseError {
        let mut cursor = block.walk();
        let open = block
            .children(&mut cursor)
            .find(|c| c.kind() == "{")
            .unwrap_or(block);
        let form = if block.kind() == kind::REF_FULL {
            block.parent().unwrap_or(block)
        } else {
            block
        };
        let mut next = form.next_named_sibling();
        while next.is_some_and(|n| n.kind() == kind::COMMENT) {
            next = next.and_then(|n| n.next_named_sibling());
        }
        let line = next
            .map(|n| n.start_position().row + self.row_offset)
            .or(self.next_piece_row)
            .map(|row| format!("; it was ended before the block on line {}", row + 1))
            .unwrap_or_default();
        ParseError {
            message: format!("syntax error: unclosed block — missing closing '}}'{line}"),
            span: self.span(open),
        }
    }

    /// Report every outermost ERROR or MISSING node under `node` that no
    /// already-reported error covers.
    pub(super) fn report_unreported_errors(&mut self, node: Node) {
        if node.is_error() || node.is_missing() {
            let span = self.span(node);
            let covered = self.errors.iter().any(|e| span_within(&span, &e.span));
            if covered {
                return;
            }
            if node.is_missing()
                && node.kind() == "}"
                && let Some(block) = top_level_body(node)
            {
                // Past the piece's source, the appended closing quote took
                // the body's `}`: the string's error stands for both.
                if self.limit.is_none_or(|limit| node.start_byte() < limit) {
                    let error = self.unclosed_body_error(block);
                    self.errors.push(error);
                }
                return;
            }
            if node.is_missing() {
                self.errors.push(ParseError {
                    message: format!("syntax error: missing '{}'", node.kind()),
                    span,
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

    pub(super) fn push_error_node(&mut self, node: Node) {
        let text = self.text(node);
        let trimmed = text.trim();
        let first_line = trimmed.lines().next().unwrap_or(trimmed);
        // At most 57 bytes, cut at the last character boundary at or before
        // byte 57: a cut inside a character would panic (the hover's rule).
        let display = if first_line.len() > 60 {
            format!("{}...", &first_line[..first_line.floor_char_boundary(57)])
        } else {
            first_line.to_string()
        };

        // Provide contextual suggestions for common mistakes
        let message = if trimmed.contains('{') && !trimmed.contains('}') {
            format!(
                "syntax error near '{}': unclosed block — missing closing '}}'",
                display
            )
        } else if trimmed.starts_with('}') {
            "syntax error: unexpected '}' — possible extra closing brace".to_string()
        } else {
            format!("syntax error: unexpected '{}'", display)
        };

        self.errors.push(ParseError {
            message,
            span: self.span(node),
        });
    }
}
