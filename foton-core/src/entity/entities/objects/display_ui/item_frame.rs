//! The item frame.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::data_components::vanilla_components::MAP_ID;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::items::ItemRef;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_damage_type_tags::DamageTypeTag;
use foton_registry::vanilla_entity_data::ItemFrameEntityData;
use foton_registry::vanilla_game_rules::ENTITY_DROPS;
use foton_registry::{sound_events, vanilla_blocks, vanilla_game_events, vanilla_items};
use foton_utils::locks::SyncMutex;
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, Direction, DowncastType, DowncastTypeKey, WorldAabb, axis::Axis};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};

use crate::behavior::InteractionResult;
use crate::entity::block_attached::{caused_by_entity, drop_would_be_wasted};
use crate::entity::damage::DamageSource;
use crate::entity::{
    BlockAttached, Entity, EntityBase, EntityBaseLoad, EntityBaseState, EntitySyncedData,
    ItemFrame, SharedEntity,
};
use crate::inventory::slot_ranges::CONTENTS_SLOT;
use crate::physics::{WorldCollisionProvider, has_block_collision};
use crate::player::Player;
use crate::world::World;
use crate::world::game_event::GameEventContext;
use foton_utils::types::GameType;

/// An item frame.
///
/// Vanilla parity: `ItemFrame`. A frame holds one item, turns it eight ways,
/// and reports the rotation to a comparator. Broken frames drop their contents
/// according to Vanilla's fixed, creative and drop-chance rules; support is
/// checked periodically by the entity tick.
#[entity_behavior(class = "ItemFrame")]
pub struct ItemFrameEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<ItemFrameEntityData>,
    block_pos: SyncMutex<BlockPos>,
    state: SyncMutex<FrameState>,
}

/// The two saved fields a frame keeps outside its synced data.
///
/// Vanilla parity: `ItemFrame.fixed` and `ItemFrame.dropChance`.
pub(super) struct FrameState {
    pub(super) fixed: bool,
    pub(super) drop_chance: f32,
}

impl Default for FrameState {
    fn default() -> Self {
        Self {
            fixed: false,
            drop_chance: 1.0,
        }
    }
}

/// The half of `ItemFrame` that the glow frame inherits in vanilla.
///
/// Foton's two frames are separate structs rather than a subclass pair, so what
/// vanilla gets from `extends ItemFrame` lives here: the fixed flag, the drop
/// chance, and the damage rules built on them. An implementation supplies the
/// five accessors its own storage decides.
pub(super) trait FrameLike: BlockAttached {
    fn frame_direction(&self) -> Direction;
    fn frame_box(&self) -> WorldAabb;
    /// The saved fields outside the synced data.
    fn frame_state(&self) -> &SyncMutex<FrameState>;

    /// Vanilla parity: `ItemFrame.getItem`.
    fn framed_item(&self) -> ItemStack;

    /// Vanilla parity: the `setItem(ItemStack.EMPTY)` of `ItemFrame.dropItem`.
    fn clear_framed_item(&self);

    /// Vanilla parity: `ItemFrame.getFrameItemStack`, which the glow frame
    /// overrides with the glowing one.
    fn frame_item(&self) -> ItemRef;

    /// Plays one of the frame's own sounds at the frame.
    fn play_frame_sound(&self, sound: SoundEventRef);

    /// Returns whether this frame refuses to be emptied or broken.
    ///
    /// Vanilla parity: the `fixed` field, set by structures so a frame that is
    /// part of the scenery survives a wandering skeleton.
    fn is_fixed(&self) -> bool {
        self.frame_state().lock().fixed
    }

    fn survives_frame(&self, world: &Arc<World>) -> bool {
        let pop_box = self.frame_box();
        if has_block_collision(&WorldCollisionProvider::new(world), pop_box) {
            return false;
        }
        let support = pop_box
            .translate(self.frame_direction().offset_vec().as_dvec3() * -0.5)
            .deflate(1.0e-7);
        let min = BlockPos::new(
            support.min_x().floor() as i32,
            support.min_y().floor() as i32,
            support.min_z().floor() as i32,
        );
        let max = BlockPos::new(
            support.max_x().floor() as i32,
            support.max_y().floor() as i32,
            support.max_z().floor() as i32,
        );
        BlockPos::between_closed(min, max).all(|pos| {
            let state = world.get_block_state(pos);
            state.is_solid()
                || state.get_block() == &vanilla_blocks::REPEATER
                || state.get_block() == &vanilla_blocks::COMPARATOR
        })
    }

    /// Returns how often the framed item survives the frame breaking.
    ///
    /// Vanilla parity: the `dropChance` field, one by default.
    fn drop_chance(&self) -> f32 {
        self.frame_state().lock().drop_chance
    }

    /// Drops what a broken or emptied frame leaves behind.
    ///
    /// Vanilla parity: the private `ItemFrame.dropItem(level, causedBy, withFrame)`.
    /// A fixed frame drops nothing and keeps its item; a creative player gets
    /// nothing back; and the framed item rolls against its own drop chance,
    /// which is how a map in a structure frame stays put.
    ///
    /// Not implemented: `removeFramedMap`, the map-tracking bookkeeping. Foton
    /// tracks no frames on a map, so there is nothing to remove.
    fn drop_frame_contents(
        &self,
        world: &World,
        caused_by: Option<&SharedEntity>,
        with_frame: bool,
    ) {
        if self.is_fixed() {
            return;
        }

        let item = self.framed_item();
        self.clear_framed_item();

        if !world.get_game_rule(&ENTITY_DROPS) || drop_would_be_wasted(caused_by) {
            return;
        }
        if with_frame {
            self.spawn_at_location(ItemStack::new(self.frame_item()), 0.0);
        }
        if !item.is_empty() && rand::random::<f32>() < self.drop_chance() {
            self.spawn_at_location(item, 0.0);
        }
    }

    /// Vanilla parity: `ItemFrame.dropItem(level, causedBy)`, the public one,
    /// which is the break rather than the emptying.
    fn drop_broken_frame(&self, world: &World, caused_by: Option<&SharedEntity>) {
        self.play_frame_sound(&sound_events::ENTITY_ITEM_FRAME_BREAK);
        self.drop_frame_contents(world, caused_by, true);
        self.frame_block_change(caused_by);
    }

    /// Vanilla parity: `ItemFrame.hurtServer`.
    ///
    /// A punch on a frame holding something empties it and leaves the frame on
    /// the wall; a punch on an empty one takes the frame down. An explosion
    /// skips the emptying and always breaks it.
    fn hurt_item_frame(&self, world: &World, source: &DamageSource) -> bool {
        if self.is_fixed() {
            return can_hurt_when_fixed(world, source) && self.hurt_block_attached(world, source);
        }
        if self.is_invulnerable_to_base(source) {
            return false;
        }
        if source.is(&DamageTypeTag::IS_EXPLOSION) || self.framed_item().is_empty() {
            return self.hurt_block_attached(world, source);
        }

        let caused_by = caused_by_entity(world, source);
        self.drop_frame_contents(world, caused_by.as_ref(), false);
        self.frame_block_change(caused_by.as_ref());
        self.play_frame_sound(&sound_events::ENTITY_ITEM_FRAME_REMOVE_ITEM);
        true
    }

    /// Vanilla parity: the `gameEvent(BLOCK_CHANGE, causedBy)` both drop paths
    /// end with.
    fn frame_block_change(&self, caused_by: Option<&SharedEntity>) {
        let Some(world) = self.level() else {
            return;
        };
        world.game_event_at(
            &vanilla_game_events::BLOCK_CHANGE,
            self.position(),
            &GameEventContext::new(caused_by.map(AsRef::as_ref), None),
        );
    }
}

/// Vanilla parity: `ItemFrame.canHurtWhenFixed`.
fn can_hurt_when_fixed(world: &World, source: &DamageSource) -> bool {
    if source.bypasses_invulnerability() {
        return true;
    }
    caused_by_entity(world, source).is_some_and(|entity| {
        entity
            .as_player()
            .is_some_and(|player| player.game_mode() == GameType::Creative)
    })
}

// SAFETY: This key is owned by Foton and uniquely identifies `ItemFrameEntity`.
unsafe impl DowncastType for ItemFrameEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/item_frame");
}

/// How many ways round a framed item can sit.
///
/// Vanilla parity: the `% 8` shared by `ItemFrame.setRotation` and its
/// comparator output.
const ROTATION_STEPS: i32 = 8;

impl ItemFrameEntity {
    /// Creates a fresh item frame from the generic entity factory path.
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

    /// Creates a fresh item frame attached to `block_pos`.
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
                    Self::frame_center(block_pos, direction),
                    entity_type.dimensions,
                    Self::frame_bounding_box(block_pos, direction, false),
                )
                .with_rotation(Self::rotation_for_direction(direction)),
                world,
            ),
            entity_type,
            entity_data: SyncMutex::new(ItemFrameEntityData::new()),
            block_pos: SyncMutex::new(block_pos),
            state: SyncMutex::new(FrameState::default()),
        };
        entity
            .entity_data
            .lock()
            .hanging_entity
            .direction
            .set(direction);
        entity
    }

    /// Creates an item frame from persistent entity data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        let position = load.position;
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(ItemFrameEntityData::new()),
            block_pos: SyncMutex::new(BlockPos::new(
                position.x.floor() as i32,
                position.y.floor() as i32,
                position.z.floor() as i32,
            )),
            state: SyncMutex::new(FrameState::default()),
        }
    }

    /// Returns the item currently displayed in the frame.
    pub fn framed_item(&self) -> ItemStack {
        self.entity_data.lock().item.get().clone()
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
        self.entity_data.lock().item.set(item);
        self.recalculate_position();
        if update_comparators && let Some(world) = self.level() {
            world.update_neighbor_for_output_signal(*self.block_pos.lock(), &vanilla_blocks::AIR);
        }
    }

    /// Returns whether the frame still has valid support and no collision.
    #[must_use]
    pub fn survives(&self) -> bool {
        let Some(world) = self.level() else {
            return false;
        };
        self.survives_frame(&world)
    }

    pub fn set_direction(&self, direction: Direction) {
        self.entity_data
            .lock()
            .hanging_entity
            .direction
            .set(direction);
        self.base
            .set_rotation(Self::rotation_for_direction(direction));
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
    /// `ItemFrame.interact`, plus the comparator update -- a frame's signal is
    /// its rotation, so turning the item is a redstone change.
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
        let direction = *self.entity_data.lock().hanging_entity.direction.get();
        let position = Self::frame_center(block_pos, direction);
        if let Err(error) = self.base.try_set_position(position) {
            panic!(
                "failed to commit item frame {} position recalculation: {error}",
                self.base.id()
            );
        }
        self.base.set_bounding_box(Self::frame_bounding_box(
            block_pos,
            direction,
            self.has_framed_map(),
        ));
    }

    fn has_framed_map(&self) -> bool {
        self.entity_data.lock().item.get().has(MAP_ID)
    }

    /// Returns where a frame attached to `block_pos` sits.
    ///
    /// Shared with [`super::GlowItemFrameEntity`], which is the same geometry.
    pub(super) fn frame_center(block_pos: BlockPos, direction: Direction) -> DVec3 {
        let off = direction.offset_vec().as_dvec3() * 0.46875;
        block_pos.0.as_dvec3() + DVec3::splat(0.5) - off
    }

    /// Returns the rotation a frame facing `direction` is drawn at.
    pub(super) fn rotation_for_direction(direction: Direction) -> (f32, f32) {
        if direction.is_horizontal() {
            (f32::from(direction_2d_data_value(direction)) * 90.0, 0.0)
        } else {
            let pitch = match direction {
                Direction::Up => -90.0,
                Direction::Down => 90.0,
                Direction::North | Direction::South | Direction::West | Direction::East => 0.0,
            };
            (0.0, pitch)
        }
    }

    /// Returns the hitbox of a frame attached to `block_pos`.
    pub(super) fn frame_bounding_box(
        block_pos: BlockPos,
        direction: Direction,
        has_framed_map: bool,
    ) -> WorldAabb {
        let center = Self::frame_center(block_pos, direction);
        let size = if has_framed_map { 1.0 } else { 0.75 };
        let x_size = if direction.axis() == Axis::X {
            0.0625
        } else {
            size
        };
        let y_size = if direction.axis() == Axis::Y {
            0.0625
        } else {
            size
        };
        let z_size = if direction.axis() == Axis::Z {
            0.0625
        } else {
            size
        };
        WorldAabb::new(
            center.x - x_size / 2.0,
            center.y - y_size / 2.0,
            center.z - z_size / 2.0,
            center.x + x_size / 2.0,
            center.y + y_size / 2.0,
            center.z + z_size / 2.0,
        )
    }
}

impl ItemFrame for ItemFrameEntity {
    fn direction(&self) -> Direction {
        *self.entity_data.lock().hanging_entity.direction.get()
    }

    fn analog_output(&self) -> i32 {
        let entity_data = self.entity_data.lock();
        if entity_data.item.get().is_empty() {
            0
        } else {
            *entity_data.rotation.get() % 8 + 1
        }
    }
}

impl Entity for ItemFrameEntity {
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
        direction_3d_data_value(*self.entity_data.lock().hanging_entity.direction.get())
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
        let mut event = crate::event::HangingBreakEvent::new(self.uuid(), "PHYSICS");
        world.fire_event(&mut event);
        if !event.is_cancelled() {
            self.set_removed(crate::entity::RemovalReason::Discarded);
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
    /// Vanilla parity: `ItemFrame.interact`. The same click does two different
    /// things depending on whether the frame is empty, which is what lets a
    /// player set a rotation without a second gesture -- and what a comparator
    /// beside the frame is reading.
    fn interact(
        &self,
        player: &Player,
        hand: InteractionHand,
        _location: DVec3,
    ) -> InteractionResult {
        let frame_is_empty = self.entity_data.lock().item.get().is_empty();

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
            self.play_sound_at_frame(&sound_events::ENTITY_ITEM_FRAME_ADD_ITEM);
            self.frame_changed(player);

            if !player.has_infinite_materials() {
                let mut inventory = player.inventory.lock();
                inventory.get_item_in_hand_mut(hand).shrink(1);
            }
            return InteractionResult::Success;
        }

        {
            let mut entity_data = self.entity_data.lock();
            let next = (*entity_data.rotation.get() + 1).rem_euclid(ROTATION_STEPS);
            entity_data.rotation.set(next);
        }
        self.play_sound_at_frame(&sound_events::ENTITY_ITEM_FRAME_ROTATE_ITEM);
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
        let item = entity_data.item.get();
        if !item.is_empty() {
            nbt.insert("Item", item.to_nbt_tag_ref());
        }
        nbt.insert("ItemRotation", *entity_data.rotation.get() as i8);
        nbt.insert("ItemDropChance", self.drop_chance());
        nbt.insert(
            "Facing",
            direction_3d_data_value(*entity_data.hanging_entity.direction.get()) as i8,
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
                .rotation
                .set(i32::from(item_rotation).rem_euclid(8));
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

impl BlockAttached for ItemFrameEntity {
    fn drop_item(&self, world: &World, caused_by: Option<&SharedEntity>) {
        self.drop_broken_frame(world, caused_by);
    }
}

impl FrameLike for ItemFrameEntity {
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
        self.entity_data.lock().item.get().clone()
    }

    fn clear_framed_item(&self) {
        self.set_item_with_update(ItemStack::empty(), true);
    }

    fn frame_item(&self) -> ItemRef {
        &vanilla_items::ITEM_FRAME
    }

    fn play_frame_sound(&self, sound: SoundEventRef) {
        self.play_sound_at_frame(sound);
    }
}

/// Vanilla parity: `Direction.get3DDataValue`.
pub(super) const fn direction_3d_data_value(direction: Direction) -> i32 {
    match direction {
        Direction::Down => 0,
        Direction::Up => 1,
        Direction::North => 2,
        Direction::South => 3,
        Direction::West => 4,
        Direction::East => 5,
    }
}

/// Vanilla parity: `Direction.from3DDataValue`, with the out-of-range case
/// rejected rather than wrapped.
pub(super) const fn direction_from_3d_data_value(value: i32) -> Option<Direction> {
    match value {
        0 => Some(Direction::Down),
        1 => Some(Direction::Up),
        2 => Some(Direction::North),
        3 => Some(Direction::South),
        4 => Some(Direction::West),
        5 => Some(Direction::East),
        _ => None,
    }
}

/// Vanilla parity: `Direction.get2DDataValue`.
pub(super) const fn direction_2d_data_value(direction: Direction) -> u8 {
    match direction {
        Direction::South | Direction::Down | Direction::Up => 0,
        Direction::West => 1,
        Direction::North => 2,
        Direction::East => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use foton_registry::{init_vanilla_registry, vanilla_damage_types, vanilla_entities};
    use foton_utils::{ChunkPos, Downcast as _, Identifier};
    use std::string::ToString;
    use std::sync::Arc;

    use crate::behavior::init_behaviors;
    use crate::entity::entities::ItemEntity;
    use crate::entity::next_entity_id;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    #[test]
    fn item_frame_persists_structure_marker_state() {
        let frame = ItemFrameEntity::new_attached(
            &vanilla_entities::ITEM_FRAME,
            1,
            BlockPos::new(12, 80, 14),
            Direction::West,
            Weak::new(),
        );
        frame.set_item(ItemStack::new(&vanilla_items::ELYTRA));

        let mut nbt = NbtCompound::new();
        frame.save_additional(&mut nbt);

        assert_eq!(nbt.byte("Facing"), Some(4));
        assert_eq!(nbt.byte("ItemRotation"), Some(0));
        assert_eq!(nbt.float("ItemDropChance"), Some(1.0));
        assert_eq!(nbt.byte("Invisible"), Some(0));
        assert_eq!(nbt.byte("Fixed"), Some(0));
        let Some(item) = nbt.compound("Item") else {
            panic!("item frame should save framed item");
        };
        assert_eq!(
            item.string("id").map(ToString::to_string),
            Some("minecraft:elytra".to_owned())
        );
        assert_eq!(item.int("count"), Some(1));
    }

    #[test]
    fn item_frame_is_pickable_like_vanilla() {
        let frame = ItemFrameEntity::new_attached(
            &vanilla_entities::ITEM_FRAME,
            1,
            BlockPos::new(12, 80, 14),
            Direction::West,
            Weak::new(),
        );

        assert!(frame.is_pickable());
    }

    #[test]
    fn analog_output_uses_item_presence_and_rotation() {
        let frame = ItemFrameEntity::new_attached(
            &vanilla_entities::ITEM_FRAME,
            1,
            BlockPos::new(12, 80, 14),
            Direction::West,
            Weak::new(),
        );
        assert_eq!(frame.analog_output(), 0);

        frame.set_item_with_update(ItemStack::new(&vanilla_items::ELYTRA), false);
        assert_eq!(frame.analog_output(), 1);
        frame.entity_data.lock().rotation.set(7);
        assert_eq!(frame.analog_output(), 8);
    }

    /// The two halves of `ItemFrame.hurtServer`: a full frame gives up its item
    /// and stays on the wall, and the next hit takes the frame itself down.
    #[test]
    fn a_punch_empties_a_full_frame_and_the_next_one_breaks_it() {
        let world = frame_world("item_frame_two_stage_break");
        let frame = frame_in(&world, BlockPos::new(8, 64, 8));
        frame.set_item(ItemStack::new(&vanilla_items::ELYTRA));

        assert!(frame.hurt(&world, &punch(), 1.0));
        assert!(!frame.is_removed(), "the frame stays up while it empties");
        assert!(dropped_items(&world).contains(&vanilla_items::ELYTRA.key.clone()));

        assert!(frame.hurt(&world, &punch(), 1.0));
        assert!(frame.is_removed(), "an empty frame comes off the wall");
        assert!(dropped_items(&world).contains(&vanilla_items::ITEM_FRAME.key.clone()));
    }

    /// Vanilla parity: `ItemFrame.shouldDamageDropItem`, which sends an
    /// explosion straight down the breaking path even on a full frame.
    #[test]
    fn an_explosion_takes_the_whole_frame_at_once() {
        let world = frame_world("item_frame_explosion_break");
        let frame = frame_in(&world, BlockPos::new(8, 64, 8));
        frame.set_item(ItemStack::new(&vanilla_items::ELYTRA));

        let blast = DamageSource::environment(&vanilla_damage_types::EXPLOSION);
        assert!(frame.hurt(&world, &blast, 6.0));

        assert!(frame.is_removed());
        let dropped = dropped_items(&world);
        assert!(dropped.contains(&vanilla_items::ITEM_FRAME.key.clone()));
        assert!(dropped.contains(&vanilla_items::ELYTRA.key.clone()));
    }

    /// Vanilla parity: the `fixed` field, which structures set so their scenery
    /// survives whatever wanders past.
    #[test]
    fn a_fixed_frame_shrugs_off_an_ordinary_hit() {
        let world = frame_world("item_frame_fixed");
        let frame = frame_in(&world, BlockPos::new(8, 64, 8));
        frame.set_item(ItemStack::new(&vanilla_items::ELYTRA));
        frame.state.lock().fixed = true;

        assert!(!frame.hurt(&world, &punch(), 1.0));
        assert!(!frame.is_removed());
        assert!(dropped_items(&world).is_empty());
    }

    fn frame_world(key: &'static str) -> Arc<World> {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world(key);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        world
    }

    fn frame_in(world: &Arc<World>, block_pos: BlockPos) -> Arc<ItemFrameEntity> {
        let frame = Arc::new(ItemFrameEntity::new_attached(
            &vanilla_entities::ITEM_FRAME,
            next_entity_id(),
            block_pos,
            Direction::West,
            Arc::downgrade(world),
        ));
        world
            .try_add_entity(frame.clone())
            .expect("the frame's chunk is loaded");
        frame
    }

    /// A hit with no entity behind it, which is the shape the tests need: the
    /// mob-griefing guard only fires for a mob, and the creative check only for
    /// a player.
    fn punch() -> DamageSource {
        DamageSource::environment(&vanilla_damage_types::GENERIC)
    }

    fn dropped_items(world: &Arc<World>) -> Vec<Identifier> {
        let everywhere = WorldAabb::new(-32.0, 0.0, -32.0, 32.0, 128.0, 32.0);
        world
            .get_entities_in_aabb(&everywhere)
            .iter()
            .filter_map(|entity| {
                Some(
                    entity
                        .as_ref()
                        .downcast_ref::<ItemEntity>()?
                        .get_item()
                        .item()
                        .key
                        .clone(),
                )
            })
            .collect()
    }
}
