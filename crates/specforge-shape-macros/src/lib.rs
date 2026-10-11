//! `#[derive(Shape)]`: the JSON Schema of what the type's `#[derive(Serialize)]`
//! writes (ADR 0048). The generated code names `::specforge_common::shape`.
//!
//! The derive sits beside `Serialize` and reads the same fields and serde
//! attributes, so a field has one vocabulary and the schema cannot disagree
//! with the serialization.
//!
//! - A **struct with named fields** is a closed object (`additionalProperties:
//!   false`) holding the fields under their serialized names. A field is
//!   required unless it has `skip_serializing_if`; an `Option<T>` written
//!   without one is required and nullable, one skipped when `Option::is_none`
//!   is `T`. `skip` and `skip_serializing` fields are not listed; `rename`
//!   and the container's `rename_all` name the keys; `flatten` merges the
//!   inner type's object (a map opens the object, a union distributes over
//!   its branches). Such a struct is also an `Object`.
//! - A **tuple struct with one field**, or `#[serde(transparent)]`, is the
//!   field's schema (and no `Object`).
//! - A **unit-only enum** is the set of its serialized names.
//! - A **`#[serde(tag = "k")]` enum** of unit, struct and newtype variants is
//!   a union of objects, each stating `k`; an **`#[serde(untagged)]` enum** of
//!   struct and newtype variants is a union of the variants' schemas (typed
//!   `object` when every branch is an object schema, decided at run time).
//! - `#[shape(as = Type)]` states a field's schema from `Type` (required for
//!   a field serialized with `serialize_with` or `with`);
//!   `#[shape(names = EXPR)]` makes a string field (or an `Option`/`Vec` of
//!   them) one of the names of the option table `EXPR`.
//!
//! - `#[shape(define)]` (or `define = "Name"`) on the type makes it a *named
//!   shape* (ADR 0051): a document defines it once under `$defs` and every
//!   place that holds it refers to it. A named shape may be recursive.
//!
//! The derive refuses, at compile time: externally tagged data enums, tuple
//! variants, unions, unknown serde attributes and `shape` keys, a field
//! serialized by function without `shape(as)`, a field whose type names
//! the type itself unless the type is a named shape (a recursive type has no
//! finite inline schema), and `define` on a type with type parameters or on a
//! field.

use proc_macro::TokenStream;
use proc_macro2::{TokenStream as TokenStream2, TokenTree};
use quote::quote;
use syn::ext::IdentExt;
use syn::meta::ParseNestedMeta;
use syn::{
    Attribute, Data, DeriveInput, Expr, Field, Fields, FieldsNamed, GenericArgument, GenericParam,
    LitStr, PathArguments, Token, Type, parse_macro_input, parse_quote,
};

#[proc_macro_derive(Shape, attributes(shape))]
pub fn derive_shape(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

// ── serde's naming rules ────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum Case {
    Lower,
    Upper,
    Pascal,
    Camel,
    Snake,
    ScreamingSnake,
    Kebab,
    ScreamingKebab,
}

impl Case {
    fn parse(name: &str, at: &LitStr) -> syn::Result<Case> {
        Ok(match name {
            "lowercase" => Case::Lower,
            "UPPERCASE" => Case::Upper,
            "PascalCase" => Case::Pascal,
            "camelCase" => Case::Camel,
            "snake_case" => Case::Snake,
            "SCREAMING_SNAKE_CASE" => Case::ScreamingSnake,
            "kebab-case" => Case::Kebab,
            "SCREAMING-KEBAB-CASE" => Case::ScreamingKebab,
            other => {
                return Err(syn::Error::new_spanned(
                    at,
                    format!("Shape: unknown rename_all rule `{other}`"),
                ));
            }
        })
    }

    /// A variant's serialized name (the source is `PascalCase`).
    fn variant(self, name: &str) -> String {
        match self {
            Case::Pascal => name.to_string(),
            Case::Lower => name.to_lowercase(),
            Case::Upper => name.to_uppercase(),
            Case::Camel => {
                let mut chars = name.chars();
                chars
                    .next()
                    .map(|first| first.to_lowercase().chain(chars).collect())
                    .unwrap_or_default()
            }
            Case::Snake => snake(name),
            Case::ScreamingSnake => snake(name).to_uppercase(),
            Case::Kebab => snake(name).replace('_', "-"),
            Case::ScreamingKebab => snake(name).to_uppercase().replace('_', "-"),
        }
    }

    /// A field's serialized name (the source is `snake_case`).
    fn field(self, name: &str) -> String {
        let pascal = || -> String {
            name.split('_')
                .map(|word| {
                    let mut chars = word.chars();
                    chars
                        .next()
                        .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                        .unwrap_or_default()
                })
                .collect()
        };
        match self {
            Case::Lower | Case::Snake => name.to_string(),
            Case::Upper | Case::ScreamingSnake => name.to_uppercase(),
            Case::Pascal => pascal(),
            Case::Camel => Case::Camel.variant(&pascal()),
            Case::Kebab => name.replace('_', "-"),
            Case::ScreamingKebab => name.to_uppercase().replace('_', "-"),
        }
    }
}

/// `PascalCase` as `snake_case`.
fn snake(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
}

// ── attributes ──────────────────────────────────────────────────────────────

/// What `#[serde(..)]` says that decides the written JSON.
#[derive(Default)]
struct Serde {
    rename: Option<String>,
    rename_all: Option<Case>,
    tag: Option<String>,
    untagged: bool,
    transparent: bool,
    skip: bool,
    skip_if: Option<String>,
    flatten: bool,
    serialized_by_function: bool,
}

/// A `key = "value"` or `key(serialize = "value", ..)` string: the
/// serialization side's.
fn string_of(meta: &ParseNestedMeta<'_>) -> syn::Result<LitStr> {
    if meta.input.peek(Token![=]) {
        return meta.value()?.parse();
    }
    let mut found = None;
    meta.parse_nested_meta(|inner| {
        if inner.path.is_ident("serialize") {
            found = Some(inner.value()?.parse::<LitStr>()?);
        } else {
            drain(&inner)?;
        }
        Ok(())
    })?;
    found.ok_or_else(|| meta.error("Shape: expected `= \"..\"` or `(serialize = \"..\")`"))
}

/// Skip whatever follows a key: `= expr` or `(..)`.
fn drain(meta: &ParseNestedMeta<'_>) -> syn::Result<()> {
    if meta.input.peek(Token![=]) {
        meta.value()?.parse::<Expr>()?;
    } else if meta.input.peek(syn::token::Paren) {
        let content;
        syn::parenthesized!(content in meta.input);
        content.parse::<TokenStream2>()?;
    }
    Ok(())
}

fn serde_attrs(attrs: &[Attribute]) -> syn::Result<Serde> {
    let mut out = Serde::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident("serde")) {
        attr.parse_nested_meta(|meta| {
            let key = meta
                .path
                .get_ident()
                .map(|i| i.to_string())
                .unwrap_or_default();
            match key.as_str() {
                "rename" => out.rename = Some(string_of(&meta)?.value()),
                "rename_all" => {
                    let lit = string_of(&meta)?;
                    out.rename_all = Some(Case::parse(&lit.value(), &lit)?);
                }
                "tag" => out.tag = Some(meta.value()?.parse::<LitStr>()?.value()),
                "untagged" => out.untagged = true,
                "transparent" => out.transparent = true,
                "skip" | "skip_serializing" => out.skip = true,
                "skip_serializing_if" => {
                    out.skip_if = Some(meta.value()?.parse::<LitStr>()?.value());
                }
                "flatten" => out.flatten = true,
                "serialize_with" | "with" => {
                    out.serialized_by_function = true;
                    drain(&meta)?;
                }
                // Read-side only: they do not change what is written.
                "default"
                | "alias"
                | "deserialize_with"
                | "skip_deserializing"
                | "deny_unknown_fields"
                | "bound"
                | "borrow" => drain(&meta)?,
                other => {
                    return Err(meta.error(format!(
                        "Shape: the serde attribute `{other}` is not supported by the derive"
                    )));
                }
            }
            Ok(())
        })?;
    }
    Ok(out)
}

#[derive(Default)]
struct ShapeAttr {
    as_type: Option<Type>,
    names: Option<Expr>,
}

/// The name a type's `#[shape(define)]` / `#[shape(define = "Name")]` gives
/// its definition: `Some` for a named shape.
fn container_define(input: &DeriveInput) -> syn::Result<Option<String>> {
    let mut define = None;
    for attr in input.attrs.iter().filter(|a| a.path().is_ident("shape")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("define") {
                define = Some(if meta.input.peek(Token![=]) {
                    meta.value()?.parse::<LitStr>()?.value()
                } else {
                    input.ident.unraw().to_string()
                });
            } else if meta.path.is_ident("as") || meta.path.is_ident("names") {
                return Err(meta.error("Shape: `as` and `names` state a field, not a type"));
            } else {
                return Err(meta.error("Shape: unknown `shape` key (expected `define`)"));
            }
            Ok(())
        })?;
    }
    if define.is_some()
        && input
            .generics
            .params
            .iter()
            .any(|param| matches!(param, GenericParam::Type(_)))
    {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "Shape: a defined shape names one schema; a type with type parameters has one per instantiation",
        ));
    }
    Ok(define)
}

fn shape_attrs(attrs: &[Attribute]) -> syn::Result<ShapeAttr> {
    let mut out = ShapeAttr::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident("shape")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("as") {
                out.as_type = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("names") {
                out.names = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("define") {
                return Err(meta.error("Shape: `define` names a type, not a field"));
            } else {
                return Err(meta.error("Shape: unknown `shape` key (expected `as` or `names`)"));
            }
            Ok(())
        })?;
    }
    Ok(out)
}

// ── types ───────────────────────────────────────────────────────────────────

/// `T` when `ty` is syntactically `Option<T>`.
fn option_inner(ty: &Type) -> Option<&Type> {
    generic_inner(ty, "Option")
}

fn generic_inner<'t>(ty: &'t Type, name: &str) -> Option<&'t Type> {
    let Type::Path(path) = ty else { return None };
    let segment = path.path.segments.last()?;
    if segment.ident != name {
        return None;
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    match arguments.args.first()? {
        GenericArgument::Type(inner) if arguments.args.len() == 1 => Some(inner),
        _ => None,
    }
}

/// Whether the tokens of `ty` name `ident`.
fn mentions(tokens: TokenStream2, ident: &syn::Ident) -> bool {
    tokens.into_iter().any(|token| match token {
        TokenTree::Ident(found) => found == *ident,
        TokenTree::Group(group) => mentions(group.stream(), ident),
        _ => false,
    })
}

const SHAPE: &str = "::specforge_common::shape";

fn shape_path() -> TokenStream2 {
    SHAPE.parse().expect("a path")
}

/// The expression for the schema of a field of type `ty`: its `Shape`, or,
/// with `names`, the table's names (inside an `Option` or a `Vec` when the
/// type has them).
fn schema_of(ty: &Type, names: Option<&Expr>) -> TokenStream2 {
    let shape = shape_path();
    let Some(names) = names else {
        return quote!(<#ty as #shape::Shape>::schema(defs));
    };
    if let Some(inner) = option_inner(ty) {
        let inner = schema_of(inner, Some(names));
        return quote!(#shape::nullable(#inner));
    }
    if let Some(inner) = generic_inner(ty, "Vec") {
        let inner = schema_of(inner, Some(names));
        return quote!(#shape::array(#inner));
    }
    quote!(#shape::names((#names).names()))
}

// ── the expansion ───────────────────────────────────────────────────────────

/// A derived schema: its expression, and the types that must be `Object`
/// for the type to be one (`None`: the type is no `Object`).
struct Body {
    schema: TokenStream2,
    object: Option<Vec<Type>>,
}

fn expand(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let container = serde_attrs(&input.attrs)?;
    let define = container_define(input)?;
    let body = match &input.data {
        Data::Struct(data) => struct_body(input, &container, &data.fields)?,
        Data::Enum(data) => enum_body(input, &container, data)?,
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "Shape: a union has no JSON shape",
            ));
        }
    };

    let shape = shape_path();
    let name = &input.ident;
    let mut generics = input.generics.clone();
    let params: Vec<_> = generics
        .params
        .iter()
        .filter_map(|p| match p {
            GenericParam::Type(t) => Some(t.ident.clone()),
            _ => None,
        })
        .collect();
    for param in &params {
        generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#param: #shape::Shape));
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let schema = body.schema;
    let named = match define {
        Some(name) => quote!(::core::option::Option::Some(#name)),
        None => quote!(::core::option::Option::None),
    };
    let mut out = quote! {
        impl #impl_generics #shape::Shape for #name #ty_generics #where_clause {
            const NAME: ::core::option::Option<&'static str> = #named;

            #[allow(unused_variables)]
            fn body(defs: &mut #shape::Definitions) -> #shape::Value {
                #schema
            }
        }
    };
    if let Some(bounds) = body.object {
        let mut generics = generics.clone();
        for bound in &bounds {
            generics
                .make_where_clause()
                .predicates
                .push(parse_quote!(#bound: #shape::Object));
        }
        let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
        out.extend(quote! {
            impl #impl_generics #shape::Object for #name #ty_generics #where_clause {}
        });
    }
    Ok(out)
}

fn struct_body(input: &DeriveInput, container: &Serde, fields: &Fields) -> syn::Result<Body> {
    match fields {
        Fields::Named(named) if container.transparent => {
            let [field] = named.named.iter().collect::<Vec<_>>()[..] else {
                return Err(syn::Error::new_spanned(
                    &input.ident,
                    "Shape: a transparent struct has exactly one field",
                ));
            };
            let attrs = shape_attrs(&field.attrs)?;
            Ok(Body {
                schema: field_schema_of(input, field, &attrs)?,
                object: None,
            })
        }
        Fields::Named(named) => Ok(Body {
            schema: object_expr(input, named, container.rename_all, None)?,
            object: Some(Vec::new()),
        }),
        Fields::Unnamed(unnamed) if unnamed.unnamed.len() == 1 => {
            let field = &unnamed.unnamed[0];
            let attrs = shape_attrs(&field.attrs)?;
            Ok(Body {
                schema: field_schema_of(input, field, &attrs)?,
                object: None,
            })
        }
        _ => Err(syn::Error::new_spanned(
            &input.ident,
            "Shape: only a struct with named fields, or a newtype, can derive it",
        )),
    }
}

/// The schema of a field serialized as itself (`shape(as)` honored).
fn field_schema_of(
    input: &DeriveInput,
    field: &Field,
    attrs: &ShapeAttr,
) -> syn::Result<TokenStream2> {
    let ty = attrs.as_type.as_ref().unwrap_or(&field.ty);
    refuse_recursion(input, field)?;
    Ok(schema_of(ty, attrs.names.as_ref()))
}

fn refuse_recursion(input: &DeriveInput, field: &Field) -> syn::Result<()> {
    use quote::ToTokens;
    // A named shape refers to itself by name.
    if container_define(input)?.is_some() {
        return Ok(());
    }
    if mentions(field.ty.to_token_stream(), &input.ident) {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "Shape: a field whose type names the type itself has no finite inline schema \
             (name it with `#[shape(define)]`)",
        ));
    }
    Ok(())
}

/// An object schema for `fields`; with `tag`, the tag's key first, as the
/// names of one value.
fn object_expr(
    input: &DeriveInput,
    fields: &FieldsNamed,
    rename_all: Option<Case>,
    tag: Option<(&str, &str)>,
) -> syn::Result<TokenStream2> {
    let shape = shape_path();
    let mut properties: Vec<TokenStream2> = Vec::new();
    let mut required: Vec<String> = Vec::new();
    let mut flattened: Vec<TokenStream2> = Vec::new();
    if let Some((key, value)) = tag {
        properties.push(quote!((#key, #shape::names([#value]))));
        required.push(key.to_string());
    }
    for field in &fields.named {
        let serde = serde_attrs(&field.attrs)?;
        if serde.skip {
            continue;
        }
        let attrs = shape_attrs(&field.attrs)?;
        if serde.serialized_by_function && attrs.as_type.is_none() {
            return Err(syn::Error::new_spanned(
                field,
                "Shape: a field serialized with `serialize_with` or `with` needs `#[shape(as = Type)]`",
            ));
        }
        refuse_recursion(input, field)?;
        let ty = attrs.as_type.as_ref().unwrap_or(&field.ty);
        if serde.flatten {
            // A flattened type is merged as its body, never as a reference.
            flattened.push(quote!(<#ty as #shape::Shape>::body(defs)));
            continue;
        }
        let ident = field.ident.as_ref().expect("a named field");
        let written = serde.rename.clone().unwrap_or_else(|| {
            let source = ident.unraw().to_string();
            match rename_all {
                Some(case) => case.field(&source),
                None => source,
            }
        });
        let skipped_none = serde
            .skip_if
            .as_deref()
            .is_some_and(|f| f.ends_with("is_none"));
        let (schema, is_required) = match (option_inner(ty), serde.skip_if.is_some()) {
            (Some(inner), true) if skipped_none => (schema_of(inner, attrs.names.as_ref()), false),
            (_, true) => (schema_of(ty, attrs.names.as_ref()), false),
            (_, false) => (schema_of(ty, attrs.names.as_ref()), true),
        };
        if is_required {
            required.push(written.clone());
        }
        properties.push(quote!((#written, #schema)));
    }
    let required = required.iter();
    Ok(quote! {{
        #[allow(unused_mut)]
        let mut schema = #shape::object(
            ::std::vec![#(#properties),*],
            ::std::vec![#(#required),*],
        );
        #(#shape::flatten(&mut schema, #flattened, defs);)*
        schema
    }})
}

fn enum_body(input: &DeriveInput, container: &Serde, data: &syn::DataEnum) -> syn::Result<Body> {
    let shape = shape_path();
    let variants: Vec<_> = data
        .variants
        .iter()
        .map(|variant| Ok((variant, serde_attrs(&variant.attrs)?)))
        .collect::<syn::Result<_>>()?;
    let live = || variants.iter().filter(|(_, serde)| !serde.skip);
    let name_of = |variant: &syn::Variant, serde: &Serde| {
        serde.rename.clone().unwrap_or_else(|| {
            let source = variant.ident.unraw().to_string();
            match container.rename_all {
                Some(case) => case.variant(&source),
                None => source,
            }
        })
    };

    let unit_only = live().all(|(variant, _)| matches!(variant.fields, Fields::Unit));
    if unit_only && container.tag.is_none() && !container.untagged {
        let names: Vec<String> = live().map(|(v, s)| name_of(v, s)).collect();
        return Ok(Body {
            schema: quote!(#shape::names([#(#names),*])),
            object: None,
        });
    }

    let mut branches = Vec::new();
    let mut bounds = Vec::new();
    for (variant, serde) in live() {
        let written = name_of(variant, serde);
        let rename_all = serde.rename_all;
        let tag = container.tag.as_deref().map(|key| (key, written.as_str()));
        let branch = match (&variant.fields, tag, container.untagged) {
            (Fields::Named(named), tag, _) => object_expr(input, named, rename_all, tag)?,
            (Fields::Unit, Some((key, value)), false) => {
                quote!(#shape::object(::std::vec![(#key, #shape::names([#value]))], ::std::vec![#key]))
            }
            (Fields::Unnamed(unnamed), Some((key, value)), false) if unnamed.unnamed.len() == 1 => {
                let inner = &unnamed.unnamed[0];
                refuse_recursion(input, inner)?;
                let ty = &inner.ty;
                bounds.push(ty.clone());
                quote!({
                    let mut schema = #shape::object(
                        ::std::vec![(#key, #shape::names([#value]))],
                        ::std::vec![#key],
                    );
                    #shape::flatten(&mut schema, <#ty as #shape::Shape>::body(defs), defs);
                    schema
                })
            }
            (Fields::Unnamed(unnamed), None, true) if unnamed.unnamed.len() == 1 => {
                let inner = &unnamed.unnamed[0];
                refuse_recursion(input, inner)?;
                let attrs = shape_attrs(&inner.attrs)?;
                let ty = attrs.as_type.as_ref().unwrap_or(&inner.ty);
                bounds.push(ty.clone());
                schema_of(ty, attrs.names.as_ref())
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    &variant.ident,
                    "Shape: an enum with data needs `#[serde(tag = \"..\")]` or `#[serde(untagged)]`, \
                     and its variants are structs or newtypes",
                ));
            }
        };
        branches.push(branch);
    }
    if container.tag.is_none() && !container.untagged {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "Shape: an externally tagged enum with data has no supported shape; \
             use `#[serde(tag = \"..\")]` or `#[serde(untagged)]`",
        ));
    }
    Ok(Body {
        schema: quote!(#shape::one_of(::std::vec![#(#branches),*], defs)),
        object: Some(bounds),
    })
}
