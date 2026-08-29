//! The named numeric slots commands address entities and containers by.
//!
//! Vanilla parity: `SlotRanges`. The numbers are not an inventory layout --
//! they are a flat namespace laid over every kind of slot an entity can have,
//! so that one command argument can name a villager's third inventory slot
//! (`mob.inventory.2`), a horse's saddle (`saddle`) and a chest's fortieth
//! slot (`container.39`) without knowing what it is talking to.
//!
//! Ids collide on purpose. `horse.chest` and `player.cursor` are both 499, and
//! `horse.0`..`horse.3` overlap `player.crafting.0`..`player.crafting.3`; what
//! separates them is which entity is asked, not which name was used.

use std::sync::LazyLock;

use foton_registry::equipment::EquipmentSlot;
use foton_registry::item_stack::ItemStack;
use rustc_hash::FxHashMap;

use crate::inventory::container::Container;

/// The only slot an entity that holds exactly one item has.
///
/// Vanilla writes the literal `0` in each of those `getSlot` overrides; this
/// is the same number under the name the `contents` range gives it.
pub const CONTENTS_SLOT: i32 = 0;

/// Vanilla `SlotRanges.MOB_INVENTORY_SLOT_OFFSET`.
pub const MOB_INVENTORY_SLOT_OFFSET: i32 = 300;

/// Vanilla `SlotRanges.MOB_INVENTORY_SIZE`.
pub const MOB_INVENTORY_SIZE: i32 = 8;

/// The base vanilla adds a hand slot's own index to.
const WEAPON_SLOT_BASE: i32 = 98;

/// The base vanilla adds a humanoid armor slot's own index to.
const ARMOR_SLOT_BASE: i32 = 100;

/// The base vanilla adds an animal armor slot's own index to.
const BODY_SLOT_BASE: i32 = 105;

/// The base vanilla adds a saddle slot's own index to.
const SADDLE_SLOT_BASE: i32 = 106;

/// The first slot id of a player's ender chest.
pub const ENDER_CHEST_SLOT_OFFSET: i32 = 200;

/// The first slot id of a horse's or a nautilus's own inventory.
pub const MOUNT_INVENTORY_SLOT_OFFSET: i32 = 500;

/// The slot id of a chested mount's chest and of a player's cursor.
pub const CURSOR_AND_MOUNT_CHEST_SLOT: i32 = 499;

/// The first slot id of a player's own two-by-two crafting grid.
pub const PLAYER_CRAFTING_SLOT_OFFSET: i32 = 500;

/// How many slots a player's own crafting grid has.
pub const PLAYER_CRAFTING_SIZE: i32 = 4;

/// One named set of numeric command slot ids.
///
/// Vanilla parity: `SlotRange`.
#[derive(Debug)]
pub struct SlotRange {
    name: String,
    slots: Vec<i32>,
}

impl SlotRange {
    /// The name a command argument spells this range with.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The slot ids this range covers, in vanilla's order.
    #[must_use]
    pub fn slots(&self) -> &[i32] {
        &self.slots
    }
}

/// Every slot range vanilla defines, in its declaration order.
///
/// The order is what a client's suggestion list looks like, so it follows
/// vanilla's `SLOTS` list rather than sorting.
pub struct SlotRanges {
    ranges: Vec<SlotRange>,
    by_name: FxHashMap<String, usize>,
}

impl SlotRanges {
    /// Looks a range up by the name a command spelled.
    ///
    /// Vanilla parity: `SlotRanges.nameToIds`.
    #[must_use]
    pub fn name_to_ids(&self, name: &str) -> Option<&SlotRange> {
        self.by_name.get(name).map(|&index| &self.ranges[index])
    }

    /// Every name, in suggestion order.
    ///
    /// Vanilla parity: `SlotRanges.allNames`.
    pub fn all_names(&self) -> impl Iterator<Item = &str> {
        self.ranges.iter().map(SlotRange::name)
    }

    fn add_single_slot(&mut self, name: impl Into<String>, id: i32) {
        self.add(name.into(), vec![id]);
    }

    fn add_slot_range(&mut self, prefix: &str, offset: i32, size: i32) {
        let mut all_slots = Vec::with_capacity(size as usize);
        for index in 0..size {
            let slot_id = offset + index;
            self.add(format!("{prefix}{index}"), vec![slot_id]);
            all_slots.push(slot_id);
        }
        self.add(format!("{prefix}*"), all_slots);
    }

    fn add_slots(&mut self, name: impl Into<String>, ids: Vec<i32>) {
        self.add(name.into(), ids);
    }

    fn add(&mut self, name: String, slots: Vec<i32>) {
        self.by_name.insert(name.clone(), self.ranges.len());
        self.ranges.push(SlotRange { name, slots });
    }
}

/// The vanilla slot-range table, built once.
pub static SLOT_RANGES: LazyLock<SlotRanges> = LazyLock::new(|| {
    let mut ranges = SlotRanges {
        ranges: Vec::new(),
        by_name: FxHashMap::default(),
    };

    ranges.add_single_slot("contents", 0);
    ranges.add_slot_range("container.", 0, 54);
    ranges.add_slot_range("hotbar.", 0, 9);
    ranges.add_slot_range("inventory.", 9, 27);
    ranges.add_slot_range("enderchest.", ENDER_CHEST_SLOT_OFFSET, 27);
    ranges.add_slot_range(
        "mob.inventory.",
        MOB_INVENTORY_SLOT_OFFSET,
        MOB_INVENTORY_SIZE,
    );
    ranges.add_slot_range("horse.", MOUNT_INVENTORY_SLOT_OFFSET, 15);

    let main_hand = command_slot_id(EquipmentSlot::MainHand);
    let off_hand = command_slot_id(EquipmentSlot::OffHand);
    ranges.add_single_slot("weapon", main_hand);
    ranges.add_single_slot("weapon.mainhand", main_hand);
    ranges.add_single_slot("weapon.offhand", off_hand);
    ranges.add_slots("weapon.*", vec![main_hand, off_hand]);

    let head = command_slot_id(EquipmentSlot::Head);
    let chest = command_slot_id(EquipmentSlot::Chest);
    let legs = command_slot_id(EquipmentSlot::Legs);
    let feet = command_slot_id(EquipmentSlot::Feet);
    let body = command_slot_id(EquipmentSlot::Body);
    ranges.add_single_slot("armor.head", head);
    ranges.add_single_slot("armor.chest", chest);
    ranges.add_single_slot("armor.legs", legs);
    ranges.add_single_slot("armor.feet", feet);
    ranges.add_single_slot("armor.body", body);
    ranges.add_slots("armor.*", vec![head, chest, legs, feet, body]);

    ranges.add_single_slot("saddle", command_slot_id(EquipmentSlot::Saddle));
    ranges.add_single_slot("horse.chest", CURSOR_AND_MOUNT_CHEST_SLOT);
    ranges.add_single_slot("player.cursor", CURSOR_AND_MOUNT_CHEST_SLOT);
    ranges.add_slot_range(
        "player.crafting.",
        PLAYER_CRAFTING_SLOT_OFFSET,
        PLAYER_CRAFTING_SIZE,
    );

    ranges
});

/// The numeric command slot id an equipment slot answers to.
///
/// Vanilla parity: the `EquipmentSlot.getIndex(base)` calls in `SlotRanges`,
/// with the base each slot type is given there.
#[must_use]
pub const fn command_slot_id(slot: EquipmentSlot) -> i32 {
    let base = match slot {
        EquipmentSlot::MainHand | EquipmentSlot::OffHand => WEAPON_SLOT_BASE,
        EquipmentSlot::Feet | EquipmentSlot::Legs | EquipmentSlot::Chest | EquipmentSlot::Head => {
            ARMOR_SLOT_BASE
        }
        EquipmentSlot::Body => BODY_SLOT_BASE,
        EquipmentSlot::Saddle => SADDLE_SLOT_BASE,
    };
    base + slot.type_index()
}

/// Reads one slot of a container by a zero-based index.
///
/// `None` when the container has no such slot, which is vanilla's null
/// `SlotAccess`. A slot that does not exist and a slot that is empty are
/// different answers: `execute if items` skips the first and tests the second.
#[must_use]
pub fn container_slot_item(container: &dyn Container, slot: i32) -> Option<ItemStack> {
    let index = usize::try_from(slot).ok()?;
    (index < container.get_container_size()).then(|| container.get_item(index).clone())
}

/// The equipment slot a numeric command slot id names, if any.
///
/// Vanilla parity: `LivingEntity.getEquipmentSlot(int)`, which is the inverse
/// of [`command_slot_id`] over the same eight slots.
#[must_use]
pub fn equipment_slot_from_command_slot(slot: i32) -> Option<EquipmentSlot> {
    EquipmentSlot::ALL
        .into_iter()
        .find(|&equipment| command_slot_id(equipment) == slot)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ids are the whole point of the table, and none of them is derivable
    /// from a name by inspection. These are the numbers vanilla's own
    /// `SlotRanges` produces.
    #[test]
    fn the_named_ranges_carry_the_slot_ids_vanilla_gives_them() {
        let cases: [(&str, &[i32]); 14] = [
            ("contents", &[0]),
            ("container.53", &[53]),
            ("hotbar.8", &[8]),
            ("inventory.0", &[9]),
            ("inventory.26", &[35]),
            ("enderchest.26", &[226]),
            ("mob.inventory.7", &[307]),
            ("horse.14", &[514]),
            ("weapon", &[98]),
            ("weapon.offhand", &[99]),
            ("armor.*", &[103, 102, 101, 100, 105]),
            ("saddle", &[106]),
            ("horse.chest", &[499]),
            ("player.crafting.3", &[503]),
        ];

        for (name, slots) in cases {
            let Some(range) = SLOT_RANGES.name_to_ids(name) else {
                panic!("{name} should be a slot range");
            };
            assert_eq!(range.slots(), slots, "{name} has the wrong slot ids");
        }
    }

    /// A `.*` range covers exactly the numbered ones under the same prefix, in
    /// order. Getting this wrong is how a `container.*` that quietly stops at
    /// slot 27 happens.
    #[test]
    fn a_wildcard_range_covers_every_numbered_range_under_it() {
        for (prefix, size) in [("container.", 54), ("hotbar.", 9), ("inventory.", 27)] {
            let Some(wildcard) = SLOT_RANGES.name_to_ids(&format!("{prefix}*")) else {
                panic!("{prefix}* should be a slot range");
            };
            let expected = (0..size)
                .map(|index| {
                    let name = format!("{prefix}{index}");
                    let Some(range) = SLOT_RANGES.name_to_ids(&name) else {
                        panic!("{name} should be a slot range");
                    };
                    assert_eq!(range.slots().len(), 1, "{name} should be one slot");
                    range.slots()[0]
                })
                .collect::<Vec<_>>();
            assert_eq!(wildcard.slots(), expected, "{prefix}* is not its own parts");
        }
    }

    /// The two directions of the equipment mapping have to agree, or a slot a
    /// range names is a slot no entity answers for.
    #[test]
    fn every_equipment_slot_round_trips_through_its_command_id() {
        for slot in EquipmentSlot::ALL {
            assert_eq!(
                equipment_slot_from_command_slot(command_slot_id(slot)),
                Some(slot)
            );
        }
        // 104 is the hole vanilla leaves between the armor block and the body
        // slot, and nothing may claim it.
        assert_eq!(equipment_slot_from_command_slot(104), None);
    }

    /// An unknown name is a parse error, not an empty range that silently
    /// matches nothing.
    #[test]
    fn an_unknown_name_is_not_a_range() {
        assert!(SLOT_RANGES.name_to_ids("container.54").is_none());
        assert!(SLOT_RANGES.name_to_ids("armor").is_none());
        assert!(SLOT_RANGES.name_to_ids("").is_none());
    }
}
