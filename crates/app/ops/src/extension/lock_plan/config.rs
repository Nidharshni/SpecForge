//! `specforge.json` as a lock plan opened it (ADR 0126): what the plan judges, and what its commit writes into.

use serde_json::Value;
use specforge_common::ProjectConfig;

use super::Appended;

/// `specforge.json` as a lock plan opened it: the document and its `extensions` items as read. The peer judgement,
/// the dry run's answers and the commit's write all read [`Self::items_after`], so what is judged is what is written.
#[derive(Debug, Clone)]
pub(crate) struct Opened {
    /// The `extensions` items as read, strings or not, in file order.
    items: Vec<Value>,
    /// Their strings: what the project peer set reads.
    entries: Vec<String>,
}

impl Opened {
    /// The config a compile, `config::required` or `config::usable` read.
    pub(crate) fn read(config: &ProjectConfig) -> Opened {
        let items = config
            .raw
            .as_ref()
            .and_then(|raw| raw.get("extensions"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_else(|| config.extensions.iter().cloned().map(Value::from).collect());
        Opened {
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

    /// The strings of `items`: what `specforge.json` enables as the change leaves it.
    pub(super) fn entries_of(items: &[Value]) -> Vec<String> {
        items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect()
    }
}
