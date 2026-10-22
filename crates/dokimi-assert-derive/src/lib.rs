//! `#[derive(Arbitrary)]`, which states the shape of a struct or an enum for
//! the property forms of `dokimi-assert`.
//!
//! `dokimi-assert` re-exports the macro as `dokimi_assert::prop::Arbitrary`
//! behind its feature `derive`, so a caller depends on that crate alone.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as Tokens;
use quote::quote;
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use syn::{
    Attribute, Data, DataEnum, DeriveInput, Expr, Fields, Index, LitStr, parse_macro_input,
    parse_quote,
};

/// Implements `dokimi_assert::prop::Arbitrary` for a struct, a tuple struct or
/// an enum.
///
/// A struct reads as a record of its fields, in declaration order. A field of
/// a tuple struct is named by its position: `0`, `1` and on. An enum reads as
/// an enum of its variants, in declaration order: a unit variant without a
/// payload, a variant of one unnamed field with the field's shape as its
/// payload, and any other variant with a record of its fields. A type that
/// refers to itself reads as a definition and a ref to it.
///
/// The attribute `#[prop(...)]` on a field states the constraints of the
/// field's shape:
///
/// | Key | Value | Applies to |
/// |---|---|---|
/// | `min`, `max` | A number, or the text of a decimal, a date, a time of day or an instant | int, float, decimal, the dates and times, offset |
/// | `min_size`, `max_size` | A count | string, bytes, list, set, map |
/// | `pattern` | A pattern of the portable subset | string |
/// | `alphabet` | Characters | string, char |
/// | `allow_nan`, `allow_infinity` | None | float |
/// | `unit` | `"s"`, `"ms"`, `"us"` or `"ns"` | the times and durations |
/// | `scale` | A count of digits | decimal |
/// | `version` | `4` or `6` | ip-address |
///
/// A constraint that a list, a fixed-list, a set or an optional does not take
/// applies to its content. `#[prop(rename = "...")]` names a field or a
/// variant in the shape.
///
/// ```
/// use dokimi_assert::prop::{self, Arbitrary};
///
/// #[derive(Debug, Clone, PartialEq, Arbitrary)]
/// enum Payment {
///     #[prop(rename = "pending")]
///     Pending,
///     #[prop(rename = "paid")]
///     Paid(#[prop(min = 1)] i64),
/// }
///
/// let shape = prop::shape_of::<Payment>()?;
/// assert!(shape.contains(r#""pending""#));
/// # Ok::<(), prop::ShapeError>(())
/// ```
///
/// A key that the table does not list fails to compile:
///
/// ```compile_fail
/// use dokimi_assert::prop::Arbitrary;
///
/// #[derive(Arbitrary)]
/// struct Order {
///     #[prop(minimum = 1)]
///     quantity: u32,
/// }
/// ```
#[proc_macro_derive(Arbitrary, attributes(prop))]
pub fn derive_arbitrary(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// What the attribute `#[prop(...)]` of a field or a variant states: the
/// calls of its constraints, and its name in the shape.
#[derive(Default)]
struct Stated {
    constraints: Vec<Tokens>,
    rename: Option<String>,
}

/// Returns what the attributes `#[prop(...)]` state. `field` is `true` for a
/// field, which takes constraints, and `false` for a variant, which takes a
/// name alone.
fn stated(attrs: &[Attribute], field: bool) -> syn::Result<Stated> {
    let mut stated = Stated::default();
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("prop")) {
        attr.parse_nested_meta(|meta| {
            let key = meta
                .path
                .get_ident()
                .map(ToString::to_string)
                .unwrap_or_default();
            let method = syn::Ident::new(&key, meta.path.span());
            match key.as_str() {
                "rename" => {
                    let name: LitStr = meta.value()?.parse()?;
                    stated.rename = Some(name.value());
                }
                "min" | "max" | "min_size" | "max_size" | "scale" | "version" if field => {
                    let value: Expr = meta.value()?.parse()?;
                    stated.constraints.push(quote!(.#method(#value)));
                }
                "pattern" | "alphabet" | "unit" if field => {
                    let value: LitStr = meta.value()?.parse()?;
                    stated.constraints.push(quote!(.#method(#value)));
                }
                "allow_nan" | "allow_infinity" if field => {
                    stated.constraints.push(quote!(.#method()));
                }
                _ if field => {
                    return Err(meta.error(format!(
                        "prop: {key} is no key of a field. The keys are min, max, min_size, max_size, \
                         pattern, alphabet, allow_nan, allow_infinity, unit, scale, version and rename"
                    )));
                }
                _ => {
                    return Err(meta.error(format!(
                        "prop: {key} is no key of a variant, which takes rename alone"
                    )));
                }
            }
            Ok(())
        })?;
    }
    Ok(stated)
}

/// One field of a struct or a variant: its member, its name in the shape, its
/// type, and the constraints of its shape.
struct Field {
    member: syn::Member,
    name: String,
    ty: syn::Type,
    constraints: Vec<Tokens>,
}

/// Returns the fields of a struct or a variant, in declaration order.
fn fields_of(fields: &Fields) -> syn::Result<Vec<Field>> {
    let mut listed = Vec::new();
    for (at, field) in fields.iter().enumerate() {
        let stated = stated(&field.attrs, true)?;
        let (member, name) = match &field.ident {
            Some(ident) => (syn::Member::Named(ident.clone()), ident.unraw().to_string()),
            None => (syn::Member::Unnamed(Index::from(at)), at.to_string()),
        };
        listed.push(Field {
            member,
            name: stated.rename.unwrap_or(name),
            ty: field.ty.clone(),
            constraints: stated.constraints,
        });
    }
    Ok(listed)
}

/// Returns the shape of one field.
fn field_shape(field: &Field) -> Tokens {
    let ty = &field.ty;
    let constraints = &field.constraints;
    quote!(definitions.shape::<#ty>() #(#constraints)*)
}

/// Returns the shape of a record of fields.
fn record_shape(fields: &[Field]) -> Tokens {
    let names = fields.iter().map(|field| &field.name);
    let shapes = fields.iter().map(field_shape);
    quote!(::dokimi_assert::prop::Shape::record([#((#names, #shapes)),*]))
}

/// Returns the expression that builds a record's fields from `value`, the
/// record's value, under the constructor `path`.
fn record_from(path: &Tokens, fields: &[Field], value: &Tokens, by_name: bool) -> Tokens {
    let names = fields.iter().map(|field| &field.name);
    let converted = fields.iter().map(|_| {
        quote!(::dokimi_assert::__private::convert_from(
            fields.next().unwrap_or(::dokimi_assert::Value::Null)
        )?)
    });
    let built = if by_name {
        let members = fields.iter().map(|field| &field.member);
        quote!(#path { #(#members: #converted),* })
    } else {
        quote!(#path(#(#converted),*))
    };
    quote!({
        let mut fields = ::dokimi_assert::__private::record_fields(#value, &[#(#names),*])?.into_iter();
        ::core::result::Result::Ok(#built)
    })
}

/// Returns the value of a record of fields, whose values the expressions
/// `values` state.
fn record_value(fields: &[Field], values: &[Tokens]) -> Tokens {
    let names = fields.iter().map(|field| &field.name);
    quote!(::dokimi_assert::Value::Record(::std::vec![
        #((::std::borrow::ToOwned::to_owned(#names), ::dokimi_assert::__private::convert_to(#values))),*
    ]))
}

/// Returns the shape, the conversion from a value and the conversion to a
/// value of a struct.
fn struct_parts(input: &DeriveInput, fields: &Fields) -> syn::Result<(Tokens, Tokens, Tokens)> {
    if matches!(fields, Fields::Unit) || fields.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "prop: a struct without a field has no shape, because a record needs a field",
        ));
    }
    let listed = fields_of(fields)?;
    let named = matches!(fields, Fields::Named(_));
    let values: Vec<Tokens> = listed
        .iter()
        .map(|field| {
            let member = &field.member;
            quote!(&self.#member)
        })
        .collect();
    Ok((
        record_shape(&listed),
        record_from(&quote!(Self), &listed, &quote!(value), named),
        record_value(&listed, &values),
    ))
}

/// Returns the shape, the conversion from a value and the conversion to a
/// value of an enum.
fn enum_parts(input: &DeriveInput, data: &DataEnum) -> syn::Result<(Tokens, Tokens, Tokens)> {
    if data.variants.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "prop: an enum without a variant has no shape, because an enum needs a variant",
        ));
    }
    let mut shapes = Vec::new();
    let mut froms = Vec::new();
    let mut tos = Vec::new();
    for variant in &data.variants {
        let ident = &variant.ident;
        let name = stated(&variant.attrs, false)?
            .rename
            .unwrap_or_else(|| ident.unraw().to_string());
        let path = quote!(Self::#ident);
        let payload = quote!(payload.unwrap_or(::dokimi_assert::Value::Null));
        match &variant.fields {
            Fields::Unit => {
                shapes.push(quote!((#name, ::core::option::Option::None)));
                froms.push(quote!(#name => ::core::result::Result::Ok(#path)));
                tos.push(quote!(#path => ::dokimi_assert::Value::Variant {
                    name: ::std::borrow::ToOwned::to_owned(#name),
                    payload: ::core::option::Option::None,
                }));
            }
            Fields::Unnamed(unnamed) if unnamed.unnamed.len() == 1 => {
                let listed = fields_of(&variant.fields)?;
                let shape = field_shape(&listed[0]);
                shapes.push(quote!((#name, ::core::option::Option::Some(#shape))));
                froms.push(quote!(#name => ::core::result::Result::Ok(#path(
                    ::dokimi_assert::__private::convert_from(#payload)?
                ))));
                tos.push(quote!(#path(field) => ::dokimi_assert::Value::Variant {
                    name: ::std::borrow::ToOwned::to_owned(#name),
                    payload: ::core::option::Option::Some(::std::boxed::Box::new(
                        ::dokimi_assert::__private::convert_to(field)
                    )),
                }));
            }
            fields => {
                let listed = fields_of(fields)?;
                let named = matches!(fields, Fields::Named(_));
                let shape = record_shape(&listed);
                shapes.push(quote!((#name, ::core::option::Option::Some(#shape))));
                let built = record_from(&path, &listed, &payload, named);
                froms.push(quote!(#name => #built));
                let bindings: Vec<syn::Ident> = (0..listed.len())
                    .map(|at| {
                        syn::Ident::new(&format!("field{at}"), proc_macro2::Span::call_site())
                    })
                    .collect();
                let pattern = if named {
                    let members = listed.iter().map(|field| &field.member);
                    quote!(#path { #(#members: #bindings),* })
                } else {
                    quote!(#path(#(#bindings),*))
                };
                let values: Vec<Tokens> = bindings.iter().map(|binding| quote!(#binding)).collect();
                let record = record_value(&listed, &values);
                tos.push(quote!(#pattern => ::dokimi_assert::Value::Variant {
                    name: ::std::borrow::ToOwned::to_owned(#name),
                    payload: ::core::option::Option::Some(::std::boxed::Box::new(#record)),
                }));
            }
        }
    }
    Ok((
        quote!(::dokimi_assert::prop::Shape::enumeration([#(#shapes),*])),
        quote!({
            let (name, payload) = ::dokimi_assert::__private::variant_parts(value)?;
            match name.as_str() {
                #(#froms,)*
                _ => ::core::result::Result::Err(::dokimi_assert::__private::unknown_variant(&name)),
            }
        }),
        quote!(match self { #(#tos,)* }),
    ))
}

/// Returns the implementation of `Arbitrary` for a struct or an enum.
fn expand(input: &DeriveInput) -> syn::Result<Tokens> {
    if let Some(attr) = input.attrs.iter().find(|attr| attr.path().is_ident("prop")) {
        return Err(syn::Error::new_spanned(
            attr,
            "prop: #[prop(...)] applies to a field or a variant, not to the type",
        ));
    }
    let (shape, from, to) = match &input.data {
        Data::Struct(data) => struct_parts(input, &data.fields)?,
        Data::Enum(data) => enum_parts(input, data)?,
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "prop: a union has no shape",
            ));
        }
    };
    let ident = &input.ident;
    let mut generics = input.generics.clone();
    for param in generics.type_params_mut() {
        param
            .bounds
            .push(parse_quote!(::dokimi_assert::prop::Arbitrary));
    }
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics ::dokimi_assert::prop::Arbitrary for #ident #type_generics #where_clause {
            fn shape(definitions: &mut ::dokimi_assert::prop::Definitions) -> ::dokimi_assert::prop::Shape {
                definitions.define::<Self>(|definitions| #shape)
            }

            fn from_value(
                value: ::dokimi_assert::Value,
            ) -> ::core::result::Result<Self, ::dokimi_assert::prop::ShapeError> {
                #from
            }

            fn to_value(&self) -> ::dokimi_assert::Value {
                #to
            }
        }
    })
}
