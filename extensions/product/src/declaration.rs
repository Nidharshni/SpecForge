//! What @specforge/product declares: its kinds, edges, shared fields, enhancements, validation rules, passes, feature flags, with the SDK builders.
//! The host loads exactly this (`ContributionsBuilder::declaration`);
//! `crates/runtime/component/tests/declarations/` pins its wire form.

use specforge_extension_sdk::prelude::*;

mod edges;
mod kinds;
mod rules;

/// Declare everything this module holds on `c`.
pub(crate) fn declare(c: &mut ContributionsBuilder) {
    kinds::kinds(c);
    edges::edges(c);
    shared_fields(c);
    rules::rules(c);
}

fn shared_fields(c: &mut ContributionsBuilder) {
    c.shared_field("tags", |f| {
        f.field_type(FieldType::StringList)
            .description("Freeform labels for filtering and categorization");
    });
}
