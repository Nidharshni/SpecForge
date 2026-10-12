//! The one conversion between the protocol's document URIs and the files they name
//! (ADR 0056). A client may spell a file's URI otherwise than `Url::from_file_path` does
//! (VS Code percent-encodes `@`, `+`, `=` and a Windows drive's `:`), so a URI is never compared
//! as a string: it is read here once, into the file it names, and the file's session key is
//! what identifies a document.

use std::path::{Path, PathBuf};

use tower_lsp::lsp_types::Url;

/// The file a `file:` URI names, its path percent-decoded; `None` for any other scheme.
pub(crate) fn file_path(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok()
}

/// The file `uri` names: [`file_path`], else (an `untitled:` buffer) the URI as written, which
/// is the key a session gives a buffer with no file.
pub(crate) fn path_of(uri: &Url) -> PathBuf {
    file_path(uri).unwrap_or_else(|| PathBuf::from(uri.as_str()))
}

/// The `file:` URI of `path` as the server spells it (`Url::from_file_path`), for a file the
/// client has not named; a path [`path_of`] kept from another scheme is parsed back as
/// written; anything else is `file:///`.
pub(crate) fn uri_of(path: &Path) -> Url {
    Url::from_file_path(path)
        .or_else(|_| Url::parse(&path.to_string_lossy()))
        .unwrap_or_else(|_| Url::parse("file:///").expect("a URI"))
}

/// The URI of the directory `dir` (a relative watcher's base); `None` when it is not absolute.
pub(crate) fn directory_uri(dir: &Path) -> Option<Url> {
    Url::from_directory_path(dir).ok()
}
