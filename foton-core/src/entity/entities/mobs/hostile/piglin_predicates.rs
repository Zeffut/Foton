//! The two piglin predicates the sensor and the brain both ask.
//!
//! Vanilla parity: `PiglinAi.isWearingSafeArmor` and
//! `PiglinAi.isPlayerHoldingLovedItem`. They live one level above the piglin so
//! that [`crate::entity::ai::brain::sensor`] can reach them without depending on
//! the piglin's private module; vanilla has the same two callers.

use foton_registry::equipment::EquipmentSlot;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{REGISTRY, TaggedRegistryExt as _, vanilla_entities};

use crate::entity::LivingEntity;
use crate::entity::ai::brain::behavior::utils;

/// Returns whether `entity` wears any piece of gold armor.
///
/// Vanilla parity: `PiglinAi.isWearingSafeArmor`. One piece is enough, which is
/// why a single golden boot buys the whole truce.
#[must_use]
pub fn is_wearing_safe_armor(entity: &dyn LivingEntity) -> bool {
    EquipmentSlot::ARMOR_SLOTS.into_iter().any(|slot| {
        REGISTRY.items.is_in_tag(
            entity.get_item_by_slot(slot).item(),
            &ItemTag::PIGLIN_SAFE_ARMOR,
        )
    })
}

/// Returns whether `entity` is a player holding gold.
///
/// Vanilla parity: `PiglinAi.isPlayerHoldingLovedItem`, which is what makes a
/// piglin stare at you when you take out an ingot.
#[must_use]
pub fn is_player_holding_loved_item(entity: &dyn LivingEntity) -> bool {
    utils::is_of_type(entity.as_entity_event_source(), &vanilla_entities::PLAYER)
        && entity.is_holding(&mut |item| {
            REGISTRY
                .items
                .is_in_tag(item.item(), &ItemTag::PIGLIN_LOVED)
        })
}
