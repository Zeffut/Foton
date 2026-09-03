//! Hopper block entity.
//!
//! Vanilla parity: `HopperBlockEntity`. A hopper pushes one item per transfer
//! into whatever container it points at, pulls one item out of the container
//! above it, and swallows item entities resting in its bowl. Everything it does
//! is one item at a time, gated by an eight-tick cooldown.

use std::{
    mem,
    sync::{Arc, Weak},
};

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_block_entity_types;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_utils::{
    BlockPos, BlockStateId, Direction, Downcast as _, DowncastType, DowncastTypeKey, WorldAabb,
    locks::SyncMutex,
};
use glam::DVec3;
use simdnbt::ToNbtTag as _;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use smallvec::smallvec;

use crate::behavior::BLOCK_BEHAVIORS;
use crate::block_entity::{
    BlockEntity, BlockEntityBase, BlockEntityName, ContainerLoot, ImplicitComponentInput,
};
use crate::entity::entities::ItemEntity;
use crate::entity::{RemovalReason, SharedEntity};
use crate::inventory::container::{Container, SlotsForFace};
use crate::inventory::lock::{
    AttachedContainers, ContainerId, ContainerLockGuard, ContainerRef, SharedContainer,
};
use crate::world::{LevelReader as _, World};
use foton_registry::data_components::DataComponentMap;
use text_components::TextComponent;

/// Slots in a hopper.
///
/// Vanilla parity: `HopperBlockEntity.HOPPER_CONTAINER_SIZE`.
pub const HOPPER_SLOTS: usize = 5;

/// Ticks a hopper waits between transfers.
///
/// Vanilla parity: `HopperBlockEntity.MOVE_ITEM_SPEED`.
pub const MOVE_ITEM_SPEED: i32 = 8;

/// Cooldown value of a hopper that has never moved anything.
///
/// Vanilla parity: `HopperBlockEntity.NO_COOLDOWN_TIME`.
const NO_COOLDOWN_TIME: i32 = -1;

/// Lowest point of the suction box above the hopper, in blocks.
///
/// Vanilla parity: the `11.0` of `Hopper.SUCK_AABB`, which is the floor of the
/// bowl, so an item resting inside the hopper is already within reach.
const SUCK_MIN_Y: f64 = 11.0 / 16.0;

/// Highest point of the suction box above the hopper, in blocks.
///
/// Vanilla parity: the `32.0` of `Hopper.SUCK_AABB`.
const SUCK_MAX_Y: f64 = 2.0;

/// Hopper block entity.
pub struct HopperBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<HopperContainer>>,
    container_ref: ContainerRef,
    /// Vanilla parity: the `RandomizableContainer` half of a hopper.
    loot: Arc<ContainerLoot>,
    /// Vanilla parity: the `name` of `BaseContainerBlockEntity`, the anvil
    /// name this block was placed with.
    name: BlockEntityName,
}

/// The five slots of a hopper plus its transfer cooldown.
pub struct HopperContainer {
    items: Vec<ItemStack>,
    /// Ticks left before this hopper may move an item again.
    cooldown_time: i32,
    /// Game time of this hopper's last tick.
    ///
    /// Vanilla parity: `HopperBlockEntity.tickedGameTime`, which keeps a chain
    /// of hoppers flowing at full speed instead of stalling every other tick.
    ticked_game_time: i64,
}

// SAFETY: This key is owned by Foton and uniquely identifies `HopperBlockEntity`.
unsafe impl DowncastType for HopperBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:block_entity/hopper");
}

// SAFETY: This key is owned by Foton and uniquely identifies the independently
// lockable inventory data used by a hopper block entity.
unsafe impl DowncastType for HopperContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:container/hopper");
}

impl HopperContainer {
    /// Returns whether the hopper is still waiting out its cooldown.
    ///
    /// Vanilla parity: `HopperBlockEntity.isOnCooldown`.
    const fn is_on_cooldown(&self) -> bool {
        self.cooldown_time > 0
    }

    /// Returns whether something set a cooldown longer than a transfer.
    ///
    /// Vanilla parity: `HopperBlockEntity.isOnCustomCooldown`.
    const fn is_on_custom_cooldown(&self) -> bool {
        self.cooldown_time > MOVE_ITEM_SPEED
    }

    /// Vanilla parity: `HopperBlockEntity.setCooldown`.
    const fn set_cooldown(&mut self, time: i32) {
        self.cooldown_time = time;
    }

    /// Returns whether every slot holds a full stack.
    ///
    /// Vanilla parity: `HopperBlockEntity.inventoryFull`.
    fn inventory_full(&self) -> bool {
        self.items
            .iter()
            .all(|item| !item.is_empty() && item.count() == item.max_stack_size())
    }
}

impl HopperBlockEntity {
    /// Returns the name an anvil gave this hopper, if any.
    ///
    /// Vanilla parity: `Nameable.getCustomName`.
    #[must_use]
    pub fn custom_name(&self) -> Option<TextComponent> {
        self.name.custom_name()
    }

    /// Sets the optional custom name stored by this hopper.
    pub fn set_custom_name(&self, name: Option<TextComponent>) {
        self.name.set_custom_name(name);
    }

    /// Creates a hopper block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        let base = Arc::new(BlockEntityBase::new(
            &vanilla_block_entity_types::HOPPER,
            level,
            pos,
            state,
        ));
        let container = Arc::new(SyncMutex::new(HopperContainer {
            items: vec![ItemStack::empty(); HOPPER_SLOTS],
            cooldown_time: NO_COOLDOWN_TIME,
            ticked_game_time: 0,
        }));
        let shared_container: SharedContainer = container.clone();
        let loot = Arc::new(ContainerLoot::new());
        Self {
            container_ref: ContainerRef::owned_by_randomizable_block_entity(
                shared_container,
                Arc::clone(&base),
                Arc::clone(&loot),
            ),
            base,
            container,
            loot,
            name: BlockEntityName::new(),
        }
    }

    /// Runs one hopper tick.
    ///
    /// Vanilla parity: `HopperBlockEntity.pushItemsTick`. The block reads
    /// `facing` and `enabled` from the state and passes them in, which is where
    /// vanilla caches the facing on the block entity instead.
    pub fn push_items_tick(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        facing: Direction,
        enabled: bool,
    ) {
        let on_cooldown = {
            let mut container = self.container.lock();
            container.cooldown_time -= 1;
            container.ticked_game_time = world.game_time();
            container.is_on_cooldown()
        };
        if on_cooldown {
            return;
        }

        self.container.lock().set_cooldown(0);
        self.try_move_items(world, pos, facing, enabled);
    }

    /// Pushes one item out and pulls one item in, if either is possible.
    ///
    /// Vanilla parity: `HopperBlockEntity.tryMoveItems`. The cooldown check
    /// vanilla repeats here is not reproduced: Foton only reaches this from the
    /// tick, which has just cleared the cooldown.
    fn try_move_items(&self, world: &Arc<World>, pos: BlockPos, facing: Direction, enabled: bool) {
        if !enabled {
            return;
        }

        // Vanilla parity: `tryMoveItems` opens with `isEmpty`, which is one of
        // the accessors `RandomizableContainerBlockEntity` unpacks from.
        self.container_ref.unpack_loot_table(None);
        let (is_empty, is_full) = {
            let container = self.container.lock();
            (Container::is_empty(&*container), container.inventory_full())
        };

        let mut changed = false;
        if !is_empty {
            changed = self.eject_items(world, pos, facing);
        }
        if !is_full {
            changed |= self.suck_in_items(world, pos);
        }

        if changed {
            self.container.lock().set_cooldown(MOVE_ITEM_SPEED);
            self.set_changed();
        }
    }

    /// Moves one item into the container the hopper points at.
    ///
    /// Vanilla parity: `HopperBlockEntity.ejectItems`.
    fn eject_items(&self, world: &Arc<World>, pos: BlockPos, facing: Direction) -> bool {
        let targets = attached_containers_at(world, pos.relative(facing));
        if targets.is_empty() {
            return false;
        }

        // The face the items arrive through is the one opposite the hopper spout.
        let direction = facing.opposite();
        let mut locked: Vec<ContainerRef> = targets.to_vec();
        locked.push(self.container_ref.clone());
        let mut guard = ContainerLockGuard::lock_all(&locked);

        let own_id = self.container_ref.container_id();
        if is_full_container(&guard, &targets, direction) {
            return false;
        }

        let game_time = world.game_time();
        for slot in 0..HOPPER_SLOTS {
            if guard
                .get(own_id)
                .is_none_or(|container| container.get_item(slot).is_empty())
            {
                continue;
            }

            let Some(moved) = guard
                .get_mut(own_id)
                .map(|container| container.remove_item(slot, 1))
            else {
                continue;
            };
            let leftover = add_item(
                &mut guard,
                Some(own_id),
                &targets,
                moved,
                Some(direction),
                game_time,
            );

            if leftover.is_empty() {
                mark_changed(&mut guard, &targets);
                return true;
            }

            restore_into_slot(&mut guard, own_id, slot, leftover);
        }

        false
    }

    /// Takes one item from above the hopper, from a container or off the ground.
    ///
    /// Vanilla parity: `HopperBlockEntity.suckInItems`.
    fn suck_in_items(&self, world: &Arc<World>, pos: BlockPos) -> bool {
        let above = pos.above();
        let sources = attached_containers_at(world, above);
        if !sources.is_empty() {
            return self.take_from_containers(world, &sources);
        }

        if is_blocked_above(world, above) {
            return false;
        }

        for entity in items_at_and_above(world, pos) {
            if self.add_item_entity(&entity) {
                return true;
            }
        }
        false
    }

    /// Pulls one item down out of the container above.
    fn take_from_containers(&self, world: &Arc<World>, sources: &[ContainerRef]) -> bool {
        take_from_containers_into(world, sources, &self.container_ref)
    }
}

/// Pulls one item down out of any of `sources` into `destination`.
///
/// Vanilla parity: `HopperBlockEntity.tryTakeInItemFromSlot`, applied to every
/// slot the downward face exposes. Free rather than a method because the
/// hopper minecart is a hopper too and owns no block entity.
pub(crate) fn take_from_containers_into(
    world: &Arc<World>,
    sources: &[ContainerRef],
    destination: &ContainerRef,
) -> bool {
    {
        let mut locked: Vec<ContainerRef> = sources.to_vec();
        locked.push(destination.clone());
        let mut guard = ContainerLockGuard::lock_all(&locked);

        let own_id = destination.container_id();
        let own: AttachedContainers = smallvec![destination.clone()];
        let game_time = world.game_time();

        for source in sources {
            let source_id = source.container_id();
            let Some(slots) = guard
                .get(source_id)
                .map(|container| container.slots_for_face(Direction::Down))
            else {
                continue;
            };

            for slot in slots {
                if !can_take_from(&guard, source_id, own_id, slot) {
                    continue;
                }

                let Some(moved) = guard
                    .get_mut(source_id)
                    .map(|container| container.remove_item(slot, 1))
                else {
                    continue;
                };
                let leftover = add_item(&mut guard, Some(source_id), &own, moved, None, game_time);

                if leftover.is_empty() {
                    guard.set_changed(source_id);
                    return true;
                }

                restore_into_slot(&mut guard, source_id, slot, leftover);
            }
        }
    }

    false
}

/// Swallows an item entity into `destination`, whole or as much as fits.
///
/// Vanilla parity: `HopperBlockEntity.addItem(Container, ItemEntity)`.
pub(crate) fn swallow_item_entity(entity: &SharedEntity, destination: &ContainerRef) -> bool {
    let Some(item_entity) = entity.as_ref().downcast_ref::<ItemEntity>() else {
        return false;
    };
    let stack = item_entity.get_item();
    if stack.is_empty() {
        return false;
    }

    let own: AttachedContainers = smallvec![destination.clone()];
    let mut guard = ContainerLockGuard::lock_all(&own);
    let leftover = add_item(&mut guard, None, &own, stack, None, 0);
    drop(guard);

    if leftover.is_empty() {
        item_entity.set_item(ItemStack::empty());
        entity.set_removed(RemovalReason::Discarded);
        return true;
    }

    item_entity.set_item(leftover);
    false
}

impl HopperBlockEntity {
    /// Swallows an item entity whole, or as much of it as fits.
    fn add_item_entity(&self, entity: &SharedEntity) -> bool {
        swallow_item_entity(entity, &self.container_ref)
    }
}

/// Puts `leftover` back into the slot it was taken from.
///
/// Vanilla mutates the stack it never really detached; Foton took a real copy
/// out of the slot, so the count goes back onto whatever remains there.
fn restore_into_slot(
    guard: &mut ContainerLockGuard,
    container_id: ContainerId,
    slot: usize,
    leftover: ItemStack,
) {
    let Some(container) = guard.get_mut(container_id) else {
        return;
    };
    let mut current = container.get_item(slot).clone();
    if current.is_empty() {
        container.set_item(slot, leftover);
    } else {
        current.grow(leftover.count());
        container.set_item(slot, current);
    }
}

/// Returns whether the hopper may take slot `slot` out of `source`.
///
/// Vanilla parity: `HopperBlockEntity.canTakeItemFromContainer`.
fn can_take_from(
    guard: &ContainerLockGuard,
    source_id: ContainerId,
    destination_id: ContainerId,
    slot: usize,
) -> bool {
    let (Some(source), Some(destination)) = (guard.get(source_id), guard.get(destination_id))
    else {
        return false;
    };
    let stack = source.get_item(slot);
    if stack.is_empty() {
        return false;
    }
    source.can_take_item(destination, slot, stack)
        && source.can_take_item_through_face(slot, stack, Direction::Down)
}

/// Returns whether every slot reachable through `direction` is already full.
///
/// Vanilla parity: `HopperBlockEntity.isFullContainer`.
fn is_full_container(
    guard: &ContainerLockGuard,
    containers: &[ContainerRef],
    direction: Direction,
) -> bool {
    containers.iter().all(|container_ref| {
        guard
            .get(container_ref.container_id())
            .is_none_or(|container| {
                container.slots_for_face(direction).into_iter().all(|slot| {
                    let stack = container.get_item(slot);
                    stack.count() >= stack.max_stack_size()
                })
            })
    })
}

/// Marks every container in `containers` as changed.
fn mark_changed(guard: &mut ContainerLockGuard, containers: &[ContainerRef]) {
    for container_ref in containers {
        guard.set_changed(container_ref.container_id());
    }
}

/// Inserts `stack` into the first slot of `targets` that accepts it.
///
/// Vanilla parity: `HopperBlockEntity.addItem(Container, Container, ItemStack,
/// Direction)`. A `None` direction means the insertion has no face, which is
/// what a hopper filling itself does.
fn add_item(
    guard: &mut ContainerLockGuard,
    from: Option<ContainerId>,
    targets: &[ContainerRef],
    mut stack: ItemStack,
    direction: Option<Direction>,
    game_time: i64,
) -> ItemStack {
    for target in targets {
        let target_id = target.container_id();
        let Some(slots) = guard.get(target_id).map(|container| match direction {
            Some(direction) => container.slots_for_face(direction),
            None => SlotsForFace::All(container.get_container_size()),
        }) else {
            continue;
        };

        for slot in slots {
            if stack.is_empty() {
                return stack;
            }
            stack = try_move_in_item(guard, from, target_id, stack, slot, direction, game_time);
        }
    }

    stack
}

/// Moves as much of `stack` as fits into one slot.
///
/// Vanilla parity: `HopperBlockEntity.tryMoveInItem`.
fn try_move_in_item(
    guard: &mut ContainerLockGuard,
    from: Option<ContainerId>,
    target_id: ContainerId,
    mut stack: ItemStack,
    slot: usize,
    direction: Option<Direction>,
    game_time: i64,
) -> ItemStack {
    let Some(target) = guard.get(target_id) else {
        return stack;
    };
    if !target.can_place_item_through_face(slot, &stack, direction) {
        return stack;
    }

    let current = target.get_item(slot).clone();
    let was_empty = target.is_empty();
    // Read before the mutable borrow: a hopper feeding another hopper must not
    // hand it a cooldown that would stall the chain.
    let source_ticked_game_time = from.and_then(|id| {
        guard
            .get_typed::<HopperContainer>(id)
            .map(|hopper| hopper.ticked_game_time)
    });

    let Some(target) = guard.get_mut(target_id) else {
        return stack;
    };
    let moved = if current.is_empty() {
        let inserted = stack.clone();
        stack = ItemStack::empty();
        target.set_item(slot, inserted);
        true
    } else if can_merge_items(&current, &stack) {
        let space = stack.max_stack_size() - current.count();
        let count = stack.count().min(space);
        if count > 0 {
            stack.shrink(count);
            target.get_item_mut(slot).grow(count);
            true
        } else {
            false
        }
    } else {
        false
    };

    if moved && was_empty {
        set_receiving_hopper_cooldown(guard, target_id, source_ticked_game_time, game_time);
    }

    stack
}

/// Gives a hopper that just received its first item a fresh cooldown.
///
/// Vanilla parity: the `HopperBlockEntity` branch of `tryMoveInItem`. A hopper
/// fed by another hopper that already ticked this game time loses a tick of
/// cooldown, which is what keeps a vertical chain moving at full speed.
fn set_receiving_hopper_cooldown(
    guard: &mut ContainerLockGuard,
    target_id: ContainerId,
    source_ticked_game_time: Option<i64>,
    game_time: i64,
) {
    let Some(hopper) = guard.get_typed_mut::<HopperContainer>(target_id) else {
        return;
    };
    if hopper.is_on_custom_cooldown() {
        return;
    }
    let skipped_ticks =
        i32::from(source_ticked_game_time.is_some_and(|source_time| game_time >= source_time));
    hopper.set_cooldown(MOVE_ITEM_SPEED - skipped_ticks);
}

/// Vanilla parity: `HopperBlockEntity.canMergeItems`.
fn can_merge_items(current: &ItemStack, incoming: &ItemStack) -> bool {
    current.count() <= current.max_stack_size()
        && ItemStack::is_same_item_same_components(current, incoming)
}

/// Returns the containers automation sees at `pos`.
///
/// Vanilla parity: `HopperBlockEntity.getContainerAt`, minus the container
/// entities. Foton has no chest minecart container capability yet, so a hopper
/// under a rail does nothing rather than loading the minecart.
pub(crate) fn attached_containers_at(world: &Arc<World>, pos: BlockPos) -> AttachedContainers {
    let state = world.get_block_state(pos);
    BLOCK_BEHAVIORS
        .get_behavior(state.get_block())
        .get_attached_containers(state, world.as_ref(), pos)
}

/// Hands `stack` to whatever container sits at `pos`, entering through `face`.
///
/// Returns `None` when there is no container there at all, which is the case a
/// dropper answers by throwing the item instead; otherwise the part of `stack`
/// that did not fit.
///
/// Vanilla parity: the `HopperBlockEntity.getContainerAt` plus
/// `HopperBlockEntity.addItem` pair that `DropperBlock.dispenseFrom` uses.
pub fn insert_into_containers_at(
    world: &Arc<World>,
    pos: BlockPos,
    stack: ItemStack,
    face: Direction,
) -> Option<ItemStack> {
    let targets = attached_containers_at(world, pos);
    if targets.is_empty() {
        return None;
    }

    let mut guard = ContainerLockGuard::lock_all(&targets);
    let leftover = add_item(
        &mut guard,
        None,
        &targets,
        stack,
        Some(face),
        world.game_time(),
    );
    if leftover.is_empty() {
        mark_changed(&mut guard, &targets);
    }
    Some(leftover)
}

/// Takes one item into `destination` from around a point that is not on the
/// block grid.
///
/// Vanilla parity: `HopperBlockEntity.suckInItems` for a `Hopper` whose
/// `isGridAligned` is false -- which is the hopper minecart, and only it. A
/// cart sits between blocks, so its reach is measured from where it actually
/// is rather than from the block it happens to overlap.
pub(crate) fn suck_into_at(
    world: &Arc<World>,
    level_pos: DVec3,
    destination: &ContainerRef,
) -> bool {
    let above = BlockPos::new(
        level_pos.x.floor() as i32,
        (level_pos.y + 1.0).floor() as i32,
        level_pos.z.floor() as i32,
    );
    let sources = attached_containers_at(world, above);
    if !sources.is_empty() {
        return take_from_containers_into(world, &sources, destination);
    }

    for entity in items_around(world, level_pos) {
        if swallow_item_entity(&entity, destination) {
            return true;
        }
    }
    false
}

/// Returns the item entities inside a loose hopper's reach.
///
/// Vanilla parity: `getItemsAtAndAbove` with the suck box moved to the
/// hopper's own position rather than snapped to a block.
fn items_around(world: &Arc<World>, level_pos: DVec3) -> Vec<SharedEntity> {
    let aabb = WorldAabb::new(
        level_pos.x - 0.5,
        level_pos.y - 0.5 + SUCK_MIN_Y,
        level_pos.z - 0.5,
        level_pos.x + 0.5,
        level_pos.y - 0.5 + SUCK_MAX_Y,
        level_pos.z + 0.5,
    );
    world
        .get_entities_in_aabb(&aabb)
        .into_iter()
        .filter(|entity| {
            entity.is_alive() && entity.as_ref().downcast_ref::<ItemEntity>().is_some()
        })
        .collect()
}

/// Returns whether a full block above the hopper stops it picking items up.
///
/// Vanilla parity: the `isBlocked` test of `suckInItems`.
fn is_blocked_above(world: &Arc<World>, above: BlockPos) -> bool {
    let state = world.get_block_state(above);
    world.is_collision_shape_full_block_at(above, state)
        && !state.get_block().has_tag(&BlockTag::DOES_NOT_BLOCK_HOPPERS)
}

/// Returns the item entities resting in and just above the hopper.
///
/// Vanilla parity: `HopperBlockEntity.getItemsAtAndAbove`.
fn items_at_and_above(world: &Arc<World>, pos: BlockPos) -> Vec<SharedEntity> {
    let aabb = WorldAabb::new(
        f64::from(pos.x()),
        f64::from(pos.y()) + SUCK_MIN_Y,
        f64::from(pos.z()),
        f64::from(pos.x()) + 1.0,
        f64::from(pos.y()) + SUCK_MAX_Y,
        f64::from(pos.z()) + 1.0,
    );
    world
        .get_entities_in_aabb(&aabb)
        .into_iter()
        .filter(|entity| {
            entity.is_alive() && entity.as_ref().downcast_ref::<ItemEntity>().is_some()
        })
        .collect()
}

impl BlockEntity for HopperBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn pre_remove_side_effects(&self, pos: BlockPos, _state: BlockStateId) {
        self.container_ref.unpack_loot_table(None);
        let items = {
            let mut container = self.container.lock();
            mem::replace(&mut container.items, vec![ItemStack::empty(); HOPPER_SLOTS])
        };
        let Some(world) = self.get_level() else {
            return;
        };
        for item in items {
            world.drop_item_stack(pos, item);
        }
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt_view: NbtCompoundView<'_, '_> = nbt.into();
        self.name.load(&nbt_view);
        // Vanilla parity: a hopper stores either a loot table or its items; the
        // transfer cooldown is read either way.
        let packed = self.loot.try_load_loot_table(&nbt_view);
        let mut container = self.container.lock();
        container.items.fill(ItemStack::empty());

        if !packed
            && let Some(items_list) = nbt_view.list("Items")
            && let Some(compounds) = items_list.compounds()
        {
            for compound in compounds {
                if let Some(slot) = compound.byte("Slot") {
                    let slot = slot as usize;
                    if slot < HOPPER_SLOTS
                        && let Some(item) = ItemStack::from_borrowed_compound(&compound)
                    {
                        container.items[slot] = item;
                    }
                }
            }
        }

        container.cooldown_time = nbt_view.int("TransferCooldown").unwrap_or(NO_COOLDOWN_TIME);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.name.save(nbt);
        let container = self.container.lock();
        if !self.loot.try_save_loot_table(nbt) {
            let mut items: Vec<NbtCompound> = Vec::new();
            for (slot, item) in container.items.iter().enumerate() {
                if !item.is_empty()
                    && let NbtTag::Compound(mut item_nbt) = item.clone().to_nbt_tag()
                {
                    item_nbt.insert("Slot", slot as i8);
                    items.push(item_nbt);
                }
            }
            nbt.insert("Items", NbtList::Compound(items));
        }
        nbt.insert("TransferCooldown", container.cooldown_time);
    }

    fn container_ref(&self) -> Option<ContainerRef> {
        Some(self.container_ref.clone())
    }

    /// Vanilla parity: `BaseContainerBlockEntity.getName`, which falls back to
    /// the block's own name.
    fn display_name(&self, default_name: TextComponent) -> TextComponent {
        self.name.display_name(default_name)
    }

    /// Vanilla parity: the `CUSTOM_NAME` half of
    /// `BaseContainerBlockEntity.collectImplicitComponents`. `CONTAINER` and
    /// `LOCK` are not collected: no vanilla loot table asks this block for
    /// either, and Foton has no lock on a container yet.
    fn collect_implicit_components(&self, components: &mut DataComponentMap) {
        self.name.collect_implicit_components(components);
    }

    /// Vanilla parity: the `CUSTOM_NAME` half of
    /// `BaseContainerBlockEntity.applyImplicitComponents`.
    fn apply_implicit_components(&self, input: &ImplicitComponentInput<'_>) {
        self.name.apply_implicit_components(input);
    }
}

impl Container for HopperContainer {
    fn items(&self) -> &[ItemStack] {
        &self.items
    }

    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }

    fn get_container_size(&self) -> usize {
        HOPPER_SLOTS
    }

    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        if slot >= HOPPER_SLOTS {
            return;
        }
        let max_stack_size = self.get_max_stack_size_for_item(&stack);
        if !stack.is_empty() && stack.count() > max_stack_size {
            stack.set_count(max_stack_size);
        }
        self.items[slot] = stack;
    }

    fn get_max_stack_size(&self) -> i32 {
        64
    }

    fn set_changed(&mut self) {}
}

#[cfg(test)]
mod tests {
    use foton_registry::{init_vanilla_registry, vanilla_items};
    use foton_utils::locks::IntoShared as _;

    use super::*;
    use crate::inventory::container::SimpleContainer;

    fn container_of(size: usize) -> ContainerRef {
        init_vanilla_registry();
        SimpleContainer::new(size).into_shared().into()
    }

    fn set(guard: &mut ContainerLockGuard, target: &ContainerRef, slot: usize, stack: ItemStack) {
        guard
            .get_mut(target.container_id())
            .expect("locked")
            .set_item(slot, stack);
    }

    #[test]
    fn add_item_fills_the_first_slot_that_will_take_it() {
        let target = container_of(3);
        let targets: AttachedContainers = smallvec![target.clone()];
        let mut guard = ContainerLockGuard::lock_all(&targets);
        set(
            &mut guard,
            &target,
            0,
            ItemStack::new(&vanilla_items::STONE),
        );

        let leftover = add_item(
            &mut guard,
            None,
            &targets,
            ItemStack::new(&vanilla_items::DIRT),
            None,
            0,
        );

        assert!(leftover.is_empty());
        let container = guard.get(target.container_id()).expect("locked");
        assert!(container.get_item(1).is(&vanilla_items::DIRT));
    }

    /// Vanilla tops up an existing stack before it opens a new slot, which is
    /// why a hopper never scatters one item type across five slots.
    #[test]
    fn add_item_tops_up_a_partial_stack_first() {
        let target = container_of(2);
        let targets: AttachedContainers = smallvec![target.clone()];
        let mut guard = ContainerLockGuard::lock_all(&targets);
        set(
            &mut guard,
            &target,
            0,
            ItemStack::with_count(&vanilla_items::STONE, 10),
        );

        let leftover = add_item(
            &mut guard,
            None,
            &targets,
            ItemStack::new(&vanilla_items::STONE),
            None,
            0,
        );

        assert!(leftover.is_empty());
        let container = guard.get(target.container_id()).expect("locked");
        assert_eq!(container.get_item(0).count(), 11);
        assert!(container.get_item(1).is_empty());
    }

    #[test]
    fn add_item_hands_back_what_does_not_fit() {
        let target = container_of(1);
        let targets: AttachedContainers = smallvec![target.clone()];
        let mut guard = ContainerLockGuard::lock_all(&targets);
        set(
            &mut guard,
            &target,
            0,
            ItemStack::with_count(&vanilla_items::STONE, 64),
        );

        let leftover = add_item(
            &mut guard,
            None,
            &targets,
            ItemStack::with_count(&vanilla_items::STONE, 5),
            None,
            0,
        );

        assert_eq!(leftover.count(), 5);
    }

    /// A double chest is two references here, so an item spills into the second
    /// half only once the first is full.
    #[test]
    fn add_item_walks_both_halves_of_a_double_chest() {
        let first = container_of(1);
        let second = container_of(1);
        let targets: AttachedContainers = smallvec![first.clone(), second.clone()];
        let mut guard = ContainerLockGuard::lock_all(&targets);
        set(
            &mut guard,
            &first,
            0,
            ItemStack::with_count(&vanilla_items::STONE, 64),
        );

        let leftover = add_item(
            &mut guard,
            None,
            &targets,
            ItemStack::new(&vanilla_items::DIRT),
            None,
            0,
        );

        assert!(leftover.is_empty());
        assert!(
            guard
                .get(second.container_id())
                .expect("locked")
                .get_item(0)
                .is(&vanilla_items::DIRT)
        );
    }

    #[test]
    fn one_gap_is_enough_to_leave_a_container_not_full() {
        let target = container_of(2);
        let targets: AttachedContainers = smallvec![target.clone()];
        let mut guard = ContainerLockGuard::lock_all(&targets);
        set(
            &mut guard,
            &target,
            0,
            ItemStack::with_count(&vanilla_items::STONE, 64),
        );

        assert!(!is_full_container(&guard, &targets, Direction::Up));

        set(
            &mut guard,
            &target,
            1,
            ItemStack::with_count(&vanilla_items::STONE, 64),
        );

        assert!(is_full_container(&guard, &targets, Direction::Up));
    }

    /// The count goes back onto whatever is left in the slot, because Foton
    /// really detached the item vanilla only pretends to.
    #[test]
    fn a_refused_item_goes_back_into_the_slot_it_came_from() {
        let source = container_of(1);
        let refs: AttachedContainers = smallvec![source.clone()];
        let mut guard = ContainerLockGuard::lock_all(&refs);
        set(
            &mut guard,
            &source,
            0,
            ItemStack::with_count(&vanilla_items::STONE, 3),
        );

        let taken = guard
            .get_mut(source.container_id())
            .expect("locked")
            .remove_item(0, 1);
        assert_eq!(taken.count(), 1);

        restore_into_slot(&mut guard, source.container_id(), 0, taken);

        assert_eq!(
            guard
                .get(source.container_id())
                .expect("locked")
                .get_item(0)
                .count(),
            3
        );
    }

    #[test]
    fn restoring_the_last_item_refills_an_emptied_slot() {
        let source = container_of(1);
        let refs: AttachedContainers = smallvec![source.clone()];
        let mut guard = ContainerLockGuard::lock_all(&refs);
        set(
            &mut guard,
            &source,
            0,
            ItemStack::new(&vanilla_items::STONE),
        );

        let taken = guard
            .get_mut(source.container_id())
            .expect("locked")
            .remove_item(0, 1);

        restore_into_slot(&mut guard, source.container_id(), 0, taken);

        let container = guard.get(source.container_id()).expect("locked");
        assert!(container.get_item(0).is(&vanilla_items::STONE));
        assert_eq!(container.get_item(0).count(), 1);
    }

    #[test]
    fn different_items_never_merge() {
        init_vanilla_registry();
        assert!(can_merge_items(
            &ItemStack::new(&vanilla_items::STONE),
            &ItemStack::new(&vanilla_items::STONE)
        ));
        assert!(!can_merge_items(
            &ItemStack::new(&vanilla_items::STONE),
            &ItemStack::new(&vanilla_items::DIRT)
        ));
    }

    #[test]
    fn a_hopper_is_only_full_when_every_slot_holds_a_full_stack() {
        init_vanilla_registry();
        let mut container = HopperContainer {
            items: vec![ItemStack::with_count(&vanilla_items::STONE, 64); HOPPER_SLOTS],
            cooldown_time: NO_COOLDOWN_TIME,
            ticked_game_time: 0,
        };
        assert!(container.inventory_full());

        container.items[4] = ItemStack::with_count(&vanilla_items::STONE, 63);
        assert!(!container.inventory_full());

        container.items[4] = ItemStack::empty();
        assert!(!container.inventory_full());
    }

    /// Vanilla parity: the cooldown shortcut of `HopperBlockEntity.tryMoveInItem`,
    /// which is what keeps a vertical chain of hoppers moving at full speed
    /// instead of stalling every other transfer.
    #[test]
    fn a_receiving_hopper_loses_a_tick_when_its_feeder_already_ticked() {
        init_vanilla_registry();
        let receiver: ContainerRef = Arc::new(SyncMutex::new(HopperContainer {
            items: vec![ItemStack::empty(); HOPPER_SLOTS],
            cooldown_time: 0,
            ticked_game_time: 0,
        }))
        .into();
        let refs: AttachedContainers = smallvec![receiver.clone()];
        let mut guard = ContainerLockGuard::lock_all(&refs);

        set_receiving_hopper_cooldown(&mut guard, receiver.container_id(), Some(40), 40);
        assert_eq!(
            guard
                .get_typed::<HopperContainer>(receiver.container_id())
                .expect("locked")
                .cooldown_time,
            MOVE_ITEM_SPEED - 1
        );

        set_receiving_hopper_cooldown(&mut guard, receiver.container_id(), None, 40);
        assert_eq!(
            guard
                .get_typed::<HopperContainer>(receiver.container_id())
                .expect("locked")
                .cooldown_time,
            MOVE_ITEM_SPEED
        );
    }
}
