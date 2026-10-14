//! What the entity under a cursor is, and what completes there, over a
//! project with `@specforge/software` (plan 06's fixtures).

use super::*;

/// A ref, the behavior its ID's middle word spells, and a behavior
/// listing the ref (with a `[` inside a string before it).
const MAIN: &str = concat!(
    "ref gh.issue:42 \"Support Wasm\"\n",
    "\n",
    "behavior issue \"Issue tracking\" {\n",
    "  contract \"tracks issues\"\n",
    "}\n",
    "\n",
    "behavior login \"Login\" {\n",
    "  contract \"see [docs\"\n",
    "  refs [gh.issue:42]\n",
    "}\n",
);

/// Entity IDs written in a comment and in a string.
const PROSE: &str = concat!(
    "behavior issue \"Issue tracking\" {\n",
    "  contract \"tracks issues\"\n",
    "}\n",
    "\n",
    "behavior login \"Login\" {\n",
    "  contract \"x\"\n",
    "}\n",
    "\n",
    "behavior delta \"Delta\" {\n",
    "  // see issue for the details\n",
    "  description \"reuses login and the contract field\"\n",
    "}\n",
);

/// Two bodies that differ only by a `[` inside a string.
const COMP: &str = concat!(
    "behavior alpha \"Alpha\" {\n",
    "  contract \"see [docs\"\n",
    "  \n",
    "}\n",
    "\n",
    "behavior beta \"Beta\" {\n",
    "  contract \"see docs\"\n",
    "  \n",
    "}\n",
    "\n",
);

/// A nested block, then a string being typed.
const NEST: &str = concat!(
    "behavior eps \"Eps\" {\n",
    "  requires {\n",
    "    \n",
    "  }\n",
    "  description \"the \"\n",
    "}\n",
);

/// The (line, UTF-16 character) of the `nth` occurrence of `needle` in
/// `text`, plus `offset` characters.
fn pos(text: &str, needle: &str, nth: usize, offset: u32) -> (u32, u32) {
    let at = text
        .match_indices(needle)
        .nth(nth)
        .unwrap_or_else(|| panic!("no {needle:?} #{nth}"))
        .0;
    let before = &text[..at];
    let line = before.matches('\n').count() as u32;
    let column = before.rsplit('\n').next().unwrap_or("");
    (line, column.encode_utf16().count() as u32 + offset)
}

/// A server over `text` as `file`, in a project with software.
async fn open(file: &str, text: &str) -> (Session, String, tempfile::TempDir) {
    Session::with_extensions(&["@specforge/software"], file, text).await
}

/// The hover's markdown at a position, or "" when there is none.
async fn hover_text(client: &mut Session, uri: &str, (line, character): (u32, u32)) -> String {
    let resp = client.hover(uri, line, character).await;
    resp["result"]["contents"]["value"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

/// The completion items at a position.
async fn items(client: &mut Session, uri: &str, line: u32, character: u32) -> Vec<Value> {
    let resp = client.completion(uri, line, character).await;
    resp["result"].as_array().cloned().unwrap_or_default()
}

#[spec(
    behavior = "prepare_rename",
    verify = "prepare rename on entity ID returns token range"
)]
#[tokio::test]
async fn prepare_rename_answers_a_ref_id() {
    let (mut client, uri, _dir) = open("main.spec", MAIN).await;
    let resp = client.rename_range_at(&uri, 0, 7).await;
    let range = &resp["result"];
    assert_eq!(
        (
            range["start"]["line"].as_u64(),
            range["start"]["character"].as_u64(),
            range["end"]["line"].as_u64(),
            range["end"]["character"].as_u64()
        ),
        (Some(0), Some(4), Some(0), Some(15)),
        "{resp}"
    );
}

#[spec(
    invariant = "cursor_names_one_entity",
    verify = "a scheme ref ID under the cursor names its ref"
)]
#[tokio::test]
async fn a_scheme_ref_id_names_its_ref() {
    let (mut client, uri, _dir) = open("main.spec", MAIN).await;
    // On the ref's item in a list, and on its own declaration.
    for at in [(8, 12), (0, 7)] {
        let value = hover_text(&mut client, &uri, at).await;
        assert!(value.contains("**ref** `gh.issue:42`"), "{at:?}: {value}");
        let resp = client.goto_definition(&uri, at.0, at.1).await;
        let start = &resp["result"]["range"]["start"];
        assert_eq!(
            (start["line"].as_u64(), start["character"].as_u64()),
            (Some(0), Some(4)),
            "{at:?}: {resp}"
        );
    }
}

/// The (line, UTF-16 character) of byte `offset` of `text`.
fn position_of(text: &str, offset: usize) -> (u32, u32) {
    let before = &text[..offset];
    let line = before.matches('\n').count() as u32;
    let column = before.rsplit('\n').next().unwrap_or("");
    (line, column.encode_utf16().count() as u32)
}

/// Whether a hover shows an entity's section (`**kind** `id``).
fn names_an_entity(hover: &str) -> bool {
    entity_in(hover).is_some()
}

/// The entity a hover's header names: the first `` `id` `` after
/// `**kind**`.
fn entity_in(hover: &str) -> Option<&str> {
    let after = &hover[hover.find("** `")? + 4..];
    Some(&after[..after.find('`')?])
}

#[spec(
    invariant = "cursor_names_one_entity",
    verify = "a word in a string or comment names no entity"
)]
#[tokio::test]
async fn words_in_prose_name_no_entity() {
    let (mut client, uri, _dir) = open("prose.spec", PROSE).await;
    for at in [pos(PROSE, "issue for", 0, 1), pos(PROSE, "login and", 0, 1)] {
        let value = hover_text(&mut client, &uri, at).await;
        assert!(!names_an_entity(&value), "{at:?}: {value}");
        let resp = client.goto_definition(&uri, at.0, at.1).await;
        assert!(resp["result"].is_null(), "{at:?}: {resp}");
    }
}

/// A string spanning lines whose second line names an entity.
const MULTI: &str = concat!(
    "behavior issue \"Issue tracking\" {\n",
    "  contract \"tracks issues\"\n",
    "}\n",
    "\n",
    "behavior login \"Login\" {\n",
    "  contract \"first line\n",
    "  mentions issue and ends\"\n",
    "}\n",
);

#[spec(
    invariant = "cursor_names_one_entity",
    verify = "a word in a string or comment names no entity"
)]
#[tokio::test]
async fn words_in_a_multi_line_string_name_no_entity() {
    let (mut client, uri, _dir) = open("multi.spec", MULTI).await;
    let at = pos(MULTI, "issue and", 0, 1);
    let value = hover_text(&mut client, &uri, at).await;
    assert!(!names_an_entity(&value), "{value}");
    let resp = client.goto_definition(&uri, at.0, at.1).await;
    assert!(resp["result"].is_null(), "{resp}");
}

#[spec(
    behavior = "hover_information",
    verify = "field help answers only on a field's name"
)]
#[tokio::test]
async fn field_help_answers_on_a_field_name_only() {
    let text = PROSE.replace(
        "  description \"reuses",
        "  contract \"x\"\n  description \"reuses",
    );
    let (mut client, uri, _dir) = open("prose.spec", &text).await;
    let value = hover_text(&mut client, &uri, pos(&text, "contract field", 0, 1)).await;
    assert!(!value.contains("**`contract`**"), "{value}");
    let value = hover_text(&mut client, &uri, pos(&text, "contract \"x\"", 1, 1)).await;
    assert!(value.contains("**`contract`**"), "{value}");
}

/// A project of `files` with software, served, every file open: the
/// client, the directory, and each file's URI and text.
async fn open_project(
    files: &[(&str, &str)],
) -> (
    Session,
    specforge_project::testing::TestProject,
    Vec<(String, String)>,
) {
    let dir = specforge_project::testing::TestProject::new()
        .config(|c| c["name"] = "test".into())
        .enabling(&["@specforge/software"])
        .files(files);
    let root = dir.root().to_str().unwrap();
    let (mut client, _) = Session::launch(Some(root), json!({})).await;
    client
        .wait_for_notification("window/logMessage", 5000)
        .await;
    client
        .wait_for_notification("window/logMessage", 5000)
        .await;
    let mut opened = Vec::new();
    for (name, text) in files {
        let uri = tower_lsp::lsp_types::Url::from_file_path(dir.root().join(name))
            .unwrap()
            .to_string();
        client.did_open(&uri, "specforge", text).await;
        client
            .wait_for_notification("textDocument/publishDiagnostics", 5000)
            .await;
        opened.push((uri, text.to_string()));
    }
    (client, dir, opened)
}

/// The text a definition answer selects, in the files of `opened`; `None`
/// for no answer or an empty selection (a file's start).
fn selected<'a>(resp: &Value, opened: &'a [(String, String)]) -> Option<&'a str> {
    let result = &resp["result"];
    let uri = result["uri"].as_str()?;
    let (_, text) = opened.iter().find(|(u, _)| u == uri)?;
    let range = &result["range"];
    let line = |end: &str| range[end]["line"].as_u64().unwrap() as usize;
    let character = |end: &str| range[end]["character"].as_u64().unwrap() as usize;
    if line("start") != line("end") {
        return None;
    }
    let line_text = text.lines().nth(line("start"))?;
    let selection = &line_text[character("start")..character("end")];
    (!selection.is_empty()).then_some(selection)
}

/// Entity IDs written in a comment, a string, a `use` binding and an enum
/// value, beside the entities they spell (plan 06's R5 file).
const PROSE_ONLY: &str = concat!(
    "use { token } from \"types/core\"\n",
    "\n",
    "behavior delta \"Delta\" {\n",
    "  // see issue for the details\n",
    "  description \"reuses login and the contract field\"\n",
    "  category command\n",
    "  refs [gh.issue:42]\n",
    "}\n",
    "\n",
    "type command \"Command\" {}\n",
);

const CORE: &str = "type token \"Token\" {}\n";

#[spec(
    invariant = "cursor_names_one_entity",
    verify = "hover and go-to-definition resolve the same entity on every token of a document"
)]
#[tokio::test]
async fn hover_and_definition_resolve_the_same_entity_everywhere() {
    let (mut client, _dir, opened) = open_project(&[
        ("main.spec", MAIN),
        ("prose.spec", PROSE_ONLY),
        ("types/core.spec", CORE),
    ])
    .await;
    let mut named = 0;
    for (uri, text) in opened.clone() {
        let mut ends: Vec<usize> = specforge_parser::lex::lex(&text)
            .into_iter()
            .flat_map(|l| [l.start, l.end])
            .collect();
        ends.dedup();
        for offset in ends {
            let at = position_of(&text, offset);
            let hover = hover_text(&mut client, &uri, at).await;
            let resp = client.goto_definition(&uri, at.0, at.1).await;
            assert_eq!(
                entity_in(&hover),
                selected(&resp, &opened),
                "{uri} {at:?}: {hover} / {resp}"
            );
            named += usize::from(entity_in(&hover).is_some());
        }
    }
    assert!(named > 10, "only {named} positions name an entity");
}

#[spec(
    invariant = "cursor_names_one_entity",
    verify = "a value of a field typed as no reference names no entity"
)]
#[tokio::test]
async fn an_enum_value_names_no_entity() {
    let text = "behavior b \"B\" {\n  category command\n}\n\ntype command \"Command\" {}\n";
    let (mut client, uri, _dir) = open("enum.spec", text).await;
    let at = pos(text, "command\n", 0, 2);
    let value = hover_text(&mut client, &uri, at).await;
    assert!(!names_an_entity(&value), "{value}");
    let resp = client.goto_definition(&uri, at.0, at.1).await;
    assert!(resp["result"].is_null(), "{resp}");
}

#[spec(
    behavior = "go_to_definition",
    verify = "a use binding's imported name goes to the entity it names"
)]
#[tokio::test]
async fn a_use_binding_names_its_entity() {
    let main = "use { token } from \"types/core\"\n\nbehavior b \"B\" {}\n";
    let (mut client, _dir, opened) =
        open_project(&[("main.spec", main), ("types/core.spec", CORE)]).await;
    let uri = &opened[0].0;
    let resp = client.goto_definition(uri, 0, 7).await;
    assert_eq!(selected(&resp, &opened), Some("token"), "{resp}");
    assert!(
        resp["result"]["uri"]
            .as_str()
            .is_some_and(|u| u.ends_with("types/core.spec")),
        "{resp}"
    );
    let value = hover_text(&mut client, uri, (0, 7)).await;
    assert_eq!(entity_in(&value), Some("token"), "{value}");
    // The path goes to the file, as on any other place of the statement.
    let resp = client.goto_definition(uri, 0, 22).await;
    let result = &resp["result"];
    assert!(
        result["uri"]
            .as_str()
            .is_some_and(|u| u.ends_with("types/core.spec")),
        "{resp}"
    );
    assert_eq!(result["range"]["start"]["line"], 0, "{resp}");
    assert_eq!(result["range"]["start"]["character"], 0, "{resp}");
}

#[spec(
    behavior = "complete_field_names",
    verify = "a bracket inside a string opens no reference list"
)]
#[tokio::test]
async fn a_bracket_in_a_string_keeps_field_completion() {
    let (mut client, uri, _dir) = open("comp.spec", COMP).await;
    let found = items(&mut client, &uri, 2, 2).await;
    assert!(!found.is_empty());
    assert!(found.iter().all(|i| i["kind"] == 5), "{found:?}");
    assert!(found.iter().any(|i| i["label"] == "refs"), "{found:?}");
    // The same answer as the body without the `[`.
    let beta = items(&mut client, &uri, 7, 2).await;
    let labels =
        |items: &[Value]| -> Vec<String> { items.iter().map(|i| i["label"].to_string()).collect() };
    assert_eq!(labels(&found), labels(&beta));
}

#[spec(
    behavior = "complete_keywords",
    verify = "use is always suggested and define never is"
)]
#[tokio::test]
async fn define_is_not_offered() {
    let (mut client, uri, _dir) = open("comp.spec", COMP).await;
    let found = items(&mut client, &uri, 10, 0).await;
    assert!(found.iter().any(|i| i["label"] == "use"), "{found:?}");
    assert!(!found.iter().any(|i| i["label"] == "define"), "{found:?}");
}

#[spec(
    behavior = "complete_field_names",
    verify = "nothing is suggested inside a string, a comment or a nested block"
)]
#[tokio::test]
async fn nested_blocks_and_strings_complete_nothing() {
    let (mut client, uri, _dir) = open("nest.spec", NEST).await;
    assert!(items(&mut client, &uri, 2, 4).await.is_empty());
    assert!(items(&mut client, &uri, 4, 19).await.is_empty());
}
