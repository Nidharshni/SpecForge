//! Which entities belong to a file: the file rule (`match_file`) the anchors use, and the outline of a spec file the LSP's document
//! symbols and MCP's `specforge.outline` both answer.

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{SourceSpan, Sym};
use specforge_graph::{MethodJson, Node};

use super::Navigator;
use crate::OpError;

/// How a query path matches a file; also the spelling of a match mode on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "snake_case")]
pub enum FileMatch {
    /// The same path.
    Exact,
    /// The query is a directory the file is under.
    Directory,
    /// The query is a trailing suffix path of the file.
    SuffixPath,
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
        return FileMatch::Directory;
    }
    if file.len() >= query.len() && file[file.len() - query.len()..] == query[..] {
        return FileMatch::SuffixPath;
    }
    FileMatch::None
}

/// One entity of a file's outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineEntry {
    pub id: Sym,
    pub kind: Sym,
    pub title: Option<String>,
    pub block: SourceSpan,
    /// The entity's name where the parse read it (its block when the compile holds no parse or text of the file,
    /// or the text does not spell it there).
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
    /// The method's name where the parse read it (its method's block when the text does not spell it there).
    pub name_span: SourceSpan,
}

/// The outline of spec file `file` (its path, canonical separators): its
/// entities in line order, each with its method members, each entry
/// selecting its name. The LSP's document symbols and MCP's outline.
///
/// `file` is read into its source key by [`ProjectView::source`](crate::view::ProjectView::source)
/// (ADR 0195): `./a.spec` is `a.spec`. A refusal when `file` names none of the compile's sources (a file
/// that does not exist, a directory, `specforge.json` or any non-`.spec` file, one `exclude` leaves out:
/// `file_not_found`) or leaves the spec root (`not_a_source`). A source with no entity, or one that could
/// not be read (E025), has an empty outline (ADR 0116).
pub fn outline(nav: &Navigator<'_>, file: &str) -> Result<Vec<OutlineEntry>, OpError> {
    let key = nav.view.source(file)?;
    let graph = nav.view.graph();
    let mut nodes: Vec<&Node> = graph
        .nodes()
        .into_iter()
        .filter(|n| n.source_span.file.as_str() == key)
        .collect();
    nodes.sort_by_key(|n| {
        let s = &n.source_span;
        (s.start_line, s.start_col, n.id.raw)
    });
    let entries = nodes
        .into_iter()
        .map(|node| {
            let definition = nav.definition_of(node);
            let mut children: Vec<OutlineMethod> = node
                .methods
                .iter()
                .map(|method| OutlineMethod {
                    name: method.name.clone(),
                    signature: MethodJson::of(method).signature(),
                    block: method.span.clone(),
                    name_span: method
                        .name_span
                        .clone()
                        .filter(|s| nav.spelled(s, &method.name))
                        .unwrap_or_else(|| method.span.clone()),
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
        .collect();
    Ok(entries)
}
