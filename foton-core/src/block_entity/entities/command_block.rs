//! The command block block entity.
//!
//! Vanilla parity: `CommandBlockEntity`. It owns a [`BaseCommandBlock`] plus the
//! three flags a command block needs on top of it: whether redstone is holding
//! it powered, whether it is set to "always active", and whether its condition
//! was met on the last attempt.
//!
//! The mode is not stored -- it is read back from which of the three blocks is
//! standing there, exactly as `getMode` does.

use std::sync::{Arc, Weak};

use foton_registry::blocks::BlockRef;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::{BlockStateProperties, Direction};
use foton_registry::{vanilla_block_entity_types, vanilla_blocks};
use foton_utils::locks::SyncMutex;
use foton_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::NbtCompound;

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::command::base_command_block::BaseCommandBlock;
use crate::world::{LevelReader as _, World};

/// How a command block decides when to run.
///
/// Vanilla parity: `CommandBlockEntity.Mode`, and the wire order of
/// `ServerboundSetCommandBlockPacket`'s enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandBlockMode {
    /// The chain command block: runs when the block behind it succeeds.
    Sequence,
    /// The repeating command block: runs every tick while active.
    Auto,
    /// The plain command block: runs on a rising redstone edge.
    Redstone,
}

impl CommandBlockMode {
    /// Returns the mode a wire value names.
    ///
    /// Vanilla parity: `FriendlyByteBuf.readEnum` over `CommandBlockEntity.Mode`,
    /// which is ordinal order.
    #[must_use]
    pub const fn from_wire(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Sequence),
            1 => Some(Self::Auto),
            2 => Some(Self::Redstone),
            _ => None,
        }
    }

    /// Returns the block this mode is stored as.
    ///
    /// Vanilla parity: the `switch (packet.getMode())` of
    /// `handleSetCommandBlock`.
    #[must_use]
    pub const fn block(self) -> BlockRef {
        match self {
            Self::Sequence => &vanilla_blocks::CHAIN_COMMAND_BLOCK,
            Self::Auto => &vanilla_blocks::REPEATING_COMMAND_BLOCK,
            Self::Redstone => &vanilla_blocks::COMMAND_BLOCK,
        }
    }
}

/// The three flags a command block keeps beyond its command.
#[derive(Debug)]
struct CommandBlockFlags {
    powered: bool,
    auto: bool,
    condition_met: bool,
}

impl CommandBlockFlags {
    const fn new() -> Self {
        Self {
            powered: false,
            auto: false,
            condition_met: false,
        }
    }
}

/// A command block.
pub struct CommandBlockEntity {
    base: BlockEntityBase,
    command_block: Arc<BaseCommandBlock>,
    flags: SyncMutex<CommandBlockFlags>,
}

// SAFETY: This key is owned by Foton and uniquely identifies the block entity.
unsafe impl DowncastType for CommandBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:block_entity/command_block");
}

impl CommandBlockEntity {
    /// Creates a command block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(
                &vanilla_block_entity_types::COMMAND_BLOCK,
                level,
                pos,
                state,
            ),
            command_block: Arc::new(BaseCommandBlock::new()),
            flags: SyncMutex::new(CommandBlockFlags::new()),
        }
    }

    /// Returns the command store.
    ///
    /// Vanilla parity: `CommandBlockEntity.getCommandBlock`.
    #[must_use]
    pub const fn command_block(&self) -> &Arc<BaseCommandBlock> {
        &self.command_block
    }

    /// Returns whether redstone is holding this block powered.
    #[must_use]
    pub fn is_powered(&self) -> bool {
        self.flags.lock().powered
    }

    /// Sets whether redstone is holding this block powered.
    pub fn set_powered(&self, powered: bool) {
        self.flags.lock().powered = powered;
    }

    /// Returns whether this block is set to "always active".
    ///
    /// Vanilla parity: `CommandBlockEntity.isAutomatic`.
    #[must_use]
    pub fn is_automatic(&self) -> bool {
        self.flags.lock().auto
    }

    /// Sets "always active", scheduling a tick when it turns on.
    ///
    /// Vanilla parity: `CommandBlockEntity.setAutomatic`. A chain block is
    /// excluded because it never schedules itself -- the block in front of it
    /// drives it.
    pub fn set_automatic(&self, auto: bool) {
        let previous = {
            let mut flags = self.flags.lock();
            let previous = flags.auto;
            flags.auto = auto;
            previous
        };

        if !previous && auto && !self.is_powered() && self.mode() != CommandBlockMode::Sequence {
            self.schedule_tick();
        }
    }

    /// Reschedules after the editor switched this block to another mode.
    ///
    /// Vanilla parity: `CommandBlockEntity.onModeSwitch`.
    pub fn on_mode_switch(&self) {
        if self.mode() != CommandBlockMode::Auto {
            return;
        }
        if self.is_powered() || self.is_automatic() {
            self.schedule_tick();
        }
    }

    fn schedule_tick(&self) {
        let Some(world) = self.get_level() else {
            return;
        };
        let block = self.get_block_state().get_block();
        if !is_command_block(block) {
            return;
        }
        self.mark_condition_met();
        world.schedule_block_tick_default(self.get_block_pos(), block, 1);
    }

    /// Returns whether the last attempt found its condition satisfied.
    ///
    /// Vanilla parity: `CommandBlockEntity.wasConditionMet`.
    #[must_use]
    pub fn was_condition_met(&self) -> bool {
        self.flags.lock().condition_met
    }

    /// Re-evaluates the condition and returns it.
    ///
    /// Vanilla parity: `CommandBlockEntity.markConditionMet`. An unconditional
    /// block is always met; a conditional one is met only when the command
    /// block *behind* it -- opposite the way it faces -- last succeeded.
    pub fn mark_condition_met(&self) -> bool {
        let met = self.evaluate_condition();
        self.flags.lock().condition_met = met;
        met
    }

    fn evaluate_condition(&self) -> bool {
        if !self.is_conditional() {
            return true;
        }
        let Some(world) = self.get_level() else {
            return false;
        };
        let pos = self.get_block_pos();
        let Some(facing) = self
            .get_block_state()
            .try_get_value(&BlockStateProperties::FACING)
        else {
            return false;
        };
        let behind = pos.relative(facing.opposite());
        if !is_command_block(world.get_block_state(behind).get_block()) {
            return false;
        }
        world
            .get_block_entity(behind)
            .and_then(|entity| {
                foton_utils::Downcast::downcast_ref::<Self>(entity.as_ref())
                    .map(|block| block.command_block.success_count() > 0)
            })
            .unwrap_or(false)
    }

    /// Returns when this block runs.
    ///
    /// Vanilla parity: `CommandBlockEntity.getMode`, which reads the block
    /// rather than any stored field.
    #[must_use]
    pub fn mode(&self) -> CommandBlockMode {
        match self.get_block_state().get_block() {
            block if block == &vanilla_blocks::REPEATING_COMMAND_BLOCK => CommandBlockMode::Auto,
            block if block == &vanilla_blocks::CHAIN_COMMAND_BLOCK => CommandBlockMode::Sequence,
            _ => CommandBlockMode::Redstone,
        }
    }

    /// Returns whether this block only runs when the one behind it succeeded.
    ///
    /// Vanilla parity: `CommandBlockEntity.isConditional`.
    ///
    /// Vanilla re-reads the world here. Foton reads the state this block entity
    /// caches, which `on_block_state_changed` keeps current -- the same answer,
    /// and one that still works when the entity has no world, as it does in a
    /// unit test and briefly during a load.
    #[must_use]
    pub fn is_conditional(&self) -> bool {
        let state = self.get_block_state();
        if !is_command_block(state.get_block()) {
            return false;
        }
        state
            .try_get_value(&BlockStateProperties::CONDITIONAL)
            .unwrap_or(false)
    }

    /// Sends this block's editor data to everyone tracking it.
    ///
    /// Vanilla parity: the `sendBlockUpdated` of `CommandBlockEntity`'s
    /// `onUpdated`, which is how an open editor learns the new output.
    pub fn broadcast_update(&self, world: &Arc<World>) {
        let mut nbt = NbtCompound::new();
        self.save_additional(&mut nbt);
        world.broadcast_block_entity_update(self.get_block_pos(), self.get_type(), nbt);
    }
}

impl CommandBlockEntity {
    /// Returns where the command runs from.
    ///
    /// Vanilla parity: the `Vec3.atCenterOf(worldPosition)` of
    /// `createCommandSourceStack`.
    #[must_use]
    pub fn command_source_position(&self) -> DVec3 {
        let pos = self.get_block_pos();
        DVec3::new(
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()) + 0.5,
            f64::from(pos.z()) + 0.5,
        )
    }

    /// Returns the rotation the command runs with.
    ///
    /// Vanilla parity: the `new Vec2(0.0F, facing.toYRot())` of
    /// `createCommandSourceStack` -- a command block looks along its face and
    /// never up or down, which is what `^ ^ ^` coordinates resolve against.
    #[must_use]
    pub fn command_source_rotation(&self) -> (f32, f32) {
        let facing = self
            .get_block_state()
            .try_get_value(&BlockStateProperties::FACING)
            .unwrap_or(Direction::North);
        (facing_to_y_rotation(facing), 0.0)
    }
}

impl BlockEntity for CommandBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.command_block.save(nbt);
        let flags = self.flags.lock();
        nbt.insert("powered", flags.powered);
        nbt.insert("conditionMet", flags.condition_met);
        nbt.insert("auto", flags.auto);
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let view: NbtCompoundView<'_, '_> = nbt.into();
        self.command_block.load(view);
        {
            let mut flags = self.flags.lock();
            flags.powered = view.byte("powered").is_some_and(|value| value != 0);
            flags.condition_met = view.byte("conditionMet").is_some_and(|value| value != 0);
        }
        self.set_automatic(view.byte("auto").is_some_and(|value| value != 0));
    }

    /// Vanilla parity: `openCommandBlock` sends `saveCustomOnly`, and the chunk
    /// packet carries the same shape, so the editor can be filled from either.
    fn get_update_tag(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        self.save_additional(&mut nbt);
        Some(nbt)
    }
}

/// Returns whether `block` is one of the three command blocks.
///
/// Vanilla parity: the `instanceof CommandBlock` checks of `CommandBlockEntity`.
#[must_use]
pub fn is_command_block(block: BlockRef) -> bool {
    block == &vanilla_blocks::COMMAND_BLOCK
        || block == &vanilla_blocks::REPEATING_COMMAND_BLOCK
        || block == &vanilla_blocks::CHAIN_COMMAND_BLOCK
}

/// Returns the yaw a block face points along.
///
/// Vanilla parity: `Direction.toYRot`, which reports zero for the vertical
/// faces because a yaw cannot express them.
const fn facing_to_y_rotation(facing: Direction) -> f32 {
    match facing {
        Direction::West => 90.0,
        Direction::North => 180.0,
        Direction::East => 270.0,
        // Vanilla parity: a vertical face has no yaw, and `toYRot` reports
        // zero for it -- the same answer as south.
        Direction::South | Direction::Down | Direction::Up => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use foton_registry::init_vanilla_registry;
    use simdnbt::borrow::read_compound as read_borrowed_compound;

    use super::*;

    fn command_block(state: BlockStateId) -> CommandBlockEntity {
        CommandBlockEntity::new(Weak::new(), BlockPos::new(8, 64, 8), state)
    }

    fn reload(state: BlockStateId, nbt: &NbtCompound) -> CommandBlockEntity {
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
            .unwrap_or_else(|error| panic!("test nbt should reborrow: {error}"));
        let entity = command_block(state);
        entity.load_additional(&borrowed);
        entity
    }

    /// The mode is read off the block, not stored, so the three command blocks
    /// have to answer differently for the same block entity type.
    #[test]
    fn the_mode_comes_from_which_command_block_is_standing_there() {
        init_vanilla_registry();
        assert_eq!(
            command_block(vanilla_blocks::COMMAND_BLOCK.default_state()).mode(),
            CommandBlockMode::Redstone
        );
        assert_eq!(
            command_block(vanilla_blocks::REPEATING_COMMAND_BLOCK.default_state()).mode(),
            CommandBlockMode::Auto
        );
        assert_eq!(
            command_block(vanilla_blocks::CHAIN_COMMAND_BLOCK.default_state()).mode(),
            CommandBlockMode::Sequence
        );
    }

    /// Choosing a mode in the editor swaps the block, and reading the block
    /// back gives the mode again. The two directions have to be inverses: if
    /// `block()` were wrong, picking "repeating" would place a chain block and
    /// the editor would reopen showing the wrong mode.
    #[test]
    fn the_mode_and_the_block_it_is_stored_as_are_inverses() {
        init_vanilla_registry();
        for mode in [
            CommandBlockMode::Sequence,
            CommandBlockMode::Auto,
            CommandBlockMode::Redstone,
        ] {
            let placed = command_block(mode.block().default_state());
            assert_eq!(placed.mode(), mode, "round trip through {mode:?}");
        }
    }

    /// `readEnum` is ordinal order, and getting it wrong would silently turn
    /// every repeating block a client saves into a chain block.
    #[test]
    fn the_wire_mode_order_is_sequence_auto_redstone() {
        assert_eq!(
            CommandBlockMode::from_wire(0),
            Some(CommandBlockMode::Sequence)
        );
        assert_eq!(CommandBlockMode::from_wire(1), Some(CommandBlockMode::Auto));
        assert_eq!(
            CommandBlockMode::from_wire(2),
            Some(CommandBlockMode::Redstone)
        );
        assert_eq!(CommandBlockMode::from_wire(3), None);
        assert_eq!(CommandBlockMode::from_wire(-1), None);
    }

    /// All three flags persist. A repeating block that forgot `auto` would stop
    /// running after a chunk reload, which is the classic broken-map symptom.
    #[test]
    fn the_three_flags_survive_a_save() {
        init_vanilla_registry();
        let entity = command_block(vanilla_blocks::COMMAND_BLOCK.default_state());
        entity.set_powered(true);
        entity.set_automatic(true);
        entity.mark_condition_met();
        entity.command_block().set_command("say hi".to_owned());

        let mut nbt = NbtCompound::new();
        entity.save_additional(&mut nbt);
        assert_eq!(nbt.byte("powered"), Some(1));
        assert_eq!(nbt.byte("auto"), Some(1));
        assert_eq!(nbt.byte("conditionMet"), Some(1));

        let reloaded = reload(vanilla_blocks::COMMAND_BLOCK.default_state(), &nbt);
        assert!(reloaded.is_powered());
        assert!(reloaded.is_automatic());
        assert!(reloaded.was_condition_met());
        assert_eq!(reloaded.command_block().command(), "say hi");
    }

    /// An unconditional block is always met without consulting the world, which
    /// is why `markConditionMet` works off-level at all.
    #[test]
    fn an_unconditional_block_is_always_met() {
        init_vanilla_registry();
        let entity = command_block(vanilla_blocks::COMMAND_BLOCK.default_state());
        assert!(entity.mark_condition_met());
        assert!(entity.was_condition_met());
    }

    /// A conditional block with nothing behind it is not met. Off-level there
    /// is no block behind it at all, so this is also the no-world case.
    #[test]
    fn a_conditional_block_with_nothing_behind_it_is_not_met() {
        init_vanilla_registry();
        let conditional = vanilla_blocks::COMMAND_BLOCK
            .default_state()
            .set_value(&BlockStateProperties::CONDITIONAL, true);
        let entity = command_block(conditional);
        assert!(!entity.mark_condition_met());
    }
}
