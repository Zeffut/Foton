//! The four states a vault moves through.
//!
//! Vanilla parity:
//! `net.minecraft.world.level.block.entity.vault.VaultState`.

use std::sync::Arc;

use glam::DVec3;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::properties::VaultState;
use steel_registry::item_stack::ItemStack;
use steel_registry::loot_table::{LootContext, LootTableRef};
use steel_registry::{level_events, sound_events};
use steel_utils::{BlockPos, Direction};

use super::config::VaultConfig;
use super::server_data::VaultServerData;
use super::shared_data::VaultSharedData;
use crate::behavior::item_utils::spawn_item_toward;
use crate::world::World;

/// Vanilla parity: `VaultState.UPDATE_CONNECTED_PLAYERS_TICK_RATE`, and the
/// three ejection delays, all twenty.
const STATE_UPDATE_TICK_RATE: i64 = 20;

/// Vanilla parity: the `relative(Direction.UP, 1.2)` of `ejectResultItem`.
const EJECT_HEIGHT: f64 = 1.2;

/// Vanilla parity: the `accuracy` argument of the eject's `spawnItem`.
const EJECT_ACCURACY: i32 = 2;

/// The light one vault state gives off.
pub trait VaultStateExt {
    /// Vanilla parity: `VaultState.lightLevel`.
    fn light_level(self) -> i32;
}

impl VaultStateExt for VaultState {
    fn light_level(self) -> i32 {
        match self {
            // Vanilla parity: `VaultState.LightLevel.HALF_LIT`.
            Self::Inactive => 6,
            // Vanilla parity: `VaultState.LightLevel.LIT`.
            Self::Active | Self::Unlocking | Self::Ejecting => 12,
        }
    }
}

/// Runs one state's tick.
///
/// Vanilla parity: `VaultState.tickAndGetNext`.
pub(super) fn tick_and_get_next(
    state: VaultState,
    world: &Arc<World>,
    pos: BlockPos,
    config: &VaultConfig,
    server_data: &mut VaultServerData,
    shared_data: &mut VaultSharedData,
) -> VaultState {
    match state {
        VaultState::Inactive => update_state_for_connected_players(
            world,
            pos,
            config,
            server_data,
            shared_data,
            config.activation_range,
        ),
        VaultState::Active => update_state_for_connected_players(
            world,
            pos,
            config,
            server_data,
            shared_data,
            config.deactivation_range,
        ),
        VaultState::Unlocking => {
            server_data.pause_state_updating_until(world.game_time() + STATE_UPDATE_TICK_RATE);
            VaultState::Ejecting
        }
        VaultState::Ejecting => {
            if server_data.items_to_eject().is_empty() {
                server_data.mark_ejection_finished();
                return update_state_for_connected_players(
                    world,
                    pos,
                    config,
                    server_data,
                    shared_data,
                    config.deactivation_range,
                );
            }
            let progress = server_data.ejection_progress();
            let item = server_data.pop_next_item_to_eject();
            eject_result_item(world, pos, item, progress);
            shared_data.set_display_item(server_data.next_item_to_eject());
            // Vanilla computes `isLastEjection` and then uses twenty either
            // way; the branch is kept out rather than kept dead.
            server_data.pause_state_updating_until(world.game_time() + STATE_UPDATE_TICK_RATE);
            VaultState::Ejecting
        }
    }
}

/// Vanilla parity: `VaultState.updateStateForConnectedPlayers`.
fn update_state_for_connected_players(
    world: &Arc<World>,
    pos: BlockPos,
    config: &VaultConfig,
    server_data: &mut VaultServerData,
    shared_data: &mut VaultSharedData,
    activation_range: f64,
) -> VaultState {
    shared_data.update_connected_players_within_range(
        world,
        pos,
        server_data,
        config,
        activation_range,
    );
    server_data.pause_state_updating_until(world.game_time() + STATE_UPDATE_TICK_RATE);
    if shared_data.has_connected_players() {
        VaultState::Active
    } else {
        VaultState::Inactive
    }
}

/// Vanilla parity: `VaultState.onTransition`, which exits the old state and
/// then enters the new one.
pub(super) fn on_transition(
    from: VaultState,
    to: VaultState,
    world: &Arc<World>,
    pos: BlockPos,
    config: &VaultConfig,
    shared_data: &mut VaultSharedData,
    is_ominous: bool,
) {
    // Vanilla parity: only `EJECTING` overrides `onExit`.
    if from == VaultState::Ejecting {
        world.play_sound(
            &sound_events::BLOCK_VAULT_CLOSE_SHUTTER,
            SoundSource::Blocks,
            pos,
            1.0,
            1.0,
            None,
        );
    }

    match to {
        VaultState::Inactive => {
            shared_data.set_display_item(ItemStack::empty());
            world.level_event(
                level_events::ANIMATION_VAULT_DEACTIVATE,
                pos,
                i32::from(is_ominous),
                None,
            );
        }
        VaultState::Active => {
            if !shared_data.has_display_item() {
                cycle_display_item_from_loot_table(world, to, config, shared_data, pos);
            }
            world.level_event(
                level_events::ANIMATION_VAULT_ACTIVATE,
                pos,
                i32::from(is_ominous),
                None,
            );
        }
        VaultState::Unlocking => world.play_sound(
            &sound_events::BLOCK_VAULT_INSERT_ITEM,
            SoundSource::Blocks,
            pos,
            1.0,
            1.0,
            None,
        ),
        VaultState::Ejecting => world.play_sound(
            &sound_events::BLOCK_VAULT_OPEN_SHUTTER,
            SoundSource::Blocks,
            pos,
            1.0,
            1.0,
            None,
        ),
    }
}

/// Vanilla parity: `VaultBlockEntity.Server.cycleDisplayItemFromLootTable`.
pub(super) fn cycle_display_item_from_loot_table(
    world: &Arc<World>,
    state: VaultState,
    config: &VaultConfig,
    shared_data: &mut VaultSharedData,
    pos: BlockPos,
) {
    if !can_eject_reward(config, state) {
        shared_data.set_display_item(ItemStack::empty());
        return;
    }
    let table = config
        .override_loot_table_to_display
        .unwrap_or(config.loot_table);
    shared_data.set_display_item(random_display_item(world, pos, table));
}

/// Vanilla parity: `VaultBlockEntity.Server.getRandomDisplayItemFromLootTable`.
fn random_display_item(world: &Arc<World>, pos: BlockPos, table: LootTableRef) -> ItemStack {
    let mut rng = rand::rng();
    let mut context = LootContext::new(&mut rng)
        .with_origin(
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()) + 0.5,
            f64::from(pos.z()) + 0.5,
        )
        .with_game_time(world.game_time());
    let results = table.get_random_items(&mut context);
    if results.is_empty() {
        return ItemStack::empty();
    }
    results[rand::random_range(0..results.len())].clone()
}

/// Vanilla parity: `VaultBlockEntity.Server.canEjectReward`.
pub(super) fn can_eject_reward(config: &VaultConfig, state: VaultState) -> bool {
    !config.key_item.is_empty() && state != VaultState::Inactive
}

/// Vanilla parity: `VaultState.ejectResultItem`.
fn eject_result_item(world: &Arc<World>, pos: BlockPos, item: ItemStack, progress: f32) {
    let origin = DVec3::new(
        f64::from(pos.x()) + 0.5,
        f64::from(pos.y()) + EJECT_HEIGHT,
        f64::from(pos.z()) + 0.5,
    );
    spawn_item_toward(world, origin, Direction::Up, EJECT_ACCURACY, item);
    world.level_event(level_events::ANIMATION_VAULT_EJECT_ITEM, pos, 0, None);
    world.play_sound(
        &sound_events::BLOCK_VAULT_EJECT_ITEM,
        SoundSource::Blocks,
        pos,
        1.0,
        0.4f32.mul_add(progress, 0.8),
        None,
    );
}
