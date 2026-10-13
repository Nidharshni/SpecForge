//! @specforge/product — the product planning vocabulary and its queries.
//!
//! The vocabulary (kinds, edges, shared fields, rules) is declared with the
//! SDK builders in [`declaration`]; the handshake is derived by the SDK from
//! the extension metadata and the contributions.
//! The CLI commands (`specforge product <command>`, auto-promoted to the MCP
//! tools `specforge.product.<id>`) are declared with their handlers in
//! [`commands`], over the queries in [`queries`]: the SDK derives their
//! `surfaces` payload and routes their `cmd__product_*` exports.

mod commands;
mod declaration;
mod evidence;
mod lifecycle;
mod queries;
#[cfg(test)]
mod tests;
mod vocabulary;

use specforge_extension_sdk::prelude::*;

#[specforge_extension_sdk::extension(
    name = "@specforge/product",
    version = "1.0.0",
    description = "Product planning: features, journeys, deliverables, milestones, modules, terms, personas, channels and releases, and the commands that query them"
)]
struct Product;

impl Contributions for Product {
    fn contribute(c: &mut ContributionsBuilder) {
        // `specforge init` writes this as the starter spec of a project that
        // enables product.
        // Diagrams (`model`, `outline`) draw the extension in this colour.
        c.theme_color("#2ecc71");
        c.starter_template(include_str!("starter.spec"));

        declaration::declare(c);
        commands::declare(c);
        lifecycle::declare(c);
    }
}

specforge_extension_sdk::component_guest!(build = specforge_extension_build);
