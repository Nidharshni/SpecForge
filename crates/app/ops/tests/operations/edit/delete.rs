use super::*;

fn delete(id: &str, unlink_references: bool) -> Edit {
    Edit::Delete {
        entity_id: id.into(),
        unlink_references,
    }
}

const TWO_FILES: [(&str, &str); 2] = [
    (
        "rules.spec",
        "// the rules\n\ninvariant task_unique \"Task Unique\" {\n  guarantee \"g\"\n}\n\n// stamping\ninvariant done_stamped \"Done Stamped\" {\n  guarantee \"h\"\n}\n",
    ),
    (
        "tasks.spec",
        "behavior complete_task \"Complete\" {\n  category   command\n  invariants [done_stamped, task_unique]\n  refines    list_tasks\n}\n\nbehavior list_tasks \"List\" {\n  category    query\n  invariants  [done_stamped]\n}\n",
    ),
];

#[specforge_test_macros::test(
    behavior = "delete_entity",
    verify = "delete removes the block and the comment above it"
)]
fn delete_removes_the_block_comment_and_one_blank_line() {
    let c = project(&TWO_FILES);
    let text = after(&c, "rules.spec", &[delete("done_stamped", true)]);
    assert_eq!(
        text,
        "// the rules\n\ninvariant task_unique \"Task Unique\" {\n  guarantee \"g\"\n}\n"
    );
    let text = after(&c, "rules.spec", &[delete("task_unique", true)]);
    assert_eq!(
        text,
        "// the rules\n\n// stamping\ninvariant done_stamped \"Done Stamped\" {\n  guarantee \"h\"\n}\n"
    );
}

#[specforge_test_macros::test(
    behavior = "delete_entity",
    verify = "delete with unlink_references removes every reference"
)]
fn delete_with_unlink_references_plans_every_referrer() {
    let c = project(&TWO_FILES);
    let planned = plan(&c, &[delete("done_stamped", true)]).unwrap();
    assert_eq!(
        planned.report().affected_files,
        vec!["rules.spec", "tasks.spec"]
    );
    assert_eq!(
        planned.report().entities,
        vec!["complete_task", "done_stamped", "list_tasks"]
    );
    let tasks = &planned.after()["tasks.spec"];
    assert!(tasks.contains("invariants [task_unique]"), "{tasks}");
    assert!(!tasks.contains("done_stamped"), "{tasks}");
    assert!(is_canonical(&c, tasks));
}

#[test]
fn delete_unsets_a_single_reference_field() {
    let events = "type Task \"Task\" {\n  id string\n}\n\nevent task_done \"Task Done\" {\n  payload Task\n}\n";
    let c = project(&[("events.spec", events)]);
    let planned = plan(&c, &[delete("Task", true)]).unwrap();
    assert_eq!(
        planned.after()["events.spec"],
        "event task_done \"Task Done\" {\n}\n"
    );
}

#[test]
fn delete_of_an_entity_that_does_not_exist_is_refused() {
    let c = project(&TWO_FILES);
    assert_eq!(refused(&c, &[delete("nope", false)]).0, "E003");
    assert_eq!(
        refused(
            &c,
            &[delete("task_unique", false), delete("task_unique", false)]
        )
        .0,
        "E003"
    );
}

const SCHEME: &str = "ref gh.issue:42 \"Issue 42\"\n\nbehavior b1 \"B1\" {\n  category c\n  refs     [gh.issue:42, \"RFC 1\"]\n}\n";
const ONE_VALUE: &str = "invariant inv_a \"A\" {\n  guarantee \"x\"\n}\n\nbehavior b4 \"B4\" {\n  category   c\n  invariants inv_a\n}\n";
const UNDECLARED: &str = "invariant inv_a \"A\" {\n  guarantee \"x\"\n}\n\nbehavior b9 \"B9\" {\n  category c\n  watchers [inv_a]\n}\n";
const DEFAULTED: &str = "defaults behavior {\n  invariants [inv_a]\n}\n\ninvariant inv_a \"A\" {\n  guarantee \"x\"\n}\n\nbehavior b1 \"B1\" {\n  category c\n}\n\nbehavior b2 \"B2\" {\n  category   c\n  invariants [inv_a]\n}\n";

#[specforge_test_macros::test(
    behavior = "delete_entity",
    verify = "delete with unlink_references removes a scheme ref ID from a string list"
)]
fn delete_with_unlink_references_removes_a_scheme_ref_from_a_string_list() {
    let c = project(&[("a.spec", SCHEME)]);
    let text = after(&c, "a.spec", &[delete("gh.issue:42", true)]);
    assert!(text.contains("\"RFC 1\"]"), "{text}");
    assert!(!text.contains("gh.issue:42"), "{text}");
    assert!(is_canonical(&c, &text));
}

#[specforge_test_macros::test(
    behavior = "delete_entity",
    verify = "delete with unlink_references removes a reference list written as one value"
)]
fn delete_with_unlink_references_removes_a_list_written_as_one_value() {
    let c = project(&[("a.spec", ONE_VALUE)]);
    let text = after(&c, "a.spec", &[delete("inv_a", true)]);
    assert!(
        !text.contains("invariants") && !text.contains("inv_a"),
        "{text}"
    );
}

#[test]
fn delete_with_unlink_references_refuses_an_undeclared_field() {
    let c = project(&[("a.spec", UNDECLARED)]);
    let (code, message) = refused(&c, &[delete("inv_a", true)]);
    assert_eq!(code, "not_editable");
    assert!(message.contains("a.spec:7 (b9, watchers)"), "{message}");
}

#[specforge_test_macros::test(
    behavior = "delete_entity",
    verify = "delete with unlink_references refuses a reference no typed edit writes"
)]
fn delete_with_unlink_references_refuses_a_reference_written_in_a_default() {
    let c = project(&[("a.spec", DEFAULTED)]);
    let (code, message) = refused(&c, &[delete("inv_a", true)]);
    assert_eq!(code, "not_editable");
    assert!(
        message.contains("a.spec:2 (defaults behavior, invariants)") && !message.contains("b2"),
        "{message}"
    );
    assert_eq!(refused(&c, &[delete("inv_a", false)]).0, "E080");
}

const MEMBER: &str =
    "type User \"A user\" {\n  name string\n}\n\ntype Order \"An order\" {\n  owner User\n}\n";
const SIGNATURE: &str = "type User \"A user\" {\n  name string\n}\n\nport Users \"Users\" {\n  direction outbound\n  \
                         category  \"db\"\n  method find(id: string) -> User\n}\n";

#[specforge_test_macros::test(
    behavior = "delete_entity",
    verify = "delete with unlink_references refuses a type name a derived field reads"
)]
fn delete_with_unlink_references_refuses_a_type_named_in_a_member() {
    let c = project(&[("a.spec", MEMBER)]);
    let (code, message) = refused(&c, &[delete("User", true)]);
    assert_eq!(code, "not_editable");
    assert!(
        message.contains("a.spec:6 (Order, composed_types)"),
        "{message}"
    );
}

#[test]
fn delete_with_unlink_references_refuses_a_type_named_in_a_signature() {
    let c = project(&[("a.spec", SIGNATURE)]);
    let (code, message) = refused(&c, &[delete("User", true)]);
    assert_eq!(code, "not_editable");
    assert!(message.contains("a.spec:8 (Users, types)"), "{message}");
    assert_eq!(refused(&c, &[delete("User", false)]).0, "E080");
}
