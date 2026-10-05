//! Paper's ordered meta-family dispatch, using actual native class capabilities.

#[cfg(test)]
use foton_core::behavior as bridge_behavior;
use std::ptr as bridge_ptr;
use std::sync::OnceLock;

use foton_core::behavior::ITEM_BEHAVIORS;
use foton_registry::{REGISTRY, RegistryExt, items::ItemRef};
use foton_utils::Identifier;
use jni::{
    JNIEnv,
    objects::{JClass, JString},
    sys::jstring,
};
use rustc_hash::{FxHashMap, FxHashSet};

use super::ItemBridgeError;

static KINDS: OnceLock<FxHashMap<Identifier, &'static str>> = OnceLock::new();

pub(crate) fn kind(item: ItemRef) -> Result<&'static str, ItemBridgeError> {
    let registry = REGISTRY.get().ok_or(ItemBridgeError::RegistryNotReady)?;
    let behaviors = ITEM_BEHAVIORS.get().ok_or(ItemBridgeError::NativeState(
        "item behaviors are not initialized".to_owned(),
    ))?;
    let kinds = KINDS.get_or_init(|| {
        // The extracted block→item association includes BlockItem subclasses
        // whose placement behavior is composed rather than directly implemented.
        let entity_items: FxHashSet<_> = registry
            .blocks
            .iter()
            .filter(|(_, block)| registry.block_entity_types.has_block_entity(block))
            .map(|(_, block)| registry.items.by_block(block).key.clone())
            .collect();
        registry
            .items
            .iter()
            .map(|(_, item)| {
                let behavior = behaviors.get_behavior(item);
                let path = if item.key.namespace == "minecraft" {
                    item.key.path.as_ref()
                } else {
                    ""
                };
                let exact = early_kind(path);
                let kind = exact.unwrap_or_else(|| {
                    if behavior.is_banner() {
                        "BANNER"
                    } else if behavior.is_spawn_egg() {
                        "SPAWN_EGG"
                    } else if path == "armor_stand" {
                        "ARMOR_STAND"
                    } else if path == "knowledge_book" {
                        "KNOWLEDGE_BOOK"
                    } else if entity_items.contains(&item.key) {
                        "BLOCK_STATE"
                    } else if let Some(kind) = middle_kind(path) {
                        kind
                    } else if behavior.is_bundle() {
                        "BUNDLE"
                    } else if path == "goat_horn" {
                        "MUSIC_INSTRUMENT"
                    } else if path == "ominous_bottle" {
                        "OMINOUS_BOTTLE"
                    } else {
                        "BASE"
                    }
                });
                (item.key.clone(), kind)
            })
            .collect()
    });
    kinds
        .get(&item.key)
        .copied()
        .ok_or(ItemBridgeError::InvalidEdit(
            "unregistered item meta family",
        ))
}

/// Exact material branches in pinned Paper `CraftItemMetas`. Class predicates
/// (banner, spawn egg, `EntityBlock`, bundle) are deliberately not suffix tests.
fn early_kind(path: &str) -> Option<&'static str> {
    Some(match path {
        "air" => "EMPTY",
        "written_book" => "BOOK_WRITTEN",
        "writable_book" => "BOOK_WRITABLE",
        "creeper_head"
        | "dragon_head"
        | "piglin_head"
        | "player_head"
        | "skeleton_skull"
        | "wither_skeleton_skull"
        | "zombie_head" => "SKULL",
        "chainmail_helmet"
        | "chainmail_chestplate"
        | "chainmail_leggings"
        | "chainmail_boots"
        | "diamond_helmet"
        | "diamond_chestplate"
        | "diamond_leggings"
        | "diamond_boots"
        | "golden_helmet"
        | "golden_chestplate"
        | "golden_leggings"
        | "golden_boots"
        | "iron_helmet"
        | "iron_chestplate"
        | "iron_leggings"
        | "iron_boots"
        | "netherite_helmet"
        | "netherite_chestplate"
        | "netherite_leggings"
        | "netherite_boots"
        | "copper_helmet"
        | "copper_chestplate"
        | "copper_leggings"
        | "copper_boots"
        | "turtle_helmet" => "ARMOR",
        "leather_helmet" | "leather_chestplate" | "leather_leggings" | "leather_boots"
        | "wolf_armor" => "COLORABLE_ARMOR",
        "leather_horse_armor" => "LEATHER_ARMOR",
        "potion" | "splash_potion" | "lingering_potion" | "tipped_arrow" => "POTION",
        "filled_map" => "MAP",
        "firework_rocket" => "FIREWORK",
        "firework_star" => "FIREWORK_EFFECT",
        "enchanted_book" => "ENCHANTMENT_STORAGE",
        _ => return None,
    })
}

fn middle_kind(path: &str) -> Option<&'static str> {
    Some(match path {
        "shield" => "SHIELD",
        "tropical_fish_bucket" => "TROPICAL_FISH_BUCKET",
        "axolotl_bucket" => "AXOLOTL_BUCKET",
        "crossbow" => "CROSSBOW",
        "suspicious_stew" => "SUSPICIOUS_STEW",
        "cod_bucket" | "pufferfish_bucket" | "tadpole_bucket" | "salmon_bucket" | "item_frame"
        | "glow_item_frame" | "painting" => "BASE",
        "compass" => "COMPASS",
        _ => return None,
    })
}

pub(crate) extern "system" fn query(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    material: JString<'_>,
) -> jstring {
    let result = (|| -> Result<jstring, ItemBridgeError> {
        let registry = REGISTRY.get().ok_or(ItemBridgeError::RegistryNotReady)?;
        let material = env.get_string(&material)?;
        let key: Identifier = material
            .to_str()
            .map_err(|_| ItemBridgeError::InvalidEdit("invalid material"))?
            .parse()
            .map_err(|_| ItemBridgeError::InvalidEdit("invalid material"))?;
        let item = registry
            .items
            .by_key(&key)
            .ok_or(ItemBridgeError::InvalidEdit("unknown material"))?;
        Ok(env.new_string(kind(item)?)?.into_raw())
    })();
    match result {
        Ok(value) => value,
        Err(error) => {
            error.throw_java(&mut env);
            bridge_ptr::null_mut()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use foton_registry::{init_vanilla_registry, vanilla_items};

    #[test]
    fn extracted_class_capabilities_preserve_paper_dispatch_precedence() {
        init_vanilla_registry();
        bridge_behavior::init_behaviors();
        for (item, expected) in [
            (&*vanilla_items::RED_BUNDLE, "BUNDLE"),
            (&*vanilla_items::BLUE_BANNER, "BANNER"),
            (&*vanilla_items::CHEST, "BLOCK_STATE"),
            (&*vanilla_items::OAK_SIGN, "BLOCK_STATE"),
            (&*vanilla_items::STONE, "BASE"),
            (&*vanilla_items::COW_SPAWN_EGG, "SPAWN_EGG"),
            (&*vanilla_items::IRON_CHESTPLATE, "ARMOR"),
            (&*vanilla_items::LEATHER_CHESTPLATE, "COLORABLE_ARMOR"),
            (&*vanilla_items::LEATHER_HORSE_ARMOR, "LEATHER_ARMOR"),
            (&*vanilla_items::FIREWORK_STAR, "FIREWORK_EFFECT"),
            (&*vanilla_items::FIREWORK_ROCKET, "FIREWORK"),
        ] {
            assert_eq!(kind(item).expect("ready family"), expected, "{}", item.key);
        }
    }
}
