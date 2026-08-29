//! The command block minecart.
//!
//! Vanilla parity: `MinecartCommandBlock`. A rolling command block: it holds
//! the same [`BaseCommandBlock`] the block does, and an activator rail runs it
//! rather than a redstone edge. Its four-tick cooldown is what stops a cart
//! sitting on a rail firing sixty times a second.
//!
//! Unlike the block it has no mode and no conditional flag, and its editor
//! opens on interaction rather than on a right-click packet.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::vanilla_entity_data::MinecartCommandBlockEntityData;
use foton_registry::vanilla_game_rules::COMMAND_BLOCKS_WORK;
use foton_utils::locks::SyncMutex;
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use text_components::TextComponent;

use super::minecart_common::{self, MinecartLike, MinecartState};
use crate::behavior::InteractionResult;
use crate::command::base_command_block::BaseCommandBlock;
use crate::command::execution::CommandSource;
use crate::entity::{Entity, EntityBase, EntityBaseLoad, EntityMovementEmission, EntitySyncedData};
use crate::player::Player;
use crate::world::World;

/// Vanilla parity: `AbstractMinecart.getDefaultGravity`.
const MINECART_GRAVITY: f64 = 0.04;

/// And the same in water.
const MINECART_GRAVITY_IN_WATER: f64 = 0.005;

/// Ticks between two runs off the same rail.
///
/// Vanilla parity: `MinecartCommandBlock.ACTIVATION_DELAY`.
const ACTIVATION_DELAY: i32 = 4;

/// A command block minecart.
#[entity_behavior(class = "MinecartCommandBlock")]
pub struct MinecartCommandBlockEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<MinecartCommandBlockEntityData>,
    minecart: SyncMutex<MinecartState>,
    command_block: Arc<BaseCommandBlock>,
    /// The tick the command last ran on.
    last_activated: AtomicI32,
}

// SAFETY: This key is owned by Foton and uniquely identifies
// `MinecartCommandBlockEntity`.
unsafe impl DowncastType for MinecartCommandBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/command_block_minecart");
}

impl MinecartCommandBlockEntity {
    /// Creates one at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates one from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self::new_with_base(
            EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
        )
    }

    fn new_with_base(base: EntityBase, entity_type: EntityTypeRef) -> Self {
        Self {
            base,
            entity_type,
            entity_data: SyncMutex::new(MinecartCommandBlockEntityData::new()),
            minecart: SyncMutex::new(MinecartState::default()),
            command_block: Arc::new(BaseCommandBlock::new()),
            last_activated: AtomicI32::new(0),
        }
    }

    /// Returns the command store.
    ///
    /// Vanilla parity: `MinecartCommandBlock.getCommandBlock`.
    #[must_use]
    pub const fn command_block(&self) -> &Arc<BaseCommandBlock> {
        &self.command_block
    }

    /// Mirrors the stored command and output into synced entity data.
    ///
    /// Vanilla parity: `MinecartCommandBase.onUpdated`, which is how the
    /// client's editor is filled -- a minecart has no block entity to send.
    pub fn publish_command_to_clients(&self) {
        let command = self.command_block.command();
        let output = self.command_block.last_output();
        let mut data = self.entity_data.lock();
        data.id_command_name.set(command);
        data.id_last_output.set(Box::new(output));
    }
}

impl Entity for MinecartCommandBlockEntity {
    fn is_minecart(&self) -> bool {
        true
    }

    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn tick(&self) {
        minecart_common::tick_minecart(self);
    }

    fn get_default_gravity(&self) -> f64 {
        if self.is_in_water() {
            MINECART_GRAVITY_IN_WATER
        } else {
            MINECART_GRAVITY
        }
    }

    fn blocks_building(&self) -> bool {
        true
    }

    fn is_pushable(&self) -> bool {
        true
    }

    fn is_pickable(&self) -> bool {
        !self.is_removed()
    }

    fn movement_emission(&self) -> EntityMovementEmission {
        EntityMovementEmission::Events
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    /// Opens the editor for a gamemaster.
    ///
    /// Vanilla parity: `MinecartCommandBlock.interact`, which opens the screen
    /// on the client and only checks the permission on the server. The client
    /// fills the screen from the synced data this cart already publishes, so
    /// there is no packet to send.
    fn interact(
        &self,
        player: &Player,
        _hand: InteractionHand,
        _location: DVec3,
    ) -> InteractionResult {
        if !player.can_use_game_master_blocks() {
            return InteractionResult::Pass;
        }
        InteractionResult::Success
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.command_block.save(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.command_block.load(nbt);
        // Vanilla parity: `readAdditionalSaveData` republishes both synced
        // fields, so a reloaded cart's editor is not blank.
        self.publish_command_to_clients();
    }
}

impl MinecartLike for MinecartCommandBlockEntity {
    fn minecart_state(&self) -> &SyncMutex<MinecartState> {
        &self.minecart
    }

    /// Runs the stored command when an activator rail powers up under the cart.
    ///
    /// Vanilla parity: `MinecartCommandBlock.activateMinecart`, cooldown
    /// included.
    fn activate_minecart(&self, world: &Arc<World>, _pos: BlockPos, powered: bool) {
        if !powered {
            return;
        }
        let tick_count = self.tick_count();
        if tick_count - self.last_activated.load(Ordering::Relaxed) < ACTIVATION_DELAY {
            return;
        }
        self.last_activated.store(tick_count, Ordering::Relaxed);
        perform_minecart_command(world, &self.command_block, self.position(), self.rotation());
        // Vanilla publishes from inside the command source, once per output
        // line; one packet after the command leaves the client in the same state.
        self.publish_command_to_clients();
    }
}

/// Runs a command block minecart's command.
///
/// Vanilla parity: `BaseCommandBlock.performCommand`, the same body the block
/// runs. It lives here rather than on `BaseCommandBlock` because running a
/// command needs the server, and the store is below the server in the crate.
fn perform_minecart_command(
    world: &Arc<World>,
    command_block: &Arc<BaseCommandBlock>,
    position: DVec3,
    rotation: (f32, f32),
) {
    let game_time = world.game_time();
    if command_block.already_ran_at(game_time) {
        return;
    }

    let command = command_block.command();
    if command.eq_ignore_ascii_case("Searge") {
        command_block.set_last_output(Some(TextComponent::plain("#itzlipofutzli")));
        command_block.set_success_count(1);
        return;
    }

    command_block.set_success_count(0);
    if world.get_game_rule(&COMMAND_BLOCKS_WORK) && !command.is_empty() {
        command_block.set_last_output(None);
        if let Some(server) = world.server() {
            let source = CommandSource::for_command_block(
                Arc::clone(command_block),
                Arc::clone(&server),
                Arc::clone(world),
                position,
                rotation,
            );
            let successes = server.run_command_now(source, &command);
            command_block.set_success_count(successes);
        }
    }

    command_block.mark_ran_at(game_time);
}
