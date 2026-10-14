//! An option table as clap's possible values (ADR 0027).

use clap::builder::{PossibleValue, PossibleValuesParser, TypedValueParser};
use specforge_ops::options::OptionTable;

/// The value parser of an enumerated flag: the table's names as possible
/// values, each with its help (aliases accepted, not listed), parsed by the
/// table. Pair it with `default_value = TABLE.default_name()`.
pub(crate) fn choice<T>(table: &'static OptionTable<T>) -> impl TypedValueParser<Value = T>
where
    T: Copy + PartialEq + Send + Sync + 'static,
{
    PossibleValuesParser::new(table.choices.iter().map(|choice| {
        let value = PossibleValue::new(choice.name).aliases(choice.aliases.iter().copied());
        if choice.help.is_empty() {
            value
        } else {
            value.help(choice.help)
        }
    }))
    .try_map(move |name: String| table.parse(&name))
}
