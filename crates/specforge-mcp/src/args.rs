//! A tool's or prompt's arguments, one definition (ADR 0033).
//!
//! Each core tool and prompt reads its call's `arguments` into one struct
//! that derives [`Arguments`]. Its fields are the arguments: a field's name
//! is the argument's, its doc comment the description, its type how a value
//! is read and the JSON type the listing states ([`Arg`]), and
//! `#[arg(default = …)]`, `#[arg(choice = TABLE)]` or `#[arg(names = …)]`
//! its default, option table (ADR 0027) or name list. The tool's input
//! schema and the prompt's listed arguments are derived from the struct
//! ([`Argument`]), and so is the reading ([`read`]): nothing else lists,
//! defaults or checks an argument.

use serde_json::{Map, Value, json};
use specforge_ops::options::OptionTable;
use specforge_protocol_types::command_args::normalize_arg;
use specforge_protocol_types::{CommandArgDescriptor, CommandArgType};

use crate::target::TargetSpec;
use crate::tool::{ErrorCode, McpError};

pub use specforge_mcp_macros::Arguments;

/// A struct of typed arguments: what a listing shows and how a call is
/// read. Implemented by `#[derive(Arguments)]`, never by hand.
pub trait Arguments: Sized {
    /// Each argument, in field order.
    fn declared() -> Vec<Argument>;
    /// The arguments `given` holds, each read by its field. A name it does
    /// not declare is not looked at (see [`undeclared`]).
    fn read(given: &Given<'_>) -> Result<Self, Box<McpError>>;
}

/// One declared argument, as a listing shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Argument {
    pub name: &'static str,
    /// The field's doc comment, its lines trimmed and joined by spaces.
    pub description: &'static str,
    /// Whether a call cannot be made without it: no default, and its type
    /// has no value for an absent argument.
    pub required: bool,
    /// Its input-schema property: the type's schema ([`Arg::schema`], or the
    /// option table's or name list's `enum`), its `default` when it has one
    /// to advertise, and `description`.
    pub schema: Value,
}

impl Argument {
    /// An argument of type `T` (for `#[derive(Arguments)]`): its `default`
    /// when the field declares one, else the one `T` reads an absent
    /// argument as, when it has a value to advertise (a flag's `false`).
    #[doc(hidden)]
    pub fn typed<T: Arg>(
        name: &'static str,
        description: &'static str,
        default: Option<&T>,
    ) -> Self {
        let advertised = match default {
            Some(value) => T::advertised(value),
            None => T::absent().and_then(|value| T::advertised(&value)),
        };
        let mut schema = T::schema();
        schema["description"] = Value::from(description);
        if let Some(default) = advertised {
            schema["default"] = default;
        }
        Argument {
            name,
            description,
            required: default.is_none() && T::absent().is_none(),
            schema,
        }
    }

    /// An argument that is one of an option table's names (for
    /// `#[derive(Arguments)]`): [`choice_schema`], or
    /// [`required_choice_schema`] when `required`.
    #[doc(hidden)]
    pub fn choice<T: Copy + PartialEq>(
        table: &OptionTable<T>,
        name: &'static str,
        description: &'static str,
        required: bool,
    ) -> Self {
        let schema = if required {
            required_choice_schema(table, description)
        } else {
            choice_schema(table, description)
        };
        Argument {
            name,
            description,
            required,
            schema,
        }
    }

    /// An argument of type `T` whose names are listed, not checked (for
    /// `#[derive(Arguments)]`): `names` is the `enum` of a string, of a
    /// list's items.
    #[doc(hidden)]
    pub fn names<T: Arg>(names: &[&str], name: &'static str, description: &'static str) -> Self {
        let mut schema = T::schema();
        let listed = if schema.get("items").is_some() {
            &mut schema["items"]
        } else {
            &mut schema
        };
        listed["enum"] = json!(names);
        schema["description"] = Value::from(description);
        Argument {
            name,
            description,
            required: T::absent().is_none(),
            schema,
        }
    }
}

/// How a field's type reads an argument's JSON value and states it in a
/// listing: one rule for every core tool and prompt, the one extension
/// command tools follow for booleans, integers and strings
/// (`specforge_protocol_types::command_args::normalize_arg`, ADR 0017 D3).
pub trait Arg: Sized {
    /// The type's schema: `{"type": "boolean"}`, `{"type": "integer",
    /// "minimum": 0}`, …
    fn schema() -> Value;
    /// What an absent (or `null`) argument reads as when the type has a
    /// value for it: `false` for a flag, none for an option, an empty list.
    /// `None` makes the argument required unless its field declares a
    /// default.
    fn absent() -> Option<Self>;
    /// The `default` a listing states for `value`: a flag's, a count's, a
    /// string's; none for an option or a list.
    fn advertised(value: &Self) -> Option<Value>;
    /// `value` (never `null`) read as this type, or the refusal's message
    /// naming `name` (`strict must be true or false, got 'yes'`).
    fn read(name: &str, value: &Value) -> Result<Self, String>;
}

/// `got` as a refusal shows it: a string quoted `'..'`, anything else as
/// JSON (`normalize_arg`'s wording).
fn shown(got: &Value) -> String {
    match got {
        Value::String(text) => format!("'{text}'"),
        other => other.to_string(),
    }
}

/// `value` through the rule extension commands follow (`normalize_arg`):
/// the normalized value, or its message.
fn normalized(
    name: &str,
    arg_type: CommandArgType,
    minimum: Option<i64>,
    value: &Value,
) -> Result<Value, String> {
    let arg = CommandArgDescriptor {
        name: name.to_string(),
        arg_type,
        required: false,
        default_value: None,
        description: None,
        minimum,
    };
    normalize_arg(&arg, value).map_err(|error| error.message())
}

impl Arg for String {
    fn schema() -> Value {
        json!({ "type": "string" })
    }
    fn absent() -> Option<Self> {
        None
    }
    fn advertised(value: &Self) -> Option<Value> {
        Some(Value::from(value.as_str()))
    }
    fn read(name: &str, value: &Value) -> Result<Self, String> {
        normalized(name, CommandArgType::String, None, value)
            .map(|text| text.as_str().unwrap_or_default().to_string())
    }
}

impl Arg for bool {
    fn schema() -> Value {
        json!({ "type": "boolean" })
    }
    fn absent() -> Option<Self> {
        Some(false)
    }
    fn advertised(value: &Self) -> Option<Value> {
        Some(Value::from(*value))
    }
    fn read(name: &str, value: &Value) -> Result<Self, String> {
        normalized(name, CommandArgType::Bool, None, value)
            .map(|flag| flag.as_bool().unwrap_or_default())
    }
}

impl Arg for usize {
    fn schema() -> Value {
        json!({ "type": "integer", "minimum": 0 })
    }
    fn absent() -> Option<Self> {
        None
    }
    fn advertised(value: &Self) -> Option<Value> {
        Some(Value::from(*value))
    }
    fn read(name: &str, value: &Value) -> Result<Self, String> {
        normalized(name, CommandArgType::Integer, Some(0), value).map(|count| {
            count
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or_default()
        })
    }
}

/// A number: a JSON number, or a string holding a finite one.
impl Arg for f64 {
    fn schema() -> Value {
        json!({ "type": "number" })
    }
    fn absent() -> Option<Self> {
        None
    }
    fn advertised(value: &Self) -> Option<Value> {
        Some(json!(value))
    }
    fn read(name: &str, value: &Value) -> Result<Self, String> {
        let number = match value {
            Value::Number(n) => n.as_f64(),
            Value::String(text) => text.trim().parse::<f64>().ok().filter(|n| n.is_finite()),
            _ => None,
        };
        number.ok_or_else(|| format!("{name} must be a number, got {}", shown(value)))
    }
}

impl Arg for Vec<String> {
    fn schema() -> Value {
        json!({ "type": "array", "items": { "type": "string" } })
    }
    fn absent() -> Option<Self> {
        Some(Vec::new())
    }
    fn advertised(_: &Self) -> Option<Value> {
        None
    }
    fn read(name: &str, value: &Value) -> Result<Self, String> {
        value
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .map(|item| item.as_str().map(String::from))
                    .collect()
            })
            .ok_or_else(|| format!("{name} must be a list of strings, got {}", shown(value)))
    }
}

/// A list of strings that holds at least one: required, and refused empty.
#[derive(Debug, Clone)]
pub struct NonEmpty(pub Vec<String>);

impl Arg for NonEmpty {
    fn schema() -> Value {
        json!({ "type": "array", "items": { "type": "string" }, "minItems": 1 })
    }
    fn absent() -> Option<Self> {
        None
    }
    fn advertised(_: &Self) -> Option<Value> {
        None
    }
    fn read(name: &str, value: &Value) -> Result<Self, String> {
        let items = Vec::<String>::read(name, value)?;
        if items.is_empty() {
            return Err(format!("{name} must list at least one string, got []"));
        }
        Ok(NonEmpty(items))
    }
}

impl Arg for Map<String, Value> {
    fn schema() -> Value {
        json!({ "type": "object", "additionalProperties": true })
    }
    fn absent() -> Option<Self> {
        None
    }
    fn advertised(_: &Self) -> Option<Value> {
        None
    }
    fn read(name: &str, value: &Value) -> Result<Self, String> {
        value
            .as_object()
            .cloned()
            .ok_or_else(|| format!("{name} must be an object, got {}", shown(value)))
    }
}

/// An agent plan argument: any JSON value, listed as an object; the
/// operation reads it (an `AgentPlan` object or JSON text of one,
/// `specforge_ops::plan::check`).
#[derive(Debug, Clone)]
pub struct AgentPlan(pub Value);

impl Arg for AgentPlan {
    fn schema() -> Value {
        json!({ "type": "object" })
    }
    fn absent() -> Option<Self> {
        None
    }
    fn advertised(_: &Self) -> Option<Value> {
        None
    }
    fn read(_: &str, value: &Value) -> Result<Self, String> {
        Ok(AgentPlan(value.clone()))
    }
}

/// Entity ids: a list of strings, or one comma-separated string (MCP sends
/// prompt arguments as strings); blank entries dropped.
#[derive(Debug, Clone, Default)]
pub struct EntityIds(pub Vec<String>);

impl Arg for EntityIds {
    fn schema() -> Value {
        json!({ "type": "array", "items": { "type": "string" } })
    }
    fn absent() -> Option<Self> {
        Some(EntityIds::default())
    }
    fn advertised(_: &Self) -> Option<Value> {
        None
    }
    fn read(name: &str, value: &Value) -> Result<Self, String> {
        let ids: Option<Vec<String>> = match value {
            Value::String(ids) => Some(ids.split(',').map(String::from).collect()),
            Value::Array(items) => items
                .iter()
                .map(|item| item.as_str().map(String::from))
                .collect(),
            _ => None,
        };
        ids.map(|ids| {
            EntityIds(
                ids.into_iter()
                    .map(|id| id.trim().to_string())
                    .filter(|id| !id.is_empty())
                    .collect(),
            )
        })
        .ok_or_else(|| {
            format!(
                "{name} must be a list of entity ids or a comma-separated string, got {}",
                shown(value)
            )
        })
    }
}

/// An optional argument: absent is none, a value is read as `T`.
impl<T: Arg> Arg for Option<T> {
    fn schema() -> Value {
        T::schema()
    }
    fn absent() -> Option<Self> {
        Some(None)
    }
    fn advertised(value: &Self) -> Option<Value> {
        value.as_ref().and_then(T::advertised)
    }
    fn read(name: &str, value: &Value) -> Result<Self, String> {
        T::read(name, value).map(Some)
    }
}

/// The arguments of one call, read field by field by the derived
/// [`Arguments::read`].
pub struct Given<'a>(&'a Map<String, Value>);

impl Given<'_> {
    /// The value of `name`, when the call gives one: `null` is absent.
    fn value(&self, name: &str) -> Option<&Value> {
        self.0.get(name).filter(|value| !value.is_null())
    }

    /// The name of an enumerated argument as given: a string, or absent.
    fn text(&self, name: &'static str) -> Result<Option<String>, Box<McpError>> {
        self.value(name)
            .map(|value| String::read(name, value))
            .transpose()
            .map_err(|message| Box::new(invalid(name, message)))
    }

    /// Field `name`: a value given (not `null`) is [`Arg::read`]; an absent
    /// one is `default`, else [`Arg::absent`], else `Missing required
    /// parameter: {name}`. Each refusal is `invalid_input` on `name`.
    pub fn field<T: Arg>(
        &self,
        name: &'static str,
        default: Option<fn() -> T>,
    ) -> Result<T, Box<McpError>> {
        match self.value(name) {
            Some(value) => T::read(name, value).map_err(|message| Box::new(invalid(name, message))),
            None => default
                .map(|default| default())
                .or_else(T::absent)
                .ok_or_else(|| Box::new(missing(name))),
        }
    }

    /// An enumerated field of the table's value type: absent is the
    /// table's default (the table must have one); a name is
    /// [`OptionTable::parse`], its refusal `invalid_input` on `name`
    /// (ADR 0027 D5).
    pub fn choice<T: Copy + PartialEq>(
        &self,
        table: &OptionTable<T>,
        name: &'static str,
    ) -> Result<T, Box<McpError>> {
        let given = self.text(name)?;
        table
            .parse_or_default(given.as_deref())
            .map_err(|error| Box::new(McpError::from(error).with_argument(name)))
    }

    /// An enumerated `Option<_>` field: absent is none.
    pub fn optional_choice<T: Copy + PartialEq>(
        &self,
        table: &OptionTable<T>,
        name: &'static str,
    ) -> Result<Option<T>, Box<McpError>> {
        let given = self.text(name)?;
        table
            .parse_optional(given.as_deref())
            .map_err(|error| Box::new(McpError::from(error).with_argument(name)))
    }

    /// An enumerated `String` field: the name as given, required; the
    /// handler parses it (render, which adds its `available_renderers`).
    pub fn choice_name<T: Copy + PartialEq>(
        &self,
        _table: &OptionTable<T>,
        name: &'static str,
    ) -> Result<String, Box<McpError>> {
        self.text(name)?.ok_or_else(|| Box::new(missing(name)))
    }
}

/// `Missing required parameter: {name}`, on `name`.
fn missing(name: &str) -> McpError {
    McpError::new(
        ErrorCode::InvalidInput,
        format!("Missing required parameter: {name}"),
    )
    .with_argument(name)
}

/// `message`, `invalid_input` on `name`.
fn invalid(name: &str, message: String) -> McpError {
    McpError::new(ErrorCode::InvalidInput, message).with_argument(name)
}

/// `arguments` (an object: the request pipeline refuses any other) read as
/// `A`.
pub fn read<A: Arguments>(arguments: &Value) -> Result<A, Box<McpError>> {
    let none = Map::new();
    let given = match arguments {
        Value::Object(given) => given,
        Value::Null => &none,
        other => {
            return Err(Box::new(McpError::new(
                ErrorCode::InvalidInput,
                format!("arguments must be an object, got {}", shown(other)),
            )));
        }
    };
    A::read(&Given(given))
}

/// The refusal of the first name in `arguments` that neither `declared` nor
/// `target` declares (`path` for every reach but `Unscoped`, `use_cached`
/// for `FreshUnlessCached`: [`TargetSpec::accepted`]): `invalid_input` on
/// that name, `unknown argument '<name>'`, and `did you mean '<close>'?` as
/// its suggestion when a declared name is close. `None` when every name is
/// declared.
pub fn undeclared(
    arguments: &Value,
    declared: &[Argument],
    target: TargetSpec,
) -> Option<McpError> {
    let known: Vec<&str> = declared
        .iter()
        .map(|argument| argument.name)
        .chain(target.accepted().iter().copied())
        .collect();
    unknown_argument(arguments, &known)
}

/// [`undeclared`] over the names `known`: the refusal of the first name in
/// `arguments` that is not one of them.
pub(crate) fn unknown_argument(arguments: &Value, known: &[&str]) -> Option<McpError> {
    let name = arguments
        .as_object()?
        .keys()
        .find(|name| !known.contains(&name.as_str()))?;
    let error = invalid(name, format!("unknown argument '{name}'"));
    Some(
        match specforge_common::suggest::find_close_match(name, known.iter().copied()) {
            Some(close) => {
                error.with_data(json!({ "suggestion": format!("did you mean '{close}'?") }))
            }
            None => error,
        },
    )
}

/// The tool's input schema: `declared`'s properties, then the target's
/// ([`TargetSpec::properties`]); `required` the required ones in field
/// order, then the target's (only when any); `additionalProperties: false`.
pub fn input_schema(declared: &[Argument], target: TargetSpec) -> Value {
    let mut properties = Map::new();
    let mut required: Vec<Value> = Vec::new();
    for argument in declared {
        properties.insert(argument.name.to_string(), argument.schema.clone());
        if argument.required {
            required.push(Value::from(argument.name));
        }
    }
    properties.extend(target.properties());
    required.extend(target.required().iter().map(|name| Value::from(*name)));
    let mut schema = json!({
        "type": "object",
        "properties": properties,
        "additionalProperties": false,
    });
    if !required.is_empty() {
        schema["required"] = Value::Array(required);
    }
    schema
}

/// The arguments of a tool that takes none.
#[derive(Debug, Default, Arguments)]
pub struct NoArgs {}

/// An enumerated argument's input schema (ADR 0027): `type` string, `enum`
/// every name the table accepts (listed names, then aliases, so a
/// validating client may send an alias), `default` the table's (none for a
/// filter), `description` followed by each choice and its help
/// (`Output format: markdown; mermaid: ER diagram; …`).
pub fn choice_schema<T: Copy + PartialEq>(table: &OptionTable<T>, description: &str) -> Value {
    let mut schema = required_choice_schema(table, description);
    if let Some(default) = table.default_name() {
        schema["default"] = Value::from(default);
    }
    schema
}

/// [`choice_schema`] of a required argument: no `default`.
pub fn required_choice_schema<T: Copy + PartialEq>(
    table: &OptionTable<T>,
    description: &str,
) -> Value {
    let choices: Vec<String> = table
        .choices
        .iter()
        .map(|choice| {
            let mut text = choice.name.to_string();
            if !choice.aliases.is_empty() {
                text.push_str(&format!(" (also {})", choice.aliases.join(", ")));
            }
            if !choice.help.is_empty() {
                text.push_str(": ");
                text.push_str(choice.help);
            }
            text
        })
        .collect();
    serde_json::json!({
        "type": "string",
        "enum": table.accepted().collect::<Vec<_>>(),
        "description": format!("{description}: {}", choices.join("; ")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_test_macros::test as specforge_test;

    #[specforge_test(
        behavior = "provide_mcp_analyze_tool",
        verify = "specforge.analyze takes min and returns the run verdict and where the gate landed"
    )]
    fn a_number_is_read_from_a_number_or_a_string_holding_one() {
        assert_eq!(f64::read("min", &json!(92.5)), Ok(92.5));
        assert_eq!(f64::read("min", &json!(50)), Ok(50.0));
        assert_eq!(f64::read("min", &json!(" 50.5 ")), Ok(50.5));
        for bad in [json!(true), json!("many"), json!("NaN"), json!(null)] {
            let message = f64::read("min", &bad).unwrap_err();
            assert!(
                message.starts_with("min must be a number, got "),
                "{message}"
            );
        }
        assert_eq!(
            f64::read("min", &json!(true)),
            Err("min must be a number, got true".to_string())
        );
    }
}
