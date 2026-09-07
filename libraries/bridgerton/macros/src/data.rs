//! Value declarations and codecs generated from the same Rust fields/variants.
use super::*;

/// Declaration options accepted after `transparent`.
#[derive(Default)]
pub struct TransparentOptions {
    pub namespace: bool,
    pub missing_as_null: bool,
    pub hashmap_as_object: bool,
    pub large_number_types_as_bigints: bool,
}

impl TransparentOptions {
    pub fn parse(attributes: Tokens) -> syn::Result<Self> {
        let mut options = Self::default();
        let mut first = true;
        syn::meta::parser(|meta| {
            if first {
                first = false;
                if meta.path.is_ident("transparent") {
                    return Ok(());
                }
            }
            if meta.path.is_ident("namespace") {
                options.namespace = true;
            } else if meta.path.is_ident("missing_as_null") {
                options.missing_as_null = true;
            } else if meta.path.is_ident("hashmap_as_object") {
                options.hashmap_as_object = true;
            } else if meta.path.is_ident("large_number_types_as_bigints") {
                options.large_number_types_as_bigints = true;
            } else {
                return Err(meta.error(
                    "unknown transparent option; expected namespace, missing_as_null, hashmap_as_object, or large_number_types_as_bigints",
                ));
            }
            Ok(())
        })
        .parse2(attributes)?;
        Ok(options)
    }
}

// Serde remains explicit: this adds binding support without changing its format.
pub fn transparent(item: Item, options: TransparentOptions) -> syn::Result<Tokens> {
    let derive_input: syn::DeriveInput = match &item {
        Item::Struct(item) => item.clone().into(),
        Item::Enum(item) => item.clone().into(),
        _ => {
            return Err(syn::Error::new_spanned(
                item,
                "transparent requires a struct or enum",
            ));
        }
    };
    if options.namespace && !matches!(item, Item::Enum(_)) {
        return Err(syn::Error::new_spanned(
            item,
            "namespace applies to enums only",
        ));
    }
    let bigint = options.large_number_types_as_bigints;
    let null = options.missing_as_null;
    let objects = options.hashmap_as_object;
    let _ = derive_input;
    let option_names = [
        ("namespace", options.namespace),
        ("missing_as_null", null),
        ("hashmap_as_object", objects),
        ("large_number_types_as_bigints", bigint),
    ]
    .into_iter()
    .filter(|(_, enabled)| *enabled)
    .map(|(name, _)| format_ident!("{name}"))
    .collect::<Vec<_>>();
    let (name, generics) = match &item {
        Item::Struct(i) => (&i.ident, &i.generics),
        Item::Enum(i) => (&i.ident, &i.generics),
        _ => unreachable!(),
    };
    let type_name = name.to_string().trim_start_matches("r#").to_owned();
    let name_len = type_name.chars().count() as u32;
    let params = generics.type_params().map(|p| &p.ident).collect::<Vec<_>>();
    let mut type_generics = generics.clone();
    for param in &params {
        type_generics.make_where_clause().predicates.push(
            syn::parse_quote! { #param: ::bridgerton::__SerdeType<#bigint, #null, #objects> },
        );
    }
    let (type_impl, _, type_where) = type_generics.split_for_impl();
    let mut serde_generics = generics.clone();
    serde_generics
        .params
        .push(syn::parse_quote! { const __B: bool });
    serde_generics
        .params
        .push(syn::parse_quote! { const __N: bool });
    serde_generics
        .params
        .push(syn::parse_quote! { const __O: bool });
    for param in &params {
        serde_generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote! { #param: ::bridgerton::__SerdeType<__B, __N, __O> });
    }
    let (serde_impl, _, serde_where) = serde_generics.split_for_impl();
    let punctuation = if params.is_empty() {
        0
    } else {
        2 + 2 * (params.len() as u32 - 1)
    };
    let describe_params = params.iter().enumerate().map(|(i, param)| {
        let prefix = if i == 0 { "<" } else { ", " };
        quote! { ::bridgerton::__describe!(#prefix); <#param as ::bridgerton::__SerdeType<__B, __N, __O>>::describe(); }
    });
    let close = (!params.is_empty()).then(|| quote! { ::bridgerton::__describe!(">"); });
    let (_, ty_generics, _) = generics.split_for_impl();
    let mut input_generics = type_generics.clone();
    input_generics
        .make_where_clause()
        .predicates
        .push(syn::parse_quote! { for<'__bridgerton_de> Self: ::bridgerton::serde::Deserialize<'__bridgerton_de> });
    let (in_generics, _, in_where) = input_generics.split_for_impl();
    let mut output_generics = type_generics.clone();
    output_generics
        .make_where_clause()
        .predicates
        .push(syn::parse_quote! { for<'__bridgerton_ser> Self: ::bridgerton::__Serializable<'__bridgerton_ser> });
    let (out_generics, _, out_where) = output_generics.split_for_impl();
    Ok(quote! {
        // Both derives see the item after rustc has removed conditional fields
        // and variants, so declarations never mention configured-out types.
        #[cfg_attr(not(target_arch = "wasm32"), derive(::bridgerton::__NativeValue))]
        #[cfg_attr(target_arch = "wasm32", derive(::bridgerton::__TypeScript))]
        #[cfg_attr(target_arch = "wasm32", bridge(transparent #(, #option_names)*))]
        #item
        #[cfg(target_arch = "wasm32")]
        impl #type_impl ::bridgerton::WasmType for #name #ty_generics #type_where {
            const INPUT_LEN: u32 = <Self as ::bridgerton::__SerdeType<#bigint, #null, #objects>>::LEN;
            fn describe_name<const INPUT: bool>() {
                <Self as ::bridgerton::__SerdeType<#bigint, #null, #objects>>::describe();
            }
        }
        #[cfg(target_arch = "wasm32")]
        impl #serde_impl ::bridgerton::__SerdeType<__B, __N, __O> for #name #ty_generics #serde_where {
            const LEN: u32 = #name_len + #punctuation #( + <#params as ::bridgerton::__SerdeType<__B, __N, __O>>::LEN )*;
            fn describe() { ::bridgerton::__describe!(#type_name); #(#describe_params)* #close }
        }
        #[cfg(target_arch = "wasm32")]
        impl #in_generics ::bridgerton::FromWasm for #name #ty_generics #in_where {
            type Input = ::bridgerton::TypedJs<Self, true>;
            fn from_wasm(value: Self::Input) -> Result<Self, ::bridgerton::__wasm_bindgen::JsValue> {
                <Self as ::bridgerton::FromWasm>::from_js(value.into())
            }
            fn from_js(value: ::bridgerton::__wasm_bindgen::JsValue) -> Result<Self, ::bridgerton::__wasm_bindgen::JsValue> {
                ::bridgerton::from_js(value)
            }
        }
        #[cfg(target_arch = "wasm32")]
        impl #out_generics ::bridgerton::IntoWasm for #name #ty_generics #out_where {
            type Output = ::bridgerton::TypedJs<Self, false>;
            fn into_wasm(self) -> Result<Self::Output, ::bridgerton::__wasm_bindgen::JsValue> {
                ::bridgerton::to_js_with(&self, #bigint, #null, #objects).map(::bridgerton::TypedJs::from)
            }
        }
        #[cfg(target_arch = "wasm32")]
        impl #out_generics ::bridgerton::WasmError for #name #ty_generics #out_where {
            fn into_js_error(self) -> ::bridgerton::__wasm_bindgen::JsValue {
                let message = ::bridgerton::__serde_json::to_string(&self).unwrap_or_default();
                let detail = ::bridgerton::IntoWasm::into_js(self).unwrap_or_else(|error| error);
                ::bridgerton::js_error(&message, Some(detail))
            }
        }
        #[cfg(target_arch = "wasm32")]
        impl #out_generics ::bridgerton::JsArguments for #name #ty_generics #out_where {
            fn invoke(self, function: &::bridgerton::__js_sys::Function) -> Result<::bridgerton::__wasm_bindgen::JsValue, ::bridgerton::__wasm_bindgen::JsValue> {
                function.call1(&::bridgerton::__wasm_bindgen::JsValue::UNDEFINED, &::bridgerton::IntoWasm::into_wasm(self)?.into())
            }
        }
    })
}

/// The TypeScript declaration, emitted from the derive input so that
/// conditional fields and variants have already been resolved by rustc.
pub fn typescript_declaration(input: syn::DeriveInput) -> syn::Result<Tokens> {
    let mut options = TransparentOptions::default();
    for attr in input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("bridge"))
    {
        let syn::Meta::List(list) = &attr.meta else {
            continue;
        };
        options = TransparentOptions::parse(list.tokens.clone())?;
    }
    let declaration = typescript::declaration(
        &input,
        typescript::ContainerAttrs {
            namespace: options.namespace,
            ty_config: typescript::TypeGenerationConfig {
                missing_as_null: options.missing_as_null,
                hashmap_as_object: options.hashmap_as_object,
                large_number_types_as_bigints: options.large_number_types_as_bigints,
                ..Default::default()
            },
        },
    )?;
    Ok(quote! {
        const _: () = {
            #[allow(unused_imports)] use ::bridgerton::__wasm::*;
            #[::bridgerton::__wasm_bindgen::prelude::wasm_bindgen(typescript_custom_section)]
            const TS_APPEND_CONTENT: &'static str = #declaration;
        };
    })
}
