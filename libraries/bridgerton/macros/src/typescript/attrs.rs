use serde_derive_internals::ast::Field;

/// Options given on `#[bridge(transparent, ...)]` that shape the TypeScript declaration.
#[derive(Debug, Default)]
pub struct ContainerAttrs {
    /// Whether an enum's variants are also declared inside a TypeScript namespace.
    pub namespace: bool,
    /// Information about how the type should be serialized.
    pub ty_config: TypeGenerationConfig,
}

/// Configuration affecting how Typescript types are generated.
#[derive(Debug, Default)]
pub struct TypeGenerationConfig {
    /// Universal prefix for generated types
    pub type_prefix: Option<String>,
    /// Universal suffix for generated types
    pub type_suffix: Option<String>,
    /// Whether missing fields should be represented as null in Typescript
    pub missing_as_null: bool,
    /// Whether a hashmap should be represented as an object in Typescript
    pub hashmap_as_object: bool,
    /// Whether large number types should be represented as BigInts in Typescript
    pub large_number_types_as_bigints: bool,
}

impl TypeGenerationConfig {
    /// Format a type `name` adding a prefix and suffix if they are set.
    pub fn format_name(&self, name: String) -> String {
        let prefix = self.type_prefix.as_ref().map_or("", String::as_str);
        let suffix = self.type_suffix.as_ref().map_or("", String::as_str);
        format!("{}{}{}", prefix, name, suffix)
    }
}

/// Field options: `#[bridge(type = "...")]` overrides the TypeScript type and
/// `#[bridge(optional)]` declares the property optional.
#[derive(Debug, Default)]
pub struct FieldAttrs {
    pub type_override: Option<String>,
    pub optional: bool,
}

impl FieldAttrs {
    pub fn from_serde_field(field: &Field) -> syn::Result<Self> {
        let mut attrs = Self {
            type_override: None,
            optional: false,
        };

        for attr in &field.original.attrs {
            if !attr.path().is_ident("bridge") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("type") {
                    if attrs.type_override.is_some() {
                        return Err(meta.error("duplicate attribute"));
                    }
                    let lit = meta.value()?.parse::<syn::LitStr>()?;
                    attrs.type_override = Some(lit.value());
                    return Ok(());
                }

                if meta.path.is_ident("optional") {
                    if attrs.optional {
                        return Err(meta.error("duplicate attribute"));
                    }
                    attrs.optional = true;
                    return Ok(());
                }

                Err(meta
                    .error("unsupported bridge field option, expected one of `type` or `optional`"))
            })?;
        }

        if let Some(expr) = field.attrs.skip_serializing_if() {
            let path = expr
                .path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");

            attrs.optional |= &path == "Option::is_none";
        }

        Ok(attrs)
    }
}
