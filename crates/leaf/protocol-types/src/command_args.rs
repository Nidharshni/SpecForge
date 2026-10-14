//! The one rule for a command's args, run by the host and by the SDK guest
//! alike (ADR 0017): which declarations a command may make, and what its
//! `cmd__` export receives for the arguments a caller gave.
//!
//! The host normalizes the args it sends on both surfaces (the command
//! line's parsed values, an MCP tool call's arguments) with
//! [`normalize_args`]; the SDK runs the same function on what it receives,
//! so a guest not built with the SDK gets the same args, and an SDK guest
//! called directly refuses what the host would.

use serde_json::{Map, Value};
use std::fmt;

use crate::{CommandArgDescriptor, CommandArgType, CommandError};

/// The error code of an arg value the rule refuses: what every surface
/// writes, the command line under `--format json`, an MCP command tool, the
/// SDK (ADR 0011).
pub const INVALID_INPUT: &str = "INVALID_INPUT";

/// The options the host gives every extension command on the command line
/// (`--path`, `--format`, `--help`), which no declared arg may take.
pub const HOST_OPTIONS: &[&str] = &["path", "format", "help"];

/// An arg's name as an option on the command line: `_` spelled `-`
/// (`sort_order` is `--sort-order`). Two args whose names spell one option
/// are one arg twice.
pub fn option_name(name: &str) -> String {
    name.replace('_', "-")
}

/// Why an arg value is refused: the `INVALID_INPUT` message every surface
/// writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgError {
    /// Required args the caller left out, in declaration order.
    Missing { names: Vec<String> },
    /// A value that is not one of the arg's values.
    NotOneOf {
        name: String,
        values: Vec<String>,
        got: Value,
    },
    /// A value that is not an integer, or one below the arg's minimum.
    NotInteger {
        name: String,
        minimum: Option<i64>,
        got: Value,
    },
    /// A value of a string or path arg that is not a string.
    NotString { name: String, got: Value },
    /// A value of a flag that is neither `true` nor `false`.
    NotBool { name: String, got: Value },
    /// An argument the command does not declare, named as the caller
    /// spelled it, and the declared one it is close to.
    Unknown {
        name: String,
        suggestion: Option<String>,
    },
}

impl ArgError {
    /// The message, naming the arg as declared: a value the caller gave as
    /// a string is quoted `'..'`, any other is its JSON.
    pub fn message(&self) -> String {
        match self {
            ArgError::Missing { names } => {
                let s = if names.len() > 1 { "s" } else { "" };
                let names: Vec<String> = names.iter().map(|n| format!("'{n}'")).collect();
                format!("missing required arg{s} {}", names.join(", "))
            }
            ArgError::NotOneOf { name, values, got } => {
                format!(
                    "{name} must be one of {}, got {}",
                    values.join(", "),
                    shown(got)
                )
            }
            ArgError::NotInteger { name, minimum, got } => {
                let what = match minimum {
                    None => "an integer".to_string(),
                    Some(0) => "a non-negative integer".to_string(),
                    Some(minimum) => format!("an integer of at least {minimum}"),
                };
                format!("{name} must be {what}, got {}", shown(got))
            }
            ArgError::NotString { name, got } => {
                format!("{name} must be a string, got {}", shown(got))
            }
            ArgError::NotBool { name, got } => {
                format!("{name} must be true or false, got {}", shown(got))
            }
            ArgError::Unknown { name, .. } => format!("unknown argument '{name}'"),
        }
    }

    /// What the caller may have meant: the declared arg an unknown one is
    /// close to, or the value of a `one_of` a refused one is close to.
    pub fn suggestion(&self) -> Option<&str> {
        match self {
            ArgError::Unknown { suggestion, .. } => suggestion.as_deref(),
            ArgError::NotOneOf { values, got, .. } => {
                got.as_str().and_then(|got| close_match(got, values))
            }
            _ => None,
        }
    }

    /// The command error object (`{code: INVALID_INPUT, message,
    /// suggestion?}`), as commands write it.
    pub fn to_command_error(&self) -> CommandError {
        CommandError {
            suggestion: self.suggestion().map(str::to_string),
            ..CommandError::new(INVALID_INPUT, self.message())
        }
    }

    /// [`Self::to_command_error`] as JSON.
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self.to_command_error())
            .expect("a command error serializes to a JSON object")
    }
}

impl fmt::Display for ArgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for ArgError {}

/// `got` as a message shows it: a string quoted `'..'`, anything else as
/// JSON.
fn shown(got: &Value) -> String {
    match got {
        Value::String(s) => format!("'{s}'"),
        other => other.to_string(),
    }
}

/// One declared arg's value, normalized to its type: a string or path a
/// string, an enum one of its values, an integer an integer at least its
/// minimum, a flag a boolean. An integer or a boolean may come as a string
/// holding one (`"5"`, `"true"`), as a command line gives it.
pub fn normalize_arg(arg: &CommandArgDescriptor, value: &Value) -> Result<Value, ArgError> {
    let name = arg.name.clone();
    match &arg.arg_type {
        CommandArgType::String | CommandArgType::Path => match value {
            Value::String(_) => Ok(value.clone()),
            _ => Err(ArgError::NotString {
                name,
                got: value.clone(),
            }),
        },
        CommandArgType::Enum { values } => match value.as_str() {
            Some(s) if values.iter().any(|v| v == s) => Ok(value.clone()),
            _ => Err(ArgError::NotOneOf {
                name,
                values: values.clone(),
                got: value.clone(),
            }),
        },
        CommandArgType::Integer => {
            let n = match value {
                Value::Number(n) => n.as_i64(),
                Value::String(s) => s.parse::<i64>().ok(),
                _ => None,
            };
            match n {
                Some(n) if arg.minimum.is_none_or(|minimum| n >= minimum) => Ok(Value::from(n)),
                // Below the minimum: the integer it is, however it came.
                Some(n) => Err(ArgError::NotInteger {
                    name,
                    minimum: arg.minimum,
                    got: Value::from(n),
                }),
                None => Err(ArgError::NotInteger {
                    name,
                    minimum: arg.minimum,
                    got: value.clone(),
                }),
            }
        }
        CommandArgType::Bool => match value {
            Value::Bool(_) => Ok(value.clone()),
            Value::String(s) if s == "true" || s == "false" => Ok(Value::Bool(s == "true")),
            _ => Err(ArgError::NotBool {
                name,
                got: value.clone(),
            }),
        },
    }
}

/// The args a command's export receives for the arguments `given`: every
/// declared arg the caller set, normalized ([`normalize_arg`]); an absent
/// one its declared default, normalized; an unset flag `false` (a flag is
/// never required: it is `false` unless set). Refused: an argument the
/// command does not declare, a value its arg's type refuses, then any
/// required arg left out.
pub fn normalize_args(
    args: &[CommandArgDescriptor],
    given: &Map<String, Value>,
) -> Result<Map<String, Value>, ArgError> {
    if let Some(unknown) = given.keys().find(|k| !args.iter().any(|a| a.name == **k)) {
        let names: Vec<String> = args.iter().map(|a| a.name.clone()).collect();
        return Err(ArgError::Unknown {
            name: unknown.clone(),
            suggestion: close_match(unknown, &names).map(str::to_string),
        });
    }
    let mut normalized = Map::new();
    let mut missing = Vec::new();
    for arg in args {
        let value = match (given.get(&arg.name), &arg.default_value) {
            (Some(value), _) => normalize_arg(arg, value)?,
            (None, _) if arg.arg_type == CommandArgType::Bool => Value::Bool(false),
            (None, Some(default)) => normalize_arg(arg, &Value::String(default.clone()))?,
            (None, None) if arg.required => {
                missing.push(arg.name.clone());
                continue;
            }
            (None, None) => continue,
        };
        normalized.insert(arg.name.clone(), value);
    }
    if missing.is_empty() {
        Ok(normalized)
    } else {
        Err(ArgError::Missing { names: missing })
    }
}

/// Why the host refuses a command's arg declarations, if it does: it runs
/// on neither surface (the command line refuses it, exit 2; MCP lists no
/// tool for it), and the SDK refuses to build an extension declaring it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgRefusal {
    /// An arg takes an option the host gives every command
    /// ([`HOST_OPTIONS`]).
    HostOption { arg: String, option: &'static str },
    /// Two args spell one option ([`option_name`]): `all_kinds` and
    /// `all-kinds`.
    Twice { name: String },
    /// A default its arg's type refuses.
    BadDefault { arg: String, error: ArgError },
    /// A flag with a default: a flag is `false` unless set.
    FlagWithDefault { arg: String },
    /// A required arg with a default.
    RequiredWithDefault { arg: String },
}

impl fmt::Display for ArgRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArgRefusal::HostOption { arg, option } => {
                write!(f, "its arg '{arg}' takes the host's --{option}")
            }
            ArgRefusal::Twice { name } => write!(f, "it declares the arg '{name}' twice"),
            ArgRefusal::BadDefault { arg, error } => {
                write!(f, "its arg '{arg}' has a default its type refuses: {error}")
            }
            ArgRefusal::FlagWithDefault { arg } => write!(
                f,
                "its flag '{arg}' has a default, but a flag is false unless set"
            ),
            ArgRefusal::RequiredWithDefault { arg } => {
                write!(f, "its arg '{arg}' is both required and with a default")
            }
        }
    }
}

/// The first refusal of `args`, in declaration order, if any
/// ([`ArgRefusal`]).
pub fn refusal(args: &[CommandArgDescriptor]) -> Option<ArgRefusal> {
    let mut seen: Vec<String> = Vec::new();
    for arg in args {
        let name = option_name(&arg.name);
        if let Some(option) = HOST_OPTIONS.iter().find(|o| **o == name) {
            return Some(ArgRefusal::HostOption {
                arg: arg.name.clone(),
                option,
            });
        }
        if seen.contains(&name) {
            return Some(ArgRefusal::Twice { name });
        }
        seen.push(name);
        let Some(default) = &arg.default_value else {
            continue;
        };
        if arg.arg_type == CommandArgType::Bool {
            return Some(ArgRefusal::FlagWithDefault {
                arg: arg.name.clone(),
            });
        }
        if arg.required {
            return Some(ArgRefusal::RequiredWithDefault {
                arg: arg.name.clone(),
            });
        }
        if let Err(error) = normalize_arg(arg, &Value::String(default.clone())) {
            return Some(ArgRefusal::BadDefault {
                arg: arg.name.clone(),
                error,
            });
        }
    }
    None
}

/// The candidate `target` is a likely misspelling of: the one fewest edits
/// away, at most two and at most half the longer one's length (the first in order
/// among equals).
fn close_match<'a>(target: &str, candidates: &'a [String]) -> Option<&'a str> {
    candidates
        .iter()
        .map(|c| (c, edit_distance(target, c)))
        .filter(|(c, d)| *d > 0 && *d <= 2 && *d * 2 <= target.len().max(c.len()))
        .min_by_key(|(_, d)| *d)
        .map(|(c, _)| c.as_str())
}

/// Levenshtein distance, by characters.
fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = if ca == *cb {
                diagonal
            } else {
                1 + diagonal.min(above).min(row[j])
            };
            diagonal = above;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests;
