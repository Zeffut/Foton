use std::fs;

use heck::ToShoutySnakeCase;
use proc_macro2::{Ident, Literal, Span, TokenStream};
use quote::quote;
use serde::Deserialize;

#[derive(Deserialize)]
struct StatTypeEntry {
    key: String,
    value_registry: String,
}

#[derive(Deserialize)]
struct CustomStatEntry {
    key: String,
}

fn bare(key: &str) -> &str {
    key.strip_prefix("minecraft:").unwrap_or(key)
}

fn constant(key: &str) -> Ident {
    Ident::new(&bare(key).to_shouty_snake_case(), Span::call_site())
}

pub fn stat_types() -> TokenStream {
    println!("cargo:rerun-if-changed=build_assets/stat_types.json");

    let json =
        fs::read_to_string("build_assets/stat_types.json").expect("Failed to read stat_types.json");
    let entries: Vec<StatTypeEntry> =
        serde_json::from_str(&json).expect("Failed to parse stat_types.json");

    let mut constants = TokenStream::new();
    let mut registrations = TokenStream::new();

    for StatTypeEntry {
        key,
        value_registry,
    } in entries
    {
        let ident = constant(&key);
        let key_literal = Literal::string(bare(&key));
        // The value registry decides how a stat value is looked up, so an
        // unknown one is a build failure rather than a silent fallback: it would
        // mean a new vanilla stat type Foton cannot address.
        let variant = match value_registry.as_str() {
            "minecraft:block" => quote! { StatValueRegistry::Block },
            "minecraft:item" => quote! { StatValueRegistry::Item },
            "minecraft:entity_type" => quote! { StatValueRegistry::EntityType },
            "minecraft:custom_stat" => quote! { StatValueRegistry::CustomStat },
            other => panic!("stat type {key} ranges over the unmodeled registry {other}"),
        };

        constants.extend(quote! {
            pub static #ident: StatType = StatType {
                key: Identifier::vanilla_static(#key_literal),
                value_registry: #variant,
            };
        });
        registrations.extend(quote! {
            registry.register(&#ident);
        });
    }

    quote! {
        use crate::stat::{StatType, StatTypeRegistry, StatValueRegistry};
        use foton_utils::Identifier;

        #constants

        pub fn register_stat_types(registry: &mut StatTypeRegistry) {
            #registrations
        }
    }
}

pub fn custom_stats() -> TokenStream {
    println!("cargo:rerun-if-changed=build_assets/custom_stats.json");

    let json = fs::read_to_string("build_assets/custom_stats.json")
        .expect("Failed to read custom_stats.json");
    let entries: Vec<CustomStatEntry> =
        serde_json::from_str(&json).expect("Failed to parse custom_stats.json");

    let mut constants = TokenStream::new();
    let mut registrations = TokenStream::new();

    for CustomStatEntry { key } in entries {
        let ident = constant(&key);
        let key_literal = Literal::string(bare(&key));

        constants.extend(quote! {
            pub static #ident: CustomStat = CustomStat {
                key: Identifier::vanilla_static(#key_literal),
            };
        });
        registrations.extend(quote! {
            registry.register(&#ident);
        });
    }

    quote! {
        use crate::stat::{CustomStat, CustomStatRegistry};
        use foton_utils::Identifier;

        #constants

        pub fn register_custom_stats(registry: &mut CustomStatRegistry) {
            #registrations
        }
    }
}
