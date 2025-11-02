use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Item, parse_macro_input};

/// Generate standard derive attributes
fn standard_derives(include_copy: bool) -> TokenStream2 {
    let copy = include_copy.then(|| quote! { Copy, });
    quote! {
        #[derive(Debug, Clone, #copy PartialEq, Eq, ::serde::Serialize, ::serde::Deserialize, ::borsh::BorshSerialize, ::borsh::BorshDeserialize)]
    }
}

/// Add attributes at the beginning of the attribute list
trait PrependAttrs {
    fn prepend_attrs(&mut self, attrs: impl IntoIterator<Item = syn::Attribute>);
}

impl PrependAttrs for Vec<syn::Attribute> {
    fn prepend_attrs(&mut self, attrs: impl IntoIterator<Item = syn::Attribute>) {
        let new_attrs: Vec<_> = attrs.into_iter().collect();
        self.splice(0..0, new_attrs);
    }
}

/// Standard schema attribute macro
///
/// Apply this to structs and enums to automatically add all standard derives.
#[proc_macro_attribute]
pub fn standard(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(item as Item);

    let derives = standard_derives(false);

    match &mut input {
        Item::Struct(s) => s.attrs.prepend_attrs([syn::parse_quote! { #derives }]),
        Item::Enum(e) => e.attrs.prepend_attrs([syn::parse_quote! { #derives }]),
        _ => {}
    }

    quote! { #input }.into()
}

/// Standard schema for enums with repr
#[proc_macro_attribute]
pub fn standard_enum(attr: TokenStream, item: TokenStream) -> TokenStream {
    let repr: syn::Type = if attr.is_empty() {
        syn::parse_quote! { u8 }
    } else {
        syn::parse(attr).expect("Expected repr type like u8")
    };

    let Item::Enum(e) = parse_macro_input!(item as Item) else {
        panic!("standard_enum can only be applied to enums")
    };

    let mut result = e;
    let derives = standard_derives(true);

    result.attrs.prepend_attrs([
        syn::parse_quote! { #derives },
        syn::parse_quote! { #[repr(#repr)] },
        syn::parse_quote! { #[borsh(use_discriminant = true)] },
    ]);

    quote! { #result }.into()
}
