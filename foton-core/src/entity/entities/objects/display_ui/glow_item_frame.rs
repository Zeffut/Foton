//! The glow item frame.
//!
//! Vanilla parity: `GlowItemFrame`, which extends `ItemFrame` and overrides
//! nothing but its five sounds and the item it drops. Everything else -- the
//! geometry, the rotation, the comparator output, the save format -- is
//! `ItemFrame`, and the pure geometry here is [`ItemFrameEntity`]'s own.

use std::sync::Weak;

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::data_components::vanilla_components::MAP_ID;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::items::ItemRef;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::GlowItemFrameEntityData;
use foton_registry::{sound_events, vanilla_blocks, vanilla_game_events, vanilla_items};
use foton_utils::locks::SyncMutex;
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, Direction, DowncastType, DowncastTypeKey, WorldAabb};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};

use super::ItemFrameEntity;
use super::item_frame::{
    FrameLike, FrameState, direction_3d_data_value, direction_from_3d_data_value,
};
use crate::behavior::InteractionResult;
use crate::entity::damage::DamageSource;
use crate::entity::{
    BlockAttached, Entity, EntityBase, EntityBaseLoad, EntityBaseState, EntitySyncedData,
    ItemFrame, RemovalReason, SharedEntity,
};
use crate::event::HangingBreakEvent;
use crate::inventory::slot_ranges::CONTENTS_SLOT;
use crate::player::Player;
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// How many ways round a framed item can sit.
///
/// Vanilla parity: the `% 8` shared by `ItemFrame.setRotation` and its
/// comparator output.
const ROTATION_STEPS: i32 = 8;

/// A glow item frame.
#[entity_behavior(class = "GlowItemFrame")]
pub struct GlowItemFrameEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<GlowItemFrameEntityData>,
    block_pos: SyncMutex<BlockPos>,
    state: SyncMutex<FrameState>,
}

// SAFETY: This key is owned by Foton and uniquely identifies
// `GlowItemFrameEntity`.
unsafe impl DowncastType for GlowItemFrameEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/glow_item_frame");
}

impl GlowItemFrameEntity {
    /// Creates a fresh glow item frame from the generic entity factory path.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_attached(
            entity_type,
            id,
            BlockPos::new(
                position.x.floor() as i32,
                position.y.floor() as i32,
                position.z.floor() as i32,
            ),
            Direction::South,
            world,
        )
    }

    /// Creates a fresh glow item frame attached to `block_pos`.
    #[must_use]
    pub fn new_attached(
        entity_type: EntityTypeRef,
        id: i32,
        block_pos: BlockPos,
        direction: Direction,
        world: Weak<World>,
    ) -> Self {
        let entity = Self {
            base: EntityBase::new_with_state(
                id,
                EntityBaseState::new_with_bounding_box(
                    ItemFrameEntity::frame_center(block_pos, direction),
                    entity_type.dimensions,
                    ItemFrameEntity::frame_bounding_box(block_pos, direction, false),
                )
                .with_rotation(ItemFrameEntity::rotation_for_direction(direction)),
                world,
            ),
            entity_type,
            entity_data: SyncMutex::new(GlowItemFrameEntityData::new()),
            block_pos: SyncMutex::new(block_pos),
            state: SyncMutex::new(FrameState::default()),
        };
        entity
            .entity_data
            .lock()
            .hanging_entity_mut()
            .direction
            .set(direction);
        entity
    }

    /// Creates a glow item frame from persistent entity data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        let position = load.position;
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(GlowItemFrameEntityData::new()),
            block_pos: SyncMutex::new(BlockPos::new(
                position.x.floor() as i32,
                position.y.floor() as i32,
                position.z.floor() as i32,
            )),
            state: SyncMutex::new(FrameState::default()),
        }
    }

    /// Sets the framed item, matching vanilla by storing a single item.
    pub fn set_item(&self, item: ItemStack) {
        self.set_item_with_update(item, true);
    }

    /// Sets the framed item and optionally notifies nearby comparators.
    pub(crate) fn set_item_with_update(&self, mut item: ItemStack, update_comparators: bool) {
        if !item.is_empty() {
            item.set_count(1);
        }
        self.entity_data.lock().item_frame_mut().item.set(item);
        self.recalculate_position();
        if update_comparators && let Some(world) = self.level() {
            world.update_neighbor_for_output_signal(*self.block_pos.lock(), &vanilla_blocks::AIR);
        }
    }

    fn set_direction(&self, direction: Direction) {
        self.entity_data
            .lock()
            .hanging_entity_mut()
            .direction
            .set(direction);
        self.base
            .set_rotation(ItemFrameEntity::rotation_for_direction(direction));
        self.recalculate_position();
    }

    /// Plays one of the frame's own sounds at the frame.
    fn play_sound_at_frame(&self, sound: SoundEventRef) {
        let Some(world) = self.level() else {
            return;
        };
        world.play_sound_at(sound, SoundSource::Neutral, self.position(), 1.0, 1.0, None);
    }

    /// Tells the world the frame changed.
    ///
    /// Vanilla parity: the `gameEvent(BLOCK_CHANGE, player)` of
    /// `ItemFrame.interact`, plus the comparator update.
    fn frame_changed(&self, player: &Player) {
        let Some(world) = self.level() else {
            return;
        };
        world.game_event_at(
            &vanilla_game_events::BLOCK_CHANGE,
            self.position(),
            &GameEventContext::new(Some(player as &dyn Entity), None),
        );
        world.update_neighbor_for_output_signal(*self.block_pos.lock(), &vanilla_blocks::AIR);
    }

    fn recalculate_position(&self) {
        let block_pos = *self.block_pos.lock();
        let direction = *self.entity_data.lock().hanging_entity().direction.get();
        let position = ItemFrameEntity::frame_center(block_pos, direction);
        if let Err(error) = self.base.try_set_position(position) {
            panic!(
                "failed to commit glow item frame {} position recalculation: {error}",
                self.base.id()
            );
        }
        self.base
            .set_bounding_box(ItemFrameEntity::frame_bounding_box(
                block_pos,
                direction,
                self.has_framed_map(),
            ));
    }

    fn has_framed_map(&self) -> bool {
        self.entity_data.lock().item_frame().item.get().has(MAP_ID)
    }

    /// Returns the hitbox of a frame attached to `block_pos`.
    #[must_use]
    pub fn frame_bounding_box(
        block_pos: BlockPos,
        direction: Direction,
        has_framed_map: bool,
    ) -> WorldAabb {
        ItemFrameEntity::frame_bounding_box(block_pos, direction, has_framed_map)
    }
}

impl ItemFrame for GlowItemFrameEntity {
    fn direction(&self) -> Direction {
        *self.entity_data.lock().hanging_entity().direction.get()
    }

    fn analog_output(&self) -> i32 {
        let entity_data = self.entity_data.lock();
        let frame = entity_data.item_frame();
        if frame.item.get().is_empty() {
            0
        } else {
            *frame.rotation.get() % ROTATION_STEPS + 1
        }
    }
}

impl Entity for GlowItemFrameEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    /// Vanilla parity: `ItemFrame.getSlot`, whose one slot is what is hung in
    /// the frame.
    fn slot_item(&self, slot: i32) -> Option<ItemStack> {
        if slot == CONTENTS_SLOT {
            return Some(self.framed_item());
        }
        self.entity_slot_item(slot)
    }

    fn spawn_data(&self) -> i32 {
        direction_3d_data_value(*self.entity_data.lock().hanging_entity().direction.get())
    }

    /// Vanilla parity: `BlockAttachedEntity.thunderHit`, an empty override.
    /// Lightning passes straight through anything hung on a block.
    fn thunder_hit(&self, _world: &World, _bolt: &dyn Entity) {}

    fn hurt(&self, world: &World, source: &DamageSource, _amount: f32) -> bool {
        self.hurt_item_frame(world, source)
    }

    fn tick(&self) {
        let Some(world) = self.level() else {
            return;
        };
        if self.tick_count() % 100 != 0 || self.is_removed() || self.survives_frame(&world) {
            return;
        }
        let mut event = HangingBreakEvent::new(self.uuid(), "PHYSICS");
        world.fire_event(&mut event);
        if !event.is_cancelled() {
            self.set_removed(RemovalReason::Discarded);
            self.drop_item(&world, None);
        }
    }

    fn spawn_position(&self) -> DVec3 {
        let block_pos = *self.block_pos.lock();
        DVec3::new(
            f64::from(block_pos.x()),
            f64::from(block_pos.y()),
            f64::from(block_pos.z()),
        )
    }

    fn is_pickable(&self) -> bool {
        true
    }

    /// Fills the frame, or turns what is already in it.
    ///
    /// Vanilla parity: `ItemFrame.interact` with `GlowItemFrame`'s sounds.
    fn interact(
        &self,
        player: &Player,
        hand: InteractionHand,
        _location: DVec3,
    ) -> InteractionResult {
        let frame_is_empty = self.entity_data.lock().item_frame().item.get().is_empty();

        if frame_is_empty {
            let held = {
                let inventory = player.inventory.lock();
                let stack = inventory.get_item_in_hand(hand);
                stack.copy_with_count(1)
            };
            if held.is_empty() || self.is_removed() {
                return InteractionResult::Pass;
            }

            self.set_item(held);
            self.play_sound_at_frame(&sound_events::ENTITY_GLOW_ITEM_FRAME_ADD_ITEM);
            self.frame_changed(player);

            if !player.has_infinite_materials() {
                let mut inventory = player.inventory.lock();
                inventory.get_item_in_hand_mut(hand).shrink(1);
            }
            return InteractionResult::Success;
        }

        {
            let mut entity_data = self.entity_data.lock();
            let frame = entity_data.item_frame_mut();
            let next = (*frame.rotation.get() + 1).rem_euclid(ROTATION_STEPS);
            frame.rotation.set(next);
        }
        self.play_sound_at_frame(&sound_events::ENTITY_GLOW_ITEM_FRAME_ROTATE_ITEM);
        self.frame_changed(player);
        InteractionResult::Success
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let block_pos = *self.block_pos.lock();
        nbt.insert(
            "block_pos",
            NbtTag::IntArray(vec![block_pos.x(), block_pos.y(), block_pos.z()]),
        );

        let entity_data = self.entity_data.lock();
        let frame = entity_data.item_frame();
        let item = frame.item.get();
        if !item.is_empty() {
            nbt.insert("Item", item.to_nbt_tag_ref());
        }
        nbt.insert("ItemRotation", *frame.rotation.get() as i8);
        nbt.insert("ItemDropChance", self.drop_chance());
        nbt.insert(
            "Facing",
            direction_3d_data_value(*entity_data.hanging_entity().direction.get()) as i8,
        );
        nbt.insert("Invisible", 0_i8);
        nbt.insert("Fixed", i8::from(self.is_fixed()));
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        if let Some(block_pos) = nbt.int_array("block_pos")
            && block_pos.len() == 3
        {
            *self.block_pos.lock() = BlockPos::new(block_pos[0], block_pos[1], block_pos[2]);
        }

        if let Some(item_tag) = nbt.compound("Item")
            && let Some(item) = ItemStack::from_borrowed_compound(&item_tag)
        {
            self.set_item_with_update(item, false);
        }

        {
            let mut state = self.state.lock();
            if let Some(drop_chance) = nbt.float("ItemDropChance") {
                state.drop_chance = drop_chance;
            }
            if let Some(fixed) = nbt.byte("Fixed") {
                state.fixed = fixed != 0;
            }
        }

        if let Some(item_rotation) = nbt.byte("ItemRotation") {
            self.entity_data
                .lock()
                .item_frame_mut()
                .rotation
                .set(i32::from(item_rotation).rem_euclid(ROTATION_STEPS));
        }

        let facing = nbt
            .byte("Facing")
            .and_then(|value| direction_from_3d_data_value(i32::from(value)))
            .or_else(|| nbt.int("Facing").and_then(direction_from_3d_data_value));
        if let Some(direction) = facing {
            self.set_direction(direction);
        }

        self.recalculate_position();
    }
}

impl BlockAttached for GlowItemFrameEntity {
    fn drop_item(&self, world: &World, caused_by: Option<&SharedEntity>) {
        self.drop_broken_frame(world, caused_by);
    }
}

impl FrameLike for GlowItemFrameEntity {
    fn frame_direction(&self) -> Direction {
        self.direction()
    }

    fn frame_box(&self) -> WorldAabb {
        self.bounding_box()
    }

    fn frame_state(&self) -> &SyncMutex<FrameState> {
        &self.state
    }

    fn framed_item(&self) -> ItemStack {
        self.entity_data.lock().item_frame().item.get().clone()
    }

    fn clear_framed_item(&self) {
        self.set_item_with_update(ItemStack::empty(), true);
    }

    /// Vanilla parity: `GlowItemFrame.getFrameItemStack`.
    fn frame_item(&self) -> ItemRef {
        &vanilla_items::GLOW_ITEM_FRAME
    }

    fn play_frame_sound(&self, sound: SoundEventRef) {
        self.play_sound_at_frame(sound);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use foton_registry::{init_vanilla_registry, vanilla_entities, vanilla_items};

    fn glow_frame() -> GlowItemFrameEntity {
        init_vanilla_registry();
        GlowItemFrameEntity::new_attached(
            &vanilla_entities::GLOW_ITEM_FRAME,
            1,
            BlockPos::new(8, 64, 8),
            Direction::West,
            Weak::new(),
        )
    }

    /// A comparator beside a glow frame reads the same signal as one beside a
    /// plain frame. This is the whole reason the glow frame is a separate
    /// entity type rather than a block state: it has to answer redstone.
    #[test]
    fn a_glow_frame_reports_the_same_comparator_signal_as_a_plain_one() {
        let frame = glow_frame();
        assert_eq!(frame.analog_output(), 0);

        frame.set_item_with_update(ItemStack::new(&vanilla_items::ELYTRA), false);
        assert_eq!(frame.analog_output(), 1);
        frame
            .entity_data
            .lock()
            .item_frame_mut()
            .rotation
            .set(ROTATION_STEPS - 1);
        assert_eq!(frame.analog_output(), ROTATION_STEPS);
    }

    /// A glow frame writes the same NBT shape as a plain frame -- vanilla's
    /// `GlowItemFrame` overrides no save method at all -- so a world saved with
    /// one has to reload it with its item and facing intact.
    #[test]
    fn a_glow_frame_saves_the_item_frame_shape() {
        let frame = glow_frame();
        frame.set_item(ItemStack::new(&vanilla_items::ELYTRA));

        let mut nbt = NbtCompound::new();
        frame.save_additional(&mut nbt);

        assert_eq!(nbt.byte("Facing"), Some(4));
        assert_eq!(nbt.byte("ItemRotation"), Some(0));
        let Some(item) = nbt.compound("Item") else {
            panic!("glow item frame should save its framed item");
        };
        assert_eq!(item.int("count"), Some(1));
    }
}
