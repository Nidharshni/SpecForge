//! Product queries: pure reads of the graph the host passes a command.
//!
//! Each one answers a planning question over this extension's own kinds and
//! the fields they declare (`status`, `priority`, `features`, ...). The
//! host knows none of them; it runs the `cmd__product_*` exports in
//! `commands.rs`, which render these results.

use crate::evidence::{self, EvidenceState, FeatureEvidence};
use crate::vocabulary::{self, *};
use serde::Serialize;
use specforge_extension_sdk::prelude::{
    CommandError, CommandEvidence, CommandGraph, GraphNode, Vocabulary,
};
use specforge_extension_sdk::EntityKindDescriptor;
use std::collections::BTreeMap;

/// The kinds this extension declares, in the order `health` reports them.
pub const PRODUCT_KINDS: &[&str] = &[
    "feature",
    "journey",
    "deliverable",
    "milestone",
    "module",
    "term",
    "persona",
    "channel",
    "release",
];

// ── Listing ────────────────────────────────────────────────────────────────

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

// ── Milestones and journeys ────────────────────────────────────────────────

/// `MilestoneCompletionPayload`: how many of the milestone's features are
/// done, as a count, a ratio in [0, 1] and their ids.
#[derive(Debug, Serialize)]
pub struct MilestoneCompletion {
    pub milestone_id: String,
    pub total_features: usize,
    pub done_count: usize,
    pub completion_ratio: f64,
    pub done_features: Vec<String>,
    /// Whether the command's input carried recorded test evidence
    /// ([`MilestoneCompletion::with_evidence`]); absent when not asked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<EvidenceState>,
    /// How many of the features the recorded tests prove, as a count, a
    /// ratio in [0, 1] and their ids, beside the declared `done_count`
    /// (ADR 0039); absent without a scored report.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proven_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proven_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proven_features: Option<Vec<String>>,
    /// Each feature's evidence, in the milestone's order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature_evidence: Option<Vec<FeatureEvidence>>,
    /// The milestone's status and its features, for the human layout.
    #[serde(skip)]
    pub status: Option<String>,
    #[serde(skip)]
    pub features: Vec<FeatureStatus>,
}

impl MilestoneCompletion {
    /// The completion with what `evidence` (a command's input) proves of
    /// its features: their evidence and the proven count, or only the
    /// evidence state when the input carries no scored report.
    pub fn with_evidence(mut self, graph: &CommandGraph, evidence: &CommandEvidence) -> Self {
        let ids: Vec<String> = self.features.iter().map(|f| f.id.clone()).collect();
        self.evidence = Some(EvidenceState::of(evidence));
        if let Some(features) = evidence::of_features(graph, &ids, evidence) {
            let proven: Vec<String> = features
                .iter()
                .filter(|f| f.proven)
                .map(|f| f.feature_id.clone())
                .collect();
            self.proven_count = Some(proven.len());
            self.proven_ratio = Some(ratio(proven.len(), ids.len()));
            self.proven_features = Some(proven);
            self.feature_evidence = Some(features);
        }
        self
    }
}

#[derive(Debug)]
pub struct FeatureStatus {
    pub id: String,
    pub status: Option<String>,
}

/// The milestone's features (in declaration order) and which are done;
/// `None` when `milestone_id` is not a milestone.
pub fn milestone_completion(
    graph: &CommandGraph,
    milestone_id: &str,
) -> Option<MilestoneCompletion> {
    let node = of_kind(graph, milestone_id, "milestone")?;
    // The referenced features, in the order the milestone lists them.
    let mut referenced = targets(graph, milestone_id, "features");
    let declared = node.list("features");
    referenced.sort_by_key(|id| declared.iter().position(|d| d == id).unwrap_or(usize::MAX));
    let features: Vec<FeatureStatus> = referenced
        .iter()
        .filter_map(|id| graph.node(id))
        .map(|f| FeatureStatus {
            id: f.id.clone(),
            status: text(f, "status"),
        })
        .collect();
    let done_features: Vec<String> = features
        .iter()
        .filter(|f| f.status.as_deref() == Some(FEATURE_DONE))
        .map(|f| f.id.clone())
        .collect();
    Some(MilestoneCompletion {
        milestone_id: milestone_id.to_string(),
        total_features: features.len(),
        done_count: done_features.len(),
        completion_ratio: ratio(done_features.len(), features.len()),
        done_features,
        evidence: None,
        proven_count: None,
        proven_ratio: None,
        proven_features: None,
        feature_evidence: None,
        status: text(node, "status"),
        features,
    })
}

/// `JourneyCoveragePayload`: how many of the journey's features are done,
/// and the ones that are not.
#[derive(Debug, Serialize)]
pub struct JourneyCoverage {
    pub journey_id: String,
    pub total_features: usize,
    pub covered_count: usize,
    pub uncovered_features: Vec<String>,
    /// The journey's persona, for the human layout.
    #[serde(skip)]
    pub persona: Option<String>,
}

/// How many of the journey's features have status `done` (a feature
/// without a status is uncovered); `None` when `journey_id` is not a
/// journey.
pub fn journey_coverage(graph: &CommandGraph, journey_id: &str) -> Option<JourneyCoverage> {
    let node = of_kind(graph, journey_id, "journey")?;
    let features = sorted_dedup(targets(graph, journey_id, "features"));
    let (covered, uncovered): (Vec<String>, Vec<String>) = features
        .into_iter()
        .partition(|f| graph.node(f).and_then(|n| n.text("status")) == Some(FEATURE_DONE));
    Some(JourneyCoverage {
        journey_id: journey_id.to_string(),
        total_features: covered.len() + uncovered.len(),
        covered_count: covered.len(),
        uncovered_features: uncovered,
        persona: text(node, "persona"),
    })
}

// ── Features ───────────────────────────────────────────────────────────────

/// `FeatureImpactPayload`: what deferring or removing the feature touches.
#[derive(Debug, Serialize)]
pub struct FeatureImpact {
    pub feature_id: String,
    pub affected_journeys: Vec<String>,
    pub affected_milestones: Vec<String>,
    pub affected_deliverables: Vec<String>,
    pub affected_modules: Vec<String>,
    pub dependent_features: Vec<String>,
    pub total_affected_entities: usize,
}

/// The journeys, milestones and modules that list the feature, the
/// deliverables holding those journeys or modules, and the features that
/// depend on it, directly or through one another (only `depends_on`: a
/// feature listing it under `features` relates to it). Each sorted by id,
/// once; `None` when `feature_id` is not a feature.
pub fn feature_impact(graph: &CommandGraph, feature_id: &str) -> Option<FeatureImpact> {
    of_kind(graph, feature_id, "feature")?;
    let (via_journeys, via_modules) = deliverables_of_feature(graph, feature_id);
    let mut dependents: Vec<String> = Vec::new();
    let mut frontier = vec![feature_id.to_string()];
    while let Some(next) = frontier.pop() {
        for dependent in into(graph, &next, "depends_on", "feature") {
            if dependent != feature_id && !dependents.contains(&dependent) {
                dependents.push(dependent.clone());
                frontier.push(dependent);
            }
        }
    }
    let mut impact = FeatureImpact {
        feature_id: feature_id.to_string(),
        affected_journeys: into(graph, feature_id, "features", "journey"),
        affected_milestones: into(graph, feature_id, "features", "milestone"),
        affected_deliverables: sorted_dedup([via_journeys, via_modules].concat()),
        affected_modules: into(graph, feature_id, "features", "module"),
        dependent_features: sorted_dedup(dependents),
        total_affected_entities: 0,
    };
    // The kinds are disjoint, so the union is the sum.
    impact.total_affected_entities = impact.affected_journeys.len()
        + impact.affected_milestones.len()
        + impact.affected_deliverables.len()
        + impact.affected_modules.len()
        + impact.dependent_features.len();
    Some(impact)
}

/// `FeatureDependentPayload`: the features that declare `depends_on` the
/// feature, sorted by id.
#[derive(Debug, Serialize)]
pub struct FeatureDependents {
    pub feature_id: String,
    pub dependents: Vec<String>,
    pub count: usize,
}

/// The features that declare `depends_on` the feature; `None` when
/// `feature_id` is not a feature.
pub fn feature_dependents(graph: &CommandGraph, feature_id: &str) -> Option<FeatureDependents> {
    of_kind(graph, feature_id, "feature")?;
    let dependents = into(graph, feature_id, "depends_on", "feature");
    Some(FeatureDependents {
        feature_id: feature_id.to_string(),
        count: dependents.len(),
        dependents,
    })
}

/// `PersonaFeaturePayload`: the features of every journey the persona
/// undertakes, and those journeys.
#[derive(Debug, Serialize)]
pub struct PersonaFeatures {
    pub persona_id: String,
    pub features: Vec<String>,
    pub via_journey_ids: Vec<String>,
    pub count: usize,
}

/// The features of every journey the persona undertakes, sorted; `None`
/// when `persona_id` is not a persona.
pub fn persona_features(graph: &CommandGraph, persona_id: &str) -> Option<PersonaFeatures> {
    of_kind(graph, persona_id, "persona")?;
    let journeys = journeys_of_persona(graph, persona_id);
    let (features, via_journey_ids) = through_journeys(graph, journeys);
    Some(PersonaFeatures {
        persona_id: persona_id.to_string(),
        count: features.len(),
        features,
        via_journey_ids,
    })
}

/// `ChannelFeaturePayload`: the features of every journey that uses the
/// channel, and those journeys.
#[derive(Debug, Serialize)]
pub struct ChannelFeatures {
    pub channel_id: String,
    pub features: Vec<String>,
    pub via_journey_ids: Vec<String>,
    pub count: usize,
}

/// The features of every journey that uses the channel, sorted; `None`
/// when `channel_id` is not a channel.
pub fn channel_features(graph: &CommandGraph, channel_id: &str) -> Option<ChannelFeatures> {
    of_kind(graph, channel_id, "channel")?;
    let journeys = journeys_of_channel(graph, channel_id);
    let (features, via_journey_ids) = through_journeys(graph, journeys);
    Some(ChannelFeatures {
        channel_id: channel_id.to_string(),
        count: features.len(),
        features,
        via_journey_ids,
    })
}

/// The features `journeys` exercise, and the journeys, each sorted and
/// listed once.
fn through_journeys(graph: &CommandGraph, journeys: Vec<String>) -> (Vec<String>, Vec<String>) {
    let journeys = sorted_dedup(journeys);
    let features = journeys
        .iter()
        .flat_map(|j| targets(graph, j, "features"))
        .filter(|f| kind_of(graph, f) == Some("feature"))
        .collect();
    (sorted_dedup(features), journeys)
}

// ── Traceability ───────────────────────────────────────────────────────────

/// `DeliverableTraceabilityPayload`: every feature the deliverable reaches
/// through its journeys or its modules, and how many each path reaches.
#[derive(Debug, Serialize)]
pub struct DeliverableTraceability {
    pub deliverable_id: String,
    pub transitive_features: Vec<String>,
    pub journey_path_count: usize,
    pub module_path_count: usize,
}

/// The features `deliverable -> journeys -> features` and `deliverable ->
/// modules -> features` reach, once each; `None` when `deliverable_id` is
/// not a deliverable.
pub fn deliverable_traceability(
    graph: &CommandGraph,
    deliverable_id: &str,
) -> Option<DeliverableTraceability> {
    of_kind(graph, deliverable_id, "deliverable")?;
    let (via_journeys, via_modules) = features_of_deliverable(graph, deliverable_id);
    Some(DeliverableTraceability {
        deliverable_id: deliverable_id.to_string(),
        journey_path_count: via_journeys.len(),
        module_path_count: via_modules.len(),
        transitive_features: sorted_dedup([via_journeys, via_modules].concat()),
    })
}

/// `FeatureDeliverablePayload`: every deliverable that holds the feature
/// through a journey or a module, and how many each path reaches.
#[derive(Debug, Serialize)]
pub struct FeatureDeliverables {
    pub feature_id: String,
    pub deliverables: Vec<String>,
    pub via_journey_count: usize,
    pub via_module_count: usize,
}

/// The deliverables the reverse paths `feature <- journey <- deliverable`
/// and `feature <- module <- deliverable` reach, once each, sorted; `None`
/// when `feature_id` is not a feature.
pub fn feature_deliverables(graph: &CommandGraph, feature_id: &str) -> Option<FeatureDeliverables> {
    of_kind(graph, feature_id, "feature")?;
    let (via_journeys, via_modules) = deliverables_of_feature(graph, feature_id);
    Some(FeatureDeliverables {
        feature_id: feature_id.to_string(),
        via_journey_count: via_journeys.len(),
        via_module_count: via_modules.len(),
        deliverables: sorted_dedup([via_journeys, via_modules].concat()),
    })
}

/// `PersonaChannelPayload`: the channels of every journey the persona
/// undertakes.
#[derive(Debug, Serialize)]
pub struct PersonaChannels {
    pub persona_id: String,
    pub channels: Vec<String>,
    pub count: usize,
}

/// The channels `persona <- journey -> channels` reaches, sorted, once
/// each; `None` when `persona_id` is not a persona.
pub fn persona_channels(graph: &CommandGraph, persona_id: &str) -> Option<PersonaChannels> {
    of_kind(graph, persona_id, "persona")?;
    let channels = sorted_dedup(
        journeys_of_persona(graph, persona_id)
            .iter()
            .flat_map(|j| out(graph, j, "channels", "channel"))
            .collect(),
    );
    Some(PersonaChannels {
        persona_id: persona_id.to_string(),
        count: channels.len(),
        channels,
    })
}

/// `DeliverablePersonaPayload`: the personas the deliverable's journeys
/// target, and those journeys.
#[derive(Debug, Serialize)]
pub struct DeliverablePersonas {
    pub deliverable_id: String,
    pub personas: Vec<String>,
    pub via_journey_ids: Vec<String>,
    pub count: usize,
}

/// The personas `deliverable -> journeys -> persona` reaches, sorted, once
/// each, and the journeys on those paths (a journey without a persona is on
/// none); `None` when `deliverable_id` is not a deliverable.
pub fn deliverable_personas(
    graph: &CommandGraph,
    deliverable_id: &str,
) -> Option<DeliverablePersonas> {
    of_kind(graph, deliverable_id, "deliverable")?;
    let mut personas = Vec::new();
    let mut via_journey_ids = Vec::new();
    for journey in out(graph, deliverable_id, "journeys", "journey") {
        let targeted = out(graph, &journey, "persona", "persona");
        if !targeted.is_empty() {
            personas.extend(targeted);
            via_journey_ids.push(journey);
        }
    }
    let personas = sorted_dedup(personas);
    Some(DeliverablePersonas {
        deliverable_id: deliverable_id.to_string(),
        count: personas.len(),
        personas,
        via_journey_ids,
    })
}

/// The journeys that target `persona`: by the reference, or by the field
/// naming it.
fn journeys_of_persona(graph: &CommandGraph, persona: &str) -> Vec<String> {
    let by_field = graph
        .nodes_of_kind("journey")
        .filter(|j| j.text("persona") == Some(persona))
        .map(|j| j.id.clone());
    sorted_dedup(
        into(graph, persona, "persona", "journey")
            .into_iter()
            .chain(by_field)
            .collect(),
    )
}

/// The journeys that use `channel`.
fn journeys_of_channel(graph: &CommandGraph, channel: &str) -> Vec<String> {
    into(graph, channel, "channels", "journey")
}

/// The features `deliverable` reaches through its journeys, and through
/// its modules, each sorted, once each.
fn features_of_deliverable(graph: &CommandGraph, deliverable: &str) -> (Vec<String>, Vec<String>) {
    let through = |label: &str, kind: &str| {
        sorted_dedup(
            out(graph, deliverable, label, kind)
                .iter()
                .flat_map(|hop| out(graph, hop, "features", "feature"))
                .collect(),
        )
    };
    (through("journeys", "journey"), through("modules", "module"))
}

/// The deliverables that reach `feature` through a journey, and through a
/// module, each sorted, once each.
pub fn deliverables_of_feature(graph: &CommandGraph, feature: &str) -> (Vec<String>, Vec<String>) {
    let through = |kind: &str, label: &str| {
        sorted_dedup(
            into(graph, feature, "features", kind)
                .iter()
                .flat_map(|hop| into(graph, hop, label, "deliverable"))
                .collect(),
        )
    };
    (through("journey", "journeys"), through("module", "modules"))
}

// ── Status and progress rollups ────────────────────────────────────────────

/// `DeliverableCompletionPayload`: how many of the deliverable's milestones
/// are completed, and (on request) each one's own feature completion.
#[derive(Debug, Serialize)]
pub struct DeliverableCompletion {
    pub deliverable_id: String,
    pub milestone_count: usize,
    pub completed_count: usize,
    pub completion_ratio: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milestone_details: Option<Vec<MilestoneCompletion>>,
}

/// The milestones the deliverable is tracked by and how many are
/// `completed` (a milestone without a status is `planned`); the ratio is
/// 0 without milestones. `details` adds each milestone's completion, by
/// id. `None` when `deliverable_id` is not a deliverable.
pub fn deliverable_completion(
    graph: &CommandGraph,
    deliverable_id: &str,
    details: bool,
) -> Option<DeliverableCompletion> {
    of_kind(graph, deliverable_id, "deliverable")?;
    let milestones = out(graph, deliverable_id, "milestones", "milestone");
    let completed_count = milestones
        .iter()
        .filter(|m| status_is(graph, m, MILESTONE_COMPLETED))
        .count();
    Some(DeliverableCompletion {
        deliverable_id: deliverable_id.to_string(),
        milestone_count: milestones.len(),
        completed_count,
        completion_ratio: ratio(completed_count, milestones.len()),
        milestone_details: details.then(|| {
            milestones
                .iter()
                .filter_map(|m| milestone_completion(graph, m))
                .collect()
        }),
    })
}

/// `ReleaseCompletionPayload`: how many of the release's deliverables are
/// shipped.
#[derive(Debug, Serialize)]
pub struct ReleaseCompletion {
    pub release_id: String,
    pub total: usize,
    pub shipped: usize,
    pub completion_ratio: Option<f64>,
}

/// The deliverables the release includes and how many are `shipped`; the
/// ratio is `null` without deliverables. `None` when `release_id` is not a
/// release.
pub fn release_completion(graph: &CommandGraph, release_id: &str) -> Option<ReleaseCompletion> {
    of_kind(graph, release_id, "release")?;
    let deliverables = out(graph, release_id, "deliverables", "deliverable");
    let shipped = deliverables
        .iter()
        .filter(|d| status_is(graph, d, DELIVERABLE_SHIPPED))
        .count();
    Some(ReleaseCompletion {
        release_id: release_id.to_string(),
        total: deliverables.len(),
        shipped,
        completion_ratio: share(shipped, deliverables.len()),
    })
}

/// `DeliverablePriorityPayload`: the highest priority among the
/// deliverable's milestones and journeys that declare one.
#[derive(Debug, Serialize)]
pub struct DeliverablePriority {
    pub deliverable_id: String,
    pub priority: Option<String>,
    pub source_count: usize,
}

/// The highest `Priority` (critical first) among the milestones and
/// journeys the deliverable references that declare one; a constituent
/// without a priority is left out, not counted as medium. `priority` is
/// `null` when none declares one. `None` when `deliverable_id` is not a
/// deliverable.
pub fn deliverable_priority(
    graph: &CommandGraph,
    deliverable_id: &str,
) -> Option<DeliverablePriority> {
    of_kind(graph, deliverable_id, "deliverable")?;
    let ranks: Vec<usize> = out(graph, deliverable_id, "milestones", "milestone")
        .into_iter()
        .chain(out(graph, deliverable_id, "journeys", "journey"))
        .filter_map(|id| priority_rank(graph.node(&id)?.text("priority")?))
        .collect();
    Some(DeliverablePriority {
        deliverable_id: deliverable_id.to_string(),
        priority: ranks
            .iter()
            .min()
            .map(|&r| PRIORITY.values()[r].to_string()),
        source_count: ranks.len(),
    })
}

/// `UnscheduledFeaturesPayload`: the features no milestone delivers.
#[derive(Debug, Serialize)]
pub struct UnscheduledFeatures {
    pub features: Vec<String>,
    pub count: usize,
    pub total_features: usize,
    pub scheduled_count: usize,
    /// Each unscheduled feature's status as written, for the human table.
    #[serde(skip)]
    pub statuses: Vec<Option<String>>,
}

/// The features no milestone lists under `features`, sorted by id.
pub fn unscheduled_features(graph: &CommandGraph) -> UnscheduledFeatures {
    let mut all: Vec<&GraphNode> = graph.nodes_of_kind("feature").collect();
    all.sort_by(|a, b| a.id.cmp(&b.id));
    let total_features = all.len();
    let unscheduled: Vec<&GraphNode> = all
        .into_iter()
        .filter(|f| count_in(graph, &f.id, "features", "milestone") == 0)
        .collect();
    UnscheduledFeatures {
        count: unscheduled.len(),
        scheduled_count: total_features - unscheduled.len(),
        total_features,
        statuses: unscheduled.iter().map(|f| text(f, "status")).collect(),
        features: unscheduled.into_iter().map(|f| f.id.clone()).collect(),
    }
}

/// The kinds with an `owner` field, as `OwnerKindBreakdown` names them.
const OWNED_KINDS: &[(&str, &str)] = &[
    ("feature", "features"),
    ("milestone", "milestones"),
    ("deliverable", "deliverables"),
    ("release", "releases"),
];

/// `OwnerWorkloadEntry`: what one owner owns.
#[derive(Debug, Serialize)]
pub struct OwnerWorkloadEntry {
    pub owner: String,
    pub entity_ids: Vec<String>,
    pub entity_count: usize,
    pub by_kind: OwnerKindBreakdown,
}

/// `OwnerKindBreakdown`.
#[derive(Debug, Default, Serialize)]
pub struct OwnerKindBreakdown {
    pub features: usize,
    pub milestones: usize,
    pub deliverables: usize,
    pub releases: usize,
}

impl OwnerKindBreakdown {
    fn count(&mut self, kind: &str) {
        match kind {
            "feature" => self.features += 1,
            "milestone" => self.milestones += 1,
            "deliverable" => self.deliverables += 1,
            _ => self.releases += 1,
        }
    }
}

/// `OwnerWorkloadPayload` before paging: every owner of a feature,
/// milestone, deliverable or release, most entities first (ties by owner),
/// and how many of those entities have no owner.
#[derive(Debug)]
pub struct OwnerWorkload {
    pub owners: Vec<OwnerWorkloadEntry>,
    pub unowned_count: usize,
    pub total_entities: usize,
}

/// Each owner string once (trimmed; an empty one is no owner), with the
/// ids it owns sorted.
pub fn owner_workload(graph: &CommandGraph) -> OwnerWorkload {
    let mut owners: BTreeMap<String, OwnerWorkloadEntry> = BTreeMap::new();
    let (mut unowned_count, mut total_entities) = (0, 0);
    for (kind, _) in OWNED_KINDS {
        for node in graph.nodes_of_kind(kind) {
            total_entities += 1;
            let Some(owner) = node.text("owner").map(str::trim).filter(|o| !o.is_empty()) else {
                unowned_count += 1;
                continue;
            };
            let entry = owners
                .entry(owner.to_string())
                .or_insert_with(|| OwnerWorkloadEntry {
                    owner: owner.to_string(),
                    entity_ids: Vec::new(),
                    entity_count: 0,
                    by_kind: OwnerKindBreakdown::default(),
                });
            entry.entity_ids.push(node.id.clone());
            entry.entity_count += 1;
            entry.by_kind.count(kind);
        }
    }
    let mut owners: Vec<OwnerWorkloadEntry> = owners
        .into_values()
        .map(|mut e| {
            e.entity_ids.sort();
            e
        })
        .collect();
    owners.sort_by(|a, b| {
        b.entity_count
            .cmp(&a.entity_count)
            .then_with(|| a.owner.cmp(&b.owner))
    });
    OwnerWorkload {
        owners,
        unowned_count,
        total_entities,
    }
}

// ── Dependency graphs ──────────────────────────────────────────────────────

/// The `depends_on` references among one kind's entities, by index into
/// `ids` (sorted, so an index orders as its id does). A reference to an
/// entity of another kind, or one left out, is not followed.
struct DepGraph {
    ids: Vec<String>,
    /// Each entity's dependencies, sorted, once each.
    succ: Vec<Vec<usize>>,
}

impl DepGraph {
    /// The `kind` entities `keep` admits and their `depends_on` among them.
    fn of(graph: &CommandGraph, kind: &str, keep: impl Fn(&GraphNode) -> bool) -> Self {
        let mut ids: Vec<String> = graph
            .nodes_of_kind(kind)
            .filter(|n| keep(n))
            .map(|n| n.id.clone())
            .collect();
        ids.sort();
        ids.dedup();
        let succ = ids
            .iter()
            .map(|id| {
                let mut deps: Vec<usize> = targets(graph, id, "depends_on")
                    .iter()
                    .filter_map(|t| ids.binary_search(t).ok())
                    .collect();
                deps.sort_unstable();
                deps.dedup();
                deps
            })
            .collect();
        DepGraph { ids, succ }
    }

    fn index(&self, id: &str) -> Option<usize> {
        self.ids.binary_search_by(|i| i.as_str().cmp(id)).ok()
    }

    /// Each entity's level (Kahn): 0 for one without dependencies, else one
    /// more than its deepest dependency's, which is the length in edges of
    /// its longest dependency chain. `None` for an entity on a cycle or
    /// depending on one, whose chains do not end.
    fn levels(&self) -> Vec<Option<usize>> {
        let n = self.ids.len();
        let mut dependents = vec![Vec::new(); n];
        for (u, deps) in self.succ.iter().enumerate() {
            for &v in deps {
                dependents[v].push(u);
            }
        }
        let mut remaining: Vec<usize> = self.succ.iter().map(Vec::len).collect();
        let mut level: Vec<Option<usize>> = vec![None; n];
        let mut ready: std::collections::VecDeque<usize> =
            (0..n).filter(|&u| remaining[u] == 0).collect();
        for &u in &ready {
            level[u] = Some(0);
        }
        while let Some(u) = ready.pop_front() {
            let next = level[u].unwrap_or(0) + 1;
            for &d in &dependents[u] {
                level[d] = Some(level[d].map_or(next, |l| l.max(next)));
                remaining[d] -= 1;
                if remaining[d] == 0 {
                    ready.push_back(d);
                }
            }
        }
        // A dependent still waiting on a dependency has no level yet.
        for u in 0..n {
            if remaining[u] > 0 {
                level[u] = None;
            }
        }
        level
    }

    /// The longest dependency chain from `start`, `start` first: at each
    /// step the dependency one level down, the first by id among equals.
    /// `levels` are [`Self::levels`]; `start` must have one.
    fn chain(&self, levels: &[Option<usize>], start: usize) -> Vec<usize> {
        let mut chain = vec![start];
        let mut at = start;
        while let Some(level) = levels[at].filter(|&l| l > 0) {
            let Some(&next) = self.succ[at]
                .iter()
                .find(|&&d| levels[d] == Some(level - 1))
            else {
                break;
            };
            chain.push(next);
            at = next;
        }
        chain
    }

    /// Whether each entity is on a dependency cycle: in a strongly
    /// connected component of two or more, or depending on itself
    /// (Tarjan, iterative, O(V+E)).
    fn in_cycle(&self) -> Vec<bool> {
        const UNSEEN: usize = usize::MAX;
        let n = self.ids.len();
        let (mut index, mut low) = (vec![UNSEEN; n], vec![0; n]);
        let (mut on_stack, mut cyclic) = (vec![false; n], vec![false; n]);
        let (mut stack, mut next) = (Vec::new(), 0);
        for root in 0..n {
            if index[root] != UNSEEN {
                continue;
            }
            index[root] = next;
            low[root] = next;
            next += 1;
            stack.push(root);
            on_stack[root] = true;
            let mut calls: Vec<(usize, usize)> = vec![(root, 0)];
            while let Some(&(v, i)) = calls.last() {
                if let Some(&w) = self.succ[v].get(i) {
                    if let Some(call) = calls.last_mut() {
                        call.1 += 1;
                    }
                    if index[w] == UNSEEN {
                        index[w] = next;
                        low[w] = next;
                        next += 1;
                        stack.push(w);
                        on_stack[w] = true;
                        calls.push((w, 0));
                    } else if on_stack[w] {
                        low[v] = low[v].min(index[w]);
                    }
                    continue;
                }
                calls.pop();
                if let Some(&(parent, _)) = calls.last() {
                    low[parent] = low[parent].min(low[v]);
                }
                if low[v] == index[v] {
                    let mut component = Vec::new();
                    while let Some(w) = stack.pop() {
                        on_stack[w] = false;
                        component.push(w);
                        if w == v {
                            break;
                        }
                    }
                    if component.len() > 1 || self.succ[v].contains(&v) {
                        for w in component {
                            cyclic[w] = true;
                        }
                    }
                }
            }
        }
        cyclic
    }

    /// The ids of the entities `cyclic` marks, sorted.
    fn members(&self, cyclic: &[bool]) -> Vec<String> {
        (0..self.ids.len())
            .filter(|&i| cyclic[i])
            .map(|i| self.ids[i].clone())
            .collect()
    }
}

/// `FeatureOrderingPayload`: the features in dependency order.
#[derive(Debug, Serialize)]
pub struct FeatureOrdering {
    pub sorted_features: Vec<String>,
    pub has_cycles: bool,
    pub cycle_members: Vec<String>,
}

/// Every feature once, dependencies before dependents: by level (Kahn),
/// within a level by priority (critical first, none counts as medium),
/// then by id. The features on a cycle or depending on one have no level
/// and come last, ordered the same way; the ones on a cycle are the
/// `cycle_members`.
pub fn feature_ordering(graph: &CommandGraph) -> FeatureOrdering {
    let deps = DepGraph::of(graph, "feature", |_| true);
    let levels = deps.levels();
    let rank = |i: usize| {
        graph
            .node(&deps.ids[i])
            .and_then(|n| n.text("priority"))
            .and_then(priority_rank)
            .unwrap_or(MEDIUM)
    };
    let mut order: Vec<usize> = (0..deps.ids.len()).collect();
    order.sort_by_key(|&i| (levels[i].unwrap_or(usize::MAX), rank(i), i));
    let cycle_members = deps.members(&deps.in_cycle());
    FeatureOrdering {
        sorted_features: order.into_iter().map(|i| deps.ids[i].clone()).collect(),
        has_cycles: !cycle_members.is_empty(),
        cycle_members,
    }
}

/// `CriticalPathNode`.
#[derive(Debug, Serialize)]
pub struct CriticalPathNode {
    pub entity_id: String,
    pub entity_kind: String,
    pub target_date: Option<String>,
    pub status: Option<String>,
    pub slack_days: Option<i64>,
}

/// `CriticalPathPayload`.
#[derive(Debug, Serialize)]
pub struct CriticalPath {
    pub critical_path: Vec<CriticalPathNode>,
    pub path_length: usize,
    pub earliest_completion: Option<String>,
    pub latest_completion: Option<String>,
    pub bottleneck_ids: Vec<String>,
    /// Why there is no path when milestones depend on each other in a
    /// cycle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// The longest `depends_on` chain of milestones not yet `completed`,
/// earliest first (each milestone before the ones depending on it); the
/// first by id among equally long ones. Every node on it has zero slack
/// (none without a `target_date`); the completions are the first's and
/// the last's target dates; the bottlenecks are its `blocked` or
/// `in_progress` milestones. When milestones depend on each other in a
/// cycle (E015) there is no critical path: the path is empty and the
/// message names the cycle.
pub fn critical_path(graph: &CommandGraph) -> CriticalPath {
    let empty = |message: Option<String>| CriticalPath {
        critical_path: Vec::new(),
        path_length: 0,
        earliest_completion: None,
        latest_completion: None,
        bottleneck_ids: Vec::new(),
        message,
    };
    let all = DepGraph::of(graph, "milestone", |_| true);
    let cycle = all.members(&all.in_cycle());
    if !cycle.is_empty() {
        return empty(Some(format!(
            "milestones depend on each other in a cycle ({}): no critical path",
            cycle.join(", ")
        )));
    }
    let open = DepGraph::of(graph, "milestone", |n| {
        status(n) != Some(MILESTONE_COMPLETED)
    });
    let levels = open.levels();
    let Some(start) = (0..open.ids.len()).max_by_key(|&i| (levels[i], std::cmp::Reverse(i))) else {
        return empty(None);
    };
    let mut chain = open.chain(&levels, start);
    chain.reverse();
    let nodes: Vec<CriticalPathNode> = chain
        .iter()
        .filter_map(|&i| graph.node(&open.ids[i]))
        .map(|m| {
            let target_date = text(m, "target_date");
            CriticalPathNode {
                entity_id: m.id.clone(),
                entity_kind: m.kind.clone(),
                slack_days: target_date.as_ref().map(|_| 0),
                target_date,
                status: text(m, "status"),
            }
        })
        .collect();
    CriticalPath {
        path_length: nodes.len(),
        earliest_completion: nodes.first().and_then(|n| n.target_date.clone()),
        latest_completion: nodes.last().and_then(|n| n.target_date.clone()),
        bottleneck_ids: nodes
            .iter()
            .filter(|n| matches!(n.status.as_deref(), Some(s) if s == MILESTONE_BLOCKED || s == MILESTONE_IN_PROGRESS))
            .map(|n| n.entity_id.clone())
            .collect(),
        critical_path: nodes,
        message: None,
    }
}

/// `ModuleDependencyDepthPayload`.
#[derive(Debug, Serialize)]
pub struct ModuleDepth {
    pub module_id: String,
    pub depth: i64,
    pub longest_chain: Vec<String>,
}

/// The longest `depends_on` chain from the module, the module first and
/// the leaf last, and its length in edges. A module on a cycle (E007), or
/// depending on one, has no longest chain: its depth is -1 and the chain
/// is the members of the cycles it reaches, sorted. `None` when
/// `module_id` is not a module.
pub fn module_dependency_depth(graph: &CommandGraph, module_id: &str) -> Option<ModuleDepth> {
    of_kind(graph, module_id, "module")?;
    let deps = DepGraph::of(graph, "module", |_| true);
    let start = deps.index(module_id)?;
    let levels = deps.levels();
    let (depth, longest_chain) = match levels[start] {
        Some(depth) => (
            depth as i64,
            deps.chain(&levels, start)
                .into_iter()
                .map(|i| deps.ids[i].clone())
                .collect(),
        ),
        None => {
            let cyclic = deps.in_cycle();
            let mut reached = vec![false; deps.ids.len()];
            let mut frontier = vec![start];
            reached[start] = true;
            while let Some(u) = frontier.pop() {
                for &v in &deps.succ[u] {
                    if !reached[v] {
                        reached[v] = true;
                        frontier.push(v);
                    }
                }
            }
            let on_reached_cycle: Vec<bool> = (0..deps.ids.len())
                .map(|i| reached[i] && cyclic[i])
                .collect();
            (-1, deps.members(&on_reached_cycle))
        }
    };
    Some(ModuleDepth {
        module_id: module_id.to_string(),
        depth,
        longest_chain,
    })
}

/// `ModuleCouplingEntry`.
#[derive(Debug, Serialize)]
pub struct ModuleCouplingEntry {
    pub module_id: String,
    pub fan_in: usize,
    pub fan_out: usize,
    pub coupling: usize,
}

/// `ModuleCouplingPayload` before paging.
#[derive(Debug)]
pub struct ModuleCoupling {
    pub modules: Vec<ModuleCouplingEntry>,
    pub avg_fan_in: Option<f64>,
    pub avg_fan_out: Option<f64>,
    pub most_coupled_id: Option<String>,
    pub total_modules: usize,
}

/// Every module's fan-in (the modules depending on it) and fan-out (the
/// modules it depends on), counting each other module once, most coupled
/// first, ties by id; the averages are over every module (`null` without
/// modules).
pub fn module_coupling(graph: &CommandGraph) -> ModuleCoupling {
    let deps = DepGraph::of(graph, "module", |_| true);
    let mut fan_in = vec![0; deps.ids.len()];
    for d in deps.succ.iter().flatten() {
        fan_in[*d] += 1;
    }
    let mut modules: Vec<ModuleCouplingEntry> = deps
        .ids
        .iter()
        .enumerate()
        .map(|(i, id)| ModuleCouplingEntry {
            module_id: id.clone(),
            fan_in: fan_in[i],
            fan_out: deps.succ[i].len(),
            coupling: fan_in[i] + deps.succ[i].len(),
        })
        .collect();
    modules.sort_by(|a, b| {
        b.coupling
            .cmp(&a.coupling)
            .then_with(|| a.module_id.cmp(&b.module_id))
    });
    let total = modules.len();
    let edges: usize = deps.succ.iter().map(Vec::len).sum();
    ModuleCoupling {
        avg_fan_in: share(edges, total),
        avg_fan_out: share(edges, total),
        most_coupled_id: modules.first().map(|m| m.module_id.clone()),
        total_modules: total,
        modules,
    }
}

/// `DeliverableDependentPayload`: the deliverables that declare
/// `depends_on` the deliverable, sorted by id.
#[derive(Debug, Serialize)]
pub struct DeliverableDependents {
    pub deliverable_id: String,
    pub dependents: Vec<String>,
    pub count: usize,
}

/// The deliverables that declare `depends_on` the deliverable; `None`
/// when `deliverable_id` is not a deliverable.
pub fn deliverable_dependents(
    graph: &CommandGraph,
    deliverable_id: &str,
) -> Option<DeliverableDependents> {
    of_kind(graph, deliverable_id, "deliverable")?;
    let dependents = into(graph, deliverable_id, "depends_on", "deliverable");
    Some(DeliverableDependents {
        deliverable_id: deliverable_id.to_string(),
        count: dependents.len(),
        dependents,
    })
}

// ── Coverage matrices ──────────────────────────────────────────────────────

/// What one persona's or channel's journeys reach (`PersonaCoverageEntry`
/// and `ChannelCoverageEntry` beside the id).
#[derive(Debug, Serialize)]
pub struct Reach {
    pub reachable_features: Vec<String>,
    pub unreachable_features: Vec<String>,
    pub coverage_ratio: f64,
    pub journey_count: usize,
}

/// `PersonaCoverageEntry`.
#[derive(Debug, Serialize)]
pub struct PersonaCoverageEntry {
    pub persona_id: String,
    #[serde(flatten)]
    pub reach: Reach,
}

/// `ChannelCoverageEntry`.
#[derive(Debug, Serialize)]
pub struct ChannelCoverageEntry {
    pub channel_id: String,
    #[serde(flatten)]
    pub reach: Reach,
}

/// A coverage matrix before paging: one entry per persona or channel, by
/// id, over `total_features`; `overall_coverage` is the mean of every
/// entry's ratio (`null` without entries).
#[derive(Debug)]
pub struct CoverageMatrix<T> {
    pub entries: Vec<T>,
    pub total_features: usize,
    pub overall_coverage: Option<f64>,
}

/// `PersonaCoverageMatrixPayload` before paging: the features each
/// persona reaches through the journeys that target it.
pub fn persona_coverage_matrix(graph: &CommandGraph) -> CoverageMatrix<PersonaCoverageEntry> {
    coverage_matrix(
        graph,
        "persona",
        journeys_of_persona,
        |persona_id, reach| PersonaCoverageEntry { persona_id, reach },
    )
}

/// `ChannelCoverageMatrixPayload` before paging: the features each
/// channel reaches through the journeys that use it.
pub fn channel_coverage_matrix(graph: &CommandGraph) -> CoverageMatrix<ChannelCoverageEntry> {
    coverage_matrix(
        graph,
        "channel",
        journeys_of_channel,
        |channel_id, reach| ChannelCoverageEntry { channel_id, reach },
    )
}

/// Each `kind` entity's [`Reach`] over every feature, through the journeys
/// `journeys` gives it (each feature once, however many journeys reach
/// it); a ratio is 0 without journeys, or without features.
fn coverage_matrix<T>(
    graph: &CommandGraph,
    kind: &str,
    journeys: fn(&CommandGraph, &str) -> Vec<String>,
    entry: impl Fn(String, Reach) -> T,
) -> CoverageMatrix<T> {
    let features = sorted_dedup(
        graph
            .nodes_of_kind("feature")
            .map(|f| f.id.clone())
            .collect(),
    );
    let mut ids: Vec<String> = graph.nodes_of_kind(kind).map(|n| n.id.clone()).collect();
    ids = sorted_dedup(ids);
    let mut ratios = 0.0;
    let entries: Vec<T> = ids
        .into_iter()
        .map(|id| {
            let (reachable, via) = through_journeys(graph, journeys(graph, &id));
            let unreachable: Vec<String> = features
                .iter()
                .filter(|f| reachable.binary_search(f).is_err())
                .cloned()
                .collect();
            let coverage_ratio = ratio(reachable.len(), features.len());
            ratios += coverage_ratio;
            entry(
                id,
                Reach {
                    reachable_features: reachable,
                    unreachable_features: unreachable,
                    coverage_ratio,
                    journey_count: via.len(),
                },
            )
        })
        .collect();
    CoverageMatrix {
        overall_coverage: (!entries.is_empty()).then(|| ratios / entries.len() as f64),
        total_features: features.len(),
        entries,
    }
}

/// `FeatureOverlapEntry`.
#[derive(Debug, Serialize)]
pub struct FeatureOverlapEntry {
    pub feature_id: String,
    pub deliverable_ids: Vec<String>,
    pub deliverable_count: usize,
}

/// `FeatureOverlapPayload` before paging.
#[derive(Debug)]
pub struct FeatureOverlap {
    pub overlapping_features: Vec<FeatureOverlapEntry>,
    pub total_features: usize,
}

/// The features two or more deliverables reach, through a journey or a
/// module (a deliverable reaching one both ways counts once), with those
/// deliverables sorted; the most shared first, ties by id.
pub fn feature_overlap(graph: &CommandGraph) -> FeatureOverlap {
    let features = sorted_dedup(
        graph
            .nodes_of_kind("feature")
            .map(|f| f.id.clone())
            .collect(),
    );
    let mut overlapping: Vec<FeatureOverlapEntry> = features
        .iter()
        .filter_map(|f| {
            let (via_journeys, via_modules) = deliverables_of_feature(graph, f);
            let deliverable_ids = sorted_dedup([via_journeys, via_modules].concat());
            (deliverable_ids.len() >= 2).then(|| FeatureOverlapEntry {
                feature_id: f.clone(),
                deliverable_count: deliverable_ids.len(),
                deliverable_ids,
            })
        })
        .collect();
    overlapping.sort_by(|a, b| {
        b.deliverable_count
            .cmp(&a.deliverable_count)
            .then_with(|| a.feature_id.cmp(&b.feature_id))
    });
    FeatureOverlap {
        overlapping_features: overlapping,
        total_features: features.len(),
    }
}

// ── Term analytics ─────────────────────────────────────────────────────────

/// The most `see_also` hops `term_graph` follows; more are clamped to it.
pub const MAX_TERM_HOPS: usize = 5;

/// The `see_also` references between terms: every term by id, and each
/// one's targets (`out`) and its neighbours either way (`adjacent`), by
/// index, sorted, once each. A term's reference to itself, or to an
/// entity that is not a term, is not one.
struct TermGraph {
    ids: Vec<String>,
    out: Vec<Vec<usize>>,
    adjacent: Vec<Vec<usize>>,
}

impl TermGraph {
    fn of(graph: &CommandGraph) -> Self {
        let ids = sorted_dedup(graph.nodes_of_kind("term").map(|t| t.id.clone()).collect());
        let mut out = vec![Vec::new(); ids.len()];
        let mut adjacent = vec![Vec::new(); ids.len()];
        for (u, id) in ids.iter().enumerate() {
            for target in targets(graph, id, "see_also") {
                match ids.binary_search(&target) {
                    Ok(v) if v != u => {
                        out[u].push(v);
                        adjacent[u].push(v);
                        adjacent[v].push(u);
                    }
                    _ => {}
                }
            }
        }
        for list in out.iter_mut().chain(adjacent.iter_mut()) {
            list.sort_unstable();
            list.dedup();
        }
        TermGraph { ids, out, adjacent }
    }

    fn index(&self, id: &str) -> Option<usize> {
        self.ids.binary_search_by(|i| i.as_str().cmp(id)).ok()
    }
}

/// `TermGraphPayload`.
#[derive(Debug, Serialize)]
pub struct TermGraphPayload {
    pub term_id: String,
    pub related_terms: Vec<String>,
    pub max_hops: usize,
}

/// The terms the term reaches over its `see_also` references within
/// `max_hops` (default 1, clamped to [`MAX_TERM_HOPS`]; 0 reaches none),
/// breadth first, each once, sorted by id, the term itself left out.
/// `None` when `term_id` is not a term.
pub fn term_graph(
    graph: &CommandGraph,
    term_id: &str,
    max_hops: Option<usize>,
) -> Option<TermGraphPayload> {
    of_kind(graph, term_id, "term")?;
    let terms = TermGraph::of(graph);
    let start = terms.index(term_id)?;
    let max_hops = max_hops.unwrap_or(1).min(MAX_TERM_HOPS);
    let mut seen = vec![false; terms.ids.len()];
    seen[start] = true;
    let mut frontier = vec![start];
    for _ in 0..max_hops {
        let mut next = Vec::new();
        for u in frontier {
            for &v in &terms.out[u] {
                if !std::mem::replace(&mut seen[v], true) {
                    next.push(v);
                }
            }
        }
        frontier = next;
    }
    seen[start] = false;
    Some(TermGraphPayload {
        term_id: term_id.to_string(),
        related_terms: (0..terms.ids.len())
            .filter(|&i| seen[i])
            .map(|i| terms.ids[i].clone())
            .collect(),
        max_hops,
    })
}

/// `TermCluster`.
#[derive(Debug, Serialize)]
pub struct TermCluster {
    pub cluster_id: usize,
    pub term_ids: Vec<String>,
    pub term_count: usize,
}

/// `TermClusterPayload`.
#[derive(Debug, Serialize)]
pub struct TermClusters {
    pub clusters: Vec<TermCluster>,
    pub cluster_count: usize,
    pub isolated_count: usize,
    pub total_terms: usize,
}

/// The connected components of the `see_also` graph between terms, a
/// reference read either way: each a cluster of its terms sorted by id,
/// largest first, ties by first term id, numbered from 1 in that order.
/// A term without a `see_also` link either way is isolated, in no
/// cluster.
pub fn term_clusters(graph: &CommandGraph) -> TermClusters {
    let terms = TermGraph::of(graph);
    let n = terms.ids.len();
    let mut seen = vec![false; n];
    let mut clusters: Vec<Vec<usize>> = Vec::new();
    let mut isolated_count = 0;
    for root in 0..n {
        if seen[root] {
            continue;
        }
        if terms.adjacent[root].is_empty() {
            isolated_count += 1;
            continue;
        }
        seen[root] = true;
        let (mut component, mut stack) = (vec![root], vec![root]);
        while let Some(u) = stack.pop() {
            for &v in &terms.adjacent[u] {
                if !std::mem::replace(&mut seen[v], true) {
                    component.push(v);
                    stack.push(v);
                }
            }
        }
        component.sort_unstable();
        clusters.push(component);
    }
    // Ids are sorted, so a component's first index is its first id.
    clusters.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));
    let clusters: Vec<TermCluster> = clusters
        .into_iter()
        .enumerate()
        .map(|(i, c)| TermCluster {
            cluster_id: i + 1,
            term_count: c.len(),
            term_ids: c.into_iter().map(|t| terms.ids[t].clone()).collect(),
        })
        .collect();
    TermClusters {
        cluster_count: clusters.len(),
        clusters,
        isolated_count,
        total_terms: n,
    }
}

/// `TermDensityPayload`.
#[derive(Debug, Serialize)]
pub struct TermDensity {
    pub total_terms: usize,
    pub total_see_also: usize,
    pub avg_connections: Option<f64>,
    pub max_connections: usize,
    pub hub_terms: Vec<String>,
    pub isolated_terms: Vec<String>,
}

/// How connected the glossary is: the terms, the `see_also` references
/// between them (each source and target pair once), their average per
/// term (`null` without terms) and each term's connections (the terms it
/// links to or is linked from, each once). A hub has more than twice the
/// average connections and at least 3; an isolated term has none. Both
/// sorted by id.
pub fn term_density(graph: &CommandGraph) -> TermDensity {
    let terms = TermGraph::of(graph);
    let n = terms.ids.len();
    let total_see_also: usize = terms.out.iter().map(Vec::len).sum();
    let avg_connections = share(total_see_also, n);
    let degree = |i: usize| terms.adjacent[i].len();
    let pick = |keep: &dyn Fn(usize) -> bool| -> Vec<String> {
        (0..n)
            .filter(|&i| keep(i))
            .map(|i| terms.ids[i].clone())
            .collect()
    };
    TermDensity {
        total_terms: n,
        total_see_also,
        max_connections: (0..n).map(degree).max().unwrap_or(0),
        hub_terms: pick(&|i| {
            avg_connections.is_some_and(|avg| degree(i) as f64 > 2.0 * avg && degree(i) >= 3)
        }),
        isolated_terms: pick(&|i| degree(i) == 0),
        avg_connections,
    }
}

// ── Dates and effort ───────────────────────────────────────────────────────

/// The days from 1970-01-01 to a `YYYY-MM-DD` date (negative before it);
/// `None` for anything else: another shape, or a day its month does not
/// have.
pub fn parse_ymd(date: &str) -> Option<i64> {
    let b = date.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let digits = |at: std::ops::Range<usize>| -> Option<i64> {
        let part = &b[at];
        part.iter().all(u8::is_ascii_digit).then(|| {
            part.iter()
                .fold(0, |n, &digit| n * 10 + i64::from(digit - b'0'))
        })
    };
    let (year, month, day) = (digits(0..4)?, digits(5..7)?, digits(8..10)?);
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let month_days = [
        31,
        28 + i64::from(leap),
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=12).contains(&month) || day < 1 || day > month_days[(month - 1) as usize] {
        return None;
    }
    // Days from civil (H. Hinnant): years start in March, so a leap day
    // ends one.
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

/// `MilestoneTimelineEntry`: the milestone's fields as written, and
/// whether it is overdue.
#[derive(Debug, Serialize)]
pub struct MilestoneTimelineEntry {
    pub milestone_id: String,
    pub target_date: Option<String>,
    pub status: Option<String>,
    pub is_overdue: bool,
    pub priority: Option<String>,
}

/// `MilestoneTimelinePayload`.
#[derive(Debug, Serialize)]
pub struct MilestoneTimeline {
    pub milestones: Vec<MilestoneTimelineEntry>,
    pub overdue_count: usize,
}

/// Every milestone, by target date, earliest first, then by id; one
/// without a target date that is a date comes after every dated one, by
/// id. A milestone is overdue when its target date is before `as_of`
/// (days, as [`parse_ymd`] counts them) and it is not `completed` (one
/// without a status is `planned`).
pub fn milestone_timeline(graph: &CommandGraph, as_of: i64) -> MilestoneTimeline {
    let mut milestones: Vec<(Option<i64>, MilestoneTimelineEntry)> = graph
        .nodes_of_kind("milestone")
        .map(|m| {
            let date = m.text("target_date").and_then(parse_ymd);
            let entry = MilestoneTimelineEntry {
                milestone_id: m.id.clone(),
                target_date: text(m, "target_date"),
                status: text(m, "status"),
                is_overdue: date.is_some_and(|d| d < as_of)
                    && status(m) != Some(MILESTONE_COMPLETED),
                priority: text(m, "priority"),
            };
            (date, entry)
        })
        .collect();
    milestones.sort_by(|(a, x), (b, y)| {
        (a.is_none(), a, &x.milestone_id).cmp(&(b.is_none(), b, &y.milestone_id))
    });
    let milestones: Vec<MilestoneTimelineEntry> = milestones.into_iter().map(|(_, e)| e).collect();
    MilestoneTimeline {
        overdue_count: milestones.iter().filter(|m| m.is_overdue).count(),
        milestones,
    }
}

/// `MilestoneVelocityPayload`.
#[derive(Debug, Serialize)]
pub struct MilestoneVelocity {
    pub milestone_id: String,
    pub total_features: usize,
    pub done_features: usize,
    pub in_progress_features: usize,
    pub remaining_features: usize,
    pub completion_ratio: Option<f64>,
    pub days_elapsed: Option<i64>,
    pub days_remaining: Option<i64>,
    pub features_per_day: Option<f64>,
}

/// The milestone's features by status (`done`, `in_progress`, the rest
/// remaining; one without a status is `proposed`), the share done (`null`
/// without features), the days from its `start_date` (else its
/// `target_date`) to `as_of`, 0 before it starts (`null` without either
/// date), the features done per elapsed day (`null` while none is done or
/// no day has elapsed) and the days that pace needs for the features not
/// done, rounded up (0 when none is left; `null` without a date or a
/// pace). `None` when `milestone_id` is not a milestone.
pub fn milestone_velocity(
    graph: &CommandGraph,
    milestone_id: &str,
    as_of: i64,
) -> Option<MilestoneVelocity> {
    let node = of_kind(graph, milestone_id, "milestone")?;
    let features = out(graph, milestone_id, "features", "feature");
    let count = |wanted: &str| {
        features
            .iter()
            .filter(|f| status_is(graph, f, wanted))
            .count()
    };
    let (total, done, in_progress) = (
        features.len(),
        count(FEATURE_DONE),
        count(FEATURE_IN_PROGRESS),
    );
    let start = node
        .text("start_date")
        .and_then(parse_ymd)
        .or_else(|| node.text("target_date").and_then(parse_ymd));
    let days_elapsed = start.map(|start| (as_of - start).max(0));
    let features_per_day = days_elapsed
        .filter(|&days| days > 0 && done > 0)
        .map(|days| done as f64 / days as f64);
    let left = total - done;
    // left / (done / days), rounded up, in integers: a float pace can land
    // a whole quotient just above itself and round it up a day.
    let days_remaining = days_elapsed.and_then(|days| {
        if left == 0 {
            Some(0)
        } else {
            features_per_day.map(|_| {
                let days = u64::try_from(days).unwrap_or(0);
                i64::try_from((left as u64 * days).div_ceil(done as u64)).unwrap_or(i64::MAX)
            })
        }
    });
    Some(MilestoneVelocity {
        milestone_id: milestone_id.to_string(),
        total_features: total,
        done_features: done,
        in_progress_features: in_progress,
        remaining_features: total - done - in_progress,
        completion_ratio: share(done, total),
        days_elapsed,
        days_remaining,
        features_per_day,
    })
}

/// Each `Effort` level's weight, in [`EFFORT`]'s order (xs=1, s=2, m=3,
/// l=5, xl=8): the scale's definition, not a setting (ADR 0011).
pub const EFFORT_WEIGHTS: [u64; EFFORT.values().len()] = [1, 2, 3, 5, 8];

/// The level (`m`, in [`EFFORT`]) a feature without an effort on the
/// scale weighs as.
const DEFAULT_EFFORT: usize = EFFORT.index("m");

/// `EffortBreakdownEntry`: how many of the milestone's features have the
/// effort level, and how many of those are done.
#[derive(Debug, Serialize)]
pub struct EffortBreakdownEntry {
    pub effort_level: String,
    pub total: usize,
    pub done: usize,
}

/// `WeightedMilestoneCompletionPayload`.
#[derive(Debug, Serialize)]
pub struct WeightedMilestoneCompletion {
    pub milestone_id: String,
    pub total_effort: u64,
    pub done_effort: u64,
    pub completion_ratio: Option<f64>,
    pub effort_breakdown: Vec<EffortBreakdownEntry>,
}

/// The milestone's features weighted by effort (xs=1, s=2, m=3, l=5,
/// xl=8; a feature without an effort on the scale weighs as m): the
/// weights of all and of the `done` ones, their ratio (`null` without
/// features) and the features per effort level they have, smallest first.
/// `None` when `milestone_id` is not a milestone.
pub fn weighted_milestone_completion(
    graph: &CommandGraph,
    milestone_id: &str,
) -> Option<WeightedMilestoneCompletion> {
    of_kind(graph, milestone_id, "milestone")?;
    let mut levels = [(0, 0); EFFORT.values().len()];
    for f in out(graph, milestone_id, "features", "feature") {
        let level = graph
            .node(&f)
            .and_then(|n| n.text("effort"))
            .and_then(|e| EFFORT.position(e))
            .unwrap_or(DEFAULT_EFFORT);
        levels[level].0 += 1;
        if status_is(graph, &f, FEATURE_DONE) {
            levels[level].1 += 1;
        }
    }
    let weigh = |pick: fn(&(usize, usize)) -> usize| -> u64 {
        levels
            .iter()
            .zip(EFFORT_WEIGHTS)
            .map(|(counts, weight)| pick(counts) as u64 * weight)
            .sum()
    };
    let (total_effort, done_effort) = (weigh(|c| c.0), weigh(|c| c.1));
    Some(WeightedMilestoneCompletion {
        milestone_id: milestone_id.to_string(),
        total_effort,
        done_effort,
        completion_ratio: (total_effort > 0).then(|| done_effort as f64 / total_effort as f64),
        effort_breakdown: levels
            .iter()
            .zip(EFFORT.values())
            .filter(|((total, _), _)| *total > 0)
            .map(|(&(total, done), level)| EffortBreakdownEntry {
                effort_level: (*level).to_string(),
                total,
                done,
            })
            .collect(),
    })
}

/// The weight of the effort level `level` names; an unknown one weighs
/// as m.
pub fn effort_weight(level: &str) -> u64 {
    EFFORT_WEIGHTS[EFFORT.position(level).unwrap_or(DEFAULT_EFFORT)]
}

// ── Project-wide ───────────────────────────────────────────────────────────

/// `BulkStatusPayload`: one entry per lifecycle kind with entities.
#[derive(Debug, Serialize)]
pub struct BulkStatus {
    pub kinds: Vec<KindStatusCounts>,
}

#[derive(Debug, Serialize)]
pub struct KindStatusCounts {
    pub kind: String,
    pub total: usize,
    pub by_status: Vec<StatusCount>,
}

#[derive(Debug, Serialize)]
pub struct StatusCount {
    pub status: String,
    pub count: usize,
}

/// Per lifecycle kind present in the graph, how many entities have each
/// status (`(none)` for those without one).
pub fn bulk_status(graph: &CommandGraph) -> BulkStatus {
    let kinds = vocabulary::LIFECYCLES
        .iter()
        .filter_map(|&(kind, _)| {
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            let mut total = 0;
            for node in graph.nodes_of_kind(kind) {
                total += 1;
                let status = text(node, "status").unwrap_or_else(|| "(none)".to_string());
                *counts.entry(status).or_insert(0) += 1;
            }
            (total > 0).then(|| KindStatusCounts {
                kind: kind.to_string(),
                total,
                by_status: counts
                    .into_iter()
                    .map(|(status, count)| StatusCount { status, count })
                    .collect(),
            })
        })
        .collect();
    BulkStatus { kinds }
}

/// `HealthPayload`: the 0 to 100 project health score and its counts.
#[derive(Debug, Serialize)]
pub struct HealthPayload {
    pub score: HealthScore,
    pub entity_counts: Vec<KindCount>,
    pub unreferenced_counts: Vec<KindUnreferencedCount>,
    pub completeness: HealthCompleteness,
}

#[derive(Debug, Serialize)]
pub struct HealthScore {
    pub overall: f64,
    pub coverage: f64,
    pub connectivity: f64,
    pub completeness: f64,
}

#[derive(Debug, Serialize)]
pub struct KindCount {
    pub kind: String,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct KindUnreferencedCount {
    pub kind: String,
    pub unreferenced: usize,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct HealthCompleteness {
    pub features_with_status: usize,
    pub features_total: usize,
    pub milestones_with_features: usize,
    pub milestones_total: usize,
}

/// A 0-100 score averaging coverage (product entities something references),
/// connectivity (edge density) and completeness (features with a status,
/// milestones with references).
pub fn project_health(graph: &CommandGraph) -> HealthPayload {
    let mut entity_counts = Vec::new();
    let mut unreferenced_counts = Vec::new();
    let (mut total_entities, mut total_unreferenced) = (0usize, 0usize);
    for &kind in PRODUCT_KINDS {
        let count = graph.nodes_of_kind(kind).count();
        let unreferenced = graph
            .nodes_of_kind(kind)
            .filter(|n| graph.edges_to(&n.id).is_empty())
            .count();
        entity_counts.push(KindCount {
            kind: kind.to_string(),
            count,
        });
        if count > 0 {
            unreferenced_counts.push(KindUnreferencedCount {
                kind: kind.to_string(),
                unreferenced,
                total: count,
            });
        }
        total_entities += count;
        total_unreferenced += unreferenced;
    }

    let coverage = if total_entities > 0 {
        pct(total_entities - total_unreferenced, total_entities)
    } else {
        100.0
    };
    let connectivity = if total_entities > 1 {
        let max_edges = total_entities * (total_entities - 1);
        (graph.edges().len() as f64 / max_edges as f64).min(1.0) * 100.0
    } else {
        100.0
    };

    let features_total = graph.nodes_of_kind("feature").count();
    let features_with_status = graph
        .nodes_of_kind("feature")
        .filter(|n| n.has_field("status"))
        .count();
    let milestones_total = graph.nodes_of_kind("milestone").count();
    let milestones_with_features = graph
        .nodes_of_kind("milestone")
        .filter(|n| !graph.edges_from(&n.id).is_empty())
        .count();
    let completeness = if features_total + milestones_total > 0 {
        pct(
            features_with_status + milestones_with_features,
            features_total + milestones_total,
        )
    } else {
        100.0
    };

    HealthPayload {
        score: HealthScore {
            overall: (coverage + connectivity + completeness) / 3.0,
            coverage,
            connectivity,
            completeness,
        },
        entity_counts,
        unreferenced_counts,
        completeness: HealthCompleteness {
            features_with_status,
            features_total,
            milestones_with_features,
            milestones_total,
        },
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn text(node: &GraphNode, field: &str) -> Option<String> {
    node.text(field).map(str::to_string)
}

fn of_kind<'a>(graph: &'a CommandGraph, id: &str, kind: &str) -> Option<&'a GraphNode> {
    graph.node(id).filter(|n| n.kind == kind)
}

fn kind_of<'a>(graph: &'a CommandGraph, id: &str) -> Option<&'a str> {
    graph.node(id).map(|n| n.kind.as_str())
}

/// The targets of `id`'s references declared in `label`.
fn targets(graph: &CommandGraph, id: &str, label: &str) -> Vec<String> {
    graph
        .edges_from(id)
        .iter()
        .filter(|e| e.label == label)
        .map(|e| e.target.clone())
        .collect()
}

/// The `kind` entities `id` references in `label`, sorted, once each.
fn out(graph: &CommandGraph, id: &str, label: &str, kind: &str) -> Vec<String> {
    sorted_dedup(
        targets(graph, id, label)
            .into_iter()
            .filter(|t| kind_of(graph, t) == Some(kind))
            .collect(),
    )
}

/// The `source_kind` entities that reference `id` in `label`, sorted,
/// once each.
fn into(graph: &CommandGraph, id: &str, label: &str, source_kind: &str) -> Vec<String> {
    sorted_dedup(
        graph
            .edges_to(id)
            .iter()
            .filter(|e| e.label == label && kind_of(graph, &e.source) == Some(source_kind))
            .map(|e| e.source.clone())
            .collect(),
    )
}

/// How many of `id`'s references are declared in `label`.
fn count_out(graph: &CommandGraph, id: &str, label: &str) -> usize {
    graph
        .edges_from(id)
        .iter()
        .filter(|e| e.label == label)
        .count()
}

/// How many `source_kind` entities reference `id` in `label`.
fn count_in(graph: &CommandGraph, id: &str, label: &str, source_kind: &str) -> usize {
    graph
        .edges_to(id)
        .iter()
        .filter(|e| e.label == label && kind_of(graph, &e.source) == Some(source_kind))
        .count()
}

/// `ids` sorted, each once.
fn sorted_dedup(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids.dedup();
    ids
}

/// `part / whole` in [0, 1]; 0 when `whole` is 0.
fn ratio(part: usize, whole: usize) -> f64 {
    if whole > 0 {
        part as f64 / whole as f64
    } else {
        0.0
    }
}

/// `part / whole` in [0, 1]; `None` when `whole` is 0.
fn share(part: usize, whole: usize) -> Option<f64> {
    (whole > 0).then(|| part as f64 / whole as f64)
}

/// `node`'s lifecycle status: as written, or its kind's absent status
/// (feature `proposed`, deliverable `draft`, milestone and release `planned`,
/// persona and channel `active`); `None` for a kind without one.
fn status(node: &GraphNode) -> Option<&str> {
    node.text("status")
        .or_else(|| vocabulary::lifecycle(&node.kind)?.absent())
}

/// Whether the entity `id` has the lifecycle status `wanted`.
fn status_is(graph: &CommandGraph, id: &str, wanted: &str) -> bool {
    graph.node(id).and_then(status) == Some(wanted)
}

/// `medium`'s place in `Priority`: a feature without a priority ranks there.
const MEDIUM: usize = PRIORITY.index("medium");

/// `priority`'s place in `Priority`, 0 for critical; `None` outside it.
fn priority_rank(priority: &str) -> Option<usize> {
    PRIORITY.position(priority)
}

// ── Errors ─────────────────────────────────────────────────────────────────

/// The `ENTITY_NOT_FOUND` exit code.
pub const NOT_FOUND_EXIT: i32 = 1;

/// The `INVALID_INPUT` exit code: the one the host gives the usage errors
/// it catches itself.
pub const INVALID_INPUT_EXIT: i32 = 2;

/// `ProductSurfaceError` for an id that names no `kind`: the message names
/// both, and the suggestion is the nearest id of that kind, if any is
/// within edit distance 2.
pub fn not_found(graph: &CommandGraph, kind: &str, id: &str) -> CommandError {
    CommandError {
        entity_id: Some(id.to_string()),
        suggestion: suggest(graph, kind, id),
        ..CommandError::new("ENTITY_NOT_FOUND", format!("{kind} '{id}' not found"))
    }
}

/// `ProductSurfaceError` for an arg the command cannot use.
pub fn invalid_input(message: impl Into<String>) -> CommandError {
    CommandError::new("INVALID_INPUT", message)
}

/// The id of a `kind` entity nearest `id`, within Levenshtein distance 2;
/// the first by id among equally near ones.
pub fn suggest(graph: &CommandGraph, kind: &str, id: &str) -> Option<String> {
    graph
        .nodes_of_kind(kind)
        .map(|n| (levenshtein(&n.id, id), &n.id))
        .filter(|(distance, _)| *distance <= 2)
        .min()
        .map(|(_, near)| near.clone())
}

/// The edit distance between `a` and `b`, by character.
fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (above + 1)
                .min(row[j] + 1)
                .min(diagonal + usize::from(ca != *cb));
            diagonal = above;
        }
    }
    row[b.len()]
}

fn pct(part: usize, whole: usize) -> f64 {
    if whole > 0 {
        (part as f64 / whole as f64) * 100.0
    } else {
        0.0
    }
}
