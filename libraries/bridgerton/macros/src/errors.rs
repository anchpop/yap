//! Outbound error enums. Opaque source fields explicitly opt into message transport.
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Fields, ItemEnum, spanned::Spanned};

pub fn strip_messages(item: &mut ItemEnum) {
    for field in item
        .variants
        .iter_mut()
        .flat_map(|variant| &mut variant.fields)
    {
        field.attrs.retain(|attr| !attr.path().is_ident("bridge"));
    }
}

pub fn expand(mut item: ItemEnum) -> syn::Result<TokenStream> {
    if !item.generics.params.is_empty() || item.variants.is_empty() {
        return Err(syn::Error::new(
            item.span(),
            "error requires a nonempty, non-generic enum",
        ));
    }
    let name = &item.ident;
    let label = name.to_string();
    let mut shapes = Vec::new();
    let mut arms = Vec::new();
    for (index, variant) in item.variants.iter().enumerate() {
        if variant.discriminant.is_some()
            || variant
                .attrs
                .iter()
                .any(|a| a.path().is_ident("cfg") || a.path().is_ident("cfg_attr"))
        {
            return Err(syn::Error::new(
                variant.span(),
                "conditional variants and explicit discriminants are unsupported",
            ));
        }
        let variant_name = &variant.ident;
        let variant_label = variant_name.to_string();
        let tag = (index + 1) as u32;
        let mut fields = Vec::new();
        let mut bindings = Vec::new();
        let mut writes = Vec::new();
        for (i, field) in variant.fields.iter().enumerate() {
            let mut message = false;
            for attr in &field.attrs {
                if attr.path().is_ident("bridge") {
                    if attr.parse_args::<syn::Ident>()? != "message" {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "expected bridge(message) for an opaque error source",
                        ));
                    }
                    message = true;
                }
                if attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr") {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "conditional error fields are unsupported",
                    ));
                }
            }
            let binding = format_ident!("field_{i}");
            let field_name = field
                .ident
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| {
                    if variant.fields.len() == 1 {
                        "value".into()
                    } else {
                        format!("field_{i}")
                    }
                });
            let ty = &field.ty;
            let field_type = if message {
                quote! { String }
            } else {
                quote! { #ty }
            };
            let value = if message {
                quote! { ::bridgerton::__message_of!(#binding) }
            } else {
                quote! { #binding }
            };
            fields.push(quote! {::bridgerton::schema::Field {name: #field_name.into(), ty: <#field_type as ::bridgerton::schema::NativeType>::native_type(registry)}});
            writes.push(quote! { ::bridgerton::value::Value::write(&#value, writer)?; });
            bindings.push(match &field.ident {
                Some(name) => quote! { #name: #binding },
                None => quote! { #binding },
            });
        }
        let pattern = match &variant.fields {
            Fields::Unit => quote! { Self::#variant_name },
            Fields::Unnamed(_) => quote! { Self::#variant_name(#(#bindings),*) },
            Fields::Named(_) => quote! { Self::#variant_name {#(#bindings),*} },
        };
        let named = matches!(variant.fields, Fields::Named(_));
        shapes.push(quote! {::bridgerton::schema::Variant { name: #variant_label.into(), fields: vec![#(#fields),*], named: #named }});
        arms.push(quote! {#pattern => {::bridgerton::value::Value::write(&#tag, writer)?; #(#writes)* Ok(()) }});
    }
    let implementations = quote! {
        impl ::bridgerton::native::NativeError for #name {
            fn into_error_result(self) -> Result<::bridgerton::native::BridgeResult, ::bridgerton::Error> {
                let bytes = ::bridgerton::value::encode_with(|writer| writer.nested(|writer| match self {#(#arms),*}))?;
                Ok(::bridgerton::native::typed_error_bytes(bytes))
            }
            fn error_type(registry: &mut ::bridgerton::schema::Registry) -> Option<String> {
                Some(registry.error_shape::<Self>(#label, |registry| ::bridgerton::schema::Shape::Enum(vec![#(#shapes),*])).swift())
            }
        }
    };
    strip_messages(&mut item);
    Ok(quote! { #item #implementations })
}

/// The web representation: an `Error` whose `detail` carries the case name and
/// its fields, declared for TypeScript as a union of the enum's cases.
pub fn expand_wasm(mut item: ItemEnum) -> syn::Result<TokenStream> {
    let name = item.ident.clone();
    let label = name.to_string();
    let mut arms = Vec::new();
    let mut declarations = Vec::new();
    for variant in &item.variants {
        let variant_name = &variant.ident;
        let variant_label = variant_name.to_string();
        let mut bindings = Vec::new();
        let mut sets = Vec::new();
        let mut members = vec![format!("type: \"{variant_label}\"")];
        for (i, field) in variant.fields.iter().enumerate() {
            let message = field.attrs.iter().any(|attr| {
                attr.path().is_ident("bridge")
                    && attr
                        .parse_args::<syn::Ident>()
                        .is_ok_and(|id| id == "message")
            });
            let binding = format_ident!("field_{i}");
            let field_name = field
                .ident
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| {
                    if variant.fields.len() == 1 {
                        "value".into()
                    } else {
                        format!("field_{i}")
                    }
                });
            let ty = &field.ty;
            let typescript = if message {
                "string".to_string()
            } else {
                crate::typescript::TsType::from_syn_type(&Default::default(), ty).to_string()
            };
            members.push(format!("{field_name}: {typescript}"));
            let value = if message {
                quote! { ::bridgerton::__wasm_bindgen::JsValue::from_str(&::bridgerton::__message_of!(#binding)) }
            } else {
                quote! { ::bridgerton::IntoWasm::into_js(#binding).unwrap_or_else(|error| error) }
            };
            sets.push(quote! {
                let _ = ::bridgerton::__js_sys::Reflect::set(&detail, &::bridgerton::__wasm_bindgen::JsValue::from_str(#field_name), &#value);
            });
            bindings.push(match &field.ident {
                Some(name) => quote! { #name: #binding },
                None => quote! { #binding },
            });
        }
        let pattern = match &variant.fields {
            Fields::Unit => quote! { Self::#variant_name },
            Fields::Unnamed(_) => quote! { Self::#variant_name(#(#bindings),*) },
            Fields::Named(_) => quote! { Self::#variant_name {#(#bindings),*} },
        };
        arms.push(quote! {
            #pattern => {
                let _ = ::bridgerton::__js_sys::Reflect::set(&detail, &::bridgerton::__wasm_bindgen::JsValue::from_str("type"), &::bridgerton::__wasm_bindgen::JsValue::from_str(#variant_label));
                #(#sets)*
            }
        });
        declarations.push(format!("{{ {} }}", members.join("; ")));
    }
    let declaration = format!(
        "export type {label} = Error & {{ detail: {} }};",
        declarations.join(" | ")
    );
    strip_messages(&mut item);
    Ok(quote! {
        #[cfg(target_arch = "wasm32")]
        #item
        #[cfg(target_arch = "wasm32")]
        #[allow(unused_imports)] use ::bridgerton::__wasm::*;
        #[cfg(target_arch = "wasm32")]
        const _: () = {
            #[::bridgerton::__wasm_bindgen::prelude::wasm_bindgen(typescript_custom_section)]
            const TS_APPEND_CONTENT: &'static str = #declaration;
        };
        #[cfg(target_arch = "wasm32")]
        impl ::bridgerton::WasmError for #name {
            fn into_js_error(self) -> ::bridgerton::__wasm_bindgen::JsValue {
                let message = ::std::string::ToString::to_string(&self);
                let detail = ::bridgerton::__js_sys::Object::new();
                match self { #(#arms),* }
                ::bridgerton::js_error(&message, Some(detail.into()))
            }
        }
    })
}
