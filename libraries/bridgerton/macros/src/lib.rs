//! One declaration, real wasm-bindgen wrappers, and generated native C/Swift wrappers.
//! Signatures outside the supported surface are rejected with a diagnostic rather than
//! silently exported on one platform only.
mod data;
mod errors;
mod native_data;
mod typescript;
mod wasm_methods;
use syn::parse::Parser;
#[derive(Default)]
struct Options {
    only: Option<Vec<String>>,
}
use proc_macro::TokenStream;
use proc_macro2::TokenStream as Tokens;
use quote::{format_ident, quote};
use syn::{
    FnArg, ImplItem, Item, ItemImpl, Pat, ReturnType, Type, Visibility, parse_macro_input,
    spanned::Spanned,
};

#[doc(hidden)]
#[proc_macro]
pub fn __describe(input: TokenStream) -> TokenStream {
    let text = syn::parse::<syn::LitStr>(input.clone())
        .map(|s| s.value())
        .or_else(|_| syn::parse::<syn::Ident>(input).map(|s| s.to_string()));
    match text {
        Ok(text) => {
            let chars = text.chars().map(|c| c as u32);
            quote! {{ #( ::bridgerton::__wasm_bindgen::describe::inform(#chars); )* }}.into()
        }
        Err(error) => error.into_compile_error().into(),
    }
}

/// Export Rust objects, methods, and values through one public attribute.
#[proc_macro_attribute]
pub fn bridge(attributes: TokenStream, input: TokenStream) -> TokenStream {
    if attributes.is_empty()
        && let Ok(Item::Fn(item)) = syn::parse::<Item>(input.clone())
    {
        return wasm_methods::free_function(item)
            .unwrap_or_else(syn::Error::into_compile_error)
            .into();
    }
    if attributes.is_empty()
        && let Ok(Item::Impl(item)) = syn::parse::<Item>(input.clone())
    {
        return direct_impl(item)
            .unwrap_or_else(syn::Error::into_compile_error)
            .into();
    }
    expand_bridge(attributes, input, true)
}

fn direct_impl(item: ItemImpl) -> syn::Result<Tokens> {
    // wasm-bindgen names the JavaScript class after the impl's type identifier,
    // so both platforms take the same rule rather than diverging on paths.
    if !matches!(&*item.self_ty, Type::Path(path) if path.qself.is_none() && path.path.segments.len() == 1)
    {
        return Err(syn::Error::new(
            item.self_ty.span(),
            "name a bridged type by a single identifier; import it with `use` instead of writing a path",
        ));
    }
    let names = item.items.iter().filter_map(|i| match i {
        ImplItem::Fn(f) if matches!(f.vis, Visibility::Public(_)) => Some(&f.sig.ident),
        _ => None,
    });
    Ok(quote! {
        #[cfg(target_arch = "wasm32")]
        #[::bridgerton::__wasm_bridge]
        #item
        #[cfg(not(target_arch = "wasm32"))]
        #[::bridgerton::__native_methods(only(#(#names),*))]
        #item
    })
}

#[doc(hidden)]
#[proc_macro_attribute]
pub fn __wasm_bridge(_: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as ItemImpl);
    configured_methods(&item, quote! { #[::bridgerton::__wasm_bridge] })
        .and_then(|configured| configured.map_or_else(|| wasm_impl(item), Ok))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

// Derive inputs have already had conditional fields and variants removed by rustc.
#[doc(hidden)]
#[proc_macro_derive(__TypeScript, attributes(bridge))]
pub fn typescript_declaration(input: TokenStream) -> TokenStream {
    data::typescript_declaration(parse_macro_input!(input as syn::DeriveInput))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[doc(hidden)]
#[proc_macro_derive(__NativeValue, attributes(bridge))]
pub fn native_value(input: TokenStream) -> TokenStream {
    native_data::expand(parse_macro_input!(input as Item))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[doc(hidden)]
#[proc_macro_attribute]
pub fn __native_methods(attributes: TokenStream, input: TokenStream) -> TokenStream {
    native_when_enabled(attributes, input)
}

#[doc(hidden)]
#[proc_macro_attribute]
pub fn __native_function(_: TokenStream, input: TokenStream) -> TokenStream {
    native_free_function(parse_macro_input!(input as syn::ItemFn))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn native_free_function(item: syn::ItemFn) -> syn::Result<Tokens> {
    let name = &item.sig.ident;
    let marker = format_ident!(
        "__BridgertonFunction_{}",
        name.to_string().trim_start_matches("r#")
    );
    let signature = &item.sig;
    let method: ItemImpl = syn::parse_quote! { impl #marker { pub #signature { unreachable!() } } };
    let exports = expand_impl_kind(method, Options::default(), Some(name.clone()))?;
    Ok(quote! { #item #[allow(non_camel_case_types)] struct #marker; #exports })
}

// Let rustc evaluate method conditions before inspecting signatures or emitting
// bindings. A conditional method becomes a conditional single-method impl;
// the existing per-impl registry then keeps wrappers and metadata together.
// cfg_attr is expanded into two compiler-gated branches, preserving attribute
// order and allowing nested cfg_attr and conditional bridge annotations.
fn configured_methods(item: &ItemImpl, bridge: Tokens) -> syn::Result<Option<Tokens>> {
    let mut plain = item.clone();
    plain.items.clear();
    let mut configured = Vec::new();
    for entry in &item.items {
        let ImplItem::Fn(method) = entry else {
            plain.items.push(entry.clone());
            continue;
        };
        let Some(index) = method
            .attrs
            .iter()
            .position(|a| a.path().is_ident("cfg") || a.path().is_ident("cfg_attr"))
        else {
            plain.items.push(entry.clone());
            continue;
        };
        let mut method = method.clone();
        let condition = method.attrs.remove(index);
        let mut branch = item.clone();
        branch.items = vec![ImplItem::Fn(method.clone())];
        if condition.path().is_ident("cfg") {
            configured.push(quote! { #condition #bridge #branch });
        } else {
            let mut arguments = condition
                .parse_args_with(
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                )?
                .into_iter();
            let predicate = arguments.next().ok_or_else(|| {
                syn::Error::new_spanned(&condition, "cfg_attr requires a predicate")
            })?;
            let attributes = arguments.map(|meta| syn::parse_quote! { #[#meta] });
            method.attrs.splice(index..index, attributes);
            let mut enabled = branch.clone();
            enabled.items = vec![ImplItem::Fn(method)];
            configured.push(quote! {
                #[cfg(#predicate)] #bridge #enabled
                #[cfg(not(#predicate))] #bridge #branch
            });
        }
    }
    if configured.is_empty() {
        return Ok(None);
    }
    let plain = (!plain.items.is_empty()).then(|| quote! { #bridge #plain });
    Ok(Some(quote! { #plain #(#configured)* }))
}

fn wasm_impl(item: ItemImpl) -> syn::Result<Tokens> {
    let mut wasm = item.clone();
    let mut rust_only = item.clone();
    rust_only.items.clear();
    let mut exported = Vec::new();
    for mut entry in wasm.items {
        let mut skip = false;
        if let ImplItem::Fn(method) = &mut entry {
            let mut attrs = Vec::new();
            for attr in std::mem::take(&mut method.attrs) {
                if !attr.path().is_ident("bridge") {
                    attrs.push(attr);
                    continue;
                }
                let option = attr.parse_args::<syn::Ident>()?;
                match option.to_string().as_str() {
                    "getter" | "constructor" => {
                        attrs.push(syn::parse_quote! {#[wasm_bindgen(#option)]});
                    }
                    "skip" => skip = true,
                    _ => {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "expected getter, constructor, or skip",
                        ));
                    }
                }
            }
            method.attrs = attrs;
        }
        if skip {
            rust_only.items.push(entry);
        } else if let ImplItem::Fn(method) = &entry
            && let Some(wrapper) = wasm_methods::adapt(method)?
        {
            let mut original = method.clone();
            original
                .attrs
                .retain(|attr| !attr.path().is_ident("wasm_bindgen"));
            rust_only.items.push(ImplItem::Fn(original));
            exported.push(ImplItem::Fn(wrapper));
        } else {
            exported.push(entry);
        }
    }
    wasm.items = exported;
    let rust_only = (!rust_only.items.is_empty()).then(|| {
        quote! {
            #[cfg(target_arch="wasm32")]
            #rust_only
        }
    });
    Ok(quote! {
        #[cfg(target_arch="wasm32")]
        #[allow(unused_imports)] use ::bridgerton::__wasm::*;
        #[cfg(target_arch="wasm32")]
        #[::bridgerton::__wasm_bindgen::prelude::wasm_bindgen]
        #wasm
        #rust_only
    })
}

// The bridge forwards its Swift feature here. Target cfg must still be evaluated
// in the consumer, because proc macros run on the build host.
#[doc(hidden)]
#[proc_macro_attribute]
pub fn __native_bridge(attributes: TokenStream, input: TokenStream) -> TokenStream {
    expand_bridge(attributes, input, false)
}

fn native_when_enabled(attributes: TokenStream, input: TokenStream) -> TokenStream {
    let attributes: Tokens = attributes.into();
    let input: Tokens = input.into();
    let mut plain = input.clone();
    if let Ok(mut item) = syn::parse2::<ItemImpl>(input.clone()) {
        match configured_methods(&item, quote! { #[::bridgerton::bridge(#attributes)] }) {
            Ok(Some(configured)) => return configured.into(),
            Err(error) => return error.into_compile_error().into(),
            Ok(None) => {}
        }
        for entry in &mut item.items {
            if let ImplItem::Fn(method) = entry {
                method.attrs.retain(|attr| !attr.path().is_ident("bridge"));
            }
        }
        plain = quote! { #item };
    }
    if let Ok(mut item) = syn::parse2::<syn::ItemEnum>(plain.clone()) {
        errors::strip_messages(&mut item);
        plain = quote! { #item };
    }
    quote! {
        #[cfg(not(target_arch = "wasm32"))]
        #[::bridgerton::__native_bridge(#attributes)]
        #input
        #[cfg(target_arch = "wasm32")]
        #plain
    }
    .into()
}

fn expand_bridge(attributes: TokenStream, input: TokenStream, gate_native: bool) -> TokenStream {
    let mode = attributes.to_string();
    if mode == "transparent" || mode.starts_with("transparent,") {
        if gate_native {
            let options = match data::TransparentOptions::parse(attributes.into()) {
                Ok(options) => options,
                Err(error) => return error.into_compile_error().into(),
            };
            return data::transparent(parse_macro_input!(input as Item), options)
                .unwrap_or_else(syn::Error::into_compile_error)
                .into();
        }
        return native_data::expand(parse_macro_input!(input as Item))
            .unwrap_or_else(syn::Error::into_compile_error)
            .into();
    }
    if matches!(mode.as_str(), "opaque" | "opaque, custom_arguments") {
        if gate_native {
            let item = parse_macro_input!(input as syn::ItemStruct);
            if mode == "opaque, custom_arguments" {
                return native_when_enabled(attributes, quote! {#item}.into());
            }
            return object(item)
                .unwrap_or_else(syn::Error::into_compile_error)
                .into();
        }
        return object_with_arguments(
            parse_macro_input!(input as syn::ItemStruct),
            mode != "opaque, custom_arguments",
        )
        .unwrap_or_else(syn::Error::into_compile_error)
        .into();
    }
    if mode == "error" {
        if gate_native {
            let item = parse_macro_input!(input as syn::ItemEnum);
            let wasm = match errors::expand_wasm(item.clone()) {
                Ok(tokens) => tokens,
                Err(error) => return error.into_compile_error().into(),
            };
            return quote! {
                #[cfg(not(target_arch = "wasm32"))]
                #[::bridgerton::__native_bridge(error)]
                #item
                #wasm
            }
            .into();
        }
        return errors::expand(parse_macro_input!(input as syn::ItemEnum))
            .unwrap_or_else(syn::Error::into_compile_error)
            .into();
    }
    let mut options = Options::default();
    let parser = syn::meta::parser(|meta| {
        let name = meta
            .path
            .get_ident()
            .map(ToString::to_string)
            .unwrap_or_default();
        if name != "only" {
            return Err(meta.error(
                "unknown bridge option; expected transparent, opaque, error, or only(...)",
            ));
        }
        let mut names = Vec::new();
        meta.parse_nested_meta(|inner| {
            names.push(
                inner
                    .path
                    .get_ident()
                    .ok_or_else(|| inner.error("expected method name"))?
                    .to_string(),
            );
            Ok(())
        })?;
        options.only = Some(names);
        Ok(())
    });
    if let Err(error) = parser.parse(attributes.clone()) {
        return error.into_compile_error().into();
    }
    if gate_native && let Ok(mut item) = syn::parse::<ItemImpl>(input.clone()) {
        if let Some(names) = &options.only {
            for entry in &mut item.items {
                if let ImplItem::Fn(method) = entry
                    && !names.contains(&method.sig.ident.to_string())
                {
                    method.attrs.retain(|a| !a.path().is_ident("bridge"));
                    method.attrs.push(syn::parse_quote! { #[bridge(skip)] });
                }
            }
        }
        return direct_impl(item)
            .unwrap_or_else(syn::Error::into_compile_error)
            .into();
    }
    let item = parse_macro_input!(input as Item);
    let result = match item {
        Item::Struct(item) => Err(syn::Error::new_spanned(
            item,
            "choose bridge(transparent) or bridge(opaque) for a type",
        )),
        Item::Impl(item) => expand_impl(item, options),
        Item::Enum(item) => Err(syn::Error::new_spanned(
            item,
            "choose bridge(transparent) or bridge(opaque) for a type",
        )),
        item => Err(syn::Error::new(
            item.span(),
            "bridge supports structs, enums, free functions, and non-generic inherent impls",
        )),
    };
    result.unwrap_or_else(syn::Error::into_compile_error).into()
}

#[doc(hidden)]
#[proc_macro_attribute]
pub fn __native_object(_: TokenStream, input: TokenStream) -> TokenStream {
    object_with_arguments(parse_macro_input!(input as syn::ItemStruct), true)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn object(item: syn::ItemStruct) -> syn::Result<Tokens> {
    let name = &item.ident;
    let wasm_name = name.to_string().trim_start_matches("r#").to_owned();
    let wasm_name_len = wasm_name.chars().count() as u32;
    Ok(quote! {
        #[cfg(target_arch = "wasm32")]
        #[allow(unused_imports)] use ::bridgerton::__wasm::*;
        #[cfg(target_arch = "wasm32")]
        #[::bridgerton::__wasm_bindgen::prelude::wasm_bindgen(wasm_bindgen = ::bridgerton::__wasm_bindgen)]
        #item
        #[cfg(target_arch = "wasm32")]
        ::bridgerton::__wasm_object!(#name, #wasm_name, #wasm_name_len);
        #[cfg(target_arch = "wasm32")]
        impl ::bridgerton::JsArguments for #name {
            fn invoke(self, function: &::bridgerton::__js_sys::Function) -> Result<::bridgerton::__wasm_bindgen::JsValue, ::bridgerton::__wasm_bindgen::JsValue> {
                function.call1(&::bridgerton::__wasm_bindgen::JsValue::UNDEFINED, &self.into())
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        #[::bridgerton::__native_object]
        #item
    })
}

fn object_with_arguments(item: syn::ItemStruct, arguments: bool) -> syn::Result<Tokens> {
    if !item.generics.params.is_empty() {
        return Err(syn::Error::new(
            item.span(),
            "generic objects are unsupported",
        ));
    }
    let name = &item.ident;
    let argument_impls = arguments.then(|| {
        quote! {
            ::bridgerton::__native_object_arguments!(#name);
        }
    });
    let class_name = name.to_string().trim_start_matches("r#").to_owned();
    let free_name = format!("bridgerton_{}_free", class_name.to_lowercase());
    let free_ident = format_ident!("{free_name}");
    let header = format!("BridgeResult {free_name}(const void *handle);\n");
    let swift = format!(
        "\n@MainActor public final class `{class_name}`: BridgeObject {{\n    fileprivate var __bridgertonOwnedHandle: UnsafeRawPointer?\n    fileprivate var __bridgertonHandle: UnsafeRawPointer {{ guard let value = __bridgertonOwnedHandle else {{ preconditionFailure(\"bridgerton: object was consumed\") }}; return value }}\n    fileprivate func __bridgertonTakeHandle() -> UnsafeRawPointer {{ let value = __bridgertonHandle; __bridgertonOwnedHandle = nil; return value }}\n    internal init(bridgeHandle: UnsafeRawPointer) {{ _ = BridgeInterface.checked; __bridgertonOwnedHandle = bridgeHandle }}\n    isolated deinit {{ if let handle = __bridgertonOwnedHandle {{ _ = try! bridgeNumber({free_name}(handle)) }} }}\n}}\n"
    );
    Ok(quote! {
        #item
        #[cfg(not(target_arch = "wasm32"))]
        impl ::bridgerton::native::NativeObject for #name {}
        #[cfg(not(target_arch = "wasm32"))]
        #argument_impls
        #[cfg(not(target_arch = "wasm32"))]
        impl ::bridgerton::native::CallbackArguments for #name { ::bridgerton::__native_callback_value!(); }
        #[cfg(not(target_arch = "wasm32"))]
        impl ::bridgerton::native::NativeReturn for #name {
            type Success = Self;
            fn into_native_return(self) -> Result<::bridgerton::native::BridgeResult, ::bridgerton::Error> {
                Ok(::bridgerton::native::object(self))
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        impl ::bridgerton::schema::NativeType for #name {
            fn return_decoder() -> &'static str { "bridgeReturn" }
            fn native_type(registry: &mut ::bridgerton::schema::Registry) -> ::bridgerton::schema::Type {
                registry.object::<Self>(#class_name)
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        #[doc = "# Safety\nConsume exactly one owned handle of the generated type."]
        #[unsafe(export_name = concat!(module_path!(), "::", stringify!(#name), "::", #free_name))]
        pub unsafe extern "C" fn #free_ident(handle: *const ::std::ffi::c_void) -> ::bridgerton::native::BridgeResult {
            unsafe { ::bridgerton::native::free_object::<#name>(handle) }
        }
        #[cfg(not(target_arch = "wasm32"))]
        const _: () = {
            fn definition() -> ::bridgerton::native::Definition {
                let mut types = ::bridgerton::schema::Registry::default();
                <#name as ::bridgerton::schema::NativeType>::native_type(&mut types);
                let (header, swift) = ::bridgerton::native::qualify_export(#header, #swift, concat!(module_path!(), "::", stringify!(#name)), #free_name);
                ::bridgerton::native::Definition { header, swift, types }
            }
            ::bridgerton::__inventory::submit! {
                ::bridgerton::exports::Export::object::<#name>(#class_name, definition)
            }
        };
    })
}

enum Input {
    Number,
    Borrowed(Box<Type>),
    Data(Box<Type>),
}

fn last_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|part| part.ident.to_string().trim_start_matches("r#").to_owned()),
        _ => None,
    }
}

fn input_kind(ty: &Type) -> syn::Result<Input> {
    if last_name(ty).as_deref() == Some("u32") {
        return Ok(Input::Number);
    }
    if let Type::Reference(reference) = ty
        && reference.mutability.is_none()
    {
        return Ok(Input::Borrowed(reference.elem.clone()));
    }
    Ok(Input::Data(Box::new(ty.clone())))
}

fn resolved_return(output: &ReturnType, object: &Type) -> Type {
    use syn::visit_mut::{VisitMut, visit_type_mut};
    struct ResolveSelf<'a>(&'a Type);
    impl VisitMut for ResolveSelf<'_> {
        fn visit_type_mut(&mut self, ty: &mut Type) {
            if matches!(ty, Type::Path(path) if path.path.is_ident("Self")) {
                *ty = self.0.clone();
            } else {
                visit_type_mut(self, ty);
            }
        }
    }
    let mut ty = match output {
        ReturnType::Default => syn::parse_quote! {()},
        ReturnType::Type(_, ty) => (**ty).clone(),
    };
    ResolveSelf(object).visit_type_mut(&mut ty);
    ty
}

fn expand_impl(item: ItemImpl, options: Options) -> syn::Result<Tokens> {
    expand_impl_kind(item, options, None)
}
fn expand_impl_kind(
    mut item: ItemImpl,
    options: Options,
    free_function: Option<syn::Ident>,
) -> syn::Result<Tokens> {
    if item.trait_.is_some() || !item.generics.params.is_empty() {
        return Err(syn::Error::new(
            item.span(),
            "bridged impls must be non-generic inherent impls",
        ));
    }
    let ty = item.self_ty.clone();
    let class_name =
        last_name(&ty).ok_or_else(|| syn::Error::new(ty.span(), "expected named type"))?;
    let mut native_functions = Vec::new();
    let mut swift = if free_function.is_some() {
        String::from("\n@MainActor\n")
    } else {
        format!("\nextension `{class_name}` {{\n")
    };
    let mut members = Vec::new();
    let mut constructors = 0;
    let mut method_definitions = Vec::new();

    for entry in &mut item.items {
        let ImplItem::Fn(method) = entry else {
            return Err(syn::Error::new(
                entry.span(),
                "only methods are supported in a bridged impl",
            ));
        };
        let mut constructor = false;
        let mut skip = false;
        let mut getter = false;
        let mut remaining = Vec::new();
        for attr in std::mem::take(&mut method.attrs) {
            if attr.path().is_ident("bridge") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("constructor") {
                        constructor = true;
                        Ok(())
                    } else if meta.path.is_ident("getter") {
                        getter = true;
                        Ok(())
                    } else if meta.path.is_ident("skip") {
                        skip = true;
                        Ok(())
                    } else {
                        Err(meta.error("expected constructor, getter, or skip"))
                    }
                })?;
            } else {
                remaining.push(attr);
            }
        }
        method.attrs = remaining;
        if skip {
            continue;
        }
        if options
            .only
            .as_ref()
            .is_some_and(|names| !names.contains(&method.sig.ident.to_string()))
        {
            continue;
        }
        if !matches!(method.vis, Visibility::Public(_)) {
            continue;
        }
        if !method.sig.generics.params.is_empty() || method.sig.unsafety.is_some() {
            return Err(syn::Error::new(
                method.sig.span(),
                "generic/unsafe methods are not supported",
            ));
        }
        let method_name = &method.sig.ident;
        let method_label = method_name.to_string().trim_start_matches("r#").to_owned();
        let symbol_name = if free_function.is_some() {
            format!("bridgerton_fn_{method_label}")
        } else {
            format!("bridgerton_{}_{}", class_name.to_lowercase(), method_label)
        };
        let symbol = format_ident!("{symbol_name}");
        let is_async = method.sig.asyncness.is_some();
        let output = resolved_return(&method.sig.output, &ty);
        let is_static = method.sig.receiver().is_none();
        if getter && (is_static || is_async || method.sig.inputs.len() != 1) {
            return Err(syn::Error::new(
                method.sig.span(),
                "a getter needs &self, no arguments, and a synchronous return",
            ));
        }
        if constructor {
            constructors += 1;
            if is_async || !is_static || constructors > 1 {
                return Err(syn::Error::new(
                    method.sig.span(),
                    "one synchronous Self-returning constructor is supported",
                ));
            }
        }

        members.push(quote! {(stringify!(#method_name), #constructor)});
        let mut native_args = Vec::new();
        let mut native_setup = Vec::new();
        let mut native_prepare = Vec::new();
        let mut native_values = Vec::new();
        let mut c_args = Vec::new();
        let mut swift_args = Vec::new();
        let mut swift_values = Vec::new();
        let mut lifetimes = Vec::new();
        let mut owned_inputs = Vec::new();
        for arg in &method.sig.inputs {
            match arg {
                FnArg::Receiver(receiver) => {
                    if constructor
                        || is_static
                        || receiver.reference.is_none()
                        || receiver.mutability.is_some()
                    {
                        return Err(syn::Error::new(
                            receiver.span(),
                            "bridged methods take &self: objects are shared by reference on both platforms, so keep mutable state in a Cell or RefCell",
                        ));
                    }
                    native_args.push(quote! { handle: *const ::std::ffi::c_void });
                    c_args.push("const void *handle".to_string());
                    native_setup.push(quote! { let object = unsafe { ::bridgerton::native::borrow_object::<#ty>(handle) }; });
                    swift_values.push("__bridgertonHandle".to_string());
                    lifetimes.push("self".to_string());
                }
                FnArg::Typed(arg) => {
                    let Pat::Ident(pattern) = &*arg.pat else {
                        return Err(syn::Error::new(arg.span(), "expected named argument"));
                    };
                    let name = &pattern.ident;
                    let label = name.to_string().trim_start_matches("r#").to_owned();
                    let name_text = format!("argument_{label}");
                    let kind = input_kind(&arg.ty)?;
                    match kind {
                        Input::Data(ty) | Input::Borrowed(ty) => {
                            let borrowed = matches!(&*arg.ty, Type::Reference(_));
                            let rust_type: Type = if borrowed {
                                syn::parse_quote! { <#ty as ::bridgerton::native::NativeBorrowed>::Owned }
                            } else {
                                (*ty).clone()
                            };
                            let index = owned_inputs.len();
                            native_args.push(quote! { #name: <#rust_type as ::bridgerton::native::NativeArgument>::Abi });
                            native_prepare.push(quote! { let #name = unsafe { <#rust_type as ::bridgerton::native::NativeArgument>::prepare_native(#name) }; });
                            native_setup.push(quote! { let #name: #rust_type = unsafe { <#rust_type as ::bridgerton::native::NativeArgument>::from_prepared(#name)? }; });
                            native_values.push(if borrowed {
                                quote! { <#ty as ::bridgerton::native::NativeBorrowed>::borrow(&#name) }
                            } else { quote! { #name } });
                            c_args.push(format!("@abi_{index}@ {name_text}"));
                            swift_args
                                .push(format!("`{label}` {name_text}: @argument_type_{index}@"));
                            swift_values.push(format!("@argument_value_{index}@"));
                            owned_inputs.push((name_text, Box::new(rust_type)));
                        }
                        Input::Number => {
                            native_args.push(quote! { #name: u32 });
                            native_values.push(quote! { #name });
                            c_args.push(format!("uint32_t {name_text}"));
                            swift_args.push(format!("`{label}` {name_text}: UInt32"));
                            swift_values.push(name_text);
                        }
                    }
                }
            }
        }
        let await_tokens = is_async.then(|| quote! { .await });
        let native_invoke = if let Some(function) = &free_function {
            quote! { #function(#(#native_values),*) #await_tokens }
        } else if constructor || is_static {
            quote! { #ty::#method_name(#(#native_values),*) #await_tokens }
        } else {
            quote! { object.#method_name(#(#native_values),*) #await_tokens }
        };
        let native_body =
            quote! { ::bridgerton::native::NativeReturn::into_native_return(#native_invoke) };
        let native_body = if is_async {
            quote! { Ok(::bridgerton::native::task(async move { #native_body })) }
        } else {
            native_body
        };
        native_functions.push(quote! {
            #[cfg(not(target_arch = "wasm32"))]
            #[doc = "Generated FFI: caller must supply live, correctly typed handles and owned callback contexts."]
            #[doc = "# Safety\nOnly generated bindings may call this function."]
            #[unsafe(export_name = concat!(module_path!(), "::", #class_name, "::", #symbol_name))]
            pub unsafe extern "C" fn #symbol(#(#native_args),*) -> ::bridgerton::native::BridgeResult {
                ::bridgerton::native::call(move || { #(#native_prepare)* #(#native_setup)* #native_body })
            }
        });
        let method_header = format!(
            "BridgeResult {symbol_name}({});\n",
            if c_args.is_empty() {
                "void".into()
            } else {
                c_args.join(", ")
            }
        );
        let invocation = format!("{symbol_name}({})", swift_values.join(", "));
        let invocation = if lifetimes.is_empty() {
            invocation
        } else {
            format!(
                "withExtendedLifetime(({})) {{ {invocation} }}",
                lifetimes.join(", ")
            )
        };
        let swift_args = swift_args.join(", ");
        let arguments = owned_inputs.iter().enumerate().map(|(i, (name, ty))| {
            let abi = format!("@abi_{i}@");
            let type_marker = format!("@argument_type_{i}@");
            let value_marker = format!("@argument_value_{i}@");
            quote! {
                method_header = method_header.replace(#abi, <#ty as ::bridgerton::native::NativeArgument>::C_TYPE);
                args = args.replace(#type_marker, &<#ty as ::bridgerton::native::NativeArgument>::swift_type(&mut types));
                invocation = invocation.replace(#value_marker, &<#ty as ::bridgerton::native::NativeArgument>::argument(#name));
                if let Some(setup) = <#ty as ::bridgerton::native::NativeArgument>::task_signal(#name) { task_signals.push((#name, setup)); }
            }
        });
        let wraps = owned_inputs.iter().rev().map(|(name, ty)| {
            quote! {
                invocation = <#ty as ::bridgerton::native::NativeArgument>::wrap(#name, invocation);
            }
        });
        let is_static = is_static && free_function.is_none();
        let placeholder = format!("@method_{}@", method_definitions.len());
        swift.push_str(&placeholder);
        let mutable = (!owned_inputs.is_empty()).then(|| quote! {mut});
        method_definitions.push(quote! {
            {
                let #mutable method_header = String::from(#method_header);
                let #mutable args = String::from(#swift_args);
                let #mutable invocation = String::from(#invocation);
                let #mutable task_signals = Vec::new();
                #(#arguments)*
                #(#wraps)*
                let (method_header, invocation) = ::bridgerton::native::qualify_export(&method_header, &invocation, concat!(module_path!(), "::", #class_name), #symbol_name);
                header.push_str(&method_header);
                let method = ::bridgerton::native::return_method::<#output>(&mut types, #method_label, &args, &invocation, #is_async, #is_static, #getter, #constructor, #class_name, &task_signals);
                swift = swift.replace(#placeholder, &method);
            }
        });
    }
    if free_function.is_none() {
        swift.push_str("}\n");
    }
    let declaration = free_function.is_none().then(|| quote! { #item });
    let register = if let Some(name) = free_function {
        quote! { ::bridgerton::exports::Export::function::<#ty>(stringify!(#name), definition) }
    } else {
        quote! { ::bridgerton::exports::Export::methods::<#ty>(#class_name, &[#(#members),*], definition) }
    };
    Ok(quote! {
        #declaration
        #(#native_functions)*
        #[cfg(not(target_arch = "wasm32"))]
        const _: () = {
            fn definition() -> ::bridgerton::native::Definition {
                let mut types=::bridgerton::schema::Registry::default();
                let mut swift=String::from(#swift);
                let mut header=String::new();
                #(#method_definitions)*
                ::bridgerton::native::Definition { header, swift, types }
            }
            ::bridgerton::__inventory::submit! {
                #register
            }
        };
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_signature_is_a_diagnostic_not_silent_wrong_codegen() {
        let item = syn::parse_quote! {
            impl Thing {
                #[bridge(constructor)] pub fn new() -> Self { todo!() }
                pub fn broken(&mut self, value: u64) {}
            }
        };
        assert!(
            expand_impl(item, Options::default())
                .unwrap_err()
                .to_string()
                .contains("&self")
        );
    }
}
