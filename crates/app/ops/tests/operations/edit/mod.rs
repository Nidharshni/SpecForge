//! Typed edits (`specforge_ops::authoring::edit`) over projects compiled in a temp dir.

mod apply;
mod create;
mod defaults;
mod delete;
mod fields;
mod holds;
mod inherited;
mod layout;
mod links;
mod obligations;
mod paths;
mod places;
mod positions;
mod replay;
mod sides;
mod skeleton;
mod validate;

use crate::navigate::{Compiled, SOFTWARE, compile};
use specforge_ops::Refusal;
use specforge_ops::authoring::edit::{self, Edit, EditField, Input, Obligation};
use specforge_ops::view::ProjectView;
use std::collections::BTreeMap;

pub const TASKS: &str = r#"// The tasks.

invariant task_unique "Task Unique" {
  guarantee "ids are unique"
}

invariant done_stamped "Done Stamped" {
  guarantee "done tasks carry a time"
}

invariant extra_rule "Extra Rule" {
  guarantee "something else"
}

type Task "Task" {
  id string
}

behavior complete_task "Complete a Task" {
  category   command
  invariants [done_stamped]
  // what completing touches
  types      [Task]
  contract   """
    When completed, the system MUST set the status.
  """
  verify unit "completing sets the status"
  verify integration "an event fires"
}

behavior list_tasks "List Tasks" {
  category query
  types    [Task]
  contract """
    When asked, the system MUST list.
  """
}
"#;

pub fn project(files: &[(&str, &str)]) -> Compiled {
    compile(SOFTWARE, files)
}

pub fn plan(c: &Compiled, edits: &[Edit]) -> Result<edit::EditPlan, Refusal<EditField>> {
    edit::plan(&ProjectView::of(&c.project), edits)
}

/// The text of `file` once `edits` are planned.
pub fn after(c: &Compiled, file: &str, edits: &[Edit]) -> String {
    let planned = plan(c, edits).unwrap();
    match planned.change().after().remove(file) {
        Some(text) => text,
        None => c.project.source_text(file).unwrap().to_string(),
    }
}

/// The refusal code of `edits`.
pub fn refused(c: &Compiled, edits: &[Edit]) -> (String, String) {
    let error = plan(c, edits).unwrap_err().error;
    (error.code.to_string(), error.message)
}

pub fn one(text: &str) -> Input {
    Input::One(text.to_string())
}

pub fn many(items: &[&str]) -> Input {
    Input::Many(items.iter().map(|i| i.to_string()).collect())
}

pub fn set(id: &str, field: &str, value: Input) -> Edit {
    Edit::Set {
        entity_id: id.to_string(),
        field: field.to_string(),
        value,
        annotations: None,
    }
}

pub fn unset(id: &str, field: &str) -> Edit {
    Edit::Unset {
        entity_id: id.to_string(),
        field: field.to_string(),
    }
}

pub fn link(id: &str, field: &str, target: &str) -> Edit {
    Edit::Link {
        entity_id: id.to_string(),
        field: field.to_string(),
        target: target.to_string(),
    }
}

pub fn unlink(id: &str, field: &str, target: &str) -> Edit {
    Edit::Unlink {
        entity_id: id.to_string(),
        field: field.to_string(),
        target: target.to_string(),
    }
}

pub fn create(kind: &str, id: &str, file: &str, fields: &[(&str, Input)]) -> Edit {
    Edit::Create {
        kind: kind.into(),
        id: id.into(),
        title: "Archive a Task".into(),
        file: file.into(),
        fields: fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<BTreeMap<_, _>>(),
        verify: vec![Obligation {
            kind: Some("unit".into()),
            text: "archiving an unknown task fails".into(),
        }],
    }
}

/// Whether formatting `text` changes it.
pub fn is_canonical(c: &Compiled, text: &str) -> bool {
    let forms = c.project.environment().registries().field_forms();
    let formatted = specforge_formatter::format_source(
        text,
        &specforge_formatter::FormatConfig::default(),
        forms,
    );
    formatted.formatted == text
}
