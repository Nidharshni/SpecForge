use super::{count_in, count_out, sorted_dedup, targets, text};
use crate::vocabulary;
use serde::Serialize;
use specforge_extension_sdk::prelude::{CommandGraph, GraphNode, Vocabulary};
use specforge_extension_sdk::EntityKindDescriptor;

/// `ProductListSortOrder`.
pub const SORT_ORDER: &[&str] = &["asc", "desc"];

/// A list's page size when the caller sets none, and the bounds a set one
/// is clamped to.
pub const DEFAULT_LIMIT: usize = 100;
pub const MAX_LIMIT: usize = 1000;

/// An arg a list command filters by.
#[derive(Debug)]
pub enum FilterArg {
    /// A field with a vocabulary ([`vocabulary::of`]): its arg is `one_of`
    /// the values, it matches the entities whose field reads as the value
    /// (one without the field reads as the absent value), and its help is
    /// derived ([`FilterArg::help`]).
    Closed(&'static str),
    /// A field matched as written, or a reference field matched by the id it
    /// names (on the field or on an edge it wrote); `help` says what it takes.
    Open {
        field: &'static str,
        help: &'static str,
    },
}

impl FilterArg {
    /// The field (and arg) it filters by.
    pub fn field(&self) -> &'static str {
        match self {
            FilterArg::Closed(field) => field,
            FilterArg::Open { field, .. } => field,
        }
    }

    /// Its arg's help: an open filter's own; a closed one's "Only <plural>
    /// with this <field, `_` as spaces>", followed by "; a <kind> without
    /// one counts as <absent>" when the vocabulary has an absent value.
    /// Panics on a closed filter whose field has no vocabulary.
    pub fn help(&self, kind: &ListKind) -> String {
        match self {
            FilterArg::Open { help, .. } => (*help).to_string(),
            FilterArg::Closed(field) => {
                let vocabulary = kind
                    .vocabulary(field)
                    .unwrap_or_else(|| panic!("{} declares no vocabulary for {field}", kind.kind));
                let mut help =
                    format!("Only {} with this {}", kind.plural, field.replace('_', " "));
                if let Some(absent) = vocabulary.absent() {
                    help.push_str(&format!("; a {} without one counts as {absent}", kind.kind));
                }
                help
            }
        }
    }
}

/// One list command's kind: the payload key its entries are under, and
/// the args beside `--tags` that filter it.
#[derive(Debug)]
pub struct ListKind {
    pub kind: &'static str,
    pub plural: &'static str,
    pub filters: &'static [FilterArg],
}

impl ListKind {
    /// The vocabulary of this kind's `field`, if it has one.
    pub fn vocabulary(&self, field: &str) -> Option<&'static Vocabulary> {
        vocabulary::of(self.kind, field)
    }
}

use FilterArg::{Closed, Open};

pub const FEATURES: ListKind = ListKind {
    kind: "feature",
    plural: "features",
    filters: &[Closed("status"), Closed("priority")],
};
pub const JOURNEYS: ListKind = ListKind {
    kind: "journey",
    plural: "journeys",
    filters: &[
        Closed("priority"),
        Open {
            field: "persona",
            help: "Only journeys of this persona (its id)",
        },
    ],
};
pub const DELIVERABLES: ListKind = ListKind {
    kind: "deliverable",
    plural: "deliverables",
    filters: &[Closed("status"), Closed("artifact_type")],
};
pub const MILESTONES: ListKind = ListKind {
    kind: "milestone",
    plural: "milestones",
    filters: &[Closed("status"), Closed("priority")],
};
pub const MODULES: ListKind = ListKind {
    kind: "module",
    plural: "modules",
    // `ModuleFamily` is open: a family outside the standard set is I062,
    // not an error, so it is a value to match, not one to refuse.
    filters: &[Open {
        field: "family",
        help: "Only modules of this family, standard or not",
    }],
};
pub const TERMS: ListKind = ListKind {
    kind: "term",
    plural: "terms",
    filters: &[],
};
pub const PERSONAS: ListKind = ListKind {
    kind: "persona",
    plural: "personas",
    filters: &[Closed("status"), Closed("technical_level")],
};
pub const CHANNELS: ListKind = ListKind {
    kind: "channel",
    plural: "channels",
    filters: &[Closed("status"), Closed("interaction_model")],
};
pub const RELEASES: ListKind = ListKind {
    kind: "release",
    plural: "releases",
    filters: &[Closed("status")],
};

/// Which entities a list command returns (`ProductListFilter`, validated):
/// one kind, narrowed by every filter set (AND), sorted, then paged.
#[derive(Debug)]
pub struct ListFilter<'a> {
    pub kind: &'a ListKind,
    /// `(field, value)`: the entity's field equals the value.
    pub equals: Vec<(&'a str, &'a str)>,
    /// The entity has one of these tags; empty matches every entity.
    pub tags: Vec<&'a str>,
    pub sort_by: &'a str,
    pub descending: bool,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

impl<'a> ListFilter<'a> {
    /// Every entity of `kind`, by id, on the first page.
    pub fn all(kind: &'a ListKind) -> Self {
        ListFilter {
            kind,
            equals: Vec::new(),
            tags: Vec::new(),
            sort_by: "id",
            descending: false,
            offset: None,
            limit: None,
        }
    }
}

/// Whether `kind` entities have `field` to sort by: `id`, `title`, the
/// shared `tags`, or a field the kind declares.
pub fn sortable(kind: &str, field: &str) -> bool {
    matches!(field, "id" | "title" | "tags") || declared_fields(kind).contains(&field)
}

/// The fields `kind` declares (its kind in [`crate::declaration`]).
fn declared_fields(kind: &str) -> Vec<&'static str> {
    static KINDS: std::sync::OnceLock<Vec<EntityKindDescriptor>> = std::sync::OnceLock::new();
    let kinds = KINDS.get_or_init(|| crate::specforge_extension_build().declaration().entities);
    kinds
        .iter()
        .filter(|k| k.keyword.as_deref() == Some(kind))
        .flat_map(|k| &k.fields)
        .map(|f| f.name.as_str())
        .collect()
}

/// The `kind` entities `filter` matches, sorted as it asks: by the field's
/// enum order when it is a closed enum, else by its text; an entity without
/// the field last; ties by id ascending.
pub fn list_nodes<'g>(graph: &'g CommandGraph, filter: &ListFilter) -> Vec<&'g GraphNode> {
    let kind = filter.kind;
    let value = |node: &GraphNode, field: &str| -> Option<String> {
        let absent_as = kind.vocabulary(field).and_then(Vocabulary::absent);
        match node.fields.get(field) {
            Some(serde_json::Value::String(s)) => Some(s.clone()),
            Some(serde_json::Value::Array(items)) => Some(
                items
                    .iter()
                    .filter_map(|i| i.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            Some(serde_json::Value::Number(n)) => Some(n.to_string()),
            _ => absent_as.map(str::to_string),
        }
    };
    let mut nodes: Vec<&GraphNode> = graph
        .nodes_of_kind(kind.kind)
        .filter(|n| {
            filter.equals.iter().all(|(field, wanted)| {
                value(n, field).as_deref() == Some(*wanted)
                    || graph
                        .edges_from(&n.id)
                        .iter()
                        .any(|e| e.label == *field && e.target == *wanted)
            })
        })
        .filter(|n| {
            filter.tags.is_empty() || n.list("tags").iter().any(|t| filter.tags.contains(t))
        })
        .collect();

    #[derive(PartialEq, Eq, PartialOrd, Ord)]
    enum Key {
        Rank(usize),
        Text(String),
    }
    let key = |node: &GraphNode| -> Option<Key> {
        match filter.sort_by {
            "id" => Some(Key::Text(node.id.clone())),
            "title" => node.title.clone().map(Key::Text),
            field => {
                let v = value(node, field)?;
                Some(match kind.vocabulary(field) {
                    Some(order) => {
                        Key::Rank(order.position(&v).unwrap_or_else(|| order.values().len()))
                    }
                    None => Key::Text(v),
                })
            }
        }
    };
    nodes.sort_by(|a, b| {
        let by_key = match (key(a), key(b)) {
            (Some(x), Some(y)) if filter.descending => y.cmp(&x),
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        };
        by_key.then_with(|| a.id.cmp(&b.id))
    });
    nodes
}

/// One page of `items` (`PaginationMetadata`): `limit` defaults to
/// [`DEFAULT_LIMIT`] and is clamped to [1, [`MAX_LIMIT`]]; an offset past
/// the end is an empty page. `total` counts every item.
#[derive(Debug)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub has_more: bool,
}

pub fn paginate<T>(items: Vec<T>, offset: Option<usize>, limit: Option<usize>) -> Page<T> {
    let total = items.len();
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let items: Vec<T> = items.into_iter().skip(offset).take(limit).collect();
    let has_more = offset.saturating_add(items.len()) < total;
    Page {
        items,
        total,
        offset,
        limit,
        has_more,
    }
}

impl<T: Serialize> Page<T> {
    /// The page as its payload: the items under `key`, then the
    /// pagination metadata.
    pub fn payload(&self, key: &str) -> serde_json::Value {
        serde_json::json!({
            key: self.items,
            "total": self.total,
            "offset": self.offset,
            "limit": self.limit,
            "has_more": self.has_more,
        })
    }
}

/// An entry of a list command's payload (`FeatureListEntry`, ...), and its
/// row in the human table.
pub trait ListEntry: Serialize + Sized {
    const KIND: &'static ListKind;
    const HEADERS: &'static [&'static str];
    fn of(graph: &CommandGraph, node: &GraphNode) -> Self;
    fn row(&self) -> Vec<String>;
}

/// The page of `T` entries `filter` selects.
pub fn list<T: ListEntry>(graph: &CommandGraph, filter: &ListFilter) -> Page<T> {
    let entries = list_nodes(graph, filter)
        .into_iter()
        .map(|n| T::of(graph, n))
        .collect();
    paginate(entries, filter.offset, filter.limit)
}

fn tags(node: &GraphNode) -> Option<Vec<String>> {
    let tags = node.list("tags");
    (!tags.is_empty()).then(|| tags.into_iter().map(str::to_string).collect())
}

fn title(node: &GraphNode) -> String {
    node.title.clone().unwrap_or_default()
}

fn cell(value: &Option<String>) -> String {
    value.clone().unwrap_or_else(|| "-".to_string())
}

#[derive(Debug, Serialize)]
pub struct FeatureListEntry {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

impl ListEntry for FeatureListEntry {
    const KIND: &'static ListKind = &FEATURES;
    const HEADERS: &'static [&'static str] = &["id", "title", "status", "priority"];
    fn of(_: &CommandGraph, n: &GraphNode) -> Self {
        FeatureListEntry {
            id: n.id.clone(),
            title: title(n),
            status: text(n, "status"),
            priority: text(n, "priority"),
            problem: text(n, "problem"),
            tags: tags(n),
        }
    }
    fn row(&self) -> Vec<String> {
        vec![
            self.id.clone(),
            self.title.clone(),
            cell(&self.status),
            cell(&self.priority),
        ]
    }
}

#[derive(Debug, Serialize)]
pub struct JourneyListEntry {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
    pub channel_count: usize,
    pub feature_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

impl ListEntry for JourneyListEntry {
    const KIND: &'static ListKind = &JOURNEYS;
    const HEADERS: &'static [&'static str] =
        &["id", "title", "persona", "channels", "features", "priority"];
    fn of(graph: &CommandGraph, n: &GraphNode) -> Self {
        JourneyListEntry {
            id: n.id.clone(),
            title: title(n),
            persona: text(n, "persona"),
            channel_count: count_out(graph, &n.id, "channels"),
            feature_count: count_out(graph, &n.id, "features"),
            priority: text(n, "priority"),
            tags: tags(n),
        }
    }
    fn row(&self) -> Vec<String> {
        vec![
            self.id.clone(),
            self.title.clone(),
            cell(&self.persona),
            self.channel_count.to_string(),
            self.feature_count.to_string(),
            cell(&self.priority),
        ]
    }
}

#[derive(Debug, Serialize)]
pub struct DeliverableListEntry {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub journey_count: usize,
    pub module_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

impl ListEntry for DeliverableListEntry {
    const KIND: &'static ListKind = &DELIVERABLES;
    const HEADERS: &'static [&'static str] = &[
        "id",
        "title",
        "artifact_type",
        "status",
        "journeys",
        "modules",
    ];
    fn of(graph: &CommandGraph, n: &GraphNode) -> Self {
        DeliverableListEntry {
            id: n.id.clone(),
            title: title(n),
            artifact_type: text(n, "artifact_type"),
            status: text(n, "status"),
            journey_count: count_out(graph, &n.id, "journeys"),
            module_count: count_out(graph, &n.id, "modules"),
            tags: tags(n),
        }
    }
    fn row(&self) -> Vec<String> {
        vec![
            self.id.clone(),
            self.title.clone(),
            cell(&self.artifact_type),
            cell(&self.status),
            self.journey_count.to_string(),
            self.module_count.to_string(),
        ]
    }
}

#[derive(Debug, Serialize)]
pub struct MilestoneListEntry {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_date: Option<String>,
    pub feature_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

impl ListEntry for MilestoneListEntry {
    const KIND: &'static ListKind = &MILESTONES;
    const HEADERS: &'static [&'static str] = &[
        "id",
        "title",
        "status",
        "target_date",
        "features",
        "priority",
    ];
    fn of(graph: &CommandGraph, n: &GraphNode) -> Self {
        MilestoneListEntry {
            id: n.id.clone(),
            title: title(n),
            status: text(n, "status"),
            target_date: text(n, "target_date"),
            feature_count: count_out(graph, &n.id, "features"),
            priority: text(n, "priority"),
            tags: tags(n),
        }
    }
    fn row(&self) -> Vec<String> {
        vec![
            self.id.clone(),
            self.title.clone(),
            cell(&self.status),
            cell(&self.target_date),
            self.feature_count.to_string(),
            cell(&self.priority),
        ]
    }
}

#[derive(Debug, Serialize)]
pub struct ModuleListEntry {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    pub feature_count: usize,
    pub depends_on: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

impl ListEntry for ModuleListEntry {
    const KIND: &'static ListKind = &MODULES;
    const HEADERS: &'static [&'static str] = &["id", "title", "family", "features", "depends_on"];
    fn of(graph: &CommandGraph, n: &GraphNode) -> Self {
        ModuleListEntry {
            id: n.id.clone(),
            title: title(n),
            family: text(n, "family"),
            feature_count: count_out(graph, &n.id, "features"),
            depends_on: sorted_dedup(targets(graph, &n.id, "depends_on")),
            tags: tags(n),
        }
    }
    fn row(&self) -> Vec<String> {
        vec![
            self.id.clone(),
            self.title.clone(),
            cell(&self.family),
            self.feature_count.to_string(),
            if self.depends_on.is_empty() {
                "-".to_string()
            } else {
                self.depends_on.join(",")
            },
        ]
    }
}

#[derive(Debug, Serialize)]
pub struct TermListEntry {
    pub id: String,
    pub title: String,
    pub definition: String,
    pub alias_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

impl ListEntry for TermListEntry {
    const KIND: &'static ListKind = &TERMS;
    const HEADERS: &'static [&'static str] = &["id", "title", "aliases", "definition"];
    fn of(_: &CommandGraph, n: &GraphNode) -> Self {
        TermListEntry {
            id: n.id.clone(),
            title: title(n),
            definition: text(n, "definition").unwrap_or_default(),
            alias_count: n.list("aliases").len(),
            tags: tags(n),
        }
    }
    fn row(&self) -> Vec<String> {
        vec![
            self.id.clone(),
            self.title.clone(),
            self.alias_count.to_string(),
            self.definition.clone(),
        ]
    }
}

#[derive(Debug, Serialize)]
pub struct PersonaListEntry {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub technical_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub journey_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

impl ListEntry for PersonaListEntry {
    const KIND: &'static ListKind = &PERSONAS;
    const HEADERS: &'static [&'static str] =
        &["id", "title", "technical_level", "status", "journeys"];
    fn of(graph: &CommandGraph, n: &GraphNode) -> Self {
        PersonaListEntry {
            id: n.id.clone(),
            title: title(n),
            technical_level: text(n, "technical_level"),
            status: text(n, "status"),
            journey_count: count_in(graph, &n.id, "persona", "journey"),
            tags: tags(n),
        }
    }
    fn row(&self) -> Vec<String> {
        vec![
            self.id.clone(),
            self.title.clone(),
            cell(&self.technical_level),
            cell(&self.status),
            self.journey_count.to_string(),
        ]
    }
}

#[derive(Debug, Serialize)]
pub struct ChannelListEntry {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interaction_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub journey_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

impl ListEntry for ChannelListEntry {
    const KIND: &'static ListKind = &CHANNELS;
    const HEADERS: &'static [&'static str] =
        &["id", "title", "interaction_model", "status", "journeys"];
    fn of(graph: &CommandGraph, n: &GraphNode) -> Self {
        ChannelListEntry {
            id: n.id.clone(),
            title: title(n),
            interaction_model: text(n, "interaction_model"),
            status: text(n, "status"),
            journey_count: count_in(graph, &n.id, "channels", "journey"),
            tags: tags(n),
        }
    }
    fn row(&self) -> Vec<String> {
        vec![
            self.id.clone(),
            self.title.clone(),
            cell(&self.interaction_model),
            cell(&self.status),
            self.journey_count.to_string(),
        ]
    }
}

#[derive(Debug, Serialize)]
pub struct ReleaseListEntry {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub deliverable_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

impl ListEntry for ReleaseListEntry {
    const KIND: &'static ListKind = &RELEASES;
    const HEADERS: &'static [&'static str] = &[
        "id",
        "title",
        "version",
        "status",
        "deliverables",
        "release_date",
    ];
    fn of(graph: &CommandGraph, n: &GraphNode) -> Self {
        ReleaseListEntry {
            id: n.id.clone(),
            title: title(n),
            version: text(n, "version"),
            status: text(n, "status"),
            deliverable_count: count_out(graph, &n.id, "deliverables"),
            release_date: text(n, "release_date"),
            tags: tags(n),
        }
    }
    fn row(&self) -> Vec<String> {
        vec![
            self.id.clone(),
            self.title.clone(),
            cell(&self.version),
            cell(&self.status),
            self.deliverable_count.to_string(),
            cell(&self.release_date),
        ]
    }
}
