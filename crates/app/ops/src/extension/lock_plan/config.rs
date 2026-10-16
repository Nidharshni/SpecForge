//! `specforge.json` as a lock plan opened it (ADR 0126): what the plan judges, and what its commit writes into.

use serde_json::Value;
use specforge_common::{ConfigRead, ProjectConfig, project_config_from};

use super::Appended;
use crate::{Held, OpError, OpErrorKind};

/// `specforge.json` as a lock plan opened it: the document and its `extensions` items as read. The peer judgement,
/// the dry run's answers and the commit's write all read [`Self::items_after`], so what is judged is what is written.
#[derive(Debug, Clone)]
pub(crate) struct Opened {
    /// The file's document as read (`ProjectConfig::raw`); `None`: there was no file.
    found: Option<Value>,
    /// The `extensions` items as read, strings or not, in file order.
    items: Vec<Value>,
    /// Their strings: what the project peer set reads.
    entries: Vec<String>,
}

impl Opened {
    /// The config a compile, `config::required` or `config::usable` read.
    pub(crate) fn read(config: &ProjectConfig, found: bool) -> Opened {
        let items = config
            .raw
            .as_ref()
            .and_then(|raw| raw.get("extensions"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_else(|| config.extensions.iter().cloned().map(Value::from).collect());
        Opened {
            found: found.then(|| {
                config
                    .raw
                    .clone()
                    .unwrap_or_else(|| Value::Object(Default::default()))
            }),
            items,
            entries: config.extensions.clone(),
        }
    }

    /// The string items as read: what `specforge.json` enables before the change.
    pub(crate) fn entries(&self) -> &[String] {
        &self.entries
    }

    /// THE rule: the items the change leaves, in order: the items read without the strings `drops` selects, then
    /// `appended`. A non-string item is kept where it was.
    pub(super) fn items_after(
        &self,
        drops: impl Fn(&str) -> bool,
        appended: &[Appended],
    ) -> Vec<Value> {
        self.items
            .iter()
            .filter(|item| item.as_str().is_none_or(|entry| !drops(entry)))
            .cloned()
            .chain(
                appended
                    .iter()
                    .map(|appended| Value::from(appended.name.as_str())),
            )
            .collect()
    }

    /// Whether the plan opened a `specforge.json`: a change writes into the file it read, never into a project that
    /// has none.
    pub(super) fn opened_a_file(&self) -> bool {
        self.found.is_some()
    }

    /// The document the change writes: the one read with `extensions` = `after`; `None` when `after` is the items
    /// read, or when the plan opened no file.
    pub(super) fn written(&self, after: &[Value]) -> Option<Value> {
        let mut document = self.found.clone()?;
        if after == self.items {
            return None;
        }
        document
            .as_object_mut()?
            .insert("extensions".into(), Value::Array(after.to_vec()));
        Some(document)
    }

    /// Refuse (`file_changed`, Conflict) unless `held` still holds the document this plan opened with, parsed as the
    /// compile parses it (nothing, when it opened none). The bytes read are returned, for the change to stage the
    /// file over.
    pub(super) fn unchanged(
        &self,
        held: &Held,
        operation: &'static str,
    ) -> Result<Option<Vec<u8>>, OpError> {
        let bytes = held.read_if_there().map_err(|error| {
            OpError::new(
                OpErrorKind::of_io(&error),
                "file_unreadable",
                format!("failed to read {}: {error}", held.spelled.display()),
            )
        })?;
        let now = match &bytes {
            Some(bytes) => project_config_from(&held.spelled, Ok(bytes.clone())),
            None => ConfigRead::default(),
        };
        let now = now.found.then_some(now.config.raw).flatten();
        if now == self.found {
            return Ok(bytes);
        }
        Err(OpError::new(
            OpErrorKind::Conflict,
            crate::FILE_CHANGED,
            format!(
                "{} changed after {operation} read it; nothing was written",
                held.spelled.display()
            ),
        )
        .with_suggestion(format!("run {operation} again")))
    }

    /// The strings of `items`: what `specforge.json` enables as the change leaves it.
    pub(super) fn entries_of(items: &[Value]) -> Vec<String> {
        items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect()
    }
}
