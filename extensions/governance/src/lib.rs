//! @governance — the governance vocabulary, authored with the extension SDK.
//!
//! Its kinds, edges and rules are declared with the SDK builders in
//! [`declaration`]; the handshake is derived by the SDK from the extension
//! metadata and the contributions.

mod declaration;

use specforge_extension_sdk::prelude::*;

#[specforge_extension_sdk::extension(
    name = "@specforge/governance",
    version = "1.0.0",
    description = "Governance: decisions, constraints and failure modes"
)]
struct Governance;

impl Contributions for Governance {
    fn contribute(c: &mut ContributionsBuilder) {
        // Diagrams (`model`, `outline`) draw the extension in this colour.
        c.theme_color("#e74c3c");
        // Only ConstrainsBehavior targets a software kind (behavior):
        // governance works without software (its manifest spec).
        c.optional_peer("@specforge/software", "^1.0")
            .optional_peer("@specforge/product", "^1.0");

        declaration::declare(c);
    }
}

specforge_extension_sdk::component_guest!(build = specforge_extension_build);
