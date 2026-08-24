//! Block entity system for blocks that need additional data storage.
//!
//! Block entities provide additional data storage and functionality for blocks
//! that need more than what block state properties can offer (e.g., chests,
//! furnaces, signs, etc.).
//!
//! # Architecture
//!
//! Similar to the block/item behavior system, block entities use a registry
//! pattern:
//! - `BlockEntityRegistry` - maps `BlockEntityType` to factory functions
//! - `BlockEntityStorage` - stores block entities in a chunk
//!
//! # Usage
//!
//! ```ignore
//! use steel_core::block_entity::{init_block_entities, BLOCK_ENTITIES};
//!
//! // After registry is frozen, call once at startup:
//! init_block_entities();
//!
//! // Create a block entity:
//! let entity = BLOCK_ENTITIES.create(block_entity_type, pos, state);
//! ```

pub(crate) mod block_state_nbt;
pub mod entities;
mod randomizable_container;
mod registry;
mod storage;

use std::{
    ptr,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, AtomicI32, Ordering},
    },
};

use simdnbt::borrow::BaseNbtCompound as BorrowedNbtCompound;
use simdnbt::owned::NbtCompound;
use smallvec::SmallVec;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_utils::{BlockPos, BlockStateId, ErasedType, locks::SyncMutex};

pub use randomizable_container::ContainerLoot;
pub use registry::{BLOCK_ENTITIES, BlockEntityFactory, BlockEntityRegistry, init_block_entities};
pub(crate) use storage::{
    BlockEntityInsert, BlockEntityLookup, BlockEntityStorage, ClearedBlockEntities,
    DetachedBlockEntity, LifecycleDispatchers,
};

use steel_registry::vanilla_game_events;

use crate::behavior::BLOCK_BEHAVIORS;
use crate::inventory::lock::ContainerRef;
use crate::player::Player;
use crate::world::game_event::GameEventContext;

use crate::world::World;
use crate::world::game_event::SharedGameEventListener;

/// Erased block-state-selected ticker for one concrete block-entity type.
///
/// Vanilla obtains this callback from the owning block behavior. Keeping the
/// expected type with the callback preserves that selection boundary while
/// allowing Steel's world ticker to store heterogeneous entries.
#[derive(Clone, Copy)]
pub struct BlockEntityTicker {
    block_entity_type: BlockEntityTypeRef,
    tick: fn(&Arc<World>, BlockPos, BlockStateId, &dyn BlockEntity),
}

impl BlockEntityTicker {
    /// Creates a ticker for one block-entity type.
    #[must_use]
    pub const fn new(
        block_entity_type: BlockEntityTypeRef,
        tick: fn(&Arc<World>, BlockPos, BlockStateId, &dyn BlockEntity),
    ) -> Self {
        Self {
            block_entity_type,
            tick,
        }
    }

    /// Creates a state-selected ticker that dispatches to [`BlockEntity::tick`].
    #[must_use]
    pub const fn for_entity_tick(block_entity_type: BlockEntityTypeRef) -> Self {
        Self::new(block_entity_type, Self::tick_entity)
    }

    /// Creates the default entity callback only when Vanilla's requested type matches.
    #[must_use]
    pub fn for_matching_entity_tick(
        actual: BlockEntityTypeRef,
        expected: BlockEntityTypeRef,
    ) -> Option<Self> {
        ptr::eq(actual, expected).then(|| Self::for_entity_tick(expected))
    }

    /// Creates a custom callback only when Vanilla's requested type matches.
    #[must_use]
    pub fn for_matching_tick(
        actual: BlockEntityTypeRef,
        expected: BlockEntityTypeRef,
        tick: fn(&Arc<World>, BlockPos, BlockStateId, &dyn BlockEntity),
    ) -> Option<Self> {
        ptr::eq(actual, expected).then(|| Self::new(expected, tick))
    }

    /// Returns whether this ticker accepts the concrete block-entity type.
    #[must_use]
    pub fn accepts(self, block_entity_type: BlockEntityTypeRef) -> bool {
        ptr::eq(self.block_entity_type, block_entity_type)
    }

    pub(crate) fn tick(
        self,
        world: &Arc<World>,
        pos: BlockPos,
        state: BlockStateId,
        block_entity: &dyn BlockEntity,
    ) {
        (self.tick)(world, pos, state, block_entity);
    }

    fn tick_entity(
        world: &Arc<World>,
        _pos: BlockPos,
        _state: BlockStateId,
        block_entity: &dyn BlockEntity,
    ) {
        block_entity.tick(world);
    }
}

struct BlockEntityLifecycle {
    block_state: BlockStateId,
    events: SmallVec<[BlockEntityLifecycleEvent; 2]>,
    dispatching_events: bool,
}

#[derive(Clone, Copy)]
enum BlockEntityLifecycleEvent {
    SetRemoved,
    ClearRemoved,
    BlockStateChanged(BlockStateId),
}

/// Immutable block-entity identity and its short-lived lifecycle state.
///
/// Concrete block entities keep gameplay data behind their own focused locks.
/// The lifecycle lock is never held while invoking world callbacks.
pub struct BlockEntityBase {
    block_entity_type: BlockEntityTypeRef,
    level: Weak<World>,
    pos: BlockPos,
    /// Lock-free removal snapshot; lifecycle writers remain serialized below.
    removed: AtomicBool,
    lifecycle: SyncMutex<BlockEntityLifecycle>,
    /// How many players have this block's container open.
    ///
    /// Vanilla parity: `ContainerOpenersCounter`. It lives on the base rather
    /// than on each container block entity because the menu that has to
    /// increment it only ever holds a `BlockEntityBase`, and because the three
    /// things it drives -- a chest's open sound, a barrel's `open` state and a
    /// trapped chest's signal -- all hang off the block, not off the inventory.
    open_count: AtomicI32,
}

struct BlockEntityLifecycleDispatchGuard<'a> {
    base: &'a BlockEntityBase,
    armed: bool,
}

impl Drop for BlockEntityLifecycleDispatchGuard<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let mut lifecycle = self.base.lifecycle.lock();
        lifecycle.events.clear();
        lifecycle.dispatching_events = false;
    }
}

impl BlockEntityBase {
    /// Creates common metadata for one block entity.
    ///
    /// # Panics
    ///
    /// Panics if `block_state` is not accepted by `block_entity_type`.
    #[must_use]
    pub fn new(
        block_entity_type: BlockEntityTypeRef,
        level: Weak<World>,
        pos: BlockPos,
        block_state: BlockStateId,
    ) -> Self {
        assert!(
            block_entity_type.is_valid(block_state.get_block()),
            "invalid block entity {} state {} at {pos:?}",
            block_entity_type.key,
            block_state.get_block().key,
        );
        Self {
            block_entity_type,
            level,
            pos,
            removed: AtomicBool::new(false),
            open_count: AtomicI32::new(0),
            lifecycle: SyncMutex::new(BlockEntityLifecycle {
                block_state,
                events: SmallVec::new(),
                dispatching_events: false,
            }),
        }
    }

    #[must_use]
    const fn block_entity_type(&self) -> BlockEntityTypeRef {
        self.block_entity_type
    }

    #[must_use]
    const fn pos(&self) -> BlockPos {
        self.pos
    }

    /// Returns how many players have this container open.
    ///
    /// Vanilla parity: `ContainerOpenersCounter.getOpenerCount`.
    #[must_use]
    pub fn opener_count(&self) -> i32 {
        self.open_count.load(Ordering::Relaxed)
    }

    /// Records one more player looking inside.
    ///
    /// Vanilla parity: `ContainerOpenersCounter.incrementOpeners`. The block is
    /// only told when the count leaves zero, because that is when a chest lid
    /// rises and a barrel opens.
    pub fn increment_openers(&self) {
        let previous = self.open_count.fetch_add(1, Ordering::Relaxed);
        self.notify_openers_changed(previous, previous + 1);
    }

    /// Records one fewer.
    ///
    /// Vanilla parity: `ContainerOpenersCounter.decrementOpeners`, with one
    /// deliberate difference. Vanilla lets the count go negative and repairs it
    /// from `scheduleRecheck`, which recounts the players actually standing
    /// near the block; Steel has no such recheck yet, so a close for a menu the
    /// server already dropped would leave a chest needing that many extra opens
    /// before it looked open again. Clamping here cannot make a container look
    /// wrong, only stop it from getting stuck.
    pub fn decrement_openers(&self) {
        let previous = self.open_count.fetch_sub(1, Ordering::Relaxed);
        let current = (previous - 1).max(0);
        if previous <= 0 {
            self.open_count.store(0, Ordering::Relaxed);
            return;
        }
        self.notify_openers_changed(previous, current);
    }

    fn notify_openers_changed(&self, previous: i32, current: i32) {
        let Some(world) = self.level.upgrade() else {
            return;
        };
        let pos = self.pos;
        let state = world.get_block_state(pos);
        let behavior = BLOCK_BEHAVIORS.get_behavior(state.get_block());

        if previous == 0 && current > 0 {
            behavior.on_container_open(&world, pos, state);
            world.game_event(
                &vanilla_game_events::CONTAINER_OPEN,
                pos,
                &GameEventContext::new(None, None),
            );
        } else if previous > 0 && current == 0 {
            behavior.on_container_close(&world, pos, state);
            world.game_event(
                &vanilla_game_events::CONTAINER_CLOSE,
                pos,
                &GameEventContext::new(None, None),
            );
        }

        behavior.on_opener_count_changed(&world, pos, state, previous, current);
    }

    #[must_use]
    fn block_state(&self) -> BlockStateId {
        self.lifecycle.lock().block_state
    }

    fn queue_block_state_change(&self, state: BlockStateId) -> bool {
        assert!(
            self.block_entity_type.is_valid(state.get_block()),
            "invalid block entity {} state {} at {:?}",
            self.block_entity_type.key,
            state.get_block().key,
            self.pos,
        );
        let mut lifecycle = self.lifecycle.lock();
        if lifecycle.block_state == state {
            return false;
        }
        lifecycle.block_state = state;
        lifecycle
            .events
            .push(BlockEntityLifecycleEvent::BlockStateChanged(state));
        if lifecycle.dispatching_events {
            false
        } else {
            lifecycle.dispatching_events = true;
            true
        }
    }

    #[must_use]
    fn is_removed(&self) -> bool {
        self.removed.load(Ordering::Relaxed)
    }

    fn queue_set_removed(&self) -> bool {
        let mut lifecycle = self.lifecycle.lock();
        self.removed.store(true, Ordering::Relaxed);
        lifecycle.events.push(BlockEntityLifecycleEvent::SetRemoved);
        if lifecycle.dispatching_events {
            false
        } else {
            lifecycle.dispatching_events = true;
            true
        }
    }

    fn queue_clear_removed(&self) -> bool {
        let mut lifecycle = self.lifecycle.lock();
        if !self.removed.load(Ordering::Relaxed) {
            return false;
        }
        self.removed.store(false, Ordering::Relaxed);
        lifecycle
            .events
            .push(BlockEntityLifecycleEvent::ClearRemoved);
        if lifecycle.dispatching_events {
            false
        } else {
            lifecycle.dispatching_events = true;
            true
        }
    }

    #[must_use]
    fn level(&self) -> Option<Arc<World>> {
        self.level.upgrade()
    }

    pub(crate) fn set_changed(&self) {
        let Some(world) = self.level() else {
            return;
        };
        let state = self.block_state();
        world.block_entity_changed(self.pos);
        if !state.is_air() {
            world.update_neighbor_for_output_signal(self.pos, state.get_block());
        }
    }

    pub(crate) fn is_valid_container_for(&self, player: &Player) -> bool {
        if self.is_removed() {
            return false;
        }
        let Some(world) = self.level() else {
            return false;
        };
        let Some(current) = world.get_block_entity(self.pos) else {
            return false;
        };
        ptr::eq(current.base(), self)
            && player.is_within_block_interaction_range_with_buffer(self.pos, 4.0)
    }
}

/// Trait for all block entities.
///
/// Block entities are attached to specific blocks in the world and provide
/// additional data storage beyond what block states can hold. Concrete
/// implementations must claim a unique [`steel_utils::DowncastTypeKey`] through
/// [`steel_utils::DowncastType`].
pub trait BlockEntity: ErasedType + Send + Sync {
    /// Returns the common metadata owned by this block entity.
    fn base(&self) -> &BlockEntityBase;

    /// Returns the type of this block entity.
    fn get_type(&self) -> BlockEntityTypeRef {
        self.base().block_entity_type()
    }

    /// Returns the position of this block entity in the world.
    fn get_block_pos(&self) -> BlockPos {
        self.base().pos()
    }

    /// Returns the current block state associated with this entity.
    fn get_block_state(&self) -> BlockStateId {
        self.base().block_state()
    }

    /// Returns whether this entity's registered type accepts `state`.
    ///
    /// Mirrors Vanilla `BlockEntity.isValidBlockState`.
    fn is_valid_block_state(&self, state: BlockStateId) -> bool {
        self.get_type().is_valid(state.get_block())
    }

    /// Called after the cached block state changes.
    ///
    /// Storage and section locks are not held during this callback. This is Steel's staged
    /// equivalent of Vanilla block entities overriding `setBlockState`; implementations should
    /// derive any cached fields from `state` here.
    fn on_block_state_changed(&self, _state: BlockStateId) {}

    /// Called after each invocation that marks this entity removed.
    ///
    /// Storage locks are not held during this callback. This mirrors Vanilla overrides of
    /// `setRemoved`, which run even if the entity was already marked removed.
    fn on_set_removed(&self) {}

    /// Called after this entity transitions from removed back to active.
    ///
    /// Storage locks are not held during this callback.
    fn on_clear_removed(&self) {}

    /// Called when the block entity's data changes.
    ///
    /// Marks the containing chunk as dirty so changes are persisted to disk.
    fn set_changed(&self) {
        self.base().set_changed();
    }

    /// Gets the world reference if still valid.
    ///
    /// Block entities receive a `Weak<World>` at construction time.
    fn get_level(&self) -> Option<Arc<World>> {
        self.base().level()
    }

    /// Handles a block event delegated by the owning block behavior.
    ///
    /// Mirrors Vanilla `BlockEntity.triggerEvent`.
    fn trigger_event(&self, _param_a: i32, _param_b: i32) -> bool {
        false
    }

    /// Called before the block entity is removed to handle side effects.
    ///
    /// For example, containers should drop their contents here.
    ///
    /// # Arguments
    /// * `pos` - The position of the block entity
    /// * `state` - The block state being removed
    #[expect(
        unused_variables,
        reason = "default trait impl; parameters used by overrides"
    )]
    fn pre_remove_side_effects(&self, pos: BlockPos, state: BlockStateId) {
        // Default: no side effects
    }

    /// Loads additional data from NBT.
    ///
    /// Called when loading the block entity from disk or receiving initial
    /// chunk data from the server.
    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>);

    /// Saves additional data to NBT.
    ///
    /// Called when saving the block entity to disk.
    fn save_additional(&self, nbt: &mut NbtCompound);

    /// Saves only entity-specific data, excluding vanilla type and position metadata.
    fn save_custom_only(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        self.save_additional(&mut nbt);
        for key in ["id", "x", "y", "z"] {
            while nbt.remove(key).is_some() {}
        }
        nbt
    }

    /// Saves command-visible data together with vanilla block-entity metadata.
    fn save_with_full_metadata(&self) -> NbtCompound {
        // TODO: Include stored block-entity components once Steel has Vanilla's
        // block-entity component foundation. NBT predicates targeting the
        // `components` field cannot match exactly until then.
        let mut nbt = self.save_custom_only();
        let pos = self.get_block_pos();
        nbt.insert("id", self.get_type().key.to_string());
        nbt.insert("x", pos.x());
        nbt.insert("y", pos.y());
        nbt.insert("z", pos.z());
        nbt
    }

    /// Returns the NBT data to send to clients for initial sync.
    ///
    /// This is included in the chunk data packet when the chunk is first sent.
    /// Return `None` if no client sync is needed.
    fn get_update_tag(&self) -> Option<NbtCompound> {
        None
    }

    /// Called every game tick for ticking block entities.
    ///
    /// The live block behavior selects this callback through its block-entity
    /// ticker, matching Vanilla's state-owned ticker selection.
    #[expect(
        unused_variables,
        reason = "default trait impl; parameter used by overrides"
    )]
    fn tick(&self, world: &Arc<World>) {}

    /// Returns the independently lockable container capability owned by this entity.
    fn container_ref(&self) -> Option<ContainerRef> {
        None
    }

    /// Returns this entity's fixed game-event listener, if it provides one.
    ///
    /// Mirrors Vanilla `GameEventListener.Provider.getListener`. The owning block behavior keeps
    /// final selection authority through `BlockBehavior::get_game_event_listener`.
    fn game_event_listener(&self) -> Option<SharedGameEventListener> {
        None
    }
}

/// Final block-entity common-state operations.
///
/// This blanket implementation prevents concrete entities from replacing metadata transitions.
/// Custom behavior belongs in the corresponding `BlockEntity::on_*` hook, which runs after the
/// update without storage or section locks.
pub trait BlockEntityLifecycleExt: BlockEntity {
    /// Returns whether this block entity has been marked for removal.
    fn is_removed(&self) -> bool {
        self.base().is_removed()
    }

    /// Updates the cached block state and orders its callback.
    fn set_block_state(&self, state: BlockStateId) {
        if self.base().queue_block_state_change(state) {
            self.dispatch_lifecycle_events();
        }
    }

    /// Marks this block entity as removed and orders its lifecycle callback.
    fn set_removed(&self) {
        if self.base().queue_set_removed() {
            self.dispatch_lifecycle_events();
        }
    }

    /// Reactivates this block entity and orders its lifecycle callback when the flag changed.
    fn clear_removed(&self) {
        if self.base().queue_clear_removed() {
            self.dispatch_lifecycle_events();
        }
    }

    /// Drains lifecycle callbacks in flag-update order without retaining the lifecycle lock.
    ///
    /// A callback may re-enter the entity. Its event is appended and drained by the active
    /// dispatcher rather than recursively invoking another callback.
    fn dispatch_lifecycle_events(&self) {
        let mut guard = BlockEntityLifecycleDispatchGuard {
            base: self.base(),
            armed: true,
        };
        loop {
            let event = {
                let mut lifecycle = self.base().lifecycle.lock();
                if lifecycle.events.is_empty() {
                    lifecycle.dispatching_events = false;
                    guard.armed = false;
                    return;
                }
                lifecycle.events.remove(0)
            };
            match event {
                BlockEntityLifecycleEvent::SetRemoved => self.on_set_removed(),
                BlockEntityLifecycleEvent::ClearRemoved => self.on_clear_removed(),
                BlockEntityLifecycleEvent::BlockStateChanged(state) => {
                    self.on_block_state_changed(state);
                }
            }
        }
    }
}

impl<T: BlockEntity + ?Sized> BlockEntityLifecycleExt for T {}

/// A stable shared block entity without a whole-object mutex.
pub type SharedBlockEntity = Arc<dyn BlockEntity>;

#[cfg(test)]
mod tests {
    use std::sync::Weak;

    use steel_registry::{init_vanilla_registry, vanilla_block_entity_types, vanilla_blocks};

    use super::BlockEntityBase;

    /// Builds a base with no world behind it.
    ///
    /// `notify_openers_changed` gives up when the level is gone, so this
    /// exercises the count itself without needing a world to look blocks up in.
    fn detached_chest() -> BlockEntityBase {
        init_vanilla_registry();
        BlockEntityBase::new(
            &vanilla_block_entity_types::CHEST,
            Weak::new(),
            steel_utils::BlockPos::new(0, 0, 0),
            vanilla_blocks::CHEST.default_state(),
        )
    }

    #[test]
    fn openers_count_up_and_back_down() {
        let base = detached_chest();
        assert_eq!(base.opener_count(), 0);

        base.increment_openers();
        base.increment_openers();
        assert_eq!(base.opener_count(), 2);

        base.decrement_openers();
        assert_eq!(base.opener_count(), 1);
        base.decrement_openers();
        assert_eq!(base.opener_count(), 0);
    }

    /// A close with nothing open must not push the count negative.
    ///
    /// It can happen for real: a client may send a container close for a menu
    /// the server already dropped. If that took the count below zero the chest
    /// would need as many extra opens to look open again, and a trapped chest
    /// would go quiet while somebody was still inside it.
    #[test]
    fn closing_what_was_never_open_does_not_go_negative() {
        let base = detached_chest();

        base.decrement_openers();
        base.decrement_openers();
        assert_eq!(base.opener_count(), 0);

        base.increment_openers();
        assert_eq!(
            base.opener_count(),
            1,
            "one player opening it must show as exactly one"
        );
    }
}
