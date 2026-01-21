//! Derive macros for cel-rust

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Attribute, Data, DeriveInput, Fields, FieldsNamed, Lit, Meta, MetaNameValue, parse_macro_input,
};

/// Derive `DynamicType` and `DynamicValueVtable` for a struct.
///
/// # Attributes
///
/// ## Struct-level attributes
///
/// - `#[dynamic(auto_materialize)]` - Override `auto_materialize()` to return `true`
///
/// ## Field-level attributes
///
/// - `#[dynamic(skip)]` - Skip this field in the generated implementation
/// - `#[dynamic(rename = "name")]` - Use a different name for this field in CEL
///
/// # Example
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
/// ```
#[proc_macro_derive(DynamicType, attributes(dynamic))]
pub fn derive_dynamic_type(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    // Parse struct-level attributes
    let auto_materialize = has_struct_attr(&input.attrs, "auto_materialize");

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
    let processed_fields: Vec<_> = fields
        .iter()
        .filter(|f| !has_field_attr(&f.attrs, "skip"))
        .map(|f| {
            let ident = f.ident.as_ref().unwrap();
            let name = get_field_rename(&f.attrs).unwrap_or_else(|| ident.to_string());
            (ident, name)
        })
        .collect();

    let field_count = processed_fields.len();

    // Generate a flexible crate reference that works both inside and outside the cel crate
    // Users inside the cel crate should use: `extern crate self as cel;` at module level
    // Users outside will use the normal `::cel` path
    let crate_path = quote! { ::cel };

    // Alternative: we could make this configurable via attribute
    // #[dynamic(crate = "crate")] or #[dynamic(crate = "::cel")]
    // For now, we'll just use ::cel

    // Generate materialize body
    let materialize_inserts: TokenStream2 = processed_fields
        .iter()
        .map(|(ident, name)| {
            quote! {
                m.insert(
                    #crate_path::objects::KeyRef::from(#name),
                    #crate_path::types::dynamic::maybe_materialize(&self.#ident),
                );
            }
        })
        .collect();

    // Generate field match arms
    let field_arms: TokenStream2 = processed_fields
        .iter()
        .map(|(ident, name)| {
            quote! {
                #name => #crate_path::types::dynamic::maybe_materialize(&self.#ident),
            }
        })
        .collect();

    // Generate auto_materialize override if needed
    let auto_materialize_impl = if auto_materialize {
        quote! {
            fn auto_materialize(&self) -> bool {
                true
            }
        }
    } else {
        quote! {}
    };

    // Handle generics - we need to support both lifetimes and type parameters
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // For the vtable impl, we'll use <'_> to work with any lifetime
    // Build a custom type parameters list with all lifetimes replaced by '_
    let vtable_ty_params = if generics.params.is_empty() {
        quote! {}
    } else {
        let params = generics.params.iter().map(|param| match param {
            syn::GenericParam::Lifetime(_) => quote! { '_ },
            syn::GenericParam::Type(ty) => {
                let ident = &ty.ident;
                quote! { #ident }
            }
            syn::GenericParam::Const(c) => {
                let ident = &c.ident;
                quote! { #ident }
            }
        });
        quote! { <#(#params),*> }
    };

    // For the unsafe casts in the vtable, we need the bare type name
    // We'll use the original name with original generics for the cast
    let bare_name_with_generics = quote! { #name #ty_generics };

    let generated = quote! {
        impl #impl_generics #crate_path::types::dynamic::DynamicType for #name #ty_generics #where_clause {
            #auto_materialize_impl

            fn materialize(&self) -> #crate_path::Value<'_> {
                let mut m = ::vector_map::VecMap::with_capacity(#field_count);
                #materialize_inserts
                #crate_path::Value::Map(#crate_path::objects::MapValue::Borrow(m))
            }

            fn field(&self, field: &str) -> ::core::option::Option<#crate_path::Value<'_>> {
                ::core::option::Option::Some(match field {
                    #field_arms
                    _ => return ::core::option::Option::None,
                })
            }
        }

        impl #crate_path::types::dynamic::DynamicValueVtable for #name #vtable_ty_params {
            fn vtable() -> &'static #crate_path::types::dynamic::Vtable {
                use ::std::sync::OnceLock;
                static VTABLE: OnceLock<#crate_path::types::dynamic::Vtable> = OnceLock::new();
                VTABLE.get_or_init(|| {
                    unsafe fn materialize_impl(ptr: *const ()) -> #crate_path::Value<'static> {
                        unsafe {
                            let this = &*(ptr as *const #bare_name_with_generics);
                            ::std::mem::transmute(
                                <#bare_name_with_generics as #crate_path::types::dynamic::DynamicType>::materialize(this)
                            )
                        }
                    }

                    unsafe fn field_impl(
                        ptr: *const (),
                        field: &str,
                    ) -> ::core::option::Option<#crate_path::Value<'static>> {
                        unsafe {
                            let this = &*(ptr as *const #bare_name_with_generics);
                            ::std::mem::transmute(
                                <#bare_name_with_generics as #crate_path::types::dynamic::DynamicType>::field(this, field)
                            )
                        }
                    }

                    unsafe fn debug_impl(
                        ptr: *const (),
                        f: &mut ::std::fmt::Formatter<'_>,
                    ) -> ::std::fmt::Result {
                        unsafe {
                            let this = &*(ptr as *const #bare_name_with_generics);
                            ::std::fmt::Debug::fmt(this, f)
                        }
                    }

                    #crate_path::types::dynamic::Vtable {
                        materialize: materialize_impl,
                        field: field_impl,
                        debug: debug_impl,
                    }
                })
            }
        }
    };

    generated.into()
}

/// Check if a struct has a specific attribute at the struct level
fn has_struct_attr(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| {
        if attr.path().is_ident("dynamic") {
            if let Ok(Meta::Path(path)) = attr.parse_args::<Meta>() {
                return path.is_ident(name);
            }
        }
        false
    })
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
