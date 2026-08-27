//! The pickup rules every zombie shares.
//!
//! Vanilla parity: the `Zombie` overrides of `canHoldItem` and `wantsToPickUp`.
//! Java gets them by inheritance; Steel's zombie, husk, drowned, zombified
//! piglin and zombie villager are separate types, so they call these instead.

use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_registry::REGISTRY;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{TaggedRegistryExt as _, vanilla_items};

use crate::entity::{Entity, Mob};
use crate::world::World;

/// Whether a zombie of any kind will hold `item_stack`.
///
/// Vanilla parity: `Zombie.canHoldItem`. The egg rule keeps a chicken jockey
/// from snatching the eggs its mount lays out from under itself.
#[must_use]
pub(super) fn can_hold_item(zombie: &dyn Mob, is_baby: bool, item_stack: &ItemStack) -> bool {
    !(is_baby
        && Entity::is_passenger(zombie)
        && REGISTRY.items.is_in_tag(item_stack.item(), &ItemTag::EGGS))
}

/// Whether a zombie of any kind walks over to `item_stack`.
///
/// Vanilla parity: `Zombie.wantsToPickUp`, whose only addition is the glow ink
/// sac -- a glow squid's drop is the one thing a drowned leaves on the seabed.
#[must_use]
pub(super) fn wants_to_pick_up(zombie: &dyn Mob, world: &World, item_stack: &ItemStack) -> bool {
    !item_stack.is(&vanilla_items::GLOW_INK_SAC) && zombie.mob_wants_to_pick_up(world, item_stack)
}

/// Saves the state every zombie shares.
///
/// Vanilla parity: `Zombie.addAdditionalSaveData`. Vanilla also writes
/// `CanBreakDoors`, `InWaterTime` and `DrownedConversionTime`; Steel has
/// neither the door-breaking goal nor the drowning conversion, so there is no
/// state behind those keys to write.
pub(in crate::entity::entities::mobs) fn save_zombie(
    zombie: &dyn Mob,
    is_baby: bool,
    nbt: &mut NbtCompound,
) {
    zombie.save_mob(nbt);
    nbt.insert("IsBaby", i8::from(is_baby));
}

/// Loads the state every zombie shares.
///
/// Vanilla parity: `Zombie.readAdditionalSaveData`, whose `getBooleanOr("IsBaby",
/// false)` is why a zombie saved before the key existed comes back an adult.
pub(in crate::entity::entities::mobs) fn load_zombie(
    zombie: &dyn Mob,
    nbt: BorrowedNbtCompoundView<'_, '_>,
) {
    zombie.load_mob(nbt);
    zombie.set_baby(nbt.byte("IsBaby").is_some_and(|value| value != 0));
}
