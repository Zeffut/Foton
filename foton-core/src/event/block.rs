//! Events about a player changing the world's blocks.
//!
//! The two a protection plugin is built out of, and the reason cancellation is
//! in the bus at all. Nine of the fifty-nine plugins surveyed in
//! `dev/plugin-api-usage.json` need the break and eleven the place.

use std::sync::Arc;

use foton_registry::item_stack::ItemStack;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use foton_utils::{BlockPos, BlockStateId};
use uuid::Uuid;

/// A player begins damaging a block.
pub struct BlockDamageEvent {
    player_id: Uuid,
    world: String,
    position: BlockPos,
    cancelled: bool,
}
unsafe impl DowncastType for BlockDamageEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/block_damage");
}
impl Event for BlockDamageEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl BlockDamageEvent {
    pub fn new(player_id: Uuid, world: String, position: BlockPos) -> Self {
        Self {
            player_id,
            world,
            position,
            cancelled: false,
        }
    }
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn position(&self) -> BlockPos {
        self.position
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

/// A flammable block is about to be consumed by fire.
pub struct BlockBurnEvent {
    world: String,
    position: BlockPos,
    cancelled: bool,
}
unsafe impl DowncastType for BlockBurnEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/block_burn");
}
impl Event for BlockBurnEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl BlockBurnEvent {
    pub fn new(world: impl Into<String>, position: BlockPos) -> Self {
        Self {
            world: world.into(),
            position,
            cancelled: false,
        }
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn position(&self) -> BlockPos {
        self.position
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

/// A block is about to disappear because of a natural fade.
pub struct BlockFadeEvent {
    world: String,
    position: BlockPos,
    cancelled: bool,
}

/// Leaves are about to decay naturally.
pub struct LeavesDecayEvent {
    world: String,
    position: BlockPos,
    cancelled: bool,
}
unsafe impl DowncastType for LeavesDecayEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/leaves_decay");
}
impl Event for LeavesDecayEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl LeavesDecayEvent {
    pub fn new(world: impl Into<String>, position: BlockPos) -> Self {
        Self {
            world: world.into(),
            position,
            cancelled: false,
        }
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn position(&self) -> BlockPos {
        self.position
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

/// A block is about to be ignited by a player or item.
pub struct BlockIgniteEvent {
    world: String,
    position: BlockPos,
    cause: String,
    player: Option<Uuid>,
    cancelled: bool,
}
unsafe impl DowncastType for BlockIgniteEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/block_ignite");
}
impl Event for BlockIgniteEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl BlockIgniteEvent {
    pub fn new(world: impl Into<String>, position: BlockPos) -> Self {
        Self {
            world: world.into(),
            position,
            cause: "FLINT_AND_STEEL".to_owned(),
            player: None,
            cancelled: false,
        }
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn position(&self) -> BlockPos {
        self.position
    }
    pub fn cause(&self) -> &str {
        &self.cause
    }
    pub const fn player_id(&self) -> Option<Uuid> {
        self.player
    }
    pub fn set_player(&mut self, player: Uuid) {
        self.player = Some(player);
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}
unsafe impl DowncastType for BlockFadeEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/block_fade");
}
impl Event for BlockFadeEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl BlockFadeEvent {
    pub fn new(world: impl Into<String>, position: BlockPos) -> Self {
        Self {
            world: world.into(),
            position,
            cancelled: false,
        }
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn position(&self) -> BlockPos {
        self.position
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

/// A dispenser is about to dispense an item.
pub struct BlockDispenseEvent {
    world: String,
    position: BlockPos,
    item: ItemStack,
    cancelled: bool,
}

/// Paper's pre-dispense hook, fired before the Bukkit dispense event.
pub struct BlockPreDispenseEvent {
    world: String,
    position: BlockPos,
    slot: usize,
    item: ItemStack,
    cancelled: bool,
}
unsafe impl DowncastType for BlockPreDispenseEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/block_pre_dispense");
}
impl Event for BlockPreDispenseEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl BlockPreDispenseEvent {
    pub fn new(world: String, position: BlockPos, slot: usize, item: ItemStack) -> Self {
        Self {
            world,
            position,
            slot,
            item,
            cancelled: false,
        }
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn position(&self) -> BlockPos {
        self.position
    }
    pub const fn slot(&self) -> usize {
        self.slot
    }
    pub fn item(&self) -> &ItemStack {
        &self.item
    }
    pub fn set_item(&mut self, item: ItemStack) {
        self.item = item;
    }
    pub fn into_item(self) -> ItemStack {
        self.item
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}
unsafe impl DowncastType for BlockDispenseEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/block_dispense");
}
impl Event for BlockDispenseEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl BlockDispenseEvent {
    pub fn new(world: String, position: BlockPos, item: ItemStack) -> Self {
        Self {
            world,
            position,
            item,
            cancelled: false,
        }
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn position(&self) -> BlockPos {
        self.position
    }
    pub fn item(&self) -> &ItemStack {
        &self.item
    }
    pub fn set_item(&mut self, item: ItemStack) {
        self.item = item;
    }
    pub fn into_item(self) -> ItemStack {
        self.item
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

use super::Event;
use crate::player::Player;

/// A player submitted new text for a sign.
pub struct SignChangeEvent {
    player_id: Uuid,
    world: String,
    position: BlockPos,
    lines: [String; 4],
    cancelled: bool,
}

/// A piston is about to move its resolved block list.
pub struct PistonEvent {
    world: String,
    piston: BlockPos,
    blocks: Vec<BlockPos>,
    direction: String,
    extending: bool,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PistonEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/piston");
}
impl Event for PistonEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl PistonEvent {
    pub fn new(
        world: String,
        piston: BlockPos,
        blocks: Vec<BlockPos>,
        direction: String,
        extending: bool,
    ) -> Self {
        Self {
            world,
            piston,
            blocks,
            direction,
            extending,
            cancelled: false,
        }
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn piston(&self) -> BlockPos {
        self.piston
    }
    pub fn blocks(&self) -> &[BlockPos] {
        &self.blocks
    }
    pub fn blocks_mut(&mut self) -> &mut Vec<BlockPos> {
        &mut self.blocks
    }
    pub fn direction(&self) -> &str {
        &self.direction
    }
    pub const fn extending(&self) -> bool {
        self.extending
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for SignChangeEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/sign_change");
}
impl Event for SignChangeEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl SignChangeEvent {
    pub fn new(player_id: Uuid, world: String, position: BlockPos, lines: [String; 4]) -> Self {
        Self {
            player_id,
            world,
            position,
            lines,
            cancelled: false,
        }
    }
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn position(&self) -> BlockPos {
        self.position
    }
    pub fn lines(&self) -> &[String; 4] {
        &self.lines
    }
    pub fn lines_mut(&mut self) -> &mut [String; 4] {
        &mut self.lines
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

/// A player is about to break a block.
///
/// Fires before anything has happened to the block, so cancelling leaves the
/// world exactly as it was rather than putting it back. That is a deliberate
/// difference from `org.bukkit.event.block.BlockBreakEvent`, which fires after
/// the state has been set and rolls back on cancel: a listener sees the same
/// thing either way, and not having to undo is worth more than matching the
/// order in which Bukkit happens to do it.
pub struct BlockBreakEvent {
    player: Arc<Player>,
    position: BlockPos,
    state: BlockStateId,
    cancelled: bool,
    drop_items: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type
// within the process.
unsafe impl DowncastType for BlockBreakEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/block_break");
}

impl Event for BlockBreakEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl BlockBreakEvent {
    /// Creates the event for a break that has not happened yet.
    #[must_use]
    pub const fn new(player: Arc<Player>, position: BlockPos, state: BlockStateId) -> Self {
        Self {
            player,
            position,
            state,
            cancelled: false,
            drop_items: true,
        }
    }

    /// The player breaking it.
    #[must_use]
    pub const fn player(&self) -> &Arc<Player> {
        &self.player
    }

    /// Where the block is.
    #[must_use]
    pub const fn position(&self) -> BlockPos {
        self.position
    }

    /// The block as it still stands.
    #[must_use]
    pub const fn state(&self) -> BlockStateId {
        self.state
    }

    /// Stops the break. The block keeps standing.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }

    /// Whether the normal block drops should be spawned.
    #[must_use]
    pub const fn drop_items(&self) -> bool {
        self.drop_items
    }

    /// Suppresses normal block drops while preserving the break itself.
    pub const fn set_drop_items(&mut self, drop_items: bool) {
        self.drop_items = drop_items;
    }
}

/// A player is about to place a block.
///
/// Fires after every vanilla check has passed -- reach, survivability,
/// obstruction -- and before the block is written, so a listener is only asked
/// about placements that would otherwise have happened.
///
/// Only a player's placement reaches this. A dispenser firing a block is not a
/// `BlockPlaceEvent` in Bukkit either.
pub struct BlockPlaceEvent {
    player: Arc<Player>,
    position: BlockPos,
    state: BlockStateId,
    item: foton_registry::item_stack::ItemStack,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type
// within the process.
unsafe impl DowncastType for BlockPlaceEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/block_place");
}

pub struct BlockFromToEvent {
    world: String,
    block: BlockPos,
    to_block: BlockPos,
    cancelled: bool,
}
unsafe impl DowncastType for BlockFromToEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/block_from_to");
}
impl Event for BlockFromToEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl BlockFromToEvent {
    pub fn new(world: String, block: BlockPos, to_block: BlockPos) -> Self {
        Self {
            world,
            block,
            to_block,
            cancelled: false,
        }
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn block(&self) -> BlockPos {
        self.block
    }
    pub const fn to_block(&self) -> BlockPos {
        self.to_block
    }
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}
impl Event for BlockPlaceEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl BlockPlaceEvent {
    /// Creates the event for a placement that has not happened yet.
    #[must_use]
    pub fn new(
        player: Arc<Player>,
        position: BlockPos,
        state: BlockStateId,
        item: foton_registry::item_stack::ItemStack,
    ) -> Self {
        Self {
            player,
            position,
            state,
            item,
            cancelled: false,
        }
    }

    /// The player placing it.
    #[must_use]
    pub const fn player(&self) -> &Arc<Player> {
        &self.player
    }

    /// Where the block would go.
    #[must_use]
    pub const fn position(&self) -> BlockPos {
        self.position
    }

    /// The state that would be placed.
    #[must_use]
    pub const fn state(&self) -> BlockStateId {
        self.state
    }

    #[must_use]
    pub fn item(&self) -> &foton_registry::item_stack::ItemStack {
        &self.item
    }

    /// Stops the placement. Nothing is written.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// Experience awarded when a block drops XP.
pub struct BlockExpEvent {
    world: String,
    position: BlockPos,
    exp_to_drop: i32,
    cancelled: bool,
}
unsafe impl DowncastType for BlockExpEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/block_exp");
}
impl Event for BlockExpEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl BlockExpEvent {
    pub fn new(world: impl Into<String>, position: BlockPos, exp_to_drop: i32) -> Self {
        Self {
            world: world.into(),
            position,
            exp_to_drop,
            cancelled: false,
        }
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn position(&self) -> BlockPos {
        self.position
    }
    pub const fn exp_to_drop(&self) -> i32 {
        self.exp_to_drop
    }
    pub fn set_exp_to_drop(&mut self, value: i32) {
        self.exp_to_drop = value.max(0);
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

#[cfg(test)]
mod block_exp_tests {
    use super::*;
    #[test]
    fn block_exp_event_mutates_and_cancels() {
        let mut event = BlockExpEvent::new("minecraft:overworld", BlockPos::new(1, 64, 2), 7);
        assert_eq!(event.exp_to_drop(), 7);
        event.set_exp_to_drop(3);
        assert_eq!(event.exp_to_drop(), 3);
        event.set_cancelled(true);
        assert!(event.is_cancelled());
    }
}
