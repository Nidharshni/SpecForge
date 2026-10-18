//! The extension vocabulary: the names an extension uses for a field's type,
//! a validation rule's check kind and its constraint's kind.
//!
//! These enums are the one place the names are defined. The SDK writes them
//! (`as_str`), the host's registry build reads them (`parse`). On the wire
//! the descriptors still carry plain strings (`FieldDescriptor::field_type`,
//! `ValidationRuleDescriptor::check`, `FieldConstraintDescriptor::kind`), so a name this host does not know
//! costs the one field or rule a diagnostic instead of failing the whole
//! describe payload.
//!
//! `parse` also accepts the older spellings already in the wild — the
//! `_type`-suffixed field types of hand-written manifests and the names
//! earlier SDK releases emitted — so extensions built against them load
//! unchanged.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

macro_rules! vocabulary {
    (
        $(#[$meta:meta])*
        $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident = $canonical:literal ),+ $(,)?
        }
        aliases { $( $alias:literal => $target:ident ),* $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $( $(#[$vmeta])* $variant, )+
        }

        impl $name {
            /// Every variant, in declaration order.
            pub const ALL: &'static [$name] = &[ $( $name::$variant, )+ ];

            /// The canonical wire name.
            pub fn as_str(self) -> &'static str {
                match self {
                    $( $name::$variant => $canonical, )+
                }
            }

            /// Read a wire name: the canonical name or an accepted older
            /// spelling. `None` for a name this host does not know.
            pub fn parse(name: &str) -> Option<Self> {
                match name {
                    $( $canonical => Some($name::$variant), )+
                    $( $alias => Some($name::$target), )*
                    _ => None,
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let name = std::borrow::Cow::<'de, str>::deserialize(deserializer)?;
                $name::parse(&name).ok_or_else(|| {
                    de::Error::unknown_variant(&name, &[ $( $canonical, )+ ])
                })
            }
        }
    };
}

vocabulary! {
    /// How the host reads a field's value. Its name ([`FieldType::as_str`])
    /// is the one every output writes: the declaration, the Graph Protocol
    /// schema and exports, the published JSON Schema, the model, the hover
    /// and E061 (ADR 0034). The older spellings are read, never written.
    FieldType {
        /// A quoted string.
        String = "string",
        /// A whole number.
        Integer = "integer",
        /// `true` or `false`.
        Bool = "bool",
        /// One of the field's `enum_values`.
        Enum = "enum",
        /// One word from an open set (a tag): written bare (ADR 0147); a
        /// value that is not a word is E061.
        Identifier = "identifier",
        /// A list of quoted strings.
        StringList = "string_list",
        /// One entity id; creates an edge.
        Reference = "reference",
        /// A list of entity ids; creates one edge per id.
        ReferenceList = "reference_list",
        /// A triple-quoted text block.
        Block = "block",
    }
    aliases {
        "string_type" => String,
        "integer_type" => Integer,
        "bool_type" => Bool,
        "boolean" => Bool,
        "enum_type" => Enum,
        "string_list_type" => StringList,
        "reference_type" => Reference,
        "reference_list_type" => ReferenceList,
        "block_type" => Block,
    }
}

impl FieldType {
    /// A value of this type names entities, one edge per id (`reference`,
    /// `reference_list`).
    pub fn is_reference(self) -> bool {
        matches!(self, FieldType::Reference | FieldType::ReferenceList)
    }

    /// A value of this type is a list (`string_list`, `reference_list`); a
    /// single value written on such a field is read as a one-item list.
    pub fn is_list(self) -> bool {
        matches!(self, FieldType::StringList | FieldType::ReferenceList)
    }
}

vocabulary! {
    /// What the prove pass reads a field as (ADR 0009). On the wire
    /// `FieldDescriptor::proof_role` stays a string, so an unknown role
    /// costs the field its role (W021), not the describe payload.
    ProofRole {
        /// A fact the solver assumes; the bounds must be consistent (E046).
        Bound = "bound",
        /// A statement that must follow from the bounds (W139 when it does
        /// not).
        Claim = "claim",
    }
    aliases {}
}

vocabulary! {
    /// What a declarative validation rule checks, per entity of its
    /// `target_kind` (every entity when unset).
    ///
    /// The host's registry build checks each rule's shape (ADR 0020): a rule
    /// missing what its check requires is W112 and is not registered; a
    /// property its check does not read is W147 and is dropped. Every
    /// check's message reads `field` as the default `{field}` and its text
    /// as the default `{value}`.
    CheckKind {
        /// No edge points at the entity (only `edge_type` edges, from the
        /// edge type's source kind, when set; an edge type no loaded
        /// extension declares makes the rule inert).
        NoIncomingEdges = "no_incoming_edges",
        /// The entity points at nothing (only `edge_type` edges, to the edge
        /// type's target kind, when set).
        NoOutgoingEdges = "no_outgoing_edges",
        /// The entity has no edges in either direction.
        NoEdges = "no_edges",
        /// The entity lacks `field` (required); an entity that owes no
        /// `verify` statements is exempt when `field` is `verify`.
        MissingFieldWhenFlagSet = "missing_field_when_flag_set",
        /// `field`'s value (required) breaks `constraint` (required:
        /// `non_empty`; `one_of` with non-empty `values`; or `matches` with a
        /// `pattern` that compiles as a regex).
        FieldValueConstraint = "field_value_constraint",
        /// The entity sits on a cycle of `edge_type` edges (`edge_type`
        /// required), following every field that writes that edge type;
        /// every entity when `target_kind` is unset.
        CycleDetection = "cycle_detection",
        /// The path in `field` (required; each item of a list field)
        /// does not exist, relative to the spec root (plan 01's base, ADR
        /// 0019).
        FileExists = "file_exists",
        /// The extension's `wasm_function` (required) decides; a function
        /// that cannot answer is W112 at load, W148 at check.
        Custom = "custom",
        /// When the field named by `constraint.pattern` holds one of
        /// `constraint.values` (both required; the constraint kind is read
        /// as `when_field_equals`), `field` (required) must be present and
        /// non-empty.
        ConditionalFieldRequired = "conditional_field_required",
        /// The entity lacks `field` (required).
        MissingRequiredField = "missing_required_field",
        /// A `verify` kind is not in `constraint.values` (required,
        /// non-empty; the constraint kind is read as `one_of`). The target
        /// kind must accept `verify` statements.
        VerifyKindAllowlist = "verify_kind_allowlist",
        /// A testable entity declares no `verify` obligations (or does not
        /// write `field` when it names another obligation field). The
        /// target kind must accept `verify` statements.
        NoVerifyStatements = "no_verify_statements",
    }
    aliases {
        // Emitted by SDK releases before the vocabulary was shared.
        "missing_field" => MissingRequiredField,
        "field_constraint" => FieldValueConstraint,
        "cycle" => CycleDetection,
        "conditional_required" => ConditionalFieldRequired,
    }
}

vocabulary! {
    /// How a rule's `constraint` reads its `pattern` and `values`.
    ConstraintKind {
        /// `field_value_constraint`: the value must not be empty.
        NonEmpty = "non_empty",
        /// The value must be one of `values` (`field_value_constraint`,
        /// `verify_kind_allowlist`).
        OneOf = "one_of",
        /// `field_value_constraint`: the value must match the regex in
        /// `pattern`.
        Matches = "matches",
        /// `conditional_field_required`: the condition holds when the field
        /// named by `pattern` has one of `values`.
        WhenFieldEquals = "when_field_equals",
    }
    aliases {}
}

#[cfg(test)]
mod tests;
