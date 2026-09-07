//! Derive native transport for existing Rust data without altering its Serde representation.
use super::*;
use syn::visit::Visit;
use syn::{Fields, Generics, Member, Meta, Token, punctuated::Punctuated};

fn mentions_type(ty: &Type, name: &syn::Ident) -> bool {
    struct Finder<'a> {
        name: &'a syn::Ident,
        found: bool,
    }
    impl<'ast> Visit<'ast> for Finder<'_> {
        fn visit_type_path(&mut self, path: &'ast syn::TypePath) {
            self.found |= path
                .path
                .segments
                .iter()
                .any(|s| s.ident == *self.name || s.ident == "Self");
            syn::visit::visit_type_path(self, path);
        }
    }
    let mut finder = Finder { name, found: false };
    finder.visit_type(ty);
    finder.found
}

fn skipped(field: &syn::Field) -> bool {
    field
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("serde"))
        .any(|a| {
            a.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                .is_ok_and(|items| {
                    items
                        .iter()
                        .any(|m| matches!(m,Meta::Path(p) if p.is_ident("skip")))
                })
        })
}
fn fields(fields: &Fields) -> (Vec<Tokens>, Vec<Tokens>, Vec<Tokens>, Vec<Tokens>) {
    let mut bindings = vec![];
    let mut reads = vec![];
    let mut writes = vec![];
    let mut metadata = vec![];
    for (index, f) in fields.iter().enumerate() {
        let binding = format_ident!("field_{index}");
        let ty = &f.ty;
        let member = if let Some(id) = &f.ident {
            Member::Named(id.clone())
        } else {
            Member::Unnamed(syn::Index::from(index))
        };
        let label = f
            .ident
            .as_ref()
            .map(|id| id.to_string().trim_start_matches("r#").to_owned())
            .unwrap_or_else(|| {
                if fields.len() == 1 {
                    "value".into()
                } else {
                    format!("field_{index}")
                }
            });
        if skipped(f) {
            bindings.push(quote! {#member: _});
            reads.push(quote! {#member: Default::default()});
        } else {
            bindings.push(quote! {#member: #binding});
            reads.push(quote! {#member: <#ty as ::bridgerton::value::Value>::read(reader)?});
            writes.push(quote! {::bridgerton::value::Value::write(#binding,writer)?;});
            metadata.push(quote!{::bridgerton::schema::Field{name:#label.into(),ty:<#ty as ::bridgerton::schema::NativeType>::native_type(registry)}});
        }
    }
    (bindings, reads, writes, metadata)
}
pub fn expand(item: Item) -> syn::Result<Tokens> {
    let (name, generics, shape, read, write) = match &item {
        Item::Struct(s) => {
            let (bindings, reads, writes, fields) = fields(&s.fields);
            (
                s.ident.clone(),
                s.generics.clone(),
                quote! {::bridgerton::schema::Shape::Record(vec![#(#fields),*])},
                quote! {Ok(Self{#(#reads),*})},
                quote! {let Self{#(#bindings),*}=self;#(#writes)* Ok(())},
            )
        }
        Item::Enum(e) => {
            let mut shapes = vec![];
            let mut reads = vec![];
            let mut writes = vec![];
            for (index, v) in e.variants.iter().enumerate() {
                let name = &v.ident;
                let label = name.to_string().trim_start_matches("r#").to_owned();
                let tag = (index + 1) as u32;
                let (bindings, fields, write, metadata) = fields(&v.fields);
                let named = matches!(v.fields, Fields::Named(_));
                reads.push(quote! {#tag=>Ok(Self::#name{#(#fields),*})});
                writes.push(quote!{Self::#name{#(#bindings),*}=>{::bridgerton::value::Value::write(&#tag,writer)?;#(#write)*}});
                shapes.push(quote!{::bridgerton::schema::Variant{name:#label.into(),fields:vec![#(#metadata),*],named:#named}});
            }
            (
                e.ident.clone(),
                e.generics.clone(),
                quote! {::bridgerton::schema::Shape::Enum(vec![#(#shapes),*])},
                quote! {match <u32 as ::bridgerton::value::Value>::read(reader)?{#(#reads,)* _=>Err(::bridgerton::Error::new("invalid enum tag"))}},
                quote! {match self{#(#writes),*} Ok(())},
            )
        }
        other => {
            return Err(syn::Error::new(
                other.span(),
                "expected a data struct or enum",
            ));
        }
    };
    let mut bounded: Generics = generics.clone();
    for p in generics.type_params() {
        let id = &p.ident;
        bounded.make_where_clause().predicates.push(
            syn::parse_quote! {#id: ::bridgerton::value::Value + ::bridgerton::schema::NativeType},
        );
    }
    let data_fields: Vec<&syn::Field> = match &item {
        Item::Struct(s) => s.fields.iter().collect(),
        Item::Enum(e) => e.variants.iter().flat_map(|v| v.fields.iter()).collect(),
        _ => unreachable!(),
    };
    for field in data_fields {
        let ty = &field.ty;
        // Concrete field support is checked by the generated method bodies.
        // Generic fields also need conditional bounds (e.g. a map alias needs
        // Ord keys), but Self bounds would create circular trait obligations.
        if skipped(field) {
            bounded
                .make_where_clause()
                .predicates
                .push(syn::parse_quote! { #ty: Default });
        } else if !generics.params.is_empty() && !mentions_type(ty, &name) {
            bounded
                .make_where_clause()
                .predicates
                .push(syn::parse_quote! {
                    #ty: ::bridgerton::value::Value + ::bridgerton::schema::NativeType
                });
        }
    }
    if generics
        .params
        .iter()
        .any(|p| !matches!(p, syn::GenericParam::Type(_)))
    {
        return Err(syn::Error::new(
            generics.span(),
            "only type parameters are supported",
        ));
    }
    let params = generics.type_params().map(|p| &p.ident).collect::<Vec<_>>();
    let (impl_generics, ty_generics, where_clause) = bounded.split_for_impl();
    let label = name.to_string().trim_start_matches("r#").to_owned();
    Ok(quote! {
        impl #impl_generics ::bridgerton::value::Value for #name #ty_generics #where_clause {
            fn read(reader:&mut ::bridgerton::value::Reader<'_>)->Result<Self,::bridgerton::Error>{reader.nested(|reader|{#read})}
            fn write(&self,writer:&mut ::bridgerton::value::Writer)->Result<(),::bridgerton::Error>{writer.nested(|writer|{#write})}
        }
        impl #impl_generics ::bridgerton::native::CallbackArguments for #name #ty_generics #where_clause { ::bridgerton::__native_callback_value!(); }
        impl #impl_generics ::bridgerton::native::NativeArgument for #name #ty_generics #where_clause { ::bridgerton::__native_value_argument!(); }
        impl #impl_generics ::bridgerton::native::NativeOptionalArgument for #name #ty_generics #where_clause { ::bridgerton::__native_optional_value_argument!(); }
        impl #impl_generics ::bridgerton::native::NativeReturn for #name #ty_generics #where_clause {
            ::bridgerton::__native_value_return!();
        }
        impl #impl_generics ::bridgerton::schema::NativeType for #name #ty_generics #where_clause {
            fn native_type(registry:&mut ::bridgerton::schema::Registry)->::bridgerton::schema::Type {
                let arguments=vec![#(<#params as ::bridgerton::schema::NativeType>::native_type(registry)),*];
                registry.named::<Self>(#label,arguments,|registry|{#shape})
            }
        }
    })
}
