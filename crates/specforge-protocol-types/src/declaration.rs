//! The extension declaration: everything one extension declares, its
//! handshake and every describe category, typed.
//!
//! The SDK builds it (`ContributionsBuilder::declaration`), the guest serves
//! it ([`ExtensionDeclaration::describe_items`]), the host loads it once
//! ([`ExtensionDeclaration::from_wire`]), the registry build reads it and a
//! package registry stores it. There is no second, host-side form of it.

use std::borrow::Cow;

use serde::de::{self, DeserializeOwned, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    AnalyzerDescriptor, AutoDetectConfig, CollectorDescriptor, CommandArgDescriptor,
    CommandDescriptor, CompilerPassDescriptor, ContributionFlags, DescribeResponse,
    EdgeTypeDescriptor, EntityEnhancementDescriptor, EntityKindDescriptor, FeatureFlagDescriptor,
    FieldConstraintDescriptor, FieldDescriptor, HandshakeResponse, McpResourceDescriptor,
    McpToolDescriptor, PeerDependency, ProtocolError, SurfaceDescriptor, ValidationRuleDescriptor,
};

/// Everything one extension declares: its handshake and every describe
/// category, typed. The SDK builds it, the guest serves it, the host loads
/// it, the registry build reads it, a package registry stores it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionDeclaration {
    pub handshake: HandshakeResponse,
    #[serde(default)]
    pub entities: Vec<EntityKindDescriptor>,
    #[serde(default)]
    pub edges: Vec<EdgeTypeDescriptor>,
    #[serde(default)]
    pub shared_fields: Vec<FieldDescriptor>,
    #[serde(default)]
    pub enhancements: Vec<EntityEnhancementDescriptor>,
    #[serde(default)]
    pub validation_rules: Vec<ValidationRuleDescriptor>,
    #[serde(default)]
    pub surfaces: SurfaceDescriptor,
    #[serde(default)]
    pub collectors: Vec<CollectorDescriptor>,
    #[serde(default)]
    pub analyzers: Vec<AnalyzerDescriptor>,
    #[serde(default)]
    pub passes: Vec<CompilerPassDescriptor>,
    #[serde(default)]
    pub feature_flags: Vec<FeatureFlagDescriptor>,
}

/// A describe category, in the order the host reads them (ADR 0012 D2). The
/// protocol defines no other: a guest answers these ten and nothing else
/// (ADR 0050).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeclaredCategory {
    Entities,
    Edges,
    SharedFields,
    Enhancements,
    ValidationRules,
    Surfaces,
    Collectors,
    Analyzers,
    Passes,
    FeatureFlags,
}

impl DeclaredCategory {
    /// Every describe category, in the host's read order: the protocol's one
    /// list of them.
    pub const ALL: [DeclaredCategory; 10] = [
        DeclaredCategory::Entities,
        DeclaredCategory::Edges,
        DeclaredCategory::SharedFields,
        DeclaredCategory::Enhancements,
        DeclaredCategory::ValidationRules,
        DeclaredCategory::Surfaces,
        DeclaredCategory::Collectors,
        DeclaredCategory::Analyzers,
        DeclaredCategory::Passes,
        DeclaredCategory::FeatureFlags,
    ];

    /// Its wire name (`shared_fields`).
    pub const fn name(self) -> &'static str {
        match self {
            DeclaredCategory::Entities => "entities",
            DeclaredCategory::Edges => "edges",
            DeclaredCategory::SharedFields => "shared_fields",
            DeclaredCategory::Enhancements => "enhancements",
            DeclaredCategory::ValidationRules => "validation_rules",
            DeclaredCategory::Surfaces => "surfaces",
            DeclaredCategory::Collectors => "collectors",
            DeclaredCategory::Analyzers => "analyzers",
            DeclaredCategory::Passes => "passes",
            DeclaredCategory::FeatureFlags => "feature_flags",
        }
    }

    /// The category named `name`; `None` for a name the protocol does not
    /// define.
    pub fn from_name(name: &str) -> Option<DeclaredCategory> {
        DeclaredCategory::ALL
            .into_iter()
            .find(|category| category.name() == name)
    }

    /// The shape of its descriptors, for the unknown-key walk.
    fn shape(self) -> &'static Shape {
        match self {
            DeclaredCategory::Entities => &KIND,
            DeclaredCategory::Edges => &EDGE,
            DeclaredCategory::SharedFields => &FIELD,
            DeclaredCategory::Enhancements => &ENHANCEMENT,
            DeclaredCategory::ValidationRules => &RULE,
            DeclaredCategory::Surfaces => &SURFACE,
            DeclaredCategory::Collectors => &COLLECTOR,
            DeclaredCategory::Analyzers => &ANALYZER,
            DeclaredCategory::Passes => &PASS,
            DeclaredCategory::FeatureFlags => &FEATURE_FLAG,
        }
    }
}

/// The names of [`DeclaredCategory::ALL`], in order.
pub const DECLARED_CATEGORIES: &[&str] = &DECLARED_NAMES;

const DECLARED_NAMES: [&str; DeclaredCategory::ALL.len()] = {
    let mut names = [""; DeclaredCategory::ALL.len()];
    let mut index = 0;
    while index < names.len() {
        names[index] = DeclaredCategory::ALL[index].name();
        index += 1;
    }
    names
};

/// A key of a describe item that the protocol does not define: a typo in
/// a hand-written category, or a field of a newer SDK this host does not
/// know. It is ignored; the host reports it (W138).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownKey {
    /// The describe category the item came in.
    pub category: &'static str,
    /// The item, by its name (`behavior`), and for a nested item the path
    /// to it (`behavior.fields[style]`); `#<index>` for an unnamed one.
    pub item: String,
    pub key: String,
}

/// The last segment of an extension name, its default short name
/// (`@specforge/product` is `product`).
pub(crate) fn default_short(name: &str) -> &str {
    name.rsplit('/')
        .next()
        .unwrap_or(name)
        .trim_start_matches('@')
}

/// Whether `short` is a valid short name: lowercase kebab case
/// (`[a-z][a-z0-9-]*`), so it can name a CLI subcommand and an MCP tool
/// prefix.
pub fn is_valid_short(short: &str) -> bool {
    let mut chars = short.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

impl ExtensionDeclaration {
    pub fn name(&self) -> &str {
        &self.handshake.name
    }

    pub fn version(&self) -> &str {
        &self.handshake.version
    }

    /// Declared `ext_short`, else the name's last segment
    /// (`@specforge/product` is `product`).
    pub fn short(&self) -> Cow<'_, str> {
        match &self.handshake.ext_short {
            Some(short) => Cow::Borrowed(short.as_str()),
            None => Cow::Borrowed(default_short(self.name())),
        }
    }

    pub fn peers(&self) -> &[PeerDependency] {
        &self.handshake.peer_dependencies
    }

    /// Every kind's `verify_kinds`, deduplicated, in declaration order.
    pub fn verify_kinds(&self) -> Vec<&str> {
        let mut kinds: Vec<&str> = Vec::new();
        for kind in self.entities.iter().flat_map(|k| &k.verify_kinds) {
            if !kinds.contains(&kind.as_str()) {
                kinds.push(kind);
            }
        }
        kinds
    }

    /// What the declaration contributes. The flags of the declared
    /// categories are derived from their content (never trusted to gate
    /// what is read); the others (`providers`, and the reserved ones) are
    /// the handshake's.
    pub fn contribution_flags(&self) -> ContributionFlags {
        ContributionFlags {
            entities: !self.entities.is_empty()
                || !self.edges.is_empty()
                || !self.shared_fields.is_empty()
                || !self.enhancements.is_empty(),
            validators: !self.validation_rules.is_empty(),
            collectors: !self.collectors.is_empty(),
            analyzers: !self.analyzers.is_empty(),
            ..self.handshake.contribution_flags.clone()
        }
    }

    /// Whether it declares anything in `category`: a non-empty list, or for
    /// `surfaces` a descriptor with a command, an MCP tool or an MCP
    /// resource. What an extension contributes is this, read from its
    /// content, never a flag it sets (ADR 0050).
    pub fn declares(&self, category: DeclaredCategory) -> bool {
        match category {
            DeclaredCategory::Entities => !self.entities.is_empty(),
            DeclaredCategory::Edges => !self.edges.is_empty(),
            DeclaredCategory::SharedFields => !self.shared_fields.is_empty(),
            DeclaredCategory::Enhancements => !self.enhancements.is_empty(),
            DeclaredCategory::ValidationRules => !self.validation_rules.is_empty(),
            DeclaredCategory::Surfaces => self.surfaces != SurfaceDescriptor::default(),
            DeclaredCategory::Collectors => !self.collectors.is_empty(),
            DeclaredCategory::Analyzers => !self.analyzers.is_empty(),
            DeclaredCategory::Passes => !self.passes.is_empty(),
            DeclaredCategory::FeatureFlags => !self.feature_flags.is_empty(),
        }
    }

    /// The categories it declares something in, in the host's read order.
    pub fn declared_categories(&self) -> impl Iterator<Item = DeclaredCategory> + '_ {
        DeclaredCategory::ALL
            .into_iter()
            .filter(|category| self.declares(*category))
    }

    /// The wire `items` of `category`: as declared, `surfaces` as one
    /// descriptor (none when nothing is declared).
    pub fn describe_items(&self, category: DeclaredCategory) -> Value {
        match category {
            DeclaredCategory::Entities => to_items(&self.entities),
            DeclaredCategory::Edges => to_items(&self.edges),
            DeclaredCategory::SharedFields => to_items(&self.shared_fields),
            DeclaredCategory::Enhancements => to_items(&self.enhancements),
            DeclaredCategory::ValidationRules => to_items(&self.validation_rules),
            DeclaredCategory::Surfaces if self.surfaces == SurfaceDescriptor::default() => {
                Value::Array(vec![])
            }
            DeclaredCategory::Surfaces => to_items(std::slice::from_ref(&self.surfaces)),
            DeclaredCategory::Collectors => to_items(&self.collectors),
            DeclaredCategory::Analyzers => to_items(&self.analyzers),
            DeclaredCategory::Passes => to_items(&self.passes),
            DeclaredCategory::FeatureFlags => to_items(&self.feature_flags),
        }
    }

    /// Replace `category`'s content with `items`, the `items` of its
    /// describe answer. Err: they do not parse as its descriptors
    /// ([`ProtocolError::DescribeFailed`] naming the category), and the
    /// declaration is unchanged.
    pub fn set_category(
        &mut self,
        category: DeclaredCategory,
        items: &Value,
    ) -> Result<(), ProtocolError> {
        let name = category.name();
        match category {
            DeclaredCategory::Entities => self.entities = parse(name, items)?,
            DeclaredCategory::Edges => self.edges = parse(name, items)?,
            DeclaredCategory::SharedFields => self.shared_fields = parse(name, items)?,
            DeclaredCategory::Enhancements => self.enhancements = parse(name, items)?,
            DeclaredCategory::ValidationRules => self.validation_rules = parse(name, items)?,
            DeclaredCategory::Surfaces => {
                self.surfaces = parse::<SurfaceDescriptor>(name, items)?
                    .into_iter()
                    .next()
                    .unwrap_or_default()
            }
            DeclaredCategory::Collectors => self.collectors = parse(name, items)?,
            DeclaredCategory::Analyzers => self.analyzers = parse(name, items)?,
            DeclaredCategory::Passes => self.passes = parse(name, items)?,
            DeclaredCategory::FeatureFlags => self.feature_flags = parse(name, items)?,
        }
        Ok(())
    }

    /// The `__handshake` wire answer: the handshake, pretty JSON.
    pub fn handshake_json(&self) -> String {
        serde_json::to_string_pretty(&self.handshake).expect("handshake serialization cannot fail")
    }

    /// The `__describe` wire answer for `category` (its
    /// [`describe_items`](Self::describe_items) in the response envelope,
    /// pretty JSON).
    pub fn describe_json(&self, category: DeclaredCategory) -> String {
        DescribeResponse {
            category: category.name().to_string(),
            items: self.describe_items(category),
        }
        .wire_json()
    }

    /// Assemble a declaration from its wire answers: `describe` fetches one
    /// category's answer, for each of [`DeclaredCategory::ALL`] in order. A
    /// category whose items do not parse is
    /// [`ProtocolError::DescribeFailed`] naming it; `unknown` receives each
    /// item key a descriptor does not define (a host reports it as W138).
    pub fn from_wire(
        handshake: HandshakeResponse,
        mut describe: impl FnMut(DeclaredCategory) -> Result<DescribeResponse, ProtocolError>,
        mut unknown: impl FnMut(UnknownKey),
    ) -> Result<Self, ProtocolError> {
        let mut declaration = ExtensionDeclaration {
            handshake,
            ..Default::default()
        };
        for category in DeclaredCategory::ALL {
            let items = describe(category)?.items;
            declaration.set_category(category, &items)?;
            report_unknown_keys(category, &items, &mut unknown);
        }
        Ok(declaration)
    }
}

/// `descriptors` as wire items.
fn to_items<T: Serialize>(descriptors: &[T]) -> Value {
    serde_json::to_value(descriptors).expect("descriptors serialize")
}

/// `items` of `category` as descriptors; the error names the category.
fn parse<T: DeserializeOwned>(category: &str, items: &Value) -> Result<Vec<T>, ProtocolError> {
    Vec::<T>::deserialize(items).map_err(|e| ProtocolError::DescribeFailed {
        category: category.to_string(),
        reason: e.to_string(),
    })
}

// ── Unknown keys ──────────────────────────────────────────────────────────
//
// A descriptor ignores the keys it does not define (serde's default), so a
// typo in a hand-written describe item (`testabel`) would cost nothing but
// the field it meant. Each item is walked against the field names its
// descriptor's `Deserialize` accepts, read from the derive itself, so the
// check cannot drift from the types.

/// The shape of a descriptor: the keys it accepts and the keys holding
/// nested descriptors.
struct Shape {
    fields: fn() -> &'static [&'static str],
    nested: &'static [(&'static str, Nested)],
}

enum Nested {
    List(&'static Shape),
    One(&'static Shape),
}

static FIELD: Shape = Shape {
    fields: struct_fields::<FieldDescriptor>,
    nested: &[],
};
static KIND: Shape = Shape {
    fields: struct_fields::<EntityKindDescriptor>,
    nested: &[("fields", Nested::List(&FIELD))],
};
static EDGE: Shape = Shape {
    fields: struct_fields::<EdgeTypeDescriptor>,
    nested: &[],
};
static ENHANCEMENT: Shape = Shape {
    fields: struct_fields::<EntityEnhancementDescriptor>,
    nested: &[
        ("fields", Nested::List(&FIELD)),
        ("edge_types", Nested::List(&EDGE)),
    ],
};
static CONSTRAINT: Shape = Shape {
    fields: struct_fields::<FieldConstraintDescriptor>,
    nested: &[],
};
static RULE: Shape = Shape {
    fields: struct_fields::<ValidationRuleDescriptor>,
    nested: &[("constraint", Nested::One(&CONSTRAINT))],
};
static ARG: Shape = Shape {
    fields: struct_fields::<CommandArgDescriptor>,
    nested: &[],
};
static COMMAND: Shape = Shape {
    fields: struct_fields::<CommandDescriptor>,
    nested: &[("args", Nested::List(&ARG))],
};
static TOOL: Shape = Shape {
    fields: struct_fields::<McpToolDescriptor>,
    nested: &[],
};
static RESOURCE: Shape = Shape {
    fields: struct_fields::<McpResourceDescriptor>,
    nested: &[],
};
static SURFACE: Shape = Shape {
    fields: struct_fields::<SurfaceDescriptor>,
    nested: &[
        ("commands", Nested::List(&COMMAND)),
        ("mcp_tools", Nested::List(&TOOL)),
        ("mcp_resources", Nested::List(&RESOURCE)),
    ],
};
static AUTO_DETECT: Shape = Shape {
    fields: struct_fields::<AutoDetectConfig>,
    nested: &[],
};
static COLLECTOR: Shape = Shape {
    fields: struct_fields::<CollectorDescriptor>,
    nested: &[("auto_detect", Nested::One(&AUTO_DETECT))],
};
static ANALYZER: Shape = Shape {
    fields: struct_fields::<AnalyzerDescriptor>,
    nested: &[],
};
static PASS: Shape = Shape {
    fields: struct_fields::<CompilerPassDescriptor>,
    nested: &[],
};
static FEATURE_FLAG: Shape = Shape {
    fields: struct_fields::<FeatureFlagDescriptor>,
    nested: &[],
};

fn report_unknown_keys(
    category: DeclaredCategory,
    items: &Value,
    unknown: &mut impl FnMut(UnknownKey),
) {
    let Some(items) = items.as_array() else {
        return;
    };
    for (index, item) in items.iter().enumerate() {
        walk(
            category.name(),
            category.shape(),
            &item_name(item, index),
            item,
            unknown,
        );
    }
}

fn walk(
    category: &'static str,
    shape: &Shape,
    path: &str,
    item: &Value,
    unknown: &mut impl FnMut(UnknownKey),
) {
    let Some(object) = item.as_object() else {
        return;
    };
    let known = (shape.fields)();
    for key in object.keys().filter(|key| !known.contains(&key.as_str())) {
        unknown(UnknownKey {
            category,
            item: path.to_string(),
            key: key.clone(),
        });
    }
    for (key, nested) in shape.nested {
        match (nested, object.get(*key)) {
            (Nested::List(shape), Some(Value::Array(children))) => {
                for (index, child) in children.iter().enumerate() {
                    let path = format!("{path}.{key}[{}]", item_name(child, index));
                    walk(category, shape, &path, child, unknown);
                }
            }
            (Nested::One(shape), Some(child)) => {
                walk(category, shape, &format!("{path}.{key}"), child, unknown);
            }
            _ => {}
        }
    }
}

/// How an item is named in an [`UnknownKey`]: by the first identifying key
/// it has, else by its position.
fn item_name(item: &Value, index: usize) -> String {
    [
        "name",
        "label",
        "code",
        "id",
        "target_kind",
        "language",
        "uri_template",
    ]
    .iter()
    .find_map(|key| item.get(*key).and_then(Value::as_str))
    .map(str::to_string)
    .unwrap_or_else(|| format!("#{index}"))
}

/// The keys a `#[derive(Deserialize)]` struct accepts, read from the
/// derive: it hands them to `deserialize_struct`, where this deserializer
/// records them and stops.
fn struct_fields<T: DeserializeOwned>() -> &'static [&'static str] {
    let mut names = FieldNames(None);
    let _ = T::deserialize(&mut names);
    names
        .0
        .expect("a descriptor is a struct deriving Deserialize")
}

struct FieldNames(Option<&'static [&'static str]>);

#[derive(Debug)]
struct Stop;

impl std::fmt::Display for Stop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("field names read")
    }
}

impl std::error::Error for Stop {}

impl de::Error for Stop {
    fn custom<T: std::fmt::Display>(_: T) -> Self {
        Stop
    }
}

impl<'de> de::Deserializer<'de> for &mut FieldNames {
    type Error = Stop;

    fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Stop> {
        Err(Stop)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        _: V,
    ) -> Result<V::Value, Stop> {
        self.0 = Some(fields);
        Err(Stop)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map enum identifier ignored_any
    }
}
