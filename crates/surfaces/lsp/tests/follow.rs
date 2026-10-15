//! How the LSP follows the project's inputs with the editor's watchers when the editor refuses
//! them (`specforge_lsp::reaction`, ADR 0099): a refused set is asked for once per follow and
//! reported, and only an update that moves the inputs moves the watchers.

use crate::recorder::{Sent, logs};
use crate::served::Served;
use specforge_lsp::changes::Change;
use specforge_lsp::watchers::default_watchers;
use specforge_test_macros::test as spec;
use tower_lsp::lsp_types::{FileChangeType, FileEvent, MessageType};

const ALPHA: &str = "behavior alpha \"Alpha\" {}\n";

/// A project whose editor refuses every watcher but the static ones.
fn refusing() -> Served {
    let served = Served::new(&[("a.spec", ALPHA)]);
    served
        .editor()
        .refuse_when(|watchers| watchers != default_watchers().as_slice());
    served
}

fn refused(sent: &[Sent]) -> Vec<usize> {
    sent.iter()
        .enumerate()
        .filter(|(_, s)| {
            matches!(
                s,
                Sent::Watched {
                    accepted: false,
                    ..
                }
            )
        })
        .map(|(i, _)| i)
        .collect()
}

/// Append a newline to `specforge.json` when it exists: its bytes change, its config does not.
fn touch_config(root: &std::path::Path) {
    let path = root.join("specforge.json");
    if let Ok(text) = std::fs::read_to_string(&path) {
        std::fs::write(path, format!("{text}\n")).unwrap();
    }
}

#[spec(
    behavior = "classify_project_changes",
    verify = "a set of watchers refused once is not asked for again in the same follow"
)]
#[ignore = "flipped by T4"]
fn a_refused_set_of_watchers_is_asked_for_once_per_follow() {
    let served = refusing();
    let root = served.root().to_path_buf();
    // Every catch-up while the watchers move is an environment reload with unchanged inputs.
    served.editor().on_watch(move || touch_config(&root));
    let served = served.open(&[]);

    assert_eq!(refused(served.opening()).len(), 1, "{:?}", served.opening());
}

#[spec(
    behavior = "classify_project_changes",
    verify = "the LSP reports a refused set of watchers and registers the static ones"
)]
#[ignore = "flipped by T4"]
fn a_refused_set_of_watchers_is_logged_after_the_static_ones() {
    let served = refusing().open(&[]);

    let opening = served.opening();
    let i = *refused(opening)
        .first()
        .unwrap_or_else(|| panic!("the project's watchers are refused: {opening:?}"));
    assert_eq!(
        opening[i + 1],
        Sent::Watched {
            watchers: default_watchers(),
            accepted: true
        }
    );
    match &opening[i + 2] {
        Sent::Logged(MessageType::WARNING, m) => assert!(
            m.starts_with("specforge-lsp: the editor refused the project's file watchers"),
            "{m}"
        ),
        other => panic!("a WARNING log follows the static watchers: {other:?}"),
    }
}

#[spec(
    behavior = "classify_project_changes",
    verify = "an environment reload that leaves the inputs as they were moves no watchers"
)]
#[ignore = "flipped by T5"]
fn a_reload_that_leaves_the_inputs_asks_for_no_watchers() {
    let mut served = refusing().open(&[]);
    served.sent();

    touch_config(served.root());
    let uri = served.uri("specforge.json");
    served.apply(Change::Watched(vec![FileEvent::new(
        uri,
        FileChangeType::CHANGED,
    )]));

    let sent = served.sent();
    assert!(
        logs(&sent)
            .iter()
            .any(|m| m.contains("extension environment changed")),
        "{sent:?}"
    );
    assert!(
        !sent.iter().any(|s| matches!(s, Sent::Watched { .. })),
        "{sent:?}"
    );
}
