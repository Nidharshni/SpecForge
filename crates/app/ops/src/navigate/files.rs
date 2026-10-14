//! Which entities belong to a file: the file rule (`match_file`) prompts
//! and tools share, the entities anchored to a source file
//! (`anchors_of_file`), and the outline of a spec file the LSP's document
//! symbols and MCP's `specforge.outline` both answer.

use super::anchors::{AnchorManifest, SourceAnchor};
use specforge_common::{SourceSpan, Sym};
use specforge_graph::Node;

use super::Navigator;

/// How a query path matches a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMatch {
    /// The same path.
    Exact,
    /// The query is a directory the file is under.
    Under,
    /// The query is a trailing suffix path of the file.
    Suffix,
    None,
}

/// A path's components: separators canonicalized to `/`, empty and `.`
/// components dropped (`./src/a.rs` and `src\a.rs` are `src/a.rs`).
fn components(path: &str) -> Vec<&str> {
    path.split(['/', '\\'])
        .filter(|c| !c.is_empty() && *c != ".")
        .collect()
}

/// Component-wise: the query is the file (exact); a directory the file is
/// under; or a trailing suffix path of it. Substrings never match
/// (`e.rs` is not `src/cache.rs`).
pub fn match_file(query: &str, file: &str) -> FileMatch {
    let query = components(query);
    let file = components(file);
    if query == file {
        return FileMatch::Exact;
    }
    if query.is_empty() {
        return FileMatch::None;
    }
    if file.len() > query.len() && file[..query.len()] == query[..] {
        return FileMatch::Under;
    }
    if file.len() >= query.len() && file[file.len() - query.len()..] == query[..] {
        return FileMatch::Suffix;
    }
    FileMatch::None
}

/// The anchors a source-file query finds, and how they matched.
#[derive(Debug, Clone)]
pub struct FileAnchors<'m> {
    /// [`FileMatch::Exact`] when some anchor's file is the query;
    /// otherwise [`FileMatch::Under`] when one is under it, else
    /// [`FileMatch::Suffix`]; [`FileMatch::None`] when none matches.
    pub mode: FileMatch,
    /// In manifest order.
    pub anchors: Vec<&'m SourceAnchor>,
}

/// Which entities belong to the source files `query` names: the anchors
/// manifest's anchors in them, under [`match_file`]. The tightest
/// non-empty mode wins: exact matches, else those under the query or
/// ending with it. The MCP `specforge.find_spec_for_source` tool and the
/// infer prompt's file scope give this one answer.
pub fn anchors_of_file<'m>(manifest: &'m AnchorManifest, query: &str) -> FileAnchors<'m> {
    let mut exact = Vec::new();
    let mut loose = Vec::new();
    let mut under = false;
    for anchor in &manifest.anchors {
        match match_file(query, &anchor.file) {
            FileMatch::Exact => exact.push(anchor),
            FileMatch::Under => {
                under = true;
                loose.push(anchor);
            }
            FileMatch::Suffix => loose.push(anchor),
            FileMatch::None => {}
        }
    }
    let (mode, anchors) = if !exact.is_empty() {
        (FileMatch::Exact, exact)
    } else if loose.is_empty() {
        (FileMatch::None, loose)
    } else if under {
        (FileMatch::Under, loose)
    } else {
        (FileMatch::Suffix, loose)
    };
    FileAnchors { mode, anchors }
}

/// One entity of a file's outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineEntry {
    pub id: Sym,
    pub kind: Sym,
    pub title: Option<String>,
    pub block: SourceSpan,
    /// The entity's name as written (its block when unreadable).
    pub name: SourceSpan,
    /// Its `method` members, in line order.
    pub children: Vec<OutlineMethod>,
}

/// A `method` member of an entity, in its outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineMethod {
    pub name: String,
    /// `name(param: Type, opt?: Type) -> Ret`, as declared.
    pub signature: String,
    pub block: SourceSpan,
    /// The method's name as written (its block when unreadable).
    pub name_span: SourceSpan,
}

/// The outline of spec file `file` (its path, canonical separators): its
/// entities in line order, each with its method members, each entry
/// selecting its name. The LSP's document symbols and MCP's outline.
pub fn outline<F: Fn(&str) -> Option<String>>(
    nav: &Navigator<'_, F>,
    file: &str,
) -> Vec<OutlineEntry> {
    let graph = nav.view.graph();
    let mut nodes: Vec<&Node> = graph
        .nodes()
        .into_iter()
        .filter(|n| match_file(file, n.source_span.file.as_str()) == FileMatch::Exact)
        .collect();
    nodes.sort_by_key(|n| {
        let s = &n.source_span;
        (s.start_line, s.start_col, n.id.raw)
    });
    nodes
        .into_iter()
        .map(|node| {
            let definition = nav.definition_of(node);
            let mut children: Vec<OutlineMethod> = node
                .methods
                .iter()
                .map(|method| {
                    let params: Vec<String> = method
                        .params
                        .iter()
                        .map(|p| {
                            let optional = if p.optional { "?" } else { "" };
                            format!("{}{optional}: {}", p.name, p.ty)
                        })
                        .collect();
                    let returns = method
                        .returns
                        .as_ref()
                        .map(|r| format!(" -> {r}"))
                        .unwrap_or_default();
                    OutlineMethod {
                        name: method.name.clone(),
                        signature: format!("{}({}){returns}", method.name, params.join(", ")),
                        block: method.span.clone(),
                        name_span: nav.method_name(&method.span, &method.name),
                    }
                })
                .collect();
            children.sort_by_key(|m| (m.block.start_line, m.block.start_col));
            OutlineEntry {
                id: node.id.raw,
                kind: node.kind.raw,
                title: node.title.clone(),
                block: node.source_span.clone(),
                name: definition.name,
                children,
            }
        })
        .collect()
}

impl<F: Fn(&str) -> Option<String>> Navigator<'_, F> {
    /// A method's name in its declaration (`method <name>(…)`): the first
    /// identifier after the keyword, when the text spells the name there;
    /// else the method's whole span.
    fn method_name(&self, span: &SourceSpan, name: &str) -> SourceSpan {
        self.text(span.file)
            .and_then(|text| {
                text.tokens(span)
                    .into_iter()
                    .filter(|t| t.is_name())
                    .nth(1)
                    .filter(|t| text.token_text(t) == name)
                    .map(|t| text.span(span.file, t.start, t.end))
            })
            .unwrap_or_else(|| span.clone())
    }
}
