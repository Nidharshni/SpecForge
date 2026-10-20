//! One finder answers which tokens name an entity (ADR 0163): rename, the navigator's tokens and a delete's
//! referrer scan read a default no block takes, and a type name a derived field reads.

use crate::edit::{project, refused};
use crate::navigate::{SOFTWARE, at, compile};
use specforge_ops::authoring::edit::{self, Edit};
use specforge_ops::authoring::rename;
use specforge_ops::read::navigate::{Direction, ReferenceQuery};
use specforge_ops::view::ProjectView;
use specforge_test_macros::test as specforge_test;
use std::collections::BTreeSet;

const INCOMING: ReferenceQuery = ReferenceQuery {
    direction: Direction::Incoming,
    include_declaration: false,
};

const RULES: &str = "invariant inv_a \"A\" {\n  guarantee \"g\"\n}\n";
/// A file whose only block is a default no block takes (W161).
const FLOWS: &str = "defaults behavior {\n  invariants [inv_a]\n}\n";

#[specforge_test(
    behavior = "apply_file_defaults",
    verify = "rename edits a default no block takes"
)]
fn rename_edits_a_default_no_block_takes() {
    let p = compile(SOFTWARE, &[("rules.spec", RULES), ("flows.spec", FLOWS)]);
    let plan = rename::plan(&p.navigator(), "inv_a", "inv_b").unwrap();
    let files = plan.affected_files();
    assert_eq!(
        files.into_iter().collect::<Vec<_>>(),
        ["flows.spec", "rules.spec"]
    );
    let flows = plan
        .edits
        .iter()
        .find(|e| e.span.file == "flows.spec")
        .unwrap();
    let span = &flows.span;
    assert_eq!((span.start_line, span.start_col, span.end_col), (2, 15, 20));
}

#[test]
fn tokens_hold_a_default_no_block_takes() {
    let p = compile(SOFTWARE, &[("rules.spec", RULES), ("flows.spec", FLOWS)]);
    let nav = p.navigator();
    let mut found: Vec<String> = nav
        .tokens("inv_a", true)
        .unwrap()
        .iter()
        .map(|t| at(&t.span))
        .collect();
    found.sort();
    assert_eq!(found, ["flows.spec 2:15-2:20", "rules.spec 1:11-1:16"]);
    // No block holds it: the holders stay on the graph's edges (ADR 0163).
    assert!(nav.references("inv_a", INCOMING).unwrap().is_empty());
}

const MEMBER: &str =
    "type User \"A user\" {\n  name string\n}\n\ntype Order \"An order\" {\n  owner User\n}\n";
const SIGNATURE: &str = "type User \"A user\" {\n  name string\n}\n\nport Users \"Users\" {\n  direction outbound\n  \
                         category  \"db\"\n  method find(id: string) -> User\n}\n";

/// Whether `line` writes `word` as a whole word.
fn spells(line: &str, word: &str) -> bool {
    line.match_indices(word).any(|(at, _)| {
        let edge = |c: Option<char>| c.is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        edge(line[..at].chars().next_back()) && edge(line[at + word.len()..].chars().next())
    })
}

/// The `(file, line)` of every other place `target` is written, read off the texts.
fn written(
    files: &[(&str, &str)],
    target: &str,
    declared: (&str, usize),
) -> BTreeSet<(String, usize)> {
    let mut found = BTreeSet::new();
    for (file, text) in files {
        let lines: Vec<&str> = text.lines().collect();
        // The declaration: a block through its closing brace, a `ref` on its one line.
        let block_end = (*file == declared.0)
            .then(|| {
                let open = lines[declared.1 - 1].ends_with('{');
                (declared.1..=lines.len()).find(|n| !open || lines[n - 1] == "}")
            })
            .flatten();
        for (n, line) in lines.iter().enumerate().map(|(i, l)| (i + 1, l)) {
            let in_block = block_end.is_some_and(|end| (declared.1..=end).contains(&n));
            if !in_block && spells(line, target) {
                found.insert((file.to_string(), n));
            }
        }
    }
    found
}

/// (name, files, target, declaration, whether a delete plans).
type Row<'a> = (
    &'a str,
    Vec<(&'a str, &'a str)>,
    &'a str,
    (&'a str, usize),
    bool,
);

/// Rename and delete answer the same tokens: what a rename edits besides the declaration is what a delete with
/// `unlink_references` removes, or, where no typed edit writes it, names in its refusal (ADR 0163).
#[test]
fn rename_and_delete_find_the_same_tokens() {
    const B: &str = "behavior b \"B\" {\n  category c\n";
    let own_list = format!("{B}  invariants [inv_a, inv_b]\n}}\n");
    let one_value = format!("{B}  invariants inv_a\n}}\n");
    let untaken = "defaults behavior {\n  invariants [inv_a]\n}\n";
    let taken = format!("{untaken}\n{B}}}\n");
    let undeclared = format!("{B}  watchers [inv_a]\n}}\n");
    let rules = "invariant inv_a \"A\" {\n  guarantee \"g\"\n}\n\ninvariant inv_b \"B\" {\n  guarantee \"g\"\n}\n";
    let events = "type Task \"Task\" {\n  id string\n}\n\nevent task_done \"Done Event\" {\n  payload Task\n}\n";
    let scheme =
        format!("ref gh.issue:42 \"Issue 42\"\n\n{B}  refs [gh.issue:42, \"RFC 1\"]\n}}\n");
    let table: Vec<Row> = vec![
        (
            "own list",
            vec![("rules.spec", rules), ("b.spec", &own_list)],
            "inv_a",
            ("rules.spec", 1),
            true,
        ),
        (
            "one-value list",
            vec![("rules.spec", rules), ("b.spec", &one_value)],
            "inv_a",
            ("rules.spec", 1),
            true,
        ),
        (
            "single reference",
            vec![("a.spec", events)],
            "Task",
            ("a.spec", 1),
            true,
        ),
        (
            "scheme ref",
            vec![("a.spec", &scheme)],
            "gh.issue:42",
            ("a.spec", 1),
            true,
        ),
        (
            "taken default",
            vec![("rules.spec", rules), ("flows.spec", &taken)],
            "inv_a",
            ("rules.spec", 1),
            false,
        ),
        (
            "untaken default",
            vec![("rules.spec", rules), ("flows.spec", untaken)],
            "inv_a",
            ("rules.spec", 1),
            false,
        ),
        (
            "undeclared field",
            vec![("rules.spec", rules), ("b.spec", &undeclared)],
            "inv_a",
            ("rules.spec", 1),
            false,
        ),
        (
            "member type",
            vec![("a.spec", MEMBER)],
            "User",
            ("a.spec", 1),
            false,
        ),
        (
            "signature type",
            vec![("a.spec", SIGNATURE)],
            "User",
            ("a.spec", 1),
            false,
        ),
    ];
    for (name, files, target, declared, plans) in table {
        let c = project(&files);
        let expected = written(&files, target, declared);
        let renamed: BTreeSet<(String, usize)> = rename::plan(&c.navigator(), target, "zz_new")
            .unwrap_or_else(|e| panic!("{name}: {e:?}"))
            .edits
            .iter()
            .map(|e| (e.span.file.to_string(), e.span.start_line))
            .filter(|(file, line)| (file.as_str(), *line) != declared)
            .collect();
        assert_eq!(renamed, expected, "{name}: rename");
        let delete = [Edit::Delete {
            entity_id: target.into(),
            unlink_references: true,
        }];
        if plans {
            let planned = edit::plan(&ProjectView::of(&c.project), &delete)
                .unwrap_or_else(|e| panic!("{name}: {e:?}"));
            let left = planned.after();
            let texts: Vec<(&str, &str)> =
                left.iter().map(|(f, t)| (f.as_str(), t.as_str())).collect();
            assert!(
                written(&texts, target, ("", 0)).is_empty(),
                "{name}: {left:?}"
            );
        } else {
            let (code, message) = refused(&c, &delete);
            assert_eq!(code, "not_editable", "{name}: {message}");
            let named: BTreeSet<(String, usize)> = message
                .split(" (")
                .filter_map(|part| part.rsplit_once(':'))
                .filter_map(|(file, line)| {
                    let file = file.rsplit([' ', ',']).next()?;
                    Some((file.to_string(), line.trim().parse().ok()?))
                })
                .collect();
            assert_eq!(named, expected, "{name}: delete named {message}");
        }
    }
}

const TYPE_WORD: &str = "type Customer \"Customer\" {\n  id string\n}\n\ntype Invoice \"Invoice\" {\n  refs Customer\n}\n";

#[test]
fn rename_and_find_references_agree_on_a_word_a_string_list_holds() {
    let p = compile(SOFTWARE, &[("a.spec", TYPE_WORD)]);
    let nav = p.navigator();
    assert!(nav.tokens("Customer", false).unwrap().is_empty());
    assert!(nav.references("Customer", INCOMING).unwrap().is_empty());
    let plan = rename::plan(&nav, "Customer", "Store").unwrap();
    assert_eq!(plan.edits.len(), 1);
    assert_eq!(plan.edits[0].span.start_line, 1);
}
