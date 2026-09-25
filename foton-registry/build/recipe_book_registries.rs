//! Network ids of the three built-in registries the recipe book is written in.
//!
//! `recipe_book_category`, `recipe_display` and `slot_display` are static
//! registries: the client knows them by heart and the server only ever sends
//! an entry's position. Those positions are protocol, so they come from the
//! extracted registries rather than from the order of a Rust enum.

use crate::generator_functions::{read_json_asset, sort_contiguous_registry_entries};
use foton_utils::Identifier;
use heck::ToShoutySnakeCase;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use serde::Deserialize;

#[derive(Deserialize)]
struct RegistryEntry {
    id: usize,
    key: Identifier,
}

/// One `pub const` per entry, named after its key, in a module of `module`.
fn id_module(asset: &str, module: &str) -> TokenStream {
    let mut entries: Vec<RegistryEntry> = read_json_asset(asset);
    sort_contiguous_registry_entries(&mut entries, asset, |entry| entry.id);

    let constants = entries.iter().map(|entry| {
        let ident = Ident::new(&entry.key.path.to_shouty_snake_case(), Span::call_site());
        let id = i32::try_from(entry.id)
            .unwrap_or_else(|_| panic!("{asset}: id {} does not fit in a VarInt", entry.id));
        quote! { pub const #ident: i32 = #id; }
    });
    let module = Ident::new(module, Span::call_site());
    quote! {
        pub mod #module {
            #(#constants)*
        }
    }
}

pub(crate) fn build() -> TokenStream {
    let categories = id_module(
        "build_assets/recipe_book_categories.json",
        "recipe_book_category",
    );
    let recipe_displays = id_module(
        "build_assets/recipe_display_types.json",
        "recipe_display_type",
    );
    let slot_displays = id_module("build_assets/slot_display_types.json", "slot_display_type");
    quote! {
        #categories
        #recipe_displays
        #slot_displays
    }
}
