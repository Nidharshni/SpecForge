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
use std::path::Path;
use std::sync::Arc;

use specforge_common::{Diagnostic, codes};
use specforge_graph::{FileChange, GraphBuild};
use specforge_parser::{Previous, SpecFile, parse_incremental};
use specforge_registry::Disk;

/// What reading one source gave.
pub(crate) enum Read {
    Text(String),
    /// Found but not readable as UTF-8 text: its E025.
    Unreadable(Diagnostic),
    /// Not there (deleted, or never created).
    Gone,
}

/// Read source `relative` under `spec_root` through `disk`: the one read of a
/// `.spec` file. Its text; its E025 when it is there and is not UTF-8 text or
/// cannot be read; `Gone` when the disk says `NotFound`.
pub(crate) fn read(disk: &dyn Disk, spec_root: &Path, relative: &str) -> Read {
    match disk.read(&spec_root.join(relative)) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => Read::Text(text),
            Err(_) => unreadable(relative, "stream did not contain valid UTF-8"),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Read::Gone,
        // The one kind a disk that bounds the session refuses a name with (ADR 0145): the message
        // says where the name landed.
        Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => Read::Unreadable(
            Diagnostic::new(codes::E025, format!("cannot read {relative}: {e}")).with_suggestion(
                "keep the file inside the served project as a plain file (not a symlink that \
                 leaves it, not a hard link: copy it in), or read this project with the CLI",
            ),
        ),
        Err(e) => unreadable(relative, e),
    }
}

/// The E025 of source `relative`, which is there and cannot be read.
fn unreadable(relative: &str, why: impl std::fmt::Display) -> Read {
    Read::Unreadable(
        Diagnostic::new(codes::E025, format!("cannot read {relative}: {why}")).with_suggestion(
            "save it as UTF-8 text, or leave it out with an `exclude` entry in specforge.json",
        ),
    )
}

/// The text each held source was parsed from, the retained trees, and the
/// sources that could not be read.
#[derive(Clone)]
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

    /// A cache holding only `texts`: no trees and no unreadable source. A test's way to give a
    /// project other texts than it was read as.
    #[cfg(feature = "testing")]
    pub(crate) fn holding(texts: HashMap<String, Arc<str>>) -> Self {
        SourceCache {
            texts,
            ..SourceCache::empty()
        }
    }

    /// Read (through `read`) and parse the sources `keys` names: the cold read. An unreadable
    /// one is recorded with its E025; no tree is kept (the first edit of a
    /// file parses it whole).
    ///
    /// A `held` text (by source key) is read in place of its file: no file a
    /// buffer holds is read. A held key discovery does not find (a new file
    /// not yet saved) is parsed too; the caller passes only source keys.
    pub fn read_all(
        keys: &[String],
        held: &BTreeMap<String, &str>,
        mut read: impl FnMut(&str) -> Read,
    ) -> (Self, Vec<SpecFile>) {
        let mut cache = SourceCache::empty();
        let mut files = Vec::with_capacity(keys.len() + held.len());
        let mut seen = BTreeSet::new();
        for key in keys {
            let text = match held.get(key.as_str()) {
                Some(text) => Read::Text((*text).to_string()),
                None => read(key),
            };
            seen.insert(key.clone());
            cache.add(key.clone(), text, &mut files);
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
    /// through `parse_incremental`'s `Previous`), or its removal when it is
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

    /// What it holds for `key`, as the read that made it gave it: the text it parsed, the E025 it recorded
    /// for a source it could not read, or `Gone`. A rebuild from it reads no file.
    pub fn as_read(&self, key: &str) -> Read {
        match (self.texts.get(key), self.unreadable.get(key)) {
            (Some(text), _) => Read::Text(text.to_string()),
            (None, Some(e025)) => Read::Unreadable(e025.clone()),
            (None, None) => Read::Gone,
        }
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
        let previous = match (self.trees.get(path), self.texts.get(path)) {
            (Some(tree), Some(old)) => Some(Previous { text: old, tree }),
            _ => None,
        };
        let (spec_file, tree) = parse_incremental(&text, path, previous);
        match tree {
            Some(tree) => self.trees.insert(path.to_string(), tree),
            None => self.trees.remove(path),
        };
        self.texts.insert(path.to_string(), Arc::from(text));
        spec_file
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_graph::GraphConfig;
    use specforge_parser::parse;
    use specforge_registry::{Asked, MemoryDisk};

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
        let disk = MemoryDisk::new().with_file("/spec/a.spec", b"type x \"X\xff\" {}\n");
        let held = BTreeMap::from([
            ("a.spec".to_string(), "type held \"H\" {}\n"),
            ("new.spec".to_string(), "type fresh \"F\" {}\n"),
        ]);
        let (cache, files) = SourceCache::read_all(&["a.spec".to_string()], &held, |key| {
            read(&disk, Path::new("/spec"), key)
        });
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|f| f.entities.len() == 1));
        assert!(cache.text("new.spec").is_some());
        assert_eq!(
            cache.text("a.spec").map(|t| &**t),
            Some("type held \"H\" {}\n")
        );
        assert_eq!(cache.unreadable().count(), 0, "the file was never read");
        assert!(disk.asked().is_empty(), "the file was never read");
    }

    /// A source reads through the disk it is given: its text, its E025, or gone.
    #[test]
    fn a_source_reads_through_the_disk_it_is_given() {
        let disk = MemoryDisk::new()
            .with_file("/spec/a.spec", b"term alpha \"A\" {}\n")
            .with_file("/spec/bad.spec", b"term beta \"B\xff\xfe\" {\n}\n");
        let spec = Path::new("/spec");

        assert!(matches!(
            read(&disk, spec, "a.spec"),
            Read::Text(text) if text == "term alpha \"A\" {}\n"
        ));
        let Read::Unreadable(bad) = read(&disk, spec, "bad.spec") else {
            panic!("a file that is not UTF-8 is unreadable");
        };
        assert_eq!(
            bad.message,
            "cannot read bad.spec: stream did not contain valid UTF-8"
        );
        assert!(matches!(read(&disk, spec, "gone.spec"), Read::Gone));
        assert_eq!(
            disk.asked(),
            [
                Asked::Read("/spec/a.spec".into()),
                Asked::Read("/spec/bad.spec".into()),
                Asked::Read("/spec/gone.spec".into()),
            ]
        );
    }

    fn unreadable_source(path: &str) -> Read {
        let disk = MemoryDisk::new().with_file(
            Path::new("/spec").join(path),
            b"term beta \"B\xff\xfe\" {\n}\n",
        );
        read(&disk, Path::new("/spec"), path)
    }

    #[test]
    fn a_gone_source_drops_its_unreadable_diagnostic() {
        let mut cache = SourceCache::empty();
        assert!(matches!(
            cache.change("bad.spec", unreadable_source("bad.spec")),
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
        cache.change("bad.spec", unreadable_source("bad.spec"));
        assert!(matches!(
            cache.change("bad.spec", Read::Text("// ok\n".to_string())),
            FileChange::Parsed(_)
        ));
        assert_eq!(cache.unreadable().count(), 0);
    }
}
