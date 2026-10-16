use super::*;

impl<'a> ParseContext<'a> {
    pub(super) fn parse_entity_block(&mut self, node: Node) {
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

        let (fields, verify, methods) = self.parse_block_body(node);
        self.entities.push(Entity {
            kind: EntityKind { raw: kind },
            id: EntityId { raw: name },
            title,
            fields,
            span: self.span(node),
            methods,
            obligations: verify,
        });
    }

    pub(super) fn parse_spec_block(&mut self, node: Node) {
        let name = node
            .child_by_field_name(field::NAME)
            .map(|n| self.unquote(n))
            .unwrap_or_default();

        let (fields, verify, methods) = self.parse_block_body(node);
        self.entities.push(Entity {
            kind: EntityKind {
                raw: Sym::new(structural::SPEC),
            },
            id: EntityId {
                raw: Sym::new(&name),
            },
            title: Some(name),
            fields,
            span: self.span(node),
            methods,
            obligations: verify,
        });
    }

    pub(super) fn parse_ref_block(&mut self, node: Node) {
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
            span: self.span(node),
            methods: Vec::new(),
            obligations: Vec::new(),
        });
    }

    pub(super) fn parse_union_block(&mut self, node: Node) {
        let kind = node
            .child_by_field_name(field::KIND)
            .map(|n| Sym::new(self.text(n)))
            .unwrap_or_else(|| Sym::new(""));
        let name = node
            .child_by_field_name(field::NAME)
            .map(|n| Sym::new(self.text(n)))
            .unwrap_or_else(|| Sym::new(""));

        let mut variants = Vec::new();
        let variants_span = node
            .child_by_field_name(field::VARIANTS)
            .map(|n| self.span(n));
        if let Some(variants_node) = node.child_by_field_name(field::VARIANTS) {
            let mut cursor = variants_node.walk();
            for child in variants_node.children(&mut cursor) {
                let (text, written) = match child.kind() {
                    kind::IDENTIFIER => (self.text(child).to_string(), Written::Bare),
                    kind::STRING => (self.unquote(child), Written::Quoted),
                    kind::INTEGER | kind::NEGATIVE_INTEGER => {
                        (self.text(child).to_string(), Written::Literal)
                    }
                    _ => continue,
                };
                variants.push(TypeMember {
                    text,
                    span: self.span(child),
                    written,
                });
            }
        }

        let mut fields = FieldMap::new();
        fields.push_entry(FieldEntry {
            key: Sym::new(crate::ast::UNION_VARIANTS_FIELD),
            value: FieldValue::VariantList(variants),
            annotations: Vec::new(),
            value_span: variants_span,
        });

        self.entities.push(Entity {
            kind: EntityKind { raw: kind },
            id: EntityId { raw: name },
            title: None,
            fields,
            span: self.span(node),
            methods: Vec::new(),
            obligations: Vec::new(),
        });
    }

    pub(super) fn parse_define_block(&mut self, node: Node) {
        let name = node
            .child_by_field_name(field::NAME)
            .map(|n| Sym::new(self.text(n)))
            .unwrap_or_else(|| Sym::new(""));

        let (fields, verify, methods) = self.parse_block_body(node);
        self.entities.push(Entity {
            kind: EntityKind {
                raw: Sym::new(structural::DEFINE),
            },
            id: EntityId { raw: name },
            title: None,
            fields,
            span: self.span(node),
            methods,
            obligations: verify,
        });
    }

    pub(super) fn parse_use_import(&mut self, node: Node, is_pub: bool) {
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
