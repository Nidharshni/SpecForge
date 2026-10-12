//! Attribute macro for the SpecForge extension SDK.
//!
//! ```ignore
//! use specforge_extension_sdk::prelude::*;
//!
//! #[specforge_extension_sdk::extension(
//!     name = "@you/greet",
//!     version = "0.1.0",
//!     short = "greet",
//!     description = "Friendly greetings"
//! )]
//! struct Greet;
//!
//! impl specforge_extension_sdk::Contributions for Greet {
//!     fn contribute(c: &mut specforge_extension_sdk::ContributionsBuilder) {
//!         // kinds, edges, rules...
//!     }
//! }
//! ```
//!
//! Generates a `specforge_extension_build()` function serving the author's
//! [`specforge_extension_sdk::Contributions`] impl; pair it with the SDK's
//! `component_guest!` macro for the wasip2 component exports. `version`
//! defaults to the crate's `CARGO_PKG_VERSION`. `short` (the name routing
//! the extension's commands, `specforge <short> <command>`; lowercase kebab
//! case, the name's last segment when absent) and `description` (what a
//! package registry shows) are optional.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use specforge_protocol_types::is_valid_short;

/// The `#[extension(...)]` attributes.
#[derive(Debug, Default, PartialEq)]
struct Attrs {
    name: Option<String>,
    version: Option<String>,
    short: Option<String>,
    description: Option<String>,
}

fn parse_attrs(ts: TokenStream2) -> syn::Result<Attrs> {
    let mut attrs = Attrs::default();
    let parser = syn::meta::parser(|meta| {
        let slot = if meta.path.is_ident("name") {
            &mut attrs.name
        } else if meta.path.is_ident("version") {
            &mut attrs.version
        } else if meta.path.is_ident("short") {
            &mut attrs.short
        } else if meta.path.is_ident("description") {
            &mut attrs.description
        } else {
            return Err(
                meta.error("unsupported property; expected name / version / short / description")
            );
        };
        meta.input.parse::<Token![=]>()?;
        let value = meta.input.parse::<LitStr>()?;
        if meta.path.is_ident("short") && !is_valid_short(&value.value()) {
            return Err(syn::Error::new(
                value.span(),
                format!(
                    "short = \"{}\" is not lowercase kebab case ([a-z][a-z0-9-]*): it names \
                     the extension's CLI subcommand and MCP tool prefix; put prose in \
                     `description`",
                    value.value()
                ),
            ));
        }
        *slot = Some(value.value());
        Ok(())
    });
    syn::parse::Parser::parse2(parser, ts)?;
    Ok(attrs)
}
use quote::quote;
use syn::{ItemStruct, LitStr, Token, parse_macro_input};

#[proc_macro_attribute]
pub fn extension(attr: TokenStream, item: TokenStream) -> TokenStream {
    let st = parse_macro_input!(item as ItemStruct);
    let ident = &st.ident;

    let Attrs {
        name,
        version,
        short,
        description,
    } = match parse_attrs(attr.into()) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error().into(),
    };

    let name = match name {
        Some(n) => n,
        None => {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "extension macro requires name = \"...\"",
            )
            .to_compile_error()
            .into();
        }
    };
    let version = version.unwrap_or_else(|| {
        std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string())
    });

    let optional = |value: Option<String>| match value {
        Some(s) => quote! { Some(::std::string::String::from(#s)) },
        None => quote! { None },
    };
    let short_expr = optional(short);
    let description_expr = optional(description);

    let ts: TokenStream2 = quote! {
        #st

        fn specforge_extension_build() -> ::specforge_extension_sdk::ContributionsBuilder {
            let mut meta = ::specforge_extension_sdk::ExtensionMeta::new(#name, #version);
            meta.short = #short_expr;
            meta.description = #description_expr;
            let mut b = ::specforge_extension_sdk::ContributionsBuilder::new(meta);
            <#ident as ::specforge_extension_sdk::Contributions>::contribute(&mut b);
            b
        }
    };
    ts.into()
}

#[cfg(test)]
mod tests;
