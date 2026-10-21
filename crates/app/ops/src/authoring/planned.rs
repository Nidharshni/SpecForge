//! What a plan of a project's sources will write (ADR 0184): a typed edit's plan and a rename's plan each hold one
//! [`PlannedChange`], which answers the affected files, the after texts, the text edits and the diff, and writes
//! them as one project change (ADR 0128).

use crate::change::ProjectChange;
use crate::read::navigate::{SourceText, TextEdit};
use crate::{Caller, OpError, Writes};
use specforge_common::Sym;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Per file under the spec root: the text it was planned on (`None`: the file is new) and the text it will hold. A
/// file whose after text is its planned-on text is not in it.
///
/// Each planned-on text is the text the project was compiled from; `new` is `pub(crate)`, so only ops' planners make
/// one. Files, edits and the diff are in key order.
#[derive(Debug, Clone)]
pub struct PlannedChange {
    spec_root: PathBuf,
    /// What the operation was doing, the head of a failure's message (`cannot rename 'a'`, ADR 0155).
    doing: String,
    files: BTreeMap<String, (Option<Arc<str>>, String)>,
}

impl PlannedChange {
    /// `files`: (path relative to the spec root, planned-on text, after text); an unchanged one is dropped.
    pub(crate) fn new(
        spec_root: PathBuf,
        doing: impl Into<String>,
        files: impl IntoIterator<Item = (String, Option<Arc<str>>, String)>,
    ) -> Self {
        let files = files
            .into_iter()
            .filter(|(_, before, after)| before.as_deref() != Some(after.as_str()))
            .map(|(key, before, after)| (key, (before, after)))
            .collect();
        PlannedChange {
            spec_root,
            doing: doing.into(),
            files,
        }
    }

    /// Nothing to write.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// The files the change writes, relative to the spec root, in key order.
    pub fn affected_files(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    /// Each file the change writes and the text it will hold, by path relative to the spec root.
    pub fn after(&self) -> BTreeMap<String, String> {
        self.files
            .iter()
            .map(|(key, (_, after))| (key.clone(), after.clone()))
            .collect()
    }

    /// The edits that turn each file's planned-on text into the text it will hold, as navigation's `TextEdit`s:
    /// spans in the planned-on text (a new file's: one insertion at its start), each file's in order, none
    /// overlapping (the formatter's minimal diff, ADR 0021 D8), files in key order.
    pub fn text_edits(&self) -> Vec<TextEdit> {
        let mut edits = Vec::new();
        for (key, (before, after)) in &self.files {
            let before = before.clone().unwrap_or_else(|| Arc::from(""));
            let text = SourceText::new(Arc::clone(&before));
            let file = Sym::new(key);
            edits.extend(
                crate::authoring::format::between(&before, after)
                    .into_iter()
                    .map(|change| TextEdit {
                        span: text.span(file, change.range.start, change.range.end),
                        new_text: change.new_text,
                    }),
            );
        }
        edits
    }

    /// A unified diff of every file, spec-root-relative headers, in key order. Empty when nothing changes.
    pub fn diff(&self) -> String {
        self.files
            .iter()
            .map(|(key, (before, after))| {
                specforge_formatter::unified_diff(key, before.as_deref().unwrap_or(""), after)
                    .diff_text
            })
            .collect()
    }

    /// Write the change as one project change (ADR 0128), each file resolved through the caller's containment (ADR
    /// 0104): checked against the text it was planned on, then replaced or created; on any failure, every file
    /// already written is put back (one that cannot be is the error's writes). Returns the files it wrote.
    pub fn apply(&self, caller: Caller<'_>) -> Result<Writes, OpError> {
        let mut change = ProjectChange::new(&self.spec_root, self.doing.as_str());
        for (key, (before, after)) in &self.files {
            let held = caller.containment().resolve(&self.spec_root.join(key))?;
            change.stage(
                held,
                before.as_ref().map(|b| b.as_bytes().to_vec()),
                after.clone().into_bytes(),
            );
        }
        change.commit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Option<Arc<str>> {
        Some(Arc::from(s))
    }

    fn change(files: Vec<(&str, Option<Arc<str>>, &str)>) -> PlannedChange {
        PlannedChange::new(
            PathBuf::from("/root"),
            "cannot test",
            files
                .into_iter()
                .map(|(k, b, a)| (k.to_string(), b, a.to_string())),
        )
    }

    #[test]
    fn a_file_whose_after_is_its_planned_on_text_is_not_in_the_change() {
        let same = change(vec![("a.spec", text("x\n"), "x\n")]);
        assert!(same.is_empty());
        assert_eq!(same.affected_files().count(), 0);
        assert!(same.after().is_empty() && same.diff().is_empty() && same.text_edits().is_empty());
        let one = change(vec![
            ("a.spec", text("x\n"), "x\n"),
            ("b.spec", text("x\n"), "y\n"),
        ]);
        assert_eq!(one.affected_files().collect::<Vec<_>>(), ["b.spec"]);
    }

    #[test]
    fn affected_files_and_the_diff_are_in_key_order() {
        let c = change(vec![
            ("b.spec", None, "b\n"),
            ("a/x.spec", None, "x\n"),
            ("a.spec", None, "a\n"),
        ]);
        assert_eq!(
            c.affected_files().collect::<Vec<_>>(),
            ["a.spec", "a/x.spec", "b.spec"]
        );
        let diff = c.diff();
        let at = |name: &str| diff.find(name).unwrap();
        assert!(at("a.spec") < at("a/x.spec") && at("a/x.spec") < at("b.spec"));
    }
}
