//! Item display entity.
//!
//! Vanilla parity: `Display.ItemDisplay`. Shows one item stack with no
//! collision and no pickup. What separates it from a dropped item is the
//! display context: the same stack is modeled differently in a hand, on a
//! head, in a GUI or on the ground, and this entity picks which of those
//! models the client draws.

use std::sync::Weak;

use foton_macros::entity_behavior;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_entity_data::ItemDisplayEntityData;
use foton_utils::locks::SyncMutex;
use foton_utils::{DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use uuid::Uuid;

use crate::entity::{Entity, EntityBase, EntityBaseLoad, EntitySyncedData};
use crate::inventory::slot_ranges::CONTENTS_SLOT;
use crate::world::World;

/// Which of an item's models the client should draw.
///
/// Vanilla parity: `ItemDisplayContext`. The ids are the wire and NBT values,
/// so they are fixed by the protocol and not free to renumber.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemDisplayContext {
    /// No specific context; the client falls back to the generic model.
    None,
    /// Held in a third-person left hand.
    ThirdPersonLeftHand,
    /// Held in a third-person right hand.
    ThirdPersonRightHand,
    /// Held in a first-person left hand.
    FirstPersonLeftHand,
    /// Held in a first-person right hand.
    FirstPersonRightHand,
    /// Worn on the head.
    Head,
    /// Drawn in an inventory slot.
    Gui,
    /// Lying on the ground.
    Ground,
    /// Mounted in an item frame.
    Fixed,
    /// Placed on a shelf.
    OnShelf,
}

impl ItemDisplayContext {
    /// Returns the synced-data and wire id for this context.
    ///
    /// Vanilla parity: `ItemDisplayContext.getId`.
    #[must_use]
    pub const fn id(self) -> i8 {
        match self {
            Self::None => 0,
            Self::ThirdPersonLeftHand => 1,
            Self::ThirdPersonRightHand => 2,
            Self::FirstPersonLeftHand => 3,
            Self::FirstPersonRightHand => 4,
            Self::Head => 5,
            Self::Gui => 6,
            Self::Ground => 7,
            Self::Fixed => 8,
            Self::OnShelf => 9,
        }
    }

    /// Resolves a context id, clamping out-of-range ids to `None`.
    ///
    /// Vanilla parity: `ItemDisplayContext.BY_ID`, built with
    /// `ByIdMap.OutOfBoundsStrategy.ZERO`.
    #[must_use]
    pub const fn by_id(id: i8) -> Self {
        match id {
            1 => Self::ThirdPersonLeftHand,
            2 => Self::ThirdPersonRightHand,
            3 => Self::FirstPersonLeftHand,
            4 => Self::FirstPersonRightHand,
            5 => Self::Head,
            6 => Self::Gui,
            7 => Self::Ground,
            8 => Self::Fixed,
            9 => Self::OnShelf,
            _ => Self::None,
        }
    }

    /// Returns the vanilla NBT name for this context.
    ///
    /// Vanilla parity: `ItemDisplayContext.getSerializedName`.
    #[must_use]
    pub const fn serialized_name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ThirdPersonLeftHand => "thirdperson_lefthand",
            Self::ThirdPersonRightHand => "thirdperson_righthand",
            Self::FirstPersonLeftHand => "firstperson_lefthand",
            Self::FirstPersonRightHand => "firstperson_righthand",
            Self::Head => "head",
            Self::Gui => "gui",
            Self::Ground => "ground",
            Self::Fixed => "fixed",
            Self::OnShelf => "on_shelf",
        }
    }

    /// Parses a vanilla NBT context name.
    #[must_use]
    pub fn from_serialized_name(name: &str) -> Option<Self> {
        match name {
            "none" => Some(Self::None),
            "thirdperson_lefthand" => Some(Self::ThirdPersonLeftHand),
            "thirdperson_righthand" => Some(Self::ThirdPersonRightHand),
            "firstperson_lefthand" => Some(Self::FirstPersonLeftHand),
            "firstperson_righthand" => Some(Self::FirstPersonRightHand),
            "head" => Some(Self::Head),
            "gui" => Some(Self::Gui),
            "ground" => Some(Self::Ground),
            "fixed" => Some(Self::Fixed),
            "on_shelf" => Some(Self::OnShelf),
            _ => None,
        }
    }
}

/// An item display entity.
///
/// Vanilla parity: `Display.ItemDisplay`. Like its `BlockDisplay` sibling this
/// carries the subclass state only. The shared `Display` layer -- transformation,
/// billboard mode, brightness, view range, shadow, culling size and glow color --
/// exists in the synced data but is neither persisted nor interpolated here,
/// because Foton has no display render-state system; see `BlockDisplayEntity`
/// for the same gap.
#[entity_behavior(class = "ItemDisplay")]
pub struct ItemDisplayEntity {
    /// Common entity fields (id, uuid, position, etc.).
    base: EntityBase,
    /// Vanilla entity type registered for this implementation.
    entity_type: EntityTypeRef,
    /// Synced entity data for network serialization.
    entity_data: SyncMutex<ItemDisplayEntityData>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `ItemDisplayEntity`.
unsafe impl DowncastType for ItemDisplayEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/item_display");
}

impl ItemDisplayEntity {
    /// Creates a new item display entity.
    ///
    /// The `id` should be obtained from `next_entity_id()`.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(ItemDisplayEntityData::new()),
        }
    }

    /// Creates a new item display entity with a specific UUID.
    ///
    /// The `id` should be obtained from `next_entity_id()`.
    #[must_use]
    pub fn with_uuid(
        entity_type: EntityTypeRef,
        id: i32,
        position: DVec3,
        uuid: Uuid,
        world: Weak<World>,
    ) -> Self {
        Self {
            base: EntityBase::with_uuid(id, uuid, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(ItemDisplayEntityData::new()),
        }
    }

    /// Creates an item display entity from saved data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(ItemDisplayEntityData::new()),
        }
    }

    /// Gets a reference to the entity data for reading/modifying synced state.
    pub const fn entity_data(&self) -> &SyncMutex<ItemDisplayEntityData> {
        &self.entity_data
    }

    /// Returns the displayed stack.
    ///
    /// Vanilla parity: `Display.ItemDisplay.getItemStack`.
    #[must_use]
    pub fn item_stack(&self) -> ItemStack {
        self.entity_data.lock().item_stack.get().clone()
    }

    /// Sets the displayed stack.
    ///
    /// Vanilla parity: `Display.ItemDisplay.setItemStack`, which is also the
    /// write half of the entity's single `SlotAccess`. Foton has no slot-access
    /// abstraction, so `/item` and dispenser-style slot writes cannot reach it
    /// yet.
    pub fn set_item_stack(&self, item: ItemStack) {
        self.entity_data.lock().item_stack.set(item);
    }

    /// Returns which model variant the client draws.
    ///
    /// Vanilla parity: `Display.ItemDisplay.getItemTransform`.
    #[must_use]
    pub fn item_transform(&self) -> ItemDisplayContext {
        ItemDisplayContext::by_id(*self.entity_data.lock().item_display.get())
    }

    /// Sets which model variant the client draws.
    ///
    /// Vanilla parity: `Display.ItemDisplay.setItemTransform`.
    pub fn set_item_transform(&self, transform: ItemDisplayContext) {
        self.entity_data.lock().item_display.set(transform.id());
    }
}

impl Entity for ItemDisplayEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    /// Vanilla parity: `Display.ItemDisplay.getSlot`, whose one slot is the item on show.
    fn slot_item(&self, slot: i32) -> Option<ItemStack> {
        if slot == CONTENTS_SLOT {
            return Some(self.item_stack());
        }
        self.entity_slot_item(slot)
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `Display.isIgnoringBlockTriggers`.
    fn is_ignoring_block_triggers(&self) -> bool {
        true
    }

    /// Vanilla parity: `Display.ItemDisplay.addAdditionalSaveData`.
    ///
    /// An empty stack is left out entirely, as vanilla does, so a reload leaves
    /// the default empty stack in place. The shared
    /// `Display.addAdditionalSaveData` half is not written, matching
    /// `BlockDisplayEntity`.
    fn save_additional(&self, nbt: &mut NbtCompound) {
        let data = self.entity_data.lock();
        let item = data.item_stack.get();
        if !item.is_empty() {
            nbt.insert("item", item.to_nbt_tag_ref());
        }
        let transform = ItemDisplayContext::by_id(*data.item_display.get());
        drop(data);
        nbt.insert("item_display", transform.serialized_name());
    }

    /// Vanilla parity: `Display.ItemDisplay.readAdditionalSaveData`.
    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        let item = nbt
            .compound("item")
            .and_then(|compound| ItemStack::from_borrowed_compound(&compound))
            .unwrap_or_else(ItemStack::empty);
        let transform = nbt
            .string("item_display")
            .and_then(|name| ItemDisplayContext::from_serialized_name(&name.to_str()))
            .unwrap_or(ItemDisplayContext::None);

        let mut data = self.entity_data.lock();
        data.item_stack.set(item);
        data.item_display.set(transform.id());
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use foton_registry::{init_vanilla_registry, vanilla_entities, vanilla_items};
    use simdnbt::borrow::read_compound;

    use super::*;
    use crate::entity::{EntityBaseSaveData, EntityFireFreezeState};

    fn reload(entity: &ItemDisplayEntity) -> ItemDisplayEntity {
        let mut saved = NbtCompound::new();
        entity.save_additional(&mut saved);
        let mut bytes = Vec::new();
        saved.write(&mut bytes);
        let Ok(borrowed) = read_compound(&mut Cursor::new(bytes.as_slice())) else {
            panic!("saved item display NBT should reborrow");
        };
        let loaded = ItemDisplayEntity::from_saved(
            &vanilla_entities::ITEM_DISPLAY,
            EntityBaseLoad {
                id: 12,
                position: DVec3::ZERO,
                uuid: Uuid::nil(),
                velocity: DVec3::ZERO,
                rotation: (0.0, 0.0),
                fall_distance: 0.0,
                fire_freeze: EntityFireFreezeState::new(),
                on_ground: false,
                save_data: EntityBaseSaveData::new(),
                world: Weak::new(),
            },
        );
        loaded.load_additional((&borrowed).into());
        loaded
    }

    #[test]
    fn a_saved_item_display_comes_back_with_its_stack_and_display_context() {
        init_vanilla_registry();
        let entity = ItemDisplayEntity::new(
            &vanilla_entities::ITEM_DISPLAY,
            11,
            DVec3::new(0.5, 70.0, 0.5),
            Weak::new(),
        );
        entity.set_item_stack(ItemStack::with_count(&vanilla_items::DIAMOND_SWORD, 1));
        entity.set_item_transform(ItemDisplayContext::Head);

        let loaded = reload(&entity);

        assert!(loaded.item_stack().is(&vanilla_items::DIAMOND_SWORD));
        assert_eq!(loaded.item_transform(), ItemDisplayContext::Head);
    }

    #[test]
    fn an_empty_item_display_writes_no_item_tag_and_reloads_empty() {
        init_vanilla_registry();
        let entity = ItemDisplayEntity::new(
            &vanilla_entities::ITEM_DISPLAY,
            11,
            DVec3::ZERO,
            Weak::new(),
        );

        let mut saved = NbtCompound::new();
        entity.save_additional(&mut saved);
        assert!(saved.get("item").is_none());

        let loaded = reload(&entity);
        assert!(loaded.item_stack().is_empty());
        assert_eq!(loaded.item_transform(), ItemDisplayContext::None);
    }

    #[test]
    fn an_unknown_display_context_id_falls_back_to_none() {
        assert_eq!(ItemDisplayContext::by_id(42), ItemDisplayContext::None);
        assert_eq!(ItemDisplayContext::by_id(-1), ItemDisplayContext::None);
    }
}
