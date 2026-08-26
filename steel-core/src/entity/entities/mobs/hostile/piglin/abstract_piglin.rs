//! The half of a piglin a brute shares.
//!
//! Vanilla parity: `net.minecraft.world.entity.monster.piglin.AbstractPiglin`.
//! Rust has no abstract base class, so the shared body is these free functions
//! and the two mobs call them from their own overrides.

use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_registry::data_components::vanilla_components::TOOL;
use steel_registry::item_stack::ItemStack;
use steel_utils::types::{Difficulty, InteractionHand};

use crate::entity::ai::path::PathType;
use crate::entity::{LivingEntity, Mob};
use crate::inventory::container::{Container as _, SimpleContainer};

/// How long a piglin has to stand in the overworld before it turns.
///
/// Vanilla parity: `AbstractPiglin.CONVERSION_TIME`.
pub const CONVERSION_TIME: i32 = 300;

/// How badly a piglin wants to keep clear of fire.
///
/// Vanilla parity: the two `setPathfindingMalus` calls of the
/// `AbstractPiglin` constructor.
const FIRE_IN_NEIGHBOR_MALUS: f32 = 16.0;
const FIRE_MALUS: f32 = -1.0;

/// What a piglin holds in its hands, on the client.
///
/// Vanilla parity: `net.minecraft.world.entity.monster.piglin.PiglinArmPose`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiglinArmPose {
    AttackingWithMeleeWeapon,
    CrossbowHold,
    CrossbowCharge,
    AdmiringItem,
    Dancing,
    Default,
}

/// Applies the shared part of the `AbstractPiglin` constructor.
///
/// Vanilla parity: `setCanPickUpLoot(true)`, `applyOpenDoorsAbility()` and the
/// two fire maluses -- the second of which is `-1.0`, meaning a piglin will not
/// path into fire at any cost.
pub fn apply_constructor<P: Mob + ?Sized>(piglin: &P) {
    Mob::set_can_pick_up_loot(piglin, true);
    piglin
        .mob_base()
        .navigation()
        .lock()
        .set_can_open_doors(true);
    piglin.set_pathfinding_malus(PathType::FireInNeighbor, FIRE_IN_NEIGHBOR_MALUS);
    piglin.set_pathfinding_malus(PathType::Fire, FIRE_MALUS);
}

/// Whether the main hand holds something that counts as a weapon.
///
/// Vanilla parity: `AbstractPiglin.isHoldingMeleeWeapon`, which asks for the
/// `tool` component rather than a tag -- so a golden shovel counts.
#[must_use]
pub fn is_holding_melee_weapon(body: &dyn LivingEntity) -> bool {
    body.get_item_in_hand(InteractionHand::MainHand)
        .get(TOOL)
        .is_some()
}

/// A piglin that can be converted and its clock.
///
/// The two mobs keep the same three pieces of state and run the same clock, so
/// the clock takes them through this trait rather than being written twice.
pub trait ConvertiblePiglin: Mob {
    /// Vanilla parity: `AbstractPiglin.isConverting`.
    fn is_converting(&self) -> bool;

    /// Returns and advances `timeInOverworld`, or resets it to zero.
    fn bump_time_in_overworld(&self, converting: bool) -> i32;

    /// Vanilla parity: `AbstractPiglin.playConvertedSound`.
    fn play_converted_sound(&self);

    /// Vanilla parity: `AbstractPiglin.finishConversion`.
    fn convert_to_zombified(&self);
}

/// Runs the overworld conversion clock.
///
/// Vanilla parity: `AbstractPiglin.customServerAiStep`. The converted sound is
/// skipped on peaceful, which is the one difficulty check in the whole method.
pub fn tick_conversion<P: ConvertiblePiglin + ?Sized>(piglin: &P) {
    let converting = piglin.is_converting();
    let time_in_overworld = piglin.bump_time_in_overworld(converting);
    if time_in_overworld <= CONVERSION_TIME {
        return;
    }

    if piglin
        .level()
        .is_some_and(|world| world.difficulty() != Difficulty::Peaceful)
    {
        piglin.play_converted_sound();
    }
    piglin.convert_to_zombified();
}

/// The NBT key a piglin's carried inventory is stored under.
///
/// Vanilla parity: `InventoryCarrier.TAG_INVENTORY`.
const TAG_INVENTORY: &str = "Inventory";

/// Writes the carried inventory.
///
/// Vanilla parity: `InventoryCarrier.writeInventoryToTag`.
pub fn save_inventory(inventory: &SimpleContainer, nbt: &mut NbtCompound) {
    let items: Vec<NbtCompound> = inventory
        .items()
        .iter()
        .filter(|item| !item.is_empty())
        .filter_map(|item| match item.to_nbt_tag_ref() {
            NbtTag::Compound(compound) => Some(compound),
            _ => None,
        })
        .collect();
    nbt.insert(TAG_INVENTORY, NbtList::Compound(items));
}

/// Reads the carried inventory back.
///
/// Vanilla parity: `InventoryCarrier.readInventoryFromTag`.
pub fn load_inventory(inventory: &mut SimpleContainer, nbt: BorrowedNbtCompoundView<'_, '_>) {
    let Some(list) = nbt.list(TAG_INVENTORY).and_then(|list| list.compounds()) else {
        return;
    };
    for compound in list {
        let Some(mut item) = ItemStack::from_borrowed_compound(&compound) else {
            continue;
        };
        inventory.add(&mut item);
    }
}
