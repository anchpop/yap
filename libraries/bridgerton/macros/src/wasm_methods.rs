//! The conversion traits resolve aliases and select both the ABI and TypeScript descriptor.
use super::*;

pub fn adapt(method: &syn::ImplItemFn) -> syn::Result<Option<syn::ImplItemFn>> {
    if !matches!(method.vis, Visibility::Public(_)) {
        return Ok(None);
    }
    let mut wrapper = method.clone();
    let mut setup = Vec::new();
    let mut arguments = Vec::new();
    for argument in &mut wrapper.sig.inputs {
        let FnArg::Typed(argument) = argument else {
            continue;
        };
        let Pat::Ident(name) = &mut *argument.pat else {
            return Err(syn::Error::new_spanned(argument, "expected named argument"));
        };
        name.mutability = None;
        let name = &name.ident;
        arguments.push(quote! { #name });
        if !matches!(&*argument.ty, Type::Reference(_)) {
            let ty = &argument.ty;
            setup.push(quote! { let #name = <#ty as ::bridgerton::FromWasm>::from_wasm(#name)?; });
            argument.ty = syn::parse_quote! { <#ty as ::bridgerton::FromWasm>::Input };
        }
    }
    let output: Type = match &method.sig.output {
        ReturnType::Default => syn::parse_quote! { () },
        ReturnType::Type(_, ty) => (**ty).clone(),
    };
    let name = &method.sig.ident;
    let invoke = if method.sig.receiver().is_some() {
        quote! { self.#name }
    } else {
        quote! { Self::#name }
    };
    let await_ = method.sig.asyncness.is_some().then(|| quote! { .await });
    wrapper
        .attrs
        .push(syn::parse_quote! { #[wasm_bindgen(js_name = #name)] });
    wrapper.sig.ident = format_ident!("__bridgerton_{}", name.to_string().trim_start_matches("r#"));
    wrapper.sig.output = syn::parse_quote! { -> Result<<#output as ::bridgerton::IntoWasm>::Output, ::bridgerton::__wasm_bindgen::JsValue> };
    wrapper.block = syn::parse_quote! {{ #(#setup)* <#output as ::bridgerton::IntoWasm>::into_wasm(#invoke(#(#arguments),*) #await_) }};
    Ok(Some(wrapper))
}

// Free functions share the same type-directed adapters as methods on both targets.
pub fn free_function(item: syn::ItemFn) -> syn::Result<Tokens> {
    let method = syn::ImplItemFn {
        attrs: item.attrs.clone(),
        vis: item.vis.clone(),
        defaultness: None,
        sig: item.sig.clone(),
        block: *item.block.clone(),
    };
    let Some(mut wrapper) = adapt(&method)? else {
        return Err(syn::Error::new_spanned(
            item,
            "exported functions must be public",
        ));
    };
    let name = &item.sig.ident;
    // Replace only the invocation's Self path using the AST, not application text.
    struct FreeCall<'a>(&'a syn::Ident);
    impl syn::visit_mut::VisitMut for FreeCall<'_> {
        fn visit_expr_path_mut(&mut self, path: &mut syn::ExprPath) {
            if path.path.segments.len() == 2
                && path.path.segments[0].ident == "Self"
                && path.path.segments[1].ident == *self.0
            {
                let name = self.0;
                path.path = syn::parse_quote! { #name };
            }
        }
    }
    let mut visit = FreeCall(name);
    syn::visit_mut::VisitMut::visit_block_mut(&mut visit, &mut wrapper.block);
    let mut options = Vec::<Tokens>::new();
    wrapper.attrs.retain(|attr| {
        if attr.path().is_ident("wasm_bindgen") {
            if let syn::Meta::List(meta) = &attr.meta {
                options.push(meta.tokens.clone());
            }
            false
        } else {
            true
        }
    });
    Ok(quote! {
        #[cfg(target_arch = "wasm32")] #item
        #[cfg(target_arch = "wasm32")]
        #[cfg(target_arch = "wasm32")] #[allow(unused_imports)] use ::bridgerton::__wasm::*;
        #[cfg(target_arch = "wasm32")]
        #[::bridgerton::__wasm_bindgen::prelude::wasm_bindgen(#(#options),*)] #wrapper
        #[cfg(not(target_arch = "wasm32"))]
        #[::bridgerton::__native_function] #item
    })
}
