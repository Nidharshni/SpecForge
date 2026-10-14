//! `#[derive(Arguments)]`: a tool's or prompt's typed arguments, one
//! definition (ADR 0033). For specforge-mcp's own structs: the generated
//! code names `::specforge_mcp::args`.
//!
//! A struct with named fields (or `{}`) derives `specforge_mcp::args::Arguments`.
//! Each field is one argument:
//!
//! - its **name** is the field's, a raw prefix removed (`r#where` is `where`);
//! - its **description** is the field's doc comment, which every field must
//!   have: the lines are trimmed and joined by one space;
//! - its **type** decides how a value is read and the JSON type the listing
//!   states (`specforge_mcp::args::Arg`);
//! - `#[arg(default = <expr>)]` is what an absent argument reads as (the
//!   expression is evaluated per call) and the `default` the listing states;
//! - `#[arg(choice = <OptionTable>)]` makes it one of the table's names
//!   (ADR 0027). The field's type picks the reading by its syntax:
//!   `Option<_>` reads none when absent, `String` is required and keeps the
//!   name as given (the handler parses it), anything else is the table's
//!   value type with the table's default;
//! - `#[arg(names = <&[&str]>)]` lists the names a `String`, `Option<String>`
//!   or `Vec<String>` field takes as the listing's `enum`; the reader does
//!   not check them, the operation refuses a name outside the list.
//!
//! The derive refuses, at compile time: anything but a struct with named
//! fields, generics, a field without a doc comment, an unknown `arg` key,
//! `default` with `choice`, and `choice` with `names`.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::ext::IdentExt;
use syn::{
    Data, DeriveInput, Expr, ExprLit, Field, Fields, Lit, LitStr, Meta, Type, parse_macro_input,
};

#[proc_macro_derive(Arguments, attributes(arg))]
pub fn derive_arguments(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// What `#[arg(..)]` says about one field.
#[derive(Default)]
struct Attributes {
    default: Option<Expr>,
    choice: Option<Expr>,
    names: Option<Expr>,
}

fn expand(input: &DeriveInput) -> syn::Result<TokenStream2> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "Arguments: a struct of arguments takes no generics",
        ));
    }
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "Arguments: only a struct with named fields can derive it",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "Arguments: only a struct with named fields can derive it",
        ));
    };

    let mut declared = Vec::new();
    let mut read = Vec::new();
    for field in &fields.named {
        let (declaration, reading) = argument(field)?;
        declared.push(declaration);
        read.push(reading);
    }

    let name = &input.ident;
    Ok(quote! {
        impl ::specforge_mcp::args::Arguments for #name {
            fn declared() -> ::std::vec::Vec<::specforge_mcp::args::Argument> {
                ::std::vec![#(#declared),*]
            }

            #[allow(unused_variables)]
            fn read(
                given: &::specforge_mcp::args::Given<'_>,
            ) -> ::core::result::Result<Self, ::std::boxed::Box<::specforge_mcp::tool::McpError>> {
                ::core::result::Result::Ok(Self { #(#read),* })
            }
        }
    })
}

/// A field's declaration in `declared()` and its initializer in `read()`.
fn argument(field: &Field) -> syn::Result<(TokenStream2, TokenStream2)> {
    let ident = field.ident.as_ref().expect("a named field");
    let name = ident.unraw().to_string();
    let description = description(field)?;
    let attributes = attributes(field)?;
    let ty = &field.ty;
    let args = quote!(::specforge_mcp::args);

    match attributes {
        Attributes {
            default: Some(_),
            choice: Some(_),
            ..
        } => Err(syn::Error::new_spanned(
            field,
            "Arguments: `default` and `choice` do not go together; the table's default is the default",
        )),
        Attributes {
            choice: Some(_),
            names: Some(_),
            ..
        } => Err(syn::Error::new_spanned(
            field,
            "Arguments: `choice` and `names` do not go together",
        )),
        Attributes {
            names: Some(_),
            default: Some(_),
            ..
        } => Err(syn::Error::new_spanned(
            field,
            "Arguments: `names` and `default` do not go together",
        )),
        Attributes {
            choice: Some(table),
            ..
        } => {
            let (required, reading) = match shape(ty) {
                Shape::Option => (false, quote!(given.optional_choice(&#table, #name)?)),
                Shape::String => (true, quote!(given.choice_name(&#table, #name)?)),
                Shape::Other => (false, quote!(given.choice(&#table, #name)?)),
            };
            Ok((
                quote!(#args::Argument::choice(&#table, #name, #description, #required)),
                quote!(#ident: #reading),
            ))
        }
        Attributes {
            names: Some(names), ..
        } => Ok((
            quote!(#args::Argument::names::<#ty>(#names, #name, #description)),
            quote!(#ident: given.field::<#ty>(#name, ::core::option::Option::None)?),
        )),
        Attributes {
            default: Some(default),
            ..
        } => Ok((
            quote!(#args::Argument::typed::<#ty>(
                #name,
                #description,
                ::core::option::Option::Some(&(#default)),
            )),
            quote!(#ident: given.field::<#ty>(
                #name,
                ::core::option::Option::Some(|| #default),
            )?),
        )),
        Attributes { .. } => Ok((
            quote!(#args::Argument::typed::<#ty>(
                #name,
                #description,
                ::core::option::Option::None,
            )),
            quote!(#ident: given.field::<#ty>(#name, ::core::option::Option::None)?),
        )),
    }
}

/// How a field's type reads an enumerated argument.
enum Shape {
    /// `Option<_>`: absent is none.
    Option,
    /// `String`: required, the name as given.
    String,
    /// The table's value type: absent is the table's default.
    Other,
}

fn shape(ty: &Type) -> Shape {
    let Type::Path(path) = ty else {
        return Shape::Other;
    };
    match path.path.segments.last() {
        Some(segment) if segment.ident == "Option" => Shape::Option,
        Some(segment) if segment.ident == "String" && segment.arguments.is_empty() => Shape::String,
        _ => Shape::Other,
    }
}

/// The field's doc comment: its lines trimmed and joined by a space.
fn description(field: &Field) -> syn::Result<LitStr> {
    let mut lines = Vec::new();
    for attribute in &field.attrs {
        if !attribute.path().is_ident("doc") {
            continue;
        }
        if let Meta::NameValue(name_value) = &attribute.meta
            && let Expr::Lit(ExprLit {
                lit: Lit::Str(text),
                ..
            }) = &name_value.value
        {
            let line = text.value();
            let line = line.trim();
            if !line.is_empty() {
                lines.push(line.to_string());
            }
        }
    }
    if lines.is_empty() {
        return Err(syn::Error::new_spanned(
            field,
            "Arguments: every argument needs a doc comment, its description in the listing",
        ));
    }
    Ok(LitStr::new(
        &lines.join(" "),
        proc_macro2::Span::call_site(),
    ))
}

fn attributes(field: &Field) -> syn::Result<Attributes> {
    let mut attributes = Attributes::default();
    for attribute in &field.attrs {
        if !attribute.path().is_ident("arg") {
            continue;
        }
        attribute.parse_nested_meta(|meta| {
            let slot = if meta.path.is_ident("default") {
                &mut attributes.default
            } else if meta.path.is_ident("choice") {
                &mut attributes.choice
            } else if meta.path.is_ident("names") {
                &mut attributes.names
            } else {
                return Err(
                    meta.error("Arguments: unknown key; expected `default`, `choice` or `names`")
                );
            };
            if slot.is_some() {
                return Err(meta.error("Arguments: this key is given twice"));
            }
            *slot = Some(meta.value()?.parse()?);
            Ok(())
        })?;
    }
    Ok(attributes)
}
