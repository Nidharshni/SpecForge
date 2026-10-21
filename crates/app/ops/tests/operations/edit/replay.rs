//! The todo-app rebuilt through typed edits alone (KPI 3): every block it can express is deleted by a typed edit,
//! created again by one, and the project has the graph it had.

use super::*;
use specforge_graph::{FieldValue, Node, compute_graph_delta};
use specforge_ops::authoring::edit::Obligation;
use specforge_project::testing::TestProject;
use std::collections::BTreeMap;

/// The input a `Create` takes for `value`; `None` for a value a typed edit does not write (an expression, a
/// union's variants).
fn input(value: &FieldValue) -> Option<Input> {
    Some(match value {
        FieldValue::String(s) | FieldValue::Identifier(s) | FieldValue::Date(s) => {
            Input::One(s.clone())
        }
        FieldValue::Integer(n) => Input::One(n.to_string()),
        FieldValue::Boolean(b) => Input::One(b.to_string()),
        FieldValue::List(items) => Input::Many(items.iter().map(|i| i.text.clone()).collect()),
        FieldValue::Block(map) => Input::Entries(
            map.entries()
                .iter()
                .map(|e| match &e.value {
                    FieldValue::String(s) => Some((e.key.to_string(), s.clone())),
                    _ => None,
                })
                .collect::<Option<BTreeMap<_, _>>>()?,
        ),
        _ => return None,
    })
}

/// The `Create` that rebuilds `node` from its own fields (not the ones its file's defaults give it), or `None`
/// for a block typed edits cannot write yet: the `spec` block, a port with methods, a union.
fn create_of(node: &Node) -> Option<Vec<Edit>> {
    if !node.methods.is_empty() || node.kind.raw.as_str() == "spec" {
        return None;
    }
    let own = |span: &Option<specforge_common::SourceSpan>| {
        span.as_ref().is_some_and(|s| {
            s.file == node.source_span.file
                && s.start_line >= node.source_span.start_line
                && s.end_line <= node.source_span.end_line
        })
    };
    let mut fields = BTreeMap::new();
    let mut annotated = Vec::new();
    for entry in node.fields.entries().iter().filter(|e| own(&e.value_span)) {
        let value = input(&entry.value)?;
        if !entry.annotations.is_empty() {
            // A create writes no annotation; a set does.
            let annotations = entry
                .annotations
                .iter()
                .map(|a| match &a.value {
                    Some(v) => format!("{}={v}", a.name),
                    None => a.name.clone(),
                })
                .collect();
            annotated.push(Edit::Set {
                entity_id: node.id.raw.to_string(),
                field: entry.key.to_string(),
                value: value.clone(),
                annotations: Some(annotations),
            });
        }
        fields.insert(entry.key.to_string(), value);
    }
    let create = Edit::Create {
        kind: node.kind.raw.to_string(),
        id: node.id.raw.to_string(),
        title: node.title.clone()?,
        file: node.source_span.file.to_string(),
        fields,
        verify: node
            .obligations
            .iter()
            .map(|o| Obligation {
                kind: (!o.kind.is_empty()).then(|| o.kind.clone()),
                text: o.description.clone(),
            })
            .collect(),
    };
    Some(std::iter::once(create).chain(annotated).collect())
}

#[specforge_test_macros::test(
    behavior = "validate_typed_edit",
    verify = "every block an edit touches is format-clean"
)]
fn the_todo_app_replays_through_typed_edits() {
    let todo = TestProject::copy_of(specforge_repo_root::repo_root().join("examples/todo-app"));
    let original = todo.compile();
    let nodes = original.graph().nodes();
    let total = nodes.len();
    let creates: Vec<(String, Vec<Edit>)> = nodes
        .iter()
        .filter_map(|n| Some((n.id.raw.to_string(), create_of(n)?)))
        .collect();
    let replayed = creates.len();
    let caller = || specforge_ops::testing::caller();

    // The blocks typed edits cannot write yet (a port with methods, a union) are kept as they are written.
    let kept: Vec<(String, String)> = nodes
        .iter()
        .filter(|n| !creates.iter().any(|(id, _)| id == n.id.raw.as_str()))
        .map(|n| {
            let file = n.source_span.file.to_string();
            let text = original.source_text(&file).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            let block = lines[n.source_span.start_line - 1..n.source_span.end_line].join("\n");
            (file, block)
        })
        .collect();

    // Remove every block, then put the kept ones back as text.
    let removal: Vec<Edit> = nodes
        .iter()
        .map(|n| Edit::Delete {
            entity_id: n.id.raw.to_string(),
            unlink_references: false,
        })
        .collect();
    let plan = edit::plan(&ProjectView::of(&original), &removal).unwrap();
    plan.change().apply(caller()).unwrap();
    let spec_root = original.environment().spec_root().to_path_buf();
    for (file, block) in &kept {
        let path = spec_root.join(file);
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, format!("{}\n\n{block}\n", text.trim_end())).unwrap();
    }
    let emptied = todo.compile();
    assert_eq!(emptied.graph().nodes().len(), total - replayed);

    // Write them back, in one plan.
    let creation: Vec<Edit> = creates.into_iter().flat_map(|(_, e)| e).collect();
    let plan = edit::plan(&ProjectView::of(&emptied), &creation).unwrap();
    plan.change().apply(caller()).unwrap();
    let rebuilt = todo.compile();

    // The review document says the same, line for line, but for where each block sits (a `file:line` line of its
    // own) and the order of a block's fields (a created block writes them in the kind's declared order).
    let review = |project: &specforge_project::CompiledProject| {
        let request = specforge_ops::read::export::Request {
            format: Some(specforge_ops::read::export::Format::Markdown),
            ..Default::default()
        };
        let document = specforge_ops::read::export::export(&ProjectView::of(project), &request)
            .unwrap()
            .into_value_and_notices()
            .0;
        let mut lines: Vec<String> = document
            .lines()
            .filter(|line| !(line.starts_with('`') && line.contains(".spec:")))
            .map(String::from)
            .collect();
        lines.sort();
        lines
    };
    assert_eq!(review(&original), review(&rebuilt));

    let delta = compute_graph_delta(original.graph(), rebuilt.graph());
    assert!(delta.is_empty(), "the rebuilt graph differs: {delta:?}");
    assert!(
        replayed >= total - 3,
        "{replayed} of {total} blocks were replayed (all but the `spec` block, a union and a port with methods)"
    );
    let forms = rebuilt.environment().registries().field_forms();
    for key in rebuilt.source_keys() {
        let text = rebuilt.source_text(key).unwrap();
        let formatted = specforge_formatter::format_source(
            &text,
            &specforge_formatter::FormatConfig::default(),
            forms,
        );
        assert_eq!(formatted.formatted, &*text, "{key} is not canonical");
    }
}
