//! Owned declarations collected by plugin bootstrap code.
use std::sync::OnceLock;

use crate::RegistryExt;
use crate::enchantment::effect::OwnedEnchantmentEffects;

use crate::enchantment::{
    Enchantment, EnchantmentCost, EnchantmentRegistrationError, EnchantmentRegistry,
};
use crate::equipment::EquipmentSlotGroup;
use foton_utils::Identifier;
use foton_utils::locks::SyncMutex;
use simdnbt::owned::NbtCompound;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginEnchantmentDefinition {
    pub key: String,
    pub weight: u32,
    pub max_level: u32,
    pub minimum_cost_base: i32,
    pub minimum_cost_per_level: i32,
    pub maximum_cost_base: i32,
    pub maximum_cost_per_level: i32,
    pub anvil_cost: i32,
    pub active_slots: Vec<String>,
    pub supported_items: String,
    pub primary_items: Option<String>,
    pub exclusive_set: Option<String>,
    pub effects: OwnedEnchantmentEffects,
}
static PENDING_ENCHANTMENTS: OnceLock<SyncMutex<Vec<PluginEnchantmentDefinition>>> =
    OnceLock::new();
fn pending() -> &'static SyncMutex<Vec<PluginEnchantmentDefinition>> {
    PENDING_ENCHANTMENTS.get_or_init(|| SyncMutex::new(Vec::new()))
}
pub fn queue_plugin_enchantment(definition: PluginEnchantmentDefinition) {
    pending().lock().push(definition);
}
#[must_use]
pub fn drain_plugin_enchantments() -> Vec<PluginEnchantmentDefinition> {
    let mut v = pending().lock();
    std::mem::take(&mut *v)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginEnchantmentRegistrationError {
    InvalidKey(String),
    InvalidSlot(String),
    InvalidTag(String),
    InvalidDefinition(String),
    DuplicateKey(Identifier),
    Frozen,
}

fn empty_effects_nbt() -> NbtCompound {
    NbtCompound::new()
}

const fn slot_group(name: &str) -> Option<EquipmentSlotGroup> {
    Some(match name {
        "any" => EquipmentSlotGroup::Any,
        "mainhand" | "main_hand" => EquipmentSlotGroup::MainHand,
        "offhand" => EquipmentSlotGroup::OffHand,
        "hand" => EquipmentSlotGroup::Hand,
        "feet" => EquipmentSlotGroup::Feet,
        "legs" => EquipmentSlotGroup::Legs,
        "chest" => EquipmentSlotGroup::Chest,
        "head" => EquipmentSlotGroup::Head,
        "armor" => EquipmentSlotGroup::Armor,
        "body" => EquipmentSlotGroup::Body,
        "saddle" => EquipmentSlotGroup::Saddle,
        _ => return None,
    })
}

/// Drains Paper metadata and registers fully representable enchantments.
///
/// This must be called before the global registry is frozen. The owned strings
/// are promoted only when the registry takes ownership of the entry; malformed
/// declarations are rejected before any entry is appended.
pub fn register_queued_plugin_enchantments(
    registry: &mut EnchantmentRegistry,
) -> Result<Vec<usize>, PluginEnchantmentRegistrationError> {
    let queued = drain_plugin_enchantments();
    let mut prepared = Vec::with_capacity(queued.len());
    for definition in queued {
        let key = definition
            .key
            .parse::<Identifier>()
            .map_err(|_| PluginEnchantmentRegistrationError::InvalidKey(definition.key.clone()))?;
        if registry.by_key(&key).is_some()
            || prepared.iter().any(|(existing, _, _)| existing == &key)
        {
            return Err(PluginEnchantmentRegistrationError::DuplicateKey(key));
        }
        if definition.max_level == 0 || definition.weight == 0 {
            return Err(PluginEnchantmentRegistrationError::InvalidDefinition(
                definition.key,
            ));
        }
        let slots = definition
            .active_slots
            .iter()
            .map(|slot| {
                slot_group(slot)
                    .ok_or_else(|| PluginEnchantmentRegistrationError::InvalidSlot(slot.clone()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        for tag in [
            &definition.supported_items,
            definition.primary_items.as_ref().unwrap_or(&String::new()),
            definition.exclusive_set.as_ref().unwrap_or(&String::new()),
        ] {
            if !tag.is_empty() && (!tag.starts_with('#') || tag[1..].parse::<Identifier>().is_err())
            {
                return Err(PluginEnchantmentRegistrationError::InvalidTag(tag.clone()));
            }
        }
        prepared.push((key, definition, slots));
    }
    let mut ids = Vec::with_capacity(prepared.len());
    for (key, definition, slots) in prepared {
        let slots: &'static [EquipmentSlotGroup] = Box::leak(slots.into_boxed_slice());
        let supported_items: &'static str = Box::leak(definition.supported_items.into_boxed_str());
        let primary_items = definition
            .primary_items
            .map(|value| Box::leak(value.into_boxed_str()) as &'static str);
        let exclusive_set = definition
            .exclusive_set
            .map(|value| Box::leak(value.into_boxed_str()) as &'static str);
        let entry = Box::leak(Box::new(Enchantment {
            key,
            max_level: definition.max_level,
            min_cost: EnchantmentCost {
                base: definition.minimum_cost_base,
                per_level_above_first: definition.minimum_cost_per_level,
            },
            max_cost: EnchantmentCost {
                base: definition.maximum_cost_base,
                per_level_above_first: definition.maximum_cost_per_level,
            },
            anvil_cost: definition.anvil_cost,
            weight: definition.weight,
            slots,
            supported_items,
            primary_items,
            exclusive_set,
            effects_nbt: empty_effects_nbt,
            effects: definition.effects.into_effects(),
        }));
        let id = registry
            .register_dynamic(entry)
            .map_err(|error| match error {
                EnchantmentRegistrationError::Frozen => PluginEnchantmentRegistrationError::Frozen,
                EnchantmentRegistrationError::DuplicateKey(key) => {
                    PluginEnchantmentRegistrationError::DuplicateKey(key)
                }
            })?;
        ids.push(id);
    }
    Ok(ids)
}

#[cfg(test)]
static TEST_MUTEX: std::sync::OnceLock<SyncMutex<()>> = std::sync::OnceLock::new();

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queue_round_trips_owned_definition() {
        let _guard = super::TEST_MUTEX.get_or_init(|| SyncMutex::new(())).lock();
        let _ = drain_plugin_enchantments();
        let d = PluginEnchantmentDefinition {
            key: "example:veinminer".into(),
            weight: 1,
            max_level: 1,
            minimum_cost_base: 15,
            minimum_cost_per_level: 0,
            maximum_cost_base: 65,
            maximum_cost_per_level: 0,
            anvil_cost: 7,
            active_slots: vec!["mainhand".into()],
            supported_items: "#minecraft:enchantable/mining".into(),
            primary_items: None,
            exclusive_set: None,
            effects: OwnedEnchantmentEffects::default(),
        };
        queue_plugin_enchantment(d.clone());
        assert_eq!(drain_plugin_enchantments(), vec![d]);
    }
}

#[cfg(test)]
mod registration_tests {
    use super::*;
    use crate::enchantment::EnchantmentRegistry;
    fn definition(key: &str) -> PluginEnchantmentDefinition {
        PluginEnchantmentDefinition {
            key: key.into(),
            weight: 1,
            max_level: 1,
            minimum_cost_base: 15,
            minimum_cost_per_level: 0,
            maximum_cost_base: 65,
            maximum_cost_per_level: 0,
            anvil_cost: 7,
            active_slots: vec!["mainhand".into()],
            supported_items: "#minecraft:enchantable/mining".into(),
            primary_items: None,
            exclusive_set: None,
            effects: OwnedEnchantmentEffects::default(),
        }
    }
    #[test]
    fn registers_veinminer_like_definition() {
        let _guard = super::TEST_MUTEX.get_or_init(|| SyncMutex::new(())).lock();
        let _ = drain_plugin_enchantments();
        let mut r = EnchantmentRegistry::new();
        queue_plugin_enchantment(definition("veinminer_enchantment:veinminer"));
        let ids = register_queued_plugin_enchantments(&mut r).expect("valid");
        assert_eq!(ids, vec![0]);
        assert!(
            r.by_key(&"veinminer_enchantment:veinminer".parse().expect("key"))
                .is_some()
        );
    }
    #[test]
    fn rejects_invalid_slot() {
        let _guard = super::TEST_MUTEX.get_or_init(|| SyncMutex::new(())).lock();
        let _ = drain_plugin_enchantments();
        let mut r = EnchantmentRegistry::new();
        let mut d = definition("example:bad_slot");
        d.active_slots = vec!["not_a_slot".into()];
        queue_plugin_enchantment(d);
        assert!(matches!(
            register_queued_plugin_enchantments(&mut r),
            Err(PluginEnchantmentRegistrationError::InvalidSlot(_))
        ));
    }
    #[test]
    fn rejects_duplicate() {
        let _guard = super::TEST_MUTEX.get_or_init(|| SyncMutex::new(())).lock();
        let _ = drain_plugin_enchantments();
        let mut r = EnchantmentRegistry::new();
        queue_plugin_enchantment(definition("example:duplicate"));
        assert!(register_queued_plugin_enchantments(&mut r).is_ok());
        queue_plugin_enchantment(definition("example:duplicate"));
        assert!(matches!(
            register_queued_plugin_enchantments(&mut r),
            Err(PluginEnchantmentRegistrationError::DuplicateKey(_))
        ));
    }
}
