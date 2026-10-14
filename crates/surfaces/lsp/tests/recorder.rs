//! The test adapter of the LSP's `Editor` port: it records what the reaction tells the editor,
//! answers watcher registrations as a test says, and can act "while the watchers move".

use std::cell::RefCell;
use std::rc::Rc;

use specforge_lsp::editor::{Editor, WorkDone};
use tower_lsp::lsp_types::{
    Diagnostic, FileSystemWatcher, GlobPattern, MessageType, NumberOrString, OneOf, Url,
};

/// What the reaction told the editor, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum Sent {
    Published {
        uri: Url,
        diagnostics: Vec<Diagnostic>,
        version: Option<i32>,
    },
    Watched {
        watchers: Vec<FileSystemWatcher>,
        accepted: bool,
    },
    Logged(MessageType, String),
    Progress(WorkDone),
    TokensRefreshed,
}

type Refusal = Box<dyn Fn(&[FileSystemWatcher]) -> bool>;

#[derive(Default)]
struct Inner {
    sent: Vec<Sent>,
    refuse: Option<Refusal>,
    on_watch: Option<Box<dyn FnMut()>>,
}

/// Records what is sent; clones share the record.
#[derive(Clone, Default)]
pub struct Recorder {
    inner: Rc<RefCell<Inner>>,
}

impl Recorder {
    /// Everything sent since the last call.
    pub fn take(&self) -> Vec<Sent> {
        std::mem::take(&mut self.inner.borrow_mut().sent)
    }

    /// From now on, refuse the watchers `refuse` holds true for.
    pub fn refuse_when(&self, refuse: impl Fn(&[FileSystemWatcher]) -> bool + 'static) {
        self.inner.borrow_mut().refuse = Some(Box::new(refuse));
    }

    /// Run `hook` each time the editor is asked to watch, before it answers: what it does
    /// happens while the watchers move.
    pub fn on_watch(&self, hook: impl FnMut() + 'static) {
        self.inner.borrow_mut().on_watch = Some(Box::new(hook));
    }

    fn push(&self, sent: Sent) {
        self.inner.borrow_mut().sent.push(sent);
    }
}

impl Editor for Recorder {
    fn publish(&mut self, uri: Url, diagnostics: Vec<Diagnostic>, version: Option<i32>) {
        self.push(Sent::Published {
            uri,
            diagnostics,
            version,
        });
    }

    fn watch(&mut self, watchers: &[FileSystemWatcher]) -> Result<(), String> {
        // The hook runs with no borrow held: it may read the record.
        let hook = self.inner.borrow_mut().on_watch.take();
        if let Some(mut hook) = hook {
            hook();
            self.inner.borrow_mut().on_watch.get_or_insert(hook);
        }
        let accepted = !self
            .inner
            .borrow()
            .refuse
            .as_ref()
            .is_some_and(|refuse| refuse(watchers));
        self.push(Sent::Watched {
            watchers: watchers.to_vec(),
            accepted,
        });
        if accepted {
            Ok(())
        } else {
            Err("the editor refused the watchers".into())
        }
    }

    fn log(&mut self, level: MessageType, message: String) {
        self.push(Sent::Logged(level, message));
    }

    fn progress(&mut self, step: WorkDone) {
        self.push(Sent::Progress(step));
    }

    fn refresh_tokens(&mut self) {
        self.push(Sent::TokensRefreshed);
    }
}

// Readers over a record.

/// Every list of diagnostics published for `uri`, in order.
pub fn publications(sent: &[Sent], uri: &Url) -> Vec<Vec<Diagnostic>> {
    sent.iter()
        .filter_map(|s| match s {
            Sent::Published {
                uri: published,
                diagnostics,
                ..
            } if published == uri => Some(diagnostics.clone()),
            _ => None,
        })
        .collect()
}

/// The codes of the last publication for `uri`; `None` when none was sent.
pub fn last_codes(sent: &[Sent], uri: &Url) -> Option<Vec<String>> {
    publications(sent, uri)
        .last()
        .map(|diagnostics| codes(diagnostics))
}

/// The codes of `diagnostics`.
pub fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .filter_map(|d| match d.code.as_ref()? {
            NumberOrString::String(code) => Some(code.clone()),
            NumberOrString::Number(_) => None,
        })
        .collect()
}

/// Every logged message, in order.
pub fn logs(sent: &[Sent]) -> Vec<String> {
    sent.iter()
        .filter_map(|s| match s {
            Sent::Logged(_, message) => Some(message.clone()),
            _ => None,
        })
        .collect()
}

/// The globs of each watch request, in order: an absolute glob as is, a relative pattern as
/// `<base path>/<pattern>`.
pub fn watched(sent: &[Sent]) -> Vec<Vec<String>> {
    sent.iter()
        .filter_map(|s| match s {
            Sent::Watched { watchers, .. } => Some(watchers.iter().map(glob_of).collect()),
            _ => None,
        })
        .collect()
}

fn glob_of(watcher: &FileSystemWatcher) -> String {
    match &watcher.glob_pattern {
        GlobPattern::String(glob) => glob.clone(),
        GlobPattern::Relative(relative) => {
            let base = match &relative.base_uri {
                OneOf::Left(folder) => folder.uri.clone(),
                OneOf::Right(uri) => uri.clone(),
            };
            let base = base.to_file_path().unwrap_or_default();
            let base = base.to_string_lossy();
            format!("{}/{}", base.trim_end_matches('/'), relative.pattern)
        }
    }
}

/// Whether the editor was asked to refresh its semantic tokens.
pub fn refreshed(sent: &[Sent]) -> bool {
    sent.iter().any(|s| matches!(s, Sent::TokensRefreshed))
}
