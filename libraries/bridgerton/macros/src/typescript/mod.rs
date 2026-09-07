//! TypeScript declarations for transparent values, generated from the same
//! Rust fields and Serde attributes as the native codec. Adapted from Tsify;
//! see THIRD_PARTY.md. The bridge emits the declaration into wasm-bindgen's
//! custom section itself, so consumers need no Tsify or wasm-bindgen dependency.
mod attrs;
mod basic;
mod comments;
mod container;
mod decl;
mod error_tracker;
mod parser;
mod ts_type;
mod ts_type_display;
mod ts_type_from_name;

pub use attrs::{ContainerAttrs, TypeGenerationConfig};
pub use basic::*;
pub use ts_type::*;

/// Render the `export interface` / `export type` declaration for one struct or enum.
pub fn declaration(input: &syn::DeriveInput, attrs: ContainerAttrs) -> syn::Result<String> {
    let container = container::Container::from_derive_input(input, attrs)?;
    let text = parser::Parser::new(&container).parse().to_string();
    container.check()?;
    Ok(text)
}
