use std::fs;

use crate::generator_functions::{generate_identifier, generate_option, generate_sound_event_ref};
use foton_utils::Identifier;
use foton_utils::value_providers::FloatProvider;
use heck::ToShoutySnakeCase;
use proc_macro2::{Ident, Literal, Span, TokenStream};
use quote::quote;
use serde::Deserialize;

/// Vanilla parity: `SulfurCubeArchetype.AttributeEntry.CODEC`.
#[derive(Deserialize, Debug)]
struct AttributeEntryJson {
    attribute: Identifier,
    id: Identifier,
    amount: f64,
    operation: String,
}

/// Vanilla parity: `SulfurCubeArchetype.ExplosionData.CODEC`.
#[derive(Deserialize, Debug)]
struct ExplosionJson {
    power: i32,
    causes_fire: bool,
    fuse: i32,
}

/// Vanilla parity: `SulfurCubeArchetype.ContactDamage.CODEC`.
#[derive(Deserialize, Debug)]
struct ContactDamageJson {
    damage_type: Identifier,
    amount: FloatProvider,
    attribute_to_source: bool,
}

/// Vanilla parity: `SulfurCubeArchetype.KnockbackModifiers.CODEC`.
#[derive(Deserialize, Debug)]
struct KnockbackModifiersJson {
    horizontal_power: f32,
    vertical_power: f32,
}

/// Vanilla parity: `SulfurCubeArchetype.SoundSettings.CODEC`.
#[derive(Deserialize, Debug)]
struct SoundSettingsJson {
    hit_sound: Identifier,
    push_sound: Identifier,
    push_sound_impulse_threshold: f32,
    push_sound_cooldown: f32,
}

/// Vanilla parity: `SulfurCubeArchetype.DIRECT_CODEC`.
#[derive(Deserialize, Debug)]
struct SulfurCubeArchetypeJson {
    items: String,
    attribute_modifiers: Vec<AttributeEntryJson>,
    #[serde(default)]
    buoyant: bool,
    #[serde(default)]
    explosion: Option<ExplosionJson>,
    #[serde(default)]
    contact_damage: Option<ContactDamageJson>,
    knockback_modifiers: KnockbackModifiersJson,
    sound_settings: SoundSettingsJson,
}

/// Reads the built-in archetype data pack.
///
/// Sorted by file stem, which is the order `RegistryDataLoader` walks a data
/// pack directory in, so the registry ids match vanilla's.
fn read_archetypes() -> Vec<(String, SulfurCubeArchetypeJson)> {
    let dir = "../foton-utils/build_assets/builtin_datapacks/minecraft/sulfur_cube_archetype";
    println!("cargo:rerun-if-changed={dir}/");

    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("Failed to read {dir}: {e}")) {
        let path = entry
            .unwrap_or_else(|e| panic!("Failed to read entry in {dir}: {e}"))
            .path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_else(|| panic!("Invalid archetype file name: {}", path.display()))
            .to_string();
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()));
        let value: SulfurCubeArchetypeJson = serde_json::from_str(&content)
            .unwrap_or_else(|e| panic!("Failed to parse sulfur cube archetype {name}: {e}"));
        out.push((name, value));
    }
    out.sort_by(|(a, _), (b, _)| a.cmp(b));
    out
}

fn vanilla_shouty(resource: &Identifier, kind: &str) -> Ident {
    assert_eq!(
        resource.namespace.as_ref(),
        "minecraft",
        "vanilla {kind} references must use the minecraft namespace: {resource}"
    );
    Ident::new(&resource.path.to_shouty_snake_case(), Span::call_site())
}

fn generate_items(items: &str) -> TokenStream {
    let tag = items.strip_prefix('#').unwrap_or_else(|| {
        panic!("sulfur cube archetypes are only defined by item tags, got: {items}")
    });
    let tag: Identifier = tag
        .parse()
        .unwrap_or_else(|e| panic!("Invalid sulfur cube archetype item tag {tag}: {e:?}"));
    let tag = generate_identifier(&tag);
    quote! { RegistryHolderSet::Tag(#tag) }
}

fn generate_operation(operation: &str) -> TokenStream {
    match operation {
        "add_value" => quote! { AttributeModifierOperation::AddValue },
        "add_multiplied_base" => quote! { AttributeModifierOperation::AddMultipliedBase },
        "add_multiplied_total" => quote! { AttributeModifierOperation::AddMultipliedTotal },
        other => panic!("unknown sulfur cube archetype attribute modifier operation: {other}"),
    }
}

fn generate_attribute_entry(entry: &AttributeEntryJson) -> TokenStream {
    let attribute = vanilla_shouty(&entry.attribute, "attribute");
    let id = generate_identifier(&entry.id);
    let amount = entry.amount;
    let operation = generate_operation(&entry.operation);
    quote! {
        SulfurCubeAttributeEntry {
            attribute: vanilla_attributes::#attribute,
            id: #id,
            amount: #amount,
            operation: #operation,
        }
    }
}

fn generate_float_provider(provider: &FloatProvider) -> TokenStream {
    match *provider {
        FloatProvider::Constant(value) => {
            let value = Literal::f32_suffixed(value);
            quote! { FloatProvider::Constant(#value) }
        }
        FloatProvider::Uniform {
            min_inclusive,
            max_exclusive,
        } => {
            let min_inclusive = Literal::f32_suffixed(min_inclusive);
            let max_exclusive = Literal::f32_suffixed(max_exclusive);
            quote! {
                FloatProvider::Uniform {
                    min_inclusive: #min_inclusive,
                    max_exclusive: #max_exclusive,
                }
            }
        }
        FloatProvider::Trapezoid { min, max, plateau } => {
            let min = Literal::f32_suffixed(min);
            let max = Literal::f32_suffixed(max);
            let plateau = Literal::f32_suffixed(plateau);
            quote! { FloatProvider::Trapezoid { min: #min, max: #max, plateau: #plateau } }
        }
        FloatProvider::ClampedNormal {
            mean,
            deviation,
            min,
            max,
        } => {
            let mean = Literal::f32_suffixed(mean);
            let deviation = Literal::f32_suffixed(deviation);
            let min = Literal::f32_suffixed(min);
            let max = Literal::f32_suffixed(max);
            quote! {
                FloatProvider::ClampedNormal {
                    mean: #mean,
                    deviation: #deviation,
                    min: #min,
                    max: #max,
                }
            }
        }
    }
}

pub(crate) fn build() -> TokenStream {
    let archetypes = read_archetypes();

    let mut stream = TokenStream::new();
    stream.extend(quote! {
        use crate::attribute::AttributeModifierOperation;
        use crate::registry::RegistryHolderSet;
        use crate::sulfur_cube_archetype::{
            SulfurCubeArchetype, SulfurCubeArchetypeRegistry, SulfurCubeAttributeEntry,
            SulfurCubeContactDamage, SulfurCubeExplosion, SulfurCubeKnockbackModifiers,
            SulfurCubeSoundSettings,
        };
        use crate::{vanilla_attributes, vanilla_damage_types};
        use std::borrow::Cow;
        use foton_utils::Identifier;
        use foton_utils::value_providers::FloatProvider;
    });

    let mut register_stream = TokenStream::new();
    for (name, archetype) in &archetypes {
        let ident = Ident::new(&name.to_shouty_snake_case(), Span::call_site());
        let name_str = name.as_str();
        let items = generate_items(&archetype.items);
        let attribute_modifiers = archetype
            .attribute_modifiers
            .iter()
            .map(generate_attribute_entry)
            .collect::<Vec<_>>();
        let buoyant = archetype.buoyant;
        let explosion = generate_option(&archetype.explosion, |explosion| {
            let power = explosion.power;
            let causes_fire = explosion.causes_fire;
            let fuse = explosion.fuse;
            quote! {
                SulfurCubeExplosion { power: #power, causes_fire: #causes_fire, fuse: #fuse }
            }
        });
        let contact_damage = generate_option(&archetype.contact_damage, |damage| {
            let damage_type = vanilla_shouty(&damage.damage_type, "damage type");
            let amount = generate_float_provider(&damage.amount);
            let attribute_to_source = damage.attribute_to_source;
            quote! {
                SulfurCubeContactDamage {
                    damage_type: &vanilla_damage_types::#damage_type,
                    amount: #amount,
                    attribute_to_source: #attribute_to_source,
                }
            }
        });
        let horizontal_power =
            Literal::f32_suffixed(archetype.knockback_modifiers.horizontal_power);
        let vertical_power = Literal::f32_suffixed(archetype.knockback_modifiers.vertical_power);
        let hit_sound = generate_sound_event_ref(&archetype.sound_settings.hit_sound);
        let push_sound = generate_sound_event_ref(&archetype.sound_settings.push_sound);
        let threshold =
            Literal::f32_suffixed(archetype.sound_settings.push_sound_impulse_threshold);
        let cooldown = Literal::f32_suffixed(archetype.sound_settings.push_sound_cooldown);

        stream.extend(quote! {
            pub static #ident: SulfurCubeArchetype = SulfurCubeArchetype {
                key: Identifier::vanilla_static(#name_str),
                items: #items,
                attribute_modifiers: &[#(#attribute_modifiers),*],
                buoyant: #buoyant,
                explosion: #explosion,
                contact_damage: #contact_damage,
                knockback_modifiers: SulfurCubeKnockbackModifiers {
                    horizontal_power: #horizontal_power,
                    vertical_power: #vertical_power,
                },
                sound_settings: SulfurCubeSoundSettings {
                    hit_sound: #hit_sound,
                    push_sound: #push_sound,
                    push_sound_impulse_threshold: #threshold,
                    push_sound_cooldown: #cooldown,
                },
            };
        });

        register_stream.extend(quote! {
            registry.register(&#ident);
        });
    }

    stream.extend(quote! {
        pub fn register_sulfur_cube_archetypes(registry: &mut SulfurCubeArchetypeRegistry) {
            #register_stream
        }
    });

    stream
}
