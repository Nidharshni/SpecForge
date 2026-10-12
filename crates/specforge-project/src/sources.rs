//! The project's source texts as the build read them: the one read of a
//! `.spec` file, for a compile and for every update, the retained
//! tree-sitter trees that let a keystroke-sized edit reuse subtrees, and the
//! sources that could not be read (E025, ADR 0032).
//!
//! A parse depends only on its own file's text, and references resolve
//! across the whole project without `use` (ADR 0004 D1-a), so a change
//! re-parses exactly the files that changed: an importer of a changed file
//! parses the same as before. The graph those parses build is
//! [`specforge_graph::GraphBuild`]'s.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use specforge_common::{Diagnostic, codes};
use specforge_graph::{FileChange, GraphBuild};
use specforge_parser::{SpecFile, parse_incremental};

use crate::inputs::source_key;

/// What reading one source gave.
pub(crate) enum Read {
    Text(String),
    /// Found but not readable as UTF-8 text: its E025.
    Unreadable(Diagnostic),
    /// Not there (deleted, or never created).
    Gone,
}

/// Read `relative` (a source key) under `spec_root`: the one read of a
/// `.spec` file.
pub(crate) fn read(spec_root: &Path, relative: &str) -> Read {
    match std::fs::read_to_string(spec_root.join(relative)) {
        Ok(text) => Read::Text(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Read::Gone,
        Err(e) => Read::Unreadable(
            Diagnostic::new(codes::E025, format!("cannot read {relative}: {e}")).with_suggestion(
                "save it as UTF-8 text, or leave it out with an `exclude` entry in specforge.json",
            ),
        ),
    }
}

/// The text each held source was parsed from, the retained trees, and the
/// sources that could not be read.
pub(crate) struct SourceCache {
    /// The text each held parse was made from: what a reader resolves a
    /// span of the graph against (a span is a position in this text), and
    /// what a whole-file replacement is diffed against to edit the
    /// retained tree.
    texts: HashMap<String, Arc<str>>,
    /// Retained tree-sitter trees, fed back into `parse_incremental` so
    /// keystroke-sized edits reuse unchanged subtrees.
    trees: HashMap<String, tree_sitter::Tree>,
    /// E025 of each source that is there and could not be read, by path.
    unreadable: BTreeMap<String, Diagnostic>,
}

impl SourceCache {
    pub fn empty() -> Self {
        SourceCache {
            texts: HashMap::new(),
            trees: HashMap::new(),
            unreadable: BTreeMap::new(),
        }
    }

    /// Read (through [`read`]) and parse `discovered` (paths under
    /// `spec_root`, keyed by [`source_key`]): the cold read. An unreadable
    /// one is recorded with its E025; no tree is kept (the first edit of a
    /// file parses it whole).
    ///
    /// A `held` text (by source key) is read in place of its file: no file a
    /// buffer holds is read. A held key discovery does not find (a new file
    /// not yet saved) is parsed too; the caller passes only source keys.
    pub fn read_all(
        spec_root: &Path,
        discovered: &[PathBuf],
        held: &BTreeMap<String, &str>,
    ) -> (Self, Vec<SpecFile>) {
        let mut cache = SourceCache::empty();
        let mut files = Vec::with_capacity(discovered.len() + held.len());
        let keys = discovered.iter().map(|path| source_key(spec_root, path));
        let mut seen = BTreeSet::new();
        for key in keys {
            let read = match held.get(&key) {
                Some(text) => Read::Text((*text).to_string()),
                None => read(spec_root, &key),
            };
            seen.insert(key.clone());
            cache.add(key, read, &mut files);
        }
        for (key, text) in held {
            if !seen.contains(key) {
                cache.add(key.clone(), Read::Text((*text).to_string()), &mut files);
            }
        }
        (cache, files)
    }

    /// One cold-read source: parsed whole, or recorded unreadable.
    fn add(&mut self, key: String, read: Read, files: &mut Vec<SpecFile>) {
        match read {
            Read::Text(text) => {
                files.push(parse_incremental(&text, &key, None).0);
                self.texts.insert(key, Arc::from(text));
            }
            Read::Unreadable(diagnostic) => {
                self.unreadable.insert(key, diagnostic);
            }
            Read::Gone => {}
        }
    }

    /// One source's new state: its parse (reusing the retained tree,
    /// through [`edited_tree_for_replacement`]), or its removal when it is
    /// gone or can no longer be read (then its E025 is kept until it is
    /// readable or gone).
    pub fn change(&mut self, path: &str, read: Read) -> FileChange {
        match read {
            Read::Text(text) => {
                self.unreadable.remove(path);
                FileChange::Parsed(self.parse(path, text))
            }
            Read::Unreadable(diagnostic) => {
                self.forget(path);
                self.unreadable.insert(path.to_string(), diagnostic);
                FileChange::Removed(path.to_string())
            }
            Read::Gone => {
                self.forget(path);
                self.unreadable.remove(path);
                FileChange::Removed(path.to_string())
            }
        }
    }

    /// Whether the cache already holds the state `read` gives for `key`:
    /// the held parse's text is `read`'s text, or `key` is gone, or it is
    /// unreadable for the same reason, so changing it would change nothing.
    pub fn is_current(&self, key: &str, read: &Read) -> bool {
        let unreadable = self.unreadable.get(key);
        match read {
            Read::Text(text) => {
                unreadable.is_none() && self.texts.get(key).is_some_and(|held| **held == **text)
            }
            Read::Gone => unreadable.is_none() && !self.texts.contains_key(key),
            Read::Unreadable(diagnostic) => {
                !self.texts.contains_key(key)
                    && unreadable.is_some_and(|held| held.message == diagnostic.message)
            }
        }
    }

    /// The text `path`'s held parse was made from.
    pub fn text(&self, path: &str) -> Option<&Arc<str>> {
        self.texts.get(path)
    }

    /// Every held parse's text, by path (shared, not copied).
    pub fn texts(&self) -> HashMap<String, Arc<str>> {
        self.texts.clone()
    }

    /// E025 for each source that could not be read, in path order.
    pub fn unreadable(&self) -> impl Iterator<Item = &Diagnostic> {
        self.unreadable.values()
    }

    /// Every source it holds, by key: each parsed one and each that could not
    /// be read. What a `use` path may name (ADR 0055).
    pub fn keys(&self) -> BTreeSet<&str> {
        self.texts
            .keys()
            .chain(self.unreadable.keys())
            .map(String::as_str)
            .collect()
    }

    /// The import diagnostics of what the compile holds: E025 for each source
    /// that could not be read, then the resolver's over `graph`'s parses (E025,
    /// I004, W113, W027), an unreadable source being one an import may name.
    /// The same function after a cold read and after every update.
    pub fn import_diagnostics(&self, graph: &GraphBuild) -> Vec<Diagnostic> {
        let parsed: Vec<(&str, &SpecFile)> = graph.files().collect();
        let unreadable: Vec<&str> = self.unreadable.keys().map(String::as_str).collect();
        self.unreadable()
            .cloned()
            .chain(specforge_resolver::resolve_imports(&parsed, &unreadable))
            .collect()
    }

    fn forget(&mut self, path: &str) {
        self.trees.remove(path);
        self.texts.remove(path);
    }

    /// Parse `text` as `path`, reusing the retained tree.
    fn parse(&mut self, path: &str, text: String) -> SpecFile {
        let old_tree = match (self.trees.get(path), self.texts.get(path)) {
            (Some(tree), Some(old)) if **old != *text => {
                Some(edited_tree_for_replacement(tree, old, &text))
            }
            (Some(tree), Some(_)) => Some(tree.clone()),
            _ => None,
        };
        let (spec_file, tree) = parse_incremental(&text, path, old_tree.as_ref());
        match tree {
            Some(tree) => self.trees.insert(path.to_string(), tree),
            None => self.trees.remove(path),
        };
        self.texts.insert(path.to_string(), Arc::from(text));
        spec_file
    }
}

/// The [`tree_sitter::InputEdit`] for a whole-file replacement, applied to
/// a copy of `old_tree`, so `parse_incremental` can reuse unchanged
/// subtrees: the longest common byte prefix and suffix of the two texts.
fn edited_tree_for_replacement(
    old_tree: &tree_sitter::Tree,
    old_src: &str,
    new_src: &str,
) -> tree_sitter::Tree {
    let mut prefix = 0;
    let max_prefix = old_src.len().min(new_src.len());
    while prefix < max_prefix && old_src.as_bytes()[prefix] == new_src.as_bytes()[prefix] {
        prefix += 1;
    }
    while prefix > 0 && !old_src.is_char_boundary(prefix) {
        prefix -= 1;
    }

    let mut suffix = 0;
    let max_suffix = max_prefix - prefix;
    while suffix < max_suffix
        && old_src.as_bytes()[old_src.len() - 1 - suffix]
            == new_src.as_bytes()[new_src.len() - 1 - suffix]
    {
        suffix += 1;
    }
    while suffix > 0 && !new_src.is_char_boundary(new_src.len() - suffix) {
        suffix -= 1;
    }

    let start_byte = prefix;
    let old_end_byte = old_src.len() - suffix;
    let new_end_byte = new_src.len() - suffix;

    let point = |src: &str, byte: usize| {
        let before = &src[..byte];
        let row = before.matches('\n').count();
        let column = byte - before.rfind('\n').map(|i| i + 1).unwrap_or(0);
        tree_sitter::Point { row, column }
    };

    let mut tree = old_tree.clone();
    tree.edit(&tree_sitter::InputEdit {
        start_byte,
        old_end_byte,
        new_end_byte,
        start_position: point(old_src, start_byte),
        old_end_position: point(old_src, old_end_byte),
        new_end_position: point(new_src, new_end_byte),
    });
    tree
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_graph::GraphConfig;
    use specforge_parser::parse;

    /// Typing reuses the retained tree; a shrinking replacement still
    /// parses as the new text alone.
    #[test]
    fn a_shrinking_replacement_parses_as_the_new_text() {
        let mut cache = SourceCache::empty();
        let mut build = GraphBuild::new(GraphConfig::default());
        build.set_verify(true);
        for text in [
            "behavior foo \"Foo\" { contract \"long text here\" }",
            "behavior foo \"Foo\" { contract \"x\" }",
            "behavior f",
            "",
            "behavior bar \"Bar\" { contract \"y\" }",
        ] {
            let change = cache.change("a.spec", Read::Text(text.to_string()));
            let FileChange::Parsed(ref parsed) = change else {
                panic!("a readable text is parsed");
            };
            assert_eq!(
                parsed.entities.len(),
                parse(text, "a.spec").entities.len(),
                "{text}"
            );
            let applied = build.apply([change]);
            assert_eq!(applied.verification, Some(Ok(())), "{text}");
            assert_eq!(cache.text("a.spec").map(|t| &**t), Some(text));
        }
    }

    /// A held text is read in place of its file: the file is never read.
    #[test]
    fn a_held_text_is_read_in_place_of_its_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.spec"), b"type x \"X\xff\" {}\n").unwrap();
        let held = BTreeMap::from([
            ("a.spec".to_string(), "type held \"H\" {}\n"),
            ("new.spec".to_string(), "type fresh \"F\" {}\n"),
        ]);
        let (cache, files) = SourceCache::read_all(dir.path(), &[dir.path().join("a.spec")], &held);
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|f| f.entities.len() == 1));
        assert!(cache.text("new.spec").is_some());
        assert_eq!(
            cache.text("a.spec").map(|t| &**t),
            Some("type held \"H\" {}\n")
        );
        assert_eq!(cache.unreadable().count(), 0, "the file was never read");
    }

    fn unreadable(path: &str) -> Read {
        read_of(path, b"term beta \"B\xff\xfe\" {\n}\n")
    }

    /// What `read` gives for a file holding `bytes`.
    fn read_of(path: &str, bytes: &[u8]) -> Read {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(path), bytes).unwrap();
        read(dir.path(), path)
    }

    #[test]
    fn a_gone_source_drops_its_unreadable_diagnostic() {
        let mut cache = SourceCache::empty();
        assert!(matches!(
            cache.change("bad.spec", unreadable("bad.spec")),
            FileChange::Removed(_)
        ));
        let messages: Vec<&str> = cache.unreadable().map(|d| d.message.as_str()).collect();
        assert_eq!(
            messages,
            ["cannot read bad.spec: stream did not contain valid UTF-8"]
        );

        assert!(matches!(
            cache.change("bad.spec", Read::Gone),
            FileChange::Removed(_)
        ));
        assert_eq!(cache.unreadable().count(), 0);

        // A source that becomes readable is parsed and no longer reported.
        cache.change("bad.spec", unreadable("bad.spec"));
        assert!(matches!(
            cache.change("bad.spec", Read::Text("// ok\n".to_string())),
            FileChange::Parsed(_)
        ));
        assert_eq!(cache.unreadable().count(), 0);
    }
}
