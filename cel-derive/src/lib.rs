//! Derive macros for cel-rust

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{ToTokens, quote};
use syn::{
    Attribute, Data, DeriveInput, Fields, FieldsNamed, Lit, Meta, MetaNameValue, parse_macro_input,
};

/// Derive `DynamicType` for a struct.
///
/// # Attributes
///
/// ## Struct-level attributes
///
/// - `#[dynamic(crate = "path")]` - Specify the path to the cel crate (default: `::cel`)
///   - Use `#[dynamic(crate = "crate")]` when using this derive inside the cel crate itself
///   - Use `#[dynamic(crate = "::cel")]` or omit for external usage
///
/// ## Field-level attributes
///
/// - `#[dynamic(skip)]` - Skip this field in the generated implementation
/// - `#[dynamic(rename = "name")]` - Use a different name for this field in CEL
/// - `#[dynamic(flatten)]` - Flatten the contents of this field into the parent struct.
///   The field must implement `DynamicFlatten`. When materializing, the field's contents are
///   merged into the parent map instead of being nested. When accessing fields, lookups
///   will fall through to the flattened field if not found in the parent.
///   
///   `DynamicFlatten` is implemented for:
///   - Structs with `#[derive(DynamicType)]`
///   - `serde_json::Value` and `serde_json::Map<String, serde_json::Value>`
///   - `std::collections::HashMap<String, String>`
///   - `http::HeaderMap`
///   
///   Example:
///   ```rust,ignore
///   #[derive(DynamicType)]
///   pub struct Claims {
///       pub key: String,
///       #[dynamic(flatten)]
///       pub metadata: serde_json::Value,
///   }
///   // Accessing: claims.foo will look up metadata.field("foo") if "foo" is not a direct field
///   ```
///
/// - `#[dynamic(with = "function")]` - Transform the field value using a helper function before
///   passing to `maybe_materialize`. The function receives `&self.field` (note: if the field
///   is already a reference like `&'a T`, the function receives `&&'a T`) and should return
///   a reference to something that implements `DynamicType`.
///   
///   **Important**: Due to type inference limitations, you should use a named helper function
///   with explicit lifetime annotations rather than inline closures.
///   
///   Example:
///   ```rust,ignore
///   // Define a helper function with explicit lifetimes
///   fn extract_claims<'a>(c: &'a &'a Claims) -> &'a serde_json::Value {
///       &c.0
///   }
///   
///   #[derive(DynamicType)]
///   pub struct HttpRequest<'a> {
///       #[dynamic(with = "extract_claims")]
///       claims: &'a Claims,
///   }
///   ```
///
/// - `#[dynamic(with_value = "function")]` - Transform the field value using a helper function
///   that returns a `Value` directly. The function receives `&self.field` and must return `Value<'_>`.
///   This is useful for types that implement `AsRef<str>` or other conversions.
///   
///   Example:
///   ```rust,ignore
///   fn method_to_value<'a, T: AsRef<str>>(c: &'a &'a T) -> Value<'a> {
///       Value::String(c.as_ref().into())
///   }
///   
///   #[derive(DynamicType)]
///   pub struct HttpRequest<'a> {
///       #[dynamic(with_value = "method_to_value")]
///       method: &'a http::Method,
///   }
///   ```
///
/// ```rust,ignore
/// use cel::DynamicType;
///
/// #[derive(DynamicType)]
/// pub struct HttpRequest<'a> {
///     method: &'a str,
///     path: &'a str,
///     #[dynamic(skip)]
///     internal_id: u64,
/// }
///
/// // Using with attribute for newtype wrappers:
/// #[derive(Clone, Debug)]
/// pub struct Claims(serde_json::Value);
///
/// // Helper function to extract the inner value
/// fn extract_claims<'a>(c: &'a &'a Claims) -> &'a serde_json::Value {
///     &c.0
/// }
///
/// #[derive(DynamicType)]
/// pub struct HttpRequestRef<'a> {
///     method: &'a str,
///     #[dynamic(with = "extract_claims")]
///     claims: &'a Claims,
/// }
///
/// // Inside the cel crate itself:
/// #[derive(DynamicType)]
/// #[dynamic(crate = "crate")]
/// pub struct InternalType {
///     field: String,
/// }
/// ```
#[proc_macro_derive(DynamicType, attributes(dynamic))]
pub fn derive_dynamic_type(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    // Parse struct-level attributes
    let crate_path_str = get_struct_crate_path(&input.attrs);

    // Generate crate path - use custom path if specified, otherwise default to ::cel
    let crate_path: TokenStream2 = if let Some(path) = crate_path_str {
        path.parse().unwrap()
    } else {
        quote! { ::cel }
    };

    // Get fields
    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(FieldsNamed { named, .. }) => named,
            _ => {
                return syn::Error::new_spanned(
                    name,
                    "DynamicType can only be derived for structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(name, "DynamicType can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };

    // Filter and process fields
    let processed_fields: Result<Vec<_>, syn::Error> = fields
        .iter()
        .filter(|f| !has_field_attr(&f.attrs, "skip"))
        .map(|f| {
            let ident = f.ident.as_ref().unwrap();
            let name = get_field_rename(&f.attrs).unwrap_or_else(|| ident.to_string());
            let ty = &f.ty;
            // Check if the type is a reference type
            let is_ref = matches!(ty, syn::Type::Reference(_));
            // Check if the type is Option<T>
            let is_option = is_option_type(ty);
            let with_expr = get_field_with_expr(&f.attrs);
            let with_value_expr = get_field_with_value_expr(&f.attrs);
            let is_flatten = has_field_attr(&f.attrs, "flatten");

            // Check for conflicting attributes
            if with_expr.is_some() && with_value_expr.is_some() {
                return Err(syn::Error::new_spanned(
                    f,
                    "Cannot use both `with` and `with_value` attributes on the same field",
                ));
            }

            // flatten conflicts with all other attributes except skip (already filtered)
            if is_flatten
                && (with_expr.is_some()
                    || with_value_expr.is_some()
                    || get_field_rename(&f.attrs).is_some())
            {
                return Err(syn::Error::new_spanned(
                    f,
                    "Cannot use `flatten` with `with`, `with_value`, or `rename` attributes",
                ));
            }

            Ok((
                ident,
                name,
                ty,
                is_ref,
                is_option,
                with_expr,
                with_value_expr,
                is_flatten,
            ))
        })
        .collect();

    let processed_fields = match processed_fields {
        Ok(fields) => fields,
        Err(e) => return e.to_compile_error().into(),
    };

    // Separate normal fields from flattened fields
    let (normal_fields, flatten_fields): (Vec<_>, Vec<_>) = processed_fields.iter().partition(
        |(_ident, _name, _ty, _is_ref, _is_option, _with_expr, _with_value_expr, is_flatten)| {
            !is_flatten
        },
    );

    let field_count = normal_fields.len();

    // Generate materialize body
    let materialize_inserts: TokenStream2 = normal_fields
        .iter()
        .map(|(ident, name, _ty, _is_ref, is_option, with_expr, with_value_expr, _is_flatten)| {
            if let Some(expr_str) = with_value_expr {
                // Parse the helper function path for with_value
                let parsed_expr: syn::Expr = match syn::parse_str(expr_str) {
                    Ok(expr) => expr,
                    Err(e) => {
                        return syn::Error::new(
                            proc_macro2::Span::call_site(),
                            format!("Failed to parse `with_value` expression `{}`: {}", expr_str, e)
                        )
                        .to_compile_error();
                    }
                };
                // Convert the parsed expression to tokens
                let expr_tokens = parsed_expr.to_token_stream();
                // Call the helper and use returned Value directly (no maybe_materialize)
                quote! {
                    __cel_map.insert(
                        #crate_path::objects::KeyRef::from(#name),
                        (#expr_tokens)(&self.#ident),
                    );
                }
            } else if let Some(expr_str) = with_expr {
                // Parse the closure expression as a proper Expr for better diagnostics
                let parsed_expr: syn::Expr = match syn::parse_str(expr_str) {
                    Ok(expr) => expr,
                    Err(e) => {
                        return syn::Error::new(
                            proc_macro2::Span::call_site(),
                            format!("Failed to parse `with` expression `{}`: {}", expr_str, e)
                        )
                        .to_compile_error();
                    }
                };
                // Convert the parsed expression to tokens
                let expr_tokens = parsed_expr.to_token_stream();
                // Call the closure and let maybe_materialize handle the result
                quote! {
                    __cel_map.insert(
                        #crate_path::objects::KeyRef::from(#name),
                        #crate_path::types::dynamic::maybe_materialize((#expr_tokens)(&self.#ident)),
                    );
                }
            } else if *is_option {
                // For Option<T> types, use always_materialize(maybe_materialize_optional)
                quote! {
                    __cel_map.insert(
                        #crate_path::objects::KeyRef::from(#name),
                        #crate_path::types::dynamic::maybe_materialize_optional(&self.#ident).always_materialize_owned(),
                    );
                }
            } else {
                // Always pass a reference to maybe_materialize
                quote! {
                    __cel_map.insert(
                        #crate_path::objects::KeyRef::from(#name),
                        #crate_path::types::dynamic::maybe_materialize(&self.#ident),
                    );
                }
            }
        })
        .collect();

    // Generate flatten field merging code
    let flatten_merges: TokenStream2 = flatten_fields
        .iter()
        .map(
            |(
                ident,
                _name,
                _ty,
                _is_ref,
                _is_option,
                _with_expr,
                _with_value_expr,
                _is_flatten,
            )| {
                quote! {
                    // Materialize the flattened field directly into the map
                    #crate_path::types::dynamic::DynamicFlatten::materialize_into(&self.#ident, __cel_map);
                }
            },
        )
        .collect();

    // Generate field match arms
    let field_arms: TokenStream2 = normal_fields
        .iter()
        .map(|(ident, name, ty, _is_ref, is_option, with_expr, with_value_expr, _is_flatten)| {
            if let Some(expr_str) = with_value_expr {
                // Parse the helper function path for with_value
                let parsed_expr: syn::Expr = match syn::parse_str(expr_str) {
                    Ok(expr) => expr,
                    Err(e) => {
                        return syn::Error::new(
                            proc_macro2::Span::call_site(),
                            format!(
                                "Failed to parse `with_value` expression `{}`: {}",
                                expr_str, e
                            ),
                        )
                        .to_compile_error();
                    }
                };
                // Convert the parsed expression to tokens
                let expr_tokens = parsed_expr.to_token_stream();
                // Call the helper and use returned Value directly (no maybe_materialize)
                quote! {
                    #name => ::core::option::Option::Some((#expr_tokens)(&self.#ident)),
                }
            } else if let Some(expr_str) = with_expr {
                // Parse the closure expression as a proper Expr for better diagnostics
                let parsed_expr: syn::Expr = match syn::parse_str(expr_str) {
                    Ok(expr) => expr,
                    Err(e) => {
                        return syn::Error::new(
                            proc_macro2::Span::call_site(),
                            format!("Failed to parse `with` expression `{}`: {}", expr_str, e),
                        )
                        .to_compile_error();
                    }
                };
                // Convert the parsed expression to tokens
                let expr_tokens = parsed_expr.to_token_stream();
                // Generate code with explicit type annotation for better type inference
                quote! {
                    #name => {
                        let __field_ref: &#ty = &self.#ident;
                        ::core::option::Option::Some(#crate_path::types::dynamic::maybe_materialize((#expr_tokens)(__field_ref)))
                    },
                }
            } else if *is_option {
                // For Option<T> types, use maybe_materialize_optional
                quote! {
                    #name => ::core::option::Option::Some(#crate_path::types::dynamic::maybe_materialize_optional(&self.#ident)),
                }
            } else {
                // Always pass a reference to maybe_materialize
                quote! {
                    #name => ::core::option::Option::Some(#crate_path::types::dynamic::maybe_materialize(&self.#ident)),
                }
            }
        })
        .collect();

    // Generate fallback to flattened fields
    let flatten_fallback: TokenStream2 = if !flatten_fields.is_empty() {
        let flatten_checks = flatten_fields.iter().map(|(ident, _name, _ty, _is_ref, _is_option, _with_expr, _with_value_expr, _is_flatten)| {
            quote! {
                if let ::core::option::Option::Some(val) = #crate_path::types::dynamic::DynamicType::field(&self.#ident, field) {
                    return ::core::option::Option::Some(val);
                }
            }
        });
        quote! {
            #(#flatten_checks)*
        }
    } else {
        quote! {}
    };

    // Handle generics - we need to support both lifetimes and type parameters
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let generated = quote! {
        impl #impl_generics #crate_path::types::dynamic::DynamicType for #name #ty_generics #where_clause {
            fn materialize(&self) -> #crate_path::Value<'_> {
                let mut m = ::vector_map::VecMap::with_capacity(#field_count);
                #crate_path::types::dynamic::DynamicFlatten::materialize_into(self, &mut m);
                #crate_path::Value::Map(#crate_path::objects::MapValue::Borrow(m))
            }

            fn field(&self, field: &str) -> ::core::option::Option<#crate_path::Value<'_>> {
                match field {
                    #field_arms
                    _ => {
                        #flatten_fallback
                        ::core::option::Option::None
                    }
                }
            }
        }

        impl #impl_generics #crate_path::types::dynamic::DynamicFlatten for #name #ty_generics #where_clause {
            fn materialize_into<'__cel_a>(&'__cel_a self, __cel_map: &mut ::vector_map::VecMap<#crate_path::objects::KeyRef<'__cel_a>, #crate_path::Value<'__cel_a>>) {
                #materialize_inserts
                #flatten_merges
            }
        }
    };

    generated.into()
}

/// Check if a field has a specific attribute
fn has_field_attr(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| {
        if attr.path().is_ident("dynamic") {
            if let Ok(Meta::Path(path)) = attr.parse_args::<Meta>() {
                return path.is_ident(name);
            }
        }
        false
    })
}

/// Get the rename value for a field
fn get_field_rename(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("dynamic") {
            if let Ok(Meta::NameValue(MetaNameValue {
                path,
                value:
                    syn::Expr::Lit(syn::ExprLit {
                        lit: Lit::Str(lit_str),
                        ..
                    }),
                ..
            })) = attr.parse_args::<Meta>()
            {
                if path.is_ident("rename") {
                    return Some(lit_str.value());
                }
            }
        }
    }
    None
}

/// Get the crate path from struct-level attributes
fn get_struct_crate_path(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("dynamic") {
            if let Ok(Meta::NameValue(MetaNameValue {
                path,
                value:
                    syn::Expr::Lit(syn::ExprLit {
                        lit: Lit::Str(lit_str),
                        ..
                    }),
                ..
            })) = attr.parse_args::<Meta>()
            {
                if path.is_ident("crate") {
                    return Some(lit_str.value());
                }
            }
        }
    }
    None
}

/// Get the `with` expression for a field (closure to transform the value)
fn get_field_with_expr(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("dynamic") {
            if let Ok(Meta::NameValue(MetaNameValue {
                path,
                value:
                    syn::Expr::Lit(syn::ExprLit {
                        lit: Lit::Str(lit_str),
                        ..
                    }),
                ..
            })) = attr.parse_args::<Meta>()
            {
                if path.is_ident("with") {
                    return Some(lit_str.value());
                }
            }
        }
    }
    None
}

/// Get the `with_value` expression for a field (function that returns Value directly)
fn get_field_with_value_expr(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("dynamic") {
            if let Ok(Meta::NameValue(MetaNameValue {
                path,
                value:
                    syn::Expr::Lit(syn::ExprLit {
                        lit: Lit::Str(lit_str),
                        ..
                    }),
                ..
            })) = attr.parse_args::<Meta>()
            {
                if path.is_ident("with_value") {
                    return Some(lit_str.value());
                }
            }
        }
    }
    None
}

/// Check if a type is Option<T> and return true if so
/// Handles: Option<T>, std::option::Option<T>, ::std::option::Option<T>, core::option::Option<T>
fn is_option_type(ty: &syn::Type) -> bool {
    if let syn::Type::Path(type_path) = ty {
        let segments = &type_path.path.segments;
        // Check for Option or ::std::option::Option or core::option::Option
        if let Some(last_segment) = segments.last() {
            if last_segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(_) = &last_segment.arguments {
                    return true;
                }
            }
        }
    }
    false
}
