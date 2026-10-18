//! Completion as production runs it: the cursor's site
//! (`Document::at(..).completion()`) and its items
//! (`completion::items`) over a project view.

use crate::registries::registries;
use specforge_common::{SourceSpan, Sym};
use specforge_extension_sdk::prelude::{FieldType, Vocabulary};
use specforge_graph::{Graph, Node};
use specforge_lsp::completion::items;
use specforge_lsp::{CompletionSite, Document};
use specforge_ops::view::ProjectView;
use specforge_parser::{EntityId, EntityKind, FieldMap};
use specforge_project::CompiledProject;
use specforge_project::testing::Assembled;
use specforge_project::testing::TestProject;
use specforge_registry::RegistryBuild;
use specforge_test_macros::test as spec;
use std::sync::Arc;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionTextEdit, Documentation, MarkupKind, Position,
};

/// A project of `files` with `@specforge/software`, compiled with its
/// extension loaded (and the directory that holds it).
pub fn compiled(files: &[(&str, &str)]) -> (TestProject, CompiledProject) {
    let dir = TestProject::new()
        .config(|c| c["name"] = "completion".into())
        .enabling(&["@specforge/software"])
        .files(files);
    let project = dir.loaded();
    (dir, project)
}

/// The site and items at (`line`, `character`) of `text`, over `view`.
fn complete<'v>(
    text: &str,
    line: u32,
    character: u32,
    view: &ProjectView<'v>,
) -> (String, Vec<CompletionItem>) {
    let doc = Document::new(text.into());
    let cursor = doc.at(Position::new(line, character)).expect("a position");
    let site = cursor.completion();
    let found = items(&site, &cursor.word_edit(), false, view);
    (format!("{site:?}"), found)
}

/// The items at a position of `text`, in a project made of `text` alone.
fn complete_in_project(text: &str, line: u32, character: u32) -> Vec<CompletionItem> {
    let (_dir, project) = compiled(&[("test.spec", text)]);
    complete(text, line, character, &ProjectView::of(&project)).1
}

fn labels(items: &[CompletionItem]) -> Vec<&str> {
    items.iter().map(|i| i.label.as_str()).collect()
}

fn all_of_kind(items: &[CompletionItem], kind: CompletionItemKind) -> bool {
    items.iter().all(|i| i.kind == Some(kind))
}

const TWO_KINDS: &str = "behavior login \"Login\" {\n  \n}\n\ntype token \"Token\" {\n  \n}\n";

#[spec(
    behavior = "complete_field_names",
    verify = "field name completion uses FieldRegistry for entity kind"
)]
fn fields_of_the_enclosing_kind() {
    let found = complete_in_project(TWO_KINDS, 1, 2);
    let names = labels(&found);
    assert!(names.contains(&"contract"), "{names:?}");
    assert!(names.contains(&"invariants"), "{names:?}");
    assert!(all_of_kind(&found, CompletionItemKind::FIELD));
}

#[spec(
    behavior = "complete_field_names",
    verify = "suggestions are filtered by entity kind"
)]
fn fields_differ_by_kind() {
    let (_dir, project) = compiled(&[("test.spec", TWO_KINDS)]);
    let view = ProjectView::of(&project);
    let behavior = complete(TWO_KINDS, 1, 2, &view).1;
    let token = complete(TWO_KINDS, 5, 2, &view).1;
    assert_ne!(labels(&behavior), labels(&token));
    assert!(labels(&token).contains(&"extends"), "{:?}", labels(&token));
}

#[spec(
    behavior = "complete_field_names",
    verify = "no field name suggestions outside entity blocks"
)]
fn no_fields_outside_entity_blocks() {
    let found = complete_in_project(TWO_KINDS, 3, 0);
    assert!(
        found
            .iter()
            .all(|i| i.kind != Some(CompletionItemKind::FIELD)),
        "{:?}",
        labels(&found)
    );
}

#[spec(
    behavior = "complete_keywords",
    verify = "keyword completion includes all registered kinds"
)]
fn registered_kinds_at_top_level() {
    let (_dir, project) = compiled(&[("test.spec", TWO_KINDS)]);
    let view = ProjectView::of(&project);
    let (site, found) = complete(TWO_KINDS, 3, 0, &view);
    assert_eq!(site, "Keywords { prefix: \"\" }");
    let names = labels(&found);
    for kind in view.registries().kinds().keywords() {
        assert!(names.contains(&kind.as_str()), "{kind} in {names:?}");
    }
    let behavior = found.iter().find(|i| i.label == "behavior").unwrap();
    assert_eq!(behavior.detail.as_deref(), Some("@specforge/software"));
}

#[spec(
    behavior = "complete_keywords",
    verify = "use is always suggested and define never is"
)]
fn use_is_suggested_and_define_never_is() {
    // With and without extensions.
    let empty = Assembled::over(RegistryBuild::default());
    let bare = ProjectView::of(empty.detached());
    let found = complete("\n", 0, 0, &bare).1;
    assert_eq!(labels(&found), ["defaults", "use"]);
    let found = complete_in_project("\n", 0, 0);
    let names = labels(&found);
    assert!(names.contains(&"use"), "{names:?}");
    assert!(!names.contains(&"define"), "{names:?}");
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(names, sorted, "sorted, no duplicates");
}

#[test]
fn no_keywords_inside_a_block() {
    let found = complete_in_project(TWO_KINDS, 1, 2);
    assert!(
        found
            .iter()
            .all(|i| i.kind != Some(CompletionItemKind::KEYWORD)),
        "{:?}",
        labels(&found)
    );
}

#[spec(
    behavior = "complete_field_names",
    verify = "a bracket inside a string opens no reference list"
)]
fn a_bracket_in_a_string_opens_no_reference_list() {
    let text = "behavior alpha \"Alpha\" {\n  contract \"see [docs\"\n  \n}\n";
    let (site, found) = complete(
        text,
        2,
        2,
        &ProjectView::of(&compiled(&[("test.spec", text)]).1),
    );
    assert_eq!(
        site, "Fields { kind: \"behavior\", prefix: \"\" }",
        "the string's `[` opens nothing"
    );
    assert!(labels(&found).contains(&"refs"), "{:?}", labels(&found));
}

#[spec(
    behavior = "complete_field_names",
    verify = "nothing is suggested inside a string, a comment or a nested block"
)]
fn nothing_completes_in_strings_comments_or_nested_blocks() {
    let text = concat!(
        "behavior eps \"Eps\" {\n",
        "  requires {\n",
        "    \n",
        "  }\n",
        "  description \"the \"\n",
        "  // a note \n",
        "}\n",
        "define widget {\n",
        "  \n",
        "}\n",
    );
    let (_dir, project) = compiled(&[("test.spec", text)]);
    let view = ProjectView::of(&project);
    for (line, character) in [(2, 4), (4, 19), (5, 7), (8, 2), (0, 10)] {
        let (site, found) = complete(text, line, character, &view);
        assert_eq!(site, "Nothing", "{line}:{character}");
        assert!(found.is_empty(), "{line}:{character}: {:?}", labels(&found));
    }
}

#[spec(
    behavior = "autocomplete_entity_ids",
    verify = "suggestions filtered by target_kind when FieldRegistry has constraint"
)]
fn list_items_complete_ids_of_the_target_kind() {
    let text = concat!(
        "type token \"Token\" {}\n",
        "invariant tidy \"Tidy\" {\n  guarantee \"x\"\n}\n",
        "behavior login \"Login\" {\n  types []\n}\n",
    );
    let found = complete_in_project(text, 5, 9);
    assert_eq!(labels(&found), ["token"]);
    assert!(all_of_kind(&found, CompletionItemKind::REFERENCE));
}

#[test]
fn the_prefix_is_the_word_before_the_cursor() {
    let text = "behavior login \"Login\" {\n  invariants []\n}\n";
    // Inside `inv|ariants`: the prefix is `inv`, not the whole word.
    let doc = Document::new(text.into());
    let cursor = doc.at(Position::new(1, 5)).unwrap();
    assert_eq!(
        cursor.completion(),
        CompletionSite::Fields {
            kind: "behavior",
            prefix: "inv"
        }
    );
    let edit = cursor.word_edit();
    assert_eq!(
        (edit.insert.start, edit.insert.end),
        (Position::new(1, 2), Position::new(1, 5))
    );
    assert_eq!(
        (edit.replace.start, edit.replace.end),
        (Position::new(1, 2), Position::new(1, 12))
    );
    let found = complete_in_project(text, 1, 5);
    assert_eq!(labels(&found), ["invariants"]);
    let Some(CompletionTextEdit::Edit(edit)) = &found[0].text_edit else {
        panic!("a plain edit: {:?}", found[0].text_edit);
    };
    assert_eq!(
        (edit.range.start, edit.range.end),
        (Position::new(1, 2), Position::new(1, 5))
    );
    assert_eq!(edit.new_text, "invariants [$1]");
    assert_eq!(found[0].filter_text.as_deref(), Some("invariants"));
}

/// A registry of one kind `task` (verify kinds `unit`, `contract`) with
/// an enum field `state`, a boolean `done`, a string list `tags` and a
/// reference `owner` (to kind `task`); a graph of `alpha`, a task, and
/// `gh.issue:7`, a ref.
struct Fixture {
    project: Assembled,
}

impl Fixture {
    fn new() -> Fixture {
        let registries = registries("@test/ext", |c| {
            c.kind("task", |k| {
                k.testable(true)
                    .supports_verify(true)
                    .verify_kinds(&["unit", "contract"]);
                let mut field = |name: &str,
                                 field_type: FieldType,
                                 target: Option<&str>,
                                 values: &'static [&'static str]| {
                    k.field(name, |f| {
                        if values.is_empty() {
                            f.field_type(field_type);
                        } else {
                            f.vocabulary(&Vocabulary::new(values));
                        }
                        f.description(&format!("the {name}"));
                        if let Some(target) = target {
                            f.target_kind(target);
                        }
                    });
                };
                field("state", FieldType::Enum, None, &["draft", "active", "done"]);
                field("done", FieldType::Bool, None, &[]);
                field("tags", FieldType::StringList, None, &[]);
                field("owner", FieldType::Reference, Some("task"), &[]);
            });
        });
        let mut graph = Graph::new();
        for (id, kind) in [("alpha", "task"), ("gh.issue:7", "ref")] {
            graph.add_node(Node {
                id: EntityId { raw: Sym::new(id) },
                kind: EntityKind {
                    raw: Sym::new(kind),
                },
                title: None,
                fields: FieldMap::new(),
                source_span: SourceSpan {
                    file: Sym::new("f.spec"),
                    start_line: 1,
                    start_col: 1,
                    end_line: 1,
                    end_col: 1,
                },
                methods: Vec::new(),
                obligations: Vec::new(),
            });
        }
        Fixture {
            project: Assembled::over(registries).graph(graph),
        }
    }

    fn view(&self) -> ProjectView<'_> {
        ProjectView::of(self.project.detached())
    }
}

#[spec(
    behavior = "complete_field_names",
    verify = "an enum field's value suggests its declared values"
)]
fn an_enum_value_suggests_its_declared_values() {
    let fixture = Fixture::new();
    let text = "task t \"T\" {\n  state \n  state d\n}\n";
    let (site, found) = complete(text, 1, 8, &fixture.view());
    assert_eq!(
        site,
        "Value { kind: \"task\", field: \"state\", prefix: \"\" }"
    );
    assert_eq!(labels(&found), ["draft", "active", "done"]);
    assert!(all_of_kind(&found, CompletionItemKind::ENUM_MEMBER));
    assert_eq!(found[0].detail.as_deref(), Some("the state"));
    let found = complete(text, 2, 9, &fixture.view()).1;
    assert_eq!(labels(&found), ["draft", "done"]);
}

#[spec(
    behavior = "complete_field_names",
    verify = "a builtin vocabulary's field completes its values"
)]
fn a_builtin_vocabulary_completes_its_values() {
    let text = "feature f \"F\" {\n  status \n}\n";
    let dir = TestProject::new()
        .config(|c| c["name"] = "vocab".into())
        .enabling(&["@specforge/product"])
        .file("test.spec", text);
    let runtime = specforge_component::ComponentRuntime::new();
    specforge_component::builtins::load_builtins_for(&runtime, &["@specforge/product".to_string()])
        .unwrap();
    let project = dir.compile_in(Arc::new(runtime));
    let found = complete(text, 1, 9, &ProjectView::of(&project)).1;
    assert_eq!(
        labels(&found),
        [
            "proposed",
            "accepted",
            "in_progress",
            "done",
            "deferred",
            "deprecated"
        ]
    );
    assert!(all_of_kind(&found, CompletionItemKind::ENUM_MEMBER));
}

#[spec(
    behavior = "complete_field_names",
    verify = "a boolean field's value suggests true and false"
)]
fn a_bool_value_suggests_true_and_false() {
    let fixture = Fixture::new();
    let text = "task t \"T\" {\n  done \n}\n";
    let found = complete(text, 1, 7, &fixture.view()).1;
    assert_eq!(labels(&found), ["true", "false"]);
    assert!(all_of_kind(&found, CompletionItemKind::KEYWORD));
}

#[spec(
    behavior = "autocomplete_entity_ids",
    verify = "a string list's items suggest no entity IDs but scheme ref IDs"
)]
fn a_string_list_suggests_no_ids() {
    let fixture = Fixture::new();
    let text = "task t \"T\" {\n  tags []\n}\n";
    let (site, found) = complete(text, 1, 8, &fixture.view());
    assert_eq!(
        site,
        "ListItem { kind: \"task\", field: \"tags\", prefix: \"\" }"
    );
    // No entity's ID; only a ref's scheme ref ID, which the core links to
    // its ref from any list.
    assert_eq!(labels(&found), ["gh.issue:7"]);
}

#[spec(
    behavior = "complete_keywords",
    verify = "verify suggests the kinds the entity's kind allows"
)]
fn verify_suggests_the_kinds_the_entity_allows() {
    let fixture = Fixture::new();
    let text = "task t \"T\" {\n  verify \n  verify c\n}\nother o \"O\" {\n  verify \n}\n";
    let (site, found) = complete(text, 1, 9, &fixture.view());
    assert_eq!(site, "VerifyKind { kind: \"task\", prefix: \"\" }");
    assert_eq!(labels(&found), ["unit", "contract"]);
    assert!(all_of_kind(&found, CompletionItemKind::ENUM_MEMBER));
    assert_eq!(
        labels(&complete(text, 2, 10, &fixture.view()).1),
        ["contract"]
    );
    // A kind the registry does not know takes no verify statements here.
    assert!(complete(text, 5, 9, &fixture.view()).1.is_empty());
}

#[test]
fn a_single_reference_value_completes_its_target_kinds_ids() {
    let fixture = Fixture::new();
    let text = "task t \"T\" {\n  owner \n}\n";
    let found = complete(text, 1, 8, &fixture.view()).1;
    assert_eq!(labels(&found), ["alpha"]);
}

/// Holds a project's indexing where it reads `path`, a named pipe named
/// `blocker.spec`: reading it blocks until something writes to it, so a
/// test decides when indexing may end. Dropping the hold releases it.
#[cfg(unix)]
struct IndexingHold(std::path::PathBuf);

#[cfg(unix)]
impl IndexingHold {
    fn new(dir: &std::path::Path) -> Self {
        let path = dir.join("blocker.spec");
        let made = std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .expect("mkfifo runs");
        assert!(made.success(), "mkfifo {}", path.display());
        IndexingHold(path)
    }

    /// Let every read of the pipe finish (an empty file each: the project
    /// stamps a file written this second by its content, then reads it).
    fn release(&self) {
        let path = self.0.clone();
        // Opening a pipe for writing waits for a reader: off this thread, so
        // a reader that never comes cannot hang the test, and once more
        // after each reader that did.
        std::thread::spawn(
            move || {
                while std::fs::OpenOptions::new().write(true).open(&path).is_ok() {}
            },
        );
    }
}

#[cfg(unix)]
impl Drop for IndexingHold {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(unix)]
#[spec(
    behavior = "complete_keywords",
    verify = "keyword completion answers with the registered kinds as soon as the environment is loaded, before indexing ends"
)]
#[tokio::test]
async fn keywords_complete_from_the_environment_while_indexing_runs() {
    use crate::session::Session;
    use serde_json::json;
    use std::time::Duration;

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        json!({"name": "t", "version": "0.1.0", "extensions": ["@specforge/software"]}).to_string(),
    )
    .unwrap();
    let hold = IndexingHold::new(dir.path());

    // The server opens the project: the environment loads, then indexing
    // blocks reading the pipe.
    let root = dir.path().to_str().unwrap();
    let (mut session, _) = Session::launch(Some(root), json!({})).await;
    let uri = crate::session::uri_of(&dir.path().join("main.spec"));
    session.did_open(&uri, "specforge", "\n").await;

    // Asked until the environment is shown (the extensions take a moment
    // to load): the answer then names the kinds, and indexing has not
    // ended, because it cannot.
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let labels = loop {
        let reply = session.completion(&uri, 0, 0).await;
        let labels: Vec<String> = reply["result"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| item["label"].as_str().map(str::to_string))
            .collect();
        if labels.iter().any(|label| label == "behavior") {
            break labels;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no kind completed while indexing: {labels:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert!(labels.iter().any(|label| label == "use"), "{labels:?}");
    assert!(!labels.iter().any(|label| label == "define"), "{labels:?}");
    let ended = session
        .notification_within("$/progress", Duration::ZERO, |p| {
            p["value"]["kind"] == "end"
        })
        .await;
    assert!(ended.is_none(), "indexing ended while it was held");

    // Released, indexing ends, and the same completion still answers.
    hold.release();
    session
        .notification("$/progress", |p| p["value"]["kind"] == "end")
        .await
        .expect("indexing ends once the pipe is read");
    let reply = session.completion(&uri, 0, 0).await;
    let after: Vec<&str> = reply["result"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["label"].as_str())
        .collect();
    assert!(after.contains(&"behavior"), "{after:?}");
}

#[spec(
    behavior = "complete_keywords",
    verify = "a kind keyword's completion documents the kind with its description and inference guide"
)]
fn a_kind_keyword_is_documented_by_its_guide() {
    let (_dir, project) = compiled(&[("test.spec", TWO_KINDS)]);
    let view = ProjectView::of(&project);
    let (_, found) = complete(TWO_KINDS, 3, 0, &view);

    let behavior = found.iter().find(|i| i.label == "behavior").unwrap();
    let Some(Documentation::MarkupContent(markup)) = &behavior.documentation else {
        panic!("behavior has no markdown documentation: {behavior:?}");
    };
    assert_eq!(markup.kind, MarkupKind::Markdown);
    assert!(
        markup
            .value
            .starts_with("A testable unit of system functionality with a defined contract"),
        "{}",
        markup.value
    );
    assert!(markup.value.contains("**Inferring it from code**"));
    assert!(markup.value.contains("Look for public functions"));

    let use_keyword = found.iter().find(|i| i.label == "use").unwrap();
    assert!(use_keyword.documentation.is_none());
}
