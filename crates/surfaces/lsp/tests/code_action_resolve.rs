//! A client that resolves `edit` is shown a stub without its edits and gets them when it resolves
//! the action: planning a stub costs a compile of the project (ADR 0161).

use crate::served::Served;
use specforge_extension_sdk::prelude::*;
use specforge_lsp::{ClientSupport, answers};
use tower_lsp::jsonrpc::ErrorCode;
use tower_lsp::lsp_types::{CodeAction, CodeActionOrCommand, Position, Range};

const TEXT: &str = "port missing_port \"Port\" {\n  direction inbound\n}\n\n\
                    behavior first \"First\" {\n  ports [missing_port]\n}\n\n\
                    behavior second \"Second\" {\n  ports [missing_prot]\n  verify unit \"s\"\n}\n";

const VERIFY: &str = "Add verify stub for first";
const PORT: &str = "Create port stub for missing_prot";
const REPLACE: &str = "Replace with 'missing_port'";

/// `behavior` (testable, takes verify, `ports` naming ports) and `port` (an enum `direction` it requires).
fn ext(c: &mut ContributionsBuilder) {
    c.kind("behavior", |k| {
        k.testable(true).supports_verify(true);
        k.field("ports", |f| {
            f.field_type(FieldType::ReferenceList).target_kind("port");
        });
    });
    c.kind("port", |k| {
        k.field("direction", |f| {
            f.vocabulary(&Vocabulary::new(&["inbound", "outbound"]))
                .required();
        });
    });
}

fn served(client: ClientSupport) -> Served {
    Served::new(&[("a.spec", TEXT)])
        .extension("@test/ext", ext)
        .client(client)
        .open(&["a.spec"])
}

fn resolving() -> ClientSupport {
    ClientSupport {
        code_action_resolve: true,
        ..Default::default()
    }
}

fn whole_file() -> Range {
    Range::new(Position::new(0, 0), Position::new(12, 0))
}

fn listed(served: &Served) -> Vec<CodeAction> {
    answers::code_actions(&served.state(), &served.uri("a.spec"), whole_file())
        .unwrap_or_default()
        .into_iter()
        .map(|action| match action {
            CodeActionOrCommand::CodeAction(action) => action,
            other => panic!("a command: {other:?}"),
        })
        .collect()
}

fn titled<'a>(actions: &'a [CodeAction], title: &str) -> &'a CodeAction {
    actions
        .iter()
        .find(|a| a.title == title)
        .unwrap_or_else(|| panic!("no '{title}' in {:?}", titles(actions)))
}

fn titles(actions: &[CodeAction]) -> Vec<&str> {
    actions.iter().map(|a| a.title.as_str()).collect()
}

/// The (range, text) of each edit of `action`.
fn edits_of(action: &CodeAction) -> Vec<(Range, String)> {
    let changes = action.edit.as_ref().unwrap().changes.as_ref().unwrap();
    changes
        .values()
        .flatten()
        .map(|e| (e.range, e.new_text.clone()))
        .collect()
}

#[test]
fn a_resolving_client_gets_a_stub_without_its_edits() {
    let served = served(resolving());
    let actions = listed(&served);
    let stub = titled(&actions, VERIFY);
    assert!(stub.edit.is_none());
    assert_eq!(stub.data.as_ref().unwrap()["title"], VERIFY);
    assert!(
        titled(&actions, PORT).edit.is_none(),
        "offered, refused when resolved"
    );
    assert!(
        titled(&actions, REPLACE).edit.is_some(),
        "a replacement carries its edit"
    );
}

#[specforge_test_macros::test(
    behavior = "code_actions_for_missing_verify",
    verify = "a client that resolves code actions gets a stub's edits on resolve"
)]
fn resolving_a_stub_gives_the_edits_an_eager_client_gets() {
    let lazy = served(resolving());
    let resolved =
        answers::resolve_code_action(&lazy.state(), titled(&listed(&lazy), VERIFY).clone())
            .unwrap();
    let eager = served(ClientSupport::default());
    let edits = edits_of(titled(&listed(&eager), VERIFY));
    assert!(!edits.is_empty());
    assert_eq!(edits_of(&resolved), edits);
}

#[test]
fn a_resolved_stub_the_plan_refuses_answers_why() {
    let served = served(resolving());
    let stub = titled(&listed(&served), PORT).clone();
    let error = answers::resolve_code_action(&served.state(), stub).unwrap_err();
    assert!(error.message.contains("E061"), "{}", error.message);
    assert!(error.message.contains("direction"), "{}", error.message);
}

#[test]
fn a_stub_resolved_over_a_buffer_typed_since_is_content_modified() {
    let mut served = served(resolving());
    let stub = titled(&listed(&served), VERIFY).clone();
    served.type_text("a.spec", &format!("// typed\n{TEXT}"));
    let error = answers::resolve_code_action(&served.state(), stub).unwrap_err();
    assert_eq!(error.code, ErrorCode::ServerError(-32801));
}

#[test]
fn a_client_without_resolve_support_gets_edits_at_once() {
    let served = served(ClientSupport::default());
    let actions = listed(&served);
    assert!(
        actions.iter().all(|a| a.edit.is_some()),
        "{:?}",
        titles(&actions)
    );
    titled(&actions, VERIFY);
    assert!(
        !titles(&actions).contains(&PORT),
        "a refused stub is not offered to a client that does not resolve"
    );
}

/// The request's file is not the compiled text: nothing is listed, typed or not (ADR 0023 D3).
#[specforge_test_macros::test(
    invariant = "lsp_utf16_positions",
    verify = "no code action is listed over a buffer typed since the compile, to a client that resolves them or not"
)]
fn an_eager_client_is_listed_nothing_over_a_buffer_typed_since() {
    let mut served = served(ClientSupport::default());
    assert!(titles(&listed(&served)).contains(&VERIFY));
    served.type_text("a.spec", &format!("// typed\n{TEXT}"));
    assert_eq!(titles(&listed(&served)), Vec::<&str>::new());
}

#[specforge_test_macros::test(
    invariant = "lsp_utf16_positions",
    verify = "no code action is listed over a buffer typed since the compile, to a client that resolves them or not"
)]
fn a_resolving_client_is_listed_nothing_over_a_buffer_typed_since() {
    let mut served = served(resolving());
    assert!(titles(&listed(&served)).contains(&VERIFY));
    served.type_text("a.spec", &format!("// typed\n{TEXT}"));
    assert_eq!(titles(&listed(&served)), Vec::<&str>::new());
}
