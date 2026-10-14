//! An enumerated argument as one table (ADR 0027).
//!
//! Each argument an operation takes from a closed set of names — an export
//! format, the model's grouping, a coverage status — is one [`OptionTable`]
//! beside the operation that reads it. The CLI builds its possible values
//! and default from the table, MCP its input schema's `enum` and `default`,
//! and both parse with it, so every surface lists, accepts, defaults and
//! refuses the same names. A set the project decides (analysis passes,
//! entity kinds) is not a table: its operation checks it against the view.

use crate::{OpError, OpErrorKind};

/// One accepted value of an enumerated argument.
#[derive(Debug, Clone, Copy)]
pub struct Choice<T: 'static> {
    /// The name every surface lists, echoes and parses (`"markdown"`).
    pub name: &'static str,
    /// Other names parsed as this value, never listed in help or in a
    /// refusal (`"json"` for the `graph` export).
    pub aliases: &'static [&'static str],
    /// What the value selects, one line; empty when the name says it.
    pub help: &'static str,
    pub value: T,
}

/// An enumerated argument: the names it accepts and the value it takes when
/// absent. An unknown name is refused as [`OpErrorKind::InvalidInput`], the
/// one failure kind of a name outside a closed set, reported under that
/// kind's own name (`invalid_input`): a table has no error vocabulary of its
/// own.
#[derive(Debug, Clone, Copy)]
pub struct OptionTable<T: 'static> {
    /// The argument as a refusal names it: `Unknown {argument}: …`.
    pub argument: &'static str,
    /// Every accepted value, in the order surfaces list them.
    pub choices: &'static [Choice<T>],
    /// The value an absent argument takes, on every surface; `None` when
    /// absence selects nothing (a filter).
    pub default: Option<T>,
}

impl<T: Copy + PartialEq + 'static> OptionTable<T> {
    /// The value `name` names: a choice's name or one of its aliases,
    /// exactly. Any other name is `Unknown {argument}: {name}. Expected:
    /// {names}` ([`Self::refusal`]) and, when one is close, `did you mean
    /// '{closest}'?` as its suggestion.
    pub fn parse(&self, name: &str) -> Result<T, OpError> {
        if let Some(choice) = self
            .choices
            .iter()
            .find(|choice| choice.name == name || choice.aliases.contains(&name))
        {
            return Ok(choice.value);
        }
        let error = self.refusal(name);
        Err(
            match specforge_common::suggest::find_close_match(name, self.names()) {
                Some(close) => error.with_suggestion(format!("did you mean '{close}'?")),
                None => error,
            },
        )
    }

    /// The refusal of `name`: `Unknown {argument}: {name}. Expected:
    /// {names}`, the names [`Self::names`] lists (never an alias), also
    /// what a surface offers as the available choices.
    pub fn refusal(&self, name: &str) -> OpError {
        let names: Vec<&str> = self.names().collect();
        OpError::new(
            OpErrorKind::InvalidInput,
            OpErrorKind::InvalidInput.as_str(),
            format!(
                "Unknown {}: {name}. Expected: {}",
                self.argument,
                names.join(", ")
            ),
        )
    }

    /// [`Self::parse`] of an argument that may be absent: absent is the
    /// default (`None` for a table without one).
    pub fn parse_optional(&self, name: Option<&str>) -> Result<Option<T>, OpError> {
        match name {
            Some(name) => self.parse(name).map(Some),
            None => Ok(self.default),
        }
    }

    /// [`Self::parse_optional`] of a table that has a default.
    ///
    /// # Panics
    /// When the table has no default — a programming error each table's
    /// unit test rules out.
    pub fn parse_or_default(&self, name: Option<&str>) -> Result<T, OpError> {
        self.parse_optional(name).map(|value| {
            value.unwrap_or_else(|| panic!("the {} table has no default", self.argument))
        })
    }

    /// The listed name of `value`.
    ///
    /// # Panics
    /// When no choice holds `value` (a table that does not list every value
    /// it is asked to name).
    pub fn name_of(&self, value: T) -> &'static str {
        self.choices
            .iter()
            .find(|choice| choice.value == value)
            .map(|choice| choice.name)
            .unwrap_or_else(|| panic!("the {} table does not list this value", self.argument))
    }

    /// The listed names, in order (what help and refusals show).
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.choices.iter().map(|choice| choice.name)
    }

    /// Every name [`Self::parse`] accepts: the listed names, then the aliases.
    pub fn accepted(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.names().chain(
            self.choices
                .iter()
                .flat_map(|choice| choice.aliases.iter().copied()),
        )
    }

    /// The default's listed name.
    pub fn default_name(&self) -> Option<&'static str> {
        self.default.map(|value| self.name_of(value))
    }

    /// Whether `value` is one of the table's (a table may list a subset of
    /// its type: [`crate::export::AGENT_FORMAT`]).
    pub fn admits(&self, value: T) -> bool {
        self.choices.iter().any(|choice| choice.value == value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Shade {
        Light,
        Dark,
    }

    const SHADE: OptionTable<Shade> = OptionTable {
        argument: "shade",
        choices: &[
            Choice {
                name: "light",
                aliases: &["pale"],
                help: "",
                value: Shade::Light,
            },
            Choice {
                name: "dark",
                aliases: &[],
                help: "",
                value: Shade::Dark,
            },
        ],
        default: Some(Shade::Light),
    };

    #[test]
    fn names_aliases_and_defaults() {
        assert_eq!(SHADE.parse("pale"), Ok(Shade::Light));
        assert_eq!(SHADE.parse_or_default(None), Ok(Shade::Light));
        assert_eq!(SHADE.parse_optional(Some("dark")), Ok(Some(Shade::Dark)));
        assert_eq!(
            SHADE.accepted().collect::<Vec<_>>(),
            ["light", "dark", "pale"]
        );
        assert_eq!(SHADE.default_name(), Some("light"));
        let error = SHADE.parse("drak").unwrap_err();
        assert_eq!(error.kind, OpErrorKind::InvalidInput);
        assert_eq!(error.code, "invalid_input");
        assert_eq!(error.message, "Unknown shade: drak. Expected: light, dark");
        assert_eq!(error.suggestion.as_deref(), Some("did you mean 'dark'?"));
    }
}
