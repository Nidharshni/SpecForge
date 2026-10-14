//! What the server and the editor share about each file, by the file's session key
//! (ADR 0056): the open document (the editor's buffer), the URI the client names the file
//! by, and the diagnostics last published for it in two parts, each labelled with the buffer
//! version it belongs to. Everything is found by the key ([`Files::key_of`] is the one rule from a URI to
//! it); the URI is kept only to name the file back to the client. It is the one writer of what
//! the editor is shown: a publish is what one of its writes answers.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use specforge_common::Diagnostic;
use tower_lsp::lsp_types::{self as lsp, TextDocumentContentChangeEvent, Url};

use crate::document::Document;
use crate::publish::Publication;
use crate::uri;

/// Every file the editor has open or was sent diagnostics for, by session key.
#[derive(Default)]
pub struct Files {
    by_key: BTreeMap<String, Entry>,
    /// The spec root the keys were computed under ([`Files::rekey`]); empty with no project.
    spec_root: PathBuf,
    /// The open document diagnostics about no entity were last published on.
    anchor: Option<String>,
}

struct Entry {
    /// The URI the client names the file by: the one it last opened the document with, else
    /// the file's own ([`uri::uri_of`]).
    uri: Url,
    /// The file the URI names ([`uri::path_of`]): what the key is computed from, again after
    /// every environment load, and what a held buffer is given to the session as.
    path: PathBuf,
    /// The editor's buffer, while the document is open.
    open: Option<Document>,
    /// What the project reports for the file, as last published.
    compile: Part,
    /// What formatting the open document reported, at the version it formatted.
    formatter: Option<Part>,
}

/// One writer's diagnostics for a file, labelled with the version of the buffer they are
/// positions in (`None`: the file as compiled from disk).
#[derive(Debug, Clone, Default, PartialEq)]
struct Part {
    /// As hover and code actions read them back (a spanless one placed).
    placed: Vec<Diagnostic>,
    /// As the client receives them.
    sent: Vec<lsp::Diagnostic>,
    version: Option<i32>,
}

/// One `textDocument/publishDiagnostics`: the file's whole list (a publish replaces it), under
/// the URI the client names the file by, labelled with the one buffer version it belongs to.
#[derive(Debug, Clone, PartialEq)]
pub struct Publish {
    pub uri: Url,
    pub diagnostics: Vec<lsp::Diagnostic>,
    pub version: Option<i32>,
}

/// What formatting the open document `key` reported (a W142 per region kept verbatim):
/// positions in its buffer at `version`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Findings {
    pub key: String,
    pub version: Option<i32>,
    /// As hover reads them back.
    pub placed: Vec<Diagnostic>,
    /// As the client receives them.
    pub sent: Vec<lsp::Diagnostic>,
}

impl Files {
    // ---- identity --------------------------------------------------------------------

    /// The session key of the file `uri` names: `specforge_project::source_key` of its
    /// percent-decoded path under the spec root. Two spellings of one file's URI are one key;
    /// so are two spellings of one directory (a symlink, a drive letter's case), which
    /// `source_key` compares canonically.
    pub fn key_of(&self, uri: &Url) -> String {
        specforge_project::source_key(&self.spec_root, &uri::path_of(uri))
    }

    /// The URI the client names `key` by: the one it opened the document with, or the one the
    /// file was last sent diagnostics under; else the file's own `file:` URI.
    pub fn uri(&self, key: &str) -> Url {
        match self.by_key.get(key) {
            Some(entry) => entry.uri.clone(),
            None => uri::uri_of(&self.file_path(key)),
        }
    }

    /// The absolute path of session file `key` (the spec root joined with it; a key outside
    /// the spec root is its own path).
    pub fn file_path(&self, key: &str) -> PathBuf {
        self.spec_root.join(key)
    }

    // ---- the open documents (the handlers write them as the client reports them) -------

    /// The client opened `uri` holding `text` at `version`. The file is named by `uri` from
    /// now on: no list is sent to another spelling of it (a client normalizes spellings).
    pub(crate) fn open(&mut self, uri: &Url, text: &str, version: i32) {
        let key = self.key_of(uri);
        let mut document = Document::new(text.to_string());
        document.set_version(version);
        let path = uri::path_of(uri);
        let entry = self
            .by_key
            .entry(key)
            .or_insert_with(|| Entry::named(uri.clone(), path.clone()));
        entry.uri = uri.clone();
        entry.path = path;
        entry.open = Some(document);
    }

    /// The client changed the open document `uri`: `changes` applied in order, then at
    /// `version`. Nothing for a document that is not open.
    pub(crate) fn change(
        &mut self,
        uri: &Url,
        version: i32,
        changes: &[TextDocumentContentChangeEvent],
    ) {
        let key = self.key_of(uri);
        if let Some(document) = self.by_key.get_mut(&key).and_then(|e| e.open.as_mut()) {
            for change in changes {
                document.apply_change(change.range, &change.text);
            }
            document.set_version(version);
        }
    }

    /// The client closed `uri`. Its buffer and its formatter part go; the file keeps the URI
    /// it was opened with, and its compile part, until a publish clears it.
    pub(crate) fn close(&mut self, uri: &Url) {
        let key = self.key_of(uri);
        if let Some(entry) = self.by_key.get_mut(&key) {
            entry.open = None;
            entry.formatter = None;
        }
    }

    /// The open document of `key`.
    pub fn document(&self, key: &str) -> Option<&Document> {
        self.by_key.get(key)?.open.as_ref()
    }

    /// The path the open document `key` was opened as (what its held buffer is given to the
    /// session as).
    pub fn path(&self, key: &str) -> Option<&Path> {
        let entry = self.by_key.get(key)?;
        entry.open.as_ref().map(|_| entry.path.as_path())
    }

    /// The keys of the open documents, in key order.
    pub fn open_keys(&self) -> impl Iterator<Item = &str> {
        self.by_key
            .iter()
            .filter(|(_, entry)| entry.open.is_some())
            .map(|(key, _)| key.as_str())
    }

    // ---- the published diagnostics -------------------------------------------------------

    /// What the project reported for `key` as last published: what code actions act on.
    pub fn compile_diagnostics(&self, key: &str) -> &[Diagnostic] {
        self.by_key
            .get(key)
            .map_or(&[], |entry| entry.compile.placed.as_slice())
    }

    /// What formatting reported for the open document `key`, while the document is still the
    /// text it formatted (positions in it, through its own line index); empty otherwise.
    pub fn formatter_diagnostics(&self, key: &str) -> &[Diagnostic] {
        match self.by_key.get(key) {
            Some(Entry {
                open: Some(document),
                formatter: Some(part),
                ..
            }) if part.version == document.version() => &part.placed,
            _ => &[],
        }
    }

    /// Every compile diagnostic last published, file by file in key order: what the hover's
    /// entity facts report.
    pub fn published(&self) -> impl Iterator<Item = &Diagnostic> {
        self.by_key
            .values()
            .flat_map(|entry| entry.compile.placed.iter())
    }

    /// The files last sent a non-empty list: each is a target of the next publication.
    pub fn published_keys(&self) -> impl Iterator<Item = &str> {
        self.by_key
            .iter()
            .filter(|(_, entry)| !entry.listed().is_empty())
            .map(|(key, _)| key.as_str())
    }

    /// The open document diagnostics about no entity were last published on.
    pub fn anchor(&self) -> Option<&str> {
        self.anchor
            .as_deref()
            .filter(|key| self.document(key).is_some())
    }

    /// Record what the project reports now as each file's compile part, and answer what to
    /// send, in key order. Each file's whole list is its compile part, plus its formatter
    /// part when that belongs to the same version (a formatter part of another version is
    /// dropped: the buffer moved since it was formatted). It goes under the URI the client
    /// names the file by. A file that is not open and was sent an empty list is forgotten.
    pub(crate) fn publish_compile(&mut self, publication: Publication) -> Vec<Publish> {
        let mut sends = Vec::new();
        for (key, file) in publication.files {
            let path = self.file_path(&key);
            let entry = self
                .by_key
                .entry(key.clone())
                .or_insert_with(|| Entry::named(uri::uri_of(&path), path));
            entry.compile = Part {
                placed: file.placed,
                sent: file.diagnostics,
                version: file.version,
            };
            if let Some(formatter) = &entry.formatter
                && formatter.version != entry.compile.version
            {
                entry.formatter = None;
            }
            sends.push(entry.publish());
            if entry.open.is_none() && entry.listed().is_empty() {
                self.by_key.remove(&key);
            }
        }
        if publication.anchor.is_some() {
            self.anchor = publication.anchor;
        }
        sends
    }

    /// Record `findings` as its open document's formatter part, and answer what to send now.
    /// `None` when the document is no longer at `findings.version` (it moved since it was
    /// formatted), when the part did not change, or when the compile part belongs to another
    /// version: the publish of that version's compile then sends both.
    pub(crate) fn publish_formatter(&mut self, findings: Findings) -> Option<Publish> {
        let entry = self.by_key.get_mut(&findings.key)?;
        if entry.open.as_ref()?.version() != findings.version {
            return None;
        }
        let part = (!findings.sent.is_empty()).then_some(Part {
            placed: findings.placed,
            sent: findings.sent,
            version: findings.version,
        });
        if entry.formatter == part {
            return None;
        }
        entry.formatter = part;
        (entry.compile.version == findings.version).then(|| entry.publish())
    }

    // ---- environment loads -------------------------------------------------------------

    /// Key every file anew under `spec_root` (an environment load moved it): each keeps its
    /// URI, its path, its document and its parts. Nothing when the keys are under it already.
    pub(crate) fn rekey(&mut self, spec_root: &Path) {
        if self.spec_root == spec_root {
            return;
        }
        let anchor = self
            .anchor
            .take()
            .and_then(|key| self.by_key.get(&key).map(|entry| entry.path.clone()));
        let entries = std::mem::take(&mut self.by_key);
        self.by_key = entries
            .into_values()
            .map(|entry| (specforge_project::source_key(spec_root, &entry.path), entry))
            .collect();
        self.anchor = anchor.map(|path| specforge_project::source_key(spec_root, &path));
        self.spec_root = spec_root.to_path_buf();
    }

    /// Forget everything (shutdown).
    pub(crate) fn clear(&mut self) {
        self.by_key.clear();
        self.anchor = None;
    }
}

impl Entry {
    fn named(uri: Url, path: PathBuf) -> Entry {
        Entry {
            uri,
            path,
            open: None,
            compile: Part::default(),
            formatter: None,
        }
    }

    /// The whole list the client is shown: the compile part, then the formatter part of the
    /// same version.
    fn listed(&self) -> Vec<lsp::Diagnostic> {
        let mut list = self.compile.sent.clone();
        if let Some(formatter) = self
            .formatter
            .as_ref()
            .filter(|f| f.version == self.compile.version)
        {
            list.extend(formatter.sent.iter().cloned());
        }
        list
    }

    fn publish(&self) -> Publish {
        Publish {
            uri: self.uri.clone(),
            diagnostics: self.listed(),
            version: self.compile.version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::publish::FilePublication;

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    fn diagnostic(code: &str) -> specforge_common::Diagnostic {
        specforge_common::Diagnostic::untyped(code, specforge_common::Severity::Error, "message")
    }

    fn listing(codes: &[&str]) -> FilePublication {
        FilePublication {
            diagnostics: codes
                .iter()
                .map(|code| crate::publish::diagnostic_to_lsp(&diagnostic(code), |_| None))
                .collect(),
            placed: codes.iter().map(|code| diagnostic(code)).collect(),
            version: Some(1),
        }
    }

    fn publication(files: &[(&str, FilePublication)], anchor: Option<&str>) -> Publication {
        Publication {
            files: files
                .iter()
                .map(|(key, file)| (key.to_string(), file.clone()))
                .collect(),
            anchor: anchor.map(str::to_string),
        }
    }

    fn rooted(spec_root: &str) -> Files {
        let mut files = Files::default();
        files.rekey(Path::new(spec_root));
        files
    }

    #[test]
    fn key_of_reads_a_uri_percent_decoded() {
        let files = rooted("/proj/spec");
        for spelling in [
            "file:///proj/spec/@acme/a+b=c/x%20y.spec",
            "file:///proj/spec/%40acme/a%2Bb%3Dc/x%20y.spec",
        ] {
            assert_eq!(files.key_of(&url(spelling)), "@acme/a+b=c/x y.spec");
        }
        // Outside the spec root the key is the path itself; a buffer with no file, its URI.
        assert_eq!(
            files.key_of(&url("file:///elsewhere/a.spec")),
            "/elsewhere/a.spec"
        );
        assert_eq!(
            files.key_of(&url("untitled:Untitled-1")),
            "untitled:Untitled-1"
        );
    }

    #[test]
    fn rekey_keeps_each_file_its_uri_document_and_parts() {
        let mut files = Files::default();
        let uri = url("file:///proj/spec/%40acme/a.spec");
        files.open(&uri, "text", 3);
        assert_eq!(files.key_of(&uri), "/proj/spec/@acme/a.spec");
        files.publish_compile(publication(
            &[("/proj/spec/@acme/a.spec", listing(&["E003"]))],
            Some("/proj/spec/@acme/a.spec"),
        ));

        // The environment loads: the files are keyed under the spec root.
        files.rekey(Path::new("/proj/spec"));
        assert_eq!(files.key_of(&uri), "@acme/a.spec");
        let document = files.document("@acme/a.spec").expect("the document moved");
        assert_eq!((document.text(), document.version()), ("text", Some(3)));
        assert_eq!(files.uri("@acme/a.spec"), uri);
        assert_eq!(files.compile_diagnostics("@acme/a.spec").len(), 1);
        assert_eq!(files.anchor(), Some("@acme/a.spec"));
        assert_eq!(
            files.path("@acme/a.spec"),
            Some(Path::new("/proj/spec/@acme/a.spec"))
        );

        // And back, when the project goes.
        files.rekey(Path::new(""));
        assert!(files.document("/proj/spec/@acme/a.spec").is_some());
        assert_eq!(files.anchor(), Some("/proj/spec/@acme/a.spec"));
    }

    #[test]
    fn a_closed_file_sent_an_empty_list_is_forgotten() {
        let mut files = rooted("/proj");
        let uri = url("file:///proj/%40a.spec");
        files.open(&uri, "text", 1);
        files.publish_compile(publication(&[("@a.spec", listing(&["E003"]))], None));
        files.close(&uri);
        assert!(files.document("@a.spec").is_none());
        assert_eq!(files.published_keys().collect::<Vec<_>>(), ["@a.spec"]);
        assert_eq!(
            files.uri("@a.spec"),
            uri,
            "the URI is kept until a publish clears it"
        );

        let sends = files.publish_compile(publication(&[("@a.spec", listing(&[]))], None));
        assert_eq!(sends.len(), 1);
        assert_eq!((sends[0].uri.clone(), sends[0].diagnostics.len()), (uri, 0));
        assert_eq!(files.published_keys().count(), 0);
        assert_eq!(
            files.uri("@a.spec"),
            url("file:///proj/@a.spec"),
            "forgotten, the file is named as the server spells it"
        );
    }
}
