//! @software — the software vocabulary, authored with the extension SDK.
//!
//! Its kinds, edges, enhancements and rules are declared with the SDK
//! builders in [`declaration`]; the handshake is derived by the SDK from the
//! extension metadata and the contributions. It also hosts the four
//! `validate__*` custom-rule exports (E004/E006/E010/W010).

mod declaration;

use specforge_extension_sdk::prelude::*;

#[specforge_extension_sdk::extension(
    name = "@specforge/software",
    version = "1.0.0",
    description = "Software design: behaviors, invariants, events, types and ports, and the checks that keep them consistent"
)]
struct Software;

impl Contributions for Software {
    fn contribute(c: &mut ContributionsBuilder) {
        // Optional: product provides the kinds behind the feature, module
        // and milestone links; without it those links are inert (I004).
        // Diagrams (`model`, `outline`) draw the extension in this colour.
        c.theme_color("#4a90d9");
        c.optional_peer("@specforge/product", "^1.0");
        c.meta.sandbox_policy = Some(SandboxPolicy {
            max_memory_mb: Some(256),
            max_execution_ms: Some(5000),
        });
        // `specforge init` writes this as the starter spec of a project that
        // enables software.
        c.starter_template(include_str!("starter.spec"));

        declaration::declare(c);
    }
}

// ── Custom validators (`check: "custom"` rules) ────────────────────────────
// Each `validate__<rule>` export is declared with its rule (`declaration`):
// it receives the protocol's `ValidatorContext` and answers a
// `ValidatorVerdict` (ADR 0013). The host precomputes everything the
// validator needs, so each is a pure function of the context.

use specforge_extension_sdk::{ValidatorContext, ValidatorVerdict};

/// Type names accepted by E004 without a declared `type` entity. Mirrors
/// the host's `PRIMITIVE_TYPES`; the host also sends its copy in
/// `context.primitives`, so the union keeps the guest correct even for a
/// host that ships an empty list.
const PRIMITIVE_TYPES: &[&str] = &[
    "string",
    "void",
    "bool",
    "i8",
    "i16",
    "i32",
    "i64",
    "u8",
    "u16",
    "u32",
    "u64",
    "f32",
    "f64",
    "usize",
    "isize",
    "any",
    // the portable primitives docs/entities/type.md documents, and `never`
    // for an impossible error channel (docs/entities/port.md)
    "number",
    "integer",
    "boolean",
    "timestamp",
    "never",
    // stdlib containers: their type arguments are checked recursively
    "Result",
    "Option",
    "Vec",
    "Box",
    "Arc",
    "Rc",
    "HashMap",
    "HashSet",
    "BTreeMap",
    "BTreeSet",
    "String",
];

/// `Result<A, B>` -> `A, B`; `string[]` -> `string`; whitespace trimmed.
fn base_type_names(ty: &str) -> Vec<String> {
    // Generics, arrays, the unit type `()` and function types
    // `fn(A) -> B` reduce to the named types inside them.
    ty.chars()
        .map(|c| {
            if matches!(c, '<' | '>' | '[' | ']' | '(' | ')' | '-') {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty() && *s != "fn")
        .map(str::to_string)
        .collect()
}

/// Reference IDs declared by a field value. The wire value of a
/// reference-list field is either the comma-joined ID string (the host's
/// `join(", ")` stringification) or an array of IDs.
fn ref_ids(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::String(s) => s
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|v| v.as_str())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn resolved_kind<'a>(context: &'a ValidatorContext, id: &str) -> Option<&'a str> {
    context
        .referenced
        .iter()
        .find(|r| r.id == id)
        .and_then(|r| r.kind.as_deref())
}

/// E006: every event trigger must reference an existing behavior.
fn validate_event_triggers(context: &ValidatorContext) -> ValidatorVerdict {
    for field in &context.entity.fields {
        if field.key != "triggers" {
            continue;
        }
        for id in ref_ids(&field.value) {
            match resolved_kind(context, &id) {
                // The ID names no node in the graph (dangling reference).
                None => {
                    return ValidatorVerdict::Fail {
                        field: Some("triggers".into()),
                        value: Some(id),
                    };
                }
                // The target exists but is not a behavior.
                Some(kind) if kind != "behavior" => {
                    return ValidatorVerdict::Fail {
                        field: Some("triggers".into()),
                        value: Some(id),
                    };
                }
                _ => {}
            }
        }
    }
    ValidatorVerdict::Pass
}

/// E010: milestone behavior references must exist.
fn validate_milestone_behavior_ranges(context: &ValidatorContext) -> ValidatorVerdict {
    for field in &context.entity.fields {
        if field.key != "behaviors" {
            continue;
        }
        for id in ref_ids(&field.value) {
            if resolved_kind(context, &id).is_none() {
                return ValidatorVerdict::Fail {
                    field: Some("behaviors".into()),
                    value: Some(id),
                };
            }
        }
    }
    ValidatorVerdict::Pass
}

/// W010: type fields may only carry known annotations.
fn validate_type_field_annotations(context: &ValidatorContext) -> ValidatorVerdict {
    const KNOWN: &[&str] = &["readonly", "unique", "optional", "literal"];
    for field in &context.entity.fields {
        for ann in &field.annotations {
            if !KNOWN.contains(&ann.as_str()) {
                return ValidatorVerdict::Fail {
                    field: Some(field.key.clone()),
                    value: Some(format!("@{ann}")),
                };
            }
        }
    }
    ValidatorVerdict::Pass
}

/// E004: port method param/return types must be primitives or declared types.
fn validate_port_methods(context: &ValidatorContext) -> ValidatorVerdict {
    let known_type = |name: &str| -> bool {
        PRIMITIVE_TYPES.contains(&name)
            || context.primitives.iter().any(|p| p == name)
            || context.declared_types.iter().any(|d| d == name)
    };
    for method in &context.entity.methods {
        let mut type_refs: Vec<String> = Vec::new();
        for p in &method.params {
            type_refs.extend(base_type_names(&p.ty));
        }
        if let Some(ret) = &method.returns {
            type_refs.extend(base_type_names(ret));
        }
        for t in type_refs {
            if !known_type(&t) {
                return ValidatorVerdict::Fail {
                    field: Some(method.name.clone()),
                    value: Some(t),
                };
            }
        }
    }
    ValidatorVerdict::Pass
}

specforge_extension_sdk::component_guest!(build = specforge_extension_build);

#[cfg(test)]
mod validator_tests;
