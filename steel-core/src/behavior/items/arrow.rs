//! Arrow items -- the item side of what a bow or a crossbow fires.

use steel_macros::item_behavior;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_entities;

use crate::behavior::{ITEM_BEHAVIORS, ItemBehavior};
use crate::entity::ENTITIES;

/// The plain arrow.
///
/// Vanilla parity: `ArrowItem`, whose whole server-side job is `createArrow`.
///
/// Steel gap: Vanilla hands the new arrow the stack it came from, so the arrow
/// remembers what to give back when it is picked up and a tipped arrow carries
/// its `potion_contents`. Steel's `ArrowEntity` stores neither a pickup stack
/// nor the weapon it was fired from, so only the entity type travels.
///
/// Steel gap: Vanilla's `ArrowItem` also implements `ProjectileItem`, which is
/// how a dispenser shoots one. Steel has no dispense-behavior registry.
#[item_behavior]
pub struct ArrowItem;

impl ItemBehavior for ArrowItem {
    fn arrow_entity_type(&self) -> Option<EntityTypeRef> {
        Some(&vanilla_entities::ARROW)
    }
}

/// The spectral arrow.
///
/// Vanilla parity: `SpectralArrowItem`, which only swaps the entity its
/// `createArrow` builds.
#[item_behavior]
pub struct SpectralArrowItem;

impl ItemBehavior for SpectralArrowItem {
    fn arrow_entity_type(&self) -> Option<EntityTypeRef> {
        Some(&vanilla_entities::SPECTRAL_ARROW)
    }
}

/// Returns the arrow entity a weapon firing `ammo` should build.
///
/// Vanilla parity: `ProjectileWeaponItem.createProjectile`, which casts the
/// ammunition to `ArrowItem` and falls back to `Items.ARROW` when it is not
/// one.
///
/// Steel deviation: an arrow type Steel has no entity factory for also falls
/// back to the plain arrow. Building an entity Steel cannot reload would lose
/// it on the next chunk load, which is worse than firing the wrong arrow.
#[must_use]
pub fn arrow_entity_type_for(ammo: &ItemStack) -> EntityTypeRef {
    let declared = ITEM_BEHAVIORS
        .get_behavior(ammo.item())
        .arrow_entity_type()
        .unwrap_or(&vanilla_entities::ARROW);

    if ENTITIES.has_factory(declared) {
        declared
    } else {
        &vanilla_entities::ARROW
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, item_stack::ItemStack, vanilla_items};

    use super::arrow_entity_type_for;
    use crate::behavior::init_behaviors;
    use crate::entity::init_entities;
    use steel_registry::vanilla_entities;

    #[test]
    fn an_item_that_is_not_an_arrow_still_fires_a_plain_arrow() {
        init_vanilla_registry();
        init_behaviors();
        init_entities();

        let firework = ItemStack::new(&vanilla_items::FIREWORK_ROCKET);
        assert_eq!(
            arrow_entity_type_for(&firework),
            &vanilla_entities::ARROW,
            "vanilla's createProjectile falls back to Items.ARROW"
        );
    }

    /// A bow loaded with spectral arrows has to fire the spectral entity, or
    /// the glowing never happens. This was the fallback arm of
    /// `arrow_entity_type_for` until `SpectralArrowEntity` existed to be
    /// registered, so it is also the check that the factory is wired.
    #[test]
    fn a_spectral_arrow_fires_the_spectral_entity() {
        init_vanilla_registry();
        init_behaviors();
        init_entities();

        let spectral = ItemStack::new(&vanilla_items::SPECTRAL_ARROW);
        assert_eq!(
            arrow_entity_type_for(&spectral),
            &vanilla_entities::SPECTRAL_ARROW
        );
    }
}
