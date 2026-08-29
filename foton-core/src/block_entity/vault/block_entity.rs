//! The vault's block entity.
//!
//! Vanilla parity:
//! `net.minecraft.world.level.block.entity.vault.VaultBlockEntity`. Vanilla's
//! nested `Client` class is absent -- it is particles, a spinning display item
//! and an ambient sound, all written only on the client.

use std::sync::{Arc, Weak};

use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::{BlockStateProperties, VaultState};
use foton_registry::item_stack::ItemStack;
use foton_registry::loot_table::LootContext;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::{sound_events, vanilla_block_entity_types};
use foton_utils::locks::SyncMutex;
use foton_utils::types::UpdateFlags;
use foton_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::NbtCompound;

use super::config::VaultConfig;
use super::server_data::VaultServerData;
use super::shared_data::VaultSharedData;
use super::state::{can_eject_reward, cycle_display_item_from_loot_table, on_transition};
use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::entity::{Entity as _, LivingEntity as _, entity_loot_ref};
use crate::player::Player;
use crate::world::World;
use foton_registry::stat::Stat;
use foton_registry::vanilla_stat_types;

/// Vanilla parity: `VaultBlockEntity.Server.UNLOCKING_DELAY_TICKS`.
const UNLOCKING_DELAY_TICKS: i64 = 14;
/// Vanilla parity: `VaultBlockEntity.Server.DISPLAY_CYCLE_TICK_RATE`.
const DISPLAY_CYCLE_TICK_RATE: i64 = 20;
/// Vanilla parity: `VaultBlockEntity.Server.INSERT_FAIL_SOUND_BUFFER_TICKS`.
const INSERT_FAIL_SOUND_BUFFER_TICKS: i64 = 15;

/// The three pieces of a vault, kept together because every tick touches all of them.
struct VaultData {
    config: VaultConfig,
    server: VaultServerData,
    shared: VaultSharedData,
}

/// Vanilla `VaultBlockEntity`.
pub struct VaultBlockEntity {
    base: BlockEntityBase,
    data: SyncMutex<VaultData>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `VaultBlockEntity`.
unsafe impl DowncastType for VaultBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:block_entity/vault");
}

impl VaultBlockEntity {
    /// Creates the storage behind one vault block.
    #[must_use]
    pub fn new(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(&vanilla_block_entity_types::VAULT, world, pos, state),
            data: SyncMutex::new(VaultData {
                config: VaultConfig::default(),
                server: VaultServerData::default(),
                shared: VaultSharedData::default(),
            }),
        }
    }

    /// Returns whether `player` has already been paid by this vault.
    #[must_use]
    pub fn has_rewarded_player(&self, player: &Player) -> bool {
        self.data.lock().server.has_rewarded_player(player.uuid())
    }

    /// Returns the item the vault is currently showing off.
    #[must_use]
    pub fn display_item(&self) -> ItemStack {
        let data = self.data.lock();
        data.shared.display_item().clone()
    }

    /// How many of the key item one unlock costs.
    ///
    /// Vanilla parity: the `config.keyItem().getCount()` of `tryInsertKey`.
    #[must_use]
    pub fn key_item_count(&self) -> i32 {
        self.data.lock().config.key_item.count()
    }

    /// Replaces the configuration, for `/setblock` and for tests.
    pub fn set_config(&self, config: VaultConfig) {
        self.data.lock().config = config;
    }

    /// Runs one server tick.
    ///
    /// Vanilla parity: `VaultBlockEntity.Server.tick`.
    fn server_tick(&self, world: &Arc<World>, pos: BlockPos) {
        let block_state = world.get_block_state(pos);
        let Some(current_state) = block_state.try_get_value(&BlockStateProperties::VAULT_STATE)
        else {
            return;
        };
        let game_time = world.game_time();

        let mut data = self.data.lock();
        let VaultData {
            config,
            server,
            shared,
        } = &mut *data;

        if game_time % DISPLAY_CYCLE_TICK_RATE == 0 && current_state == VaultState::Active {
            cycle_display_item_from_loot_table(world, current_state.clone(), config, shared, pos);
        }

        if game_time >= server.state_updating_resumes_at() {
            let next_state =
                super::state::tick_and_get_next(current_state, world, pos, config, server, shared);
            let next_block_state =
                block_state.set_value(&BlockStateProperties::VAULT_STATE, next_state);
            if next_block_state != block_state {
                set_vault_state(world, pos, block_state, next_block_state, config, shared);
            }
        }

        if server.is_dirty || shared.is_dirty {
            self.set_changed();
            if shared.is_dirty {
                world.send_block_updated(pos);
            }
            server.is_dirty = false;
            shared.is_dirty = false;
        }
    }

    /// Tries to unlock the vault with the item a player is holding.
    ///
    /// Vanilla parity: `VaultBlockEntity.Server.tryInsertKey`. Returns whether
    /// the key was taken, so the caller can shrink the stack -- vanilla does the
    /// shrink inside, but Foton's held item lives behind the inventory guard the
    /// block behavior holds.
    pub fn try_insert_key(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        stack_to_insert: &ItemStack,
    ) -> bool {
        let block_state = world.get_block_state(pos);
        let Some(vault_state) = block_state.try_get_value(&BlockStateProperties::VAULT_STATE)
        else {
            return false;
        };

        let mut data = self.data.lock();
        if !can_eject_reward(&data.config, vault_state) {
            return false;
        }
        if !is_valid_to_insert(&data.config, stack_to_insert) {
            play_insert_fail_sound(
                world,
                &mut data.server,
                pos,
                &sound_events::BLOCK_VAULT_INSERT_ITEM_FAIL,
            );
            return false;
        }
        if data.server.has_rewarded_player(player.uuid()) {
            play_insert_fail_sound(
                world,
                &mut data.server,
                pos,
                &sound_events::BLOCK_VAULT_REJECT_REWARDED_PLAYER,
            );
            return false;
        }

        let items_to_eject =
            resolve_items_to_eject(world, &data.config, pos, player, stack_to_insert);
        if items_to_eject.is_empty() {
            return false;
        }

        player.award_stat(Stat::new(&vanilla_stat_types::USED, stack_to_insert.item));
        let VaultData {
            config,
            server,
            shared,
        } = &mut *data;
        server.set_items_to_eject(items_to_eject);
        shared.set_display_item(server.next_item_to_eject());
        server.pause_state_updating_until(world.game_time() + UNLOCKING_DELAY_TICKS);
        let unlocking =
            block_state.set_value(&BlockStateProperties::VAULT_STATE, VaultState::Unlocking);
        set_vault_state(world, pos, block_state, unlocking, config, shared);
        server.add_to_rewarded_players(player.uuid());
        let deactivation_range = config.deactivation_range;
        shared.update_connected_players_within_range(
            world,
            pos,
            server,
            config,
            deactivation_range,
        );
        true
    }
}

/// Vanilla parity: `VaultBlockEntity.Server.setVaultState`.
fn set_vault_state(
    world: &Arc<World>,
    pos: BlockPos,
    current: BlockStateId,
    next: BlockStateId,
    config: &VaultConfig,
    shared: &mut VaultSharedData,
) {
    let Some(current_vault_state) = current.try_get_value(&BlockStateProperties::VAULT_STATE)
    else {
        return;
    };
    let Some(next_vault_state) = next.try_get_value(&BlockStateProperties::VAULT_STATE) else {
        return;
    };
    world.set_block(pos, next, UpdateFlags::UPDATE_ALL);
    let is_ominous = next
        .try_get_value(&BlockStateProperties::OMINOUS)
        .unwrap_or(false);
    on_transition(
        current_vault_state,
        next_vault_state,
        world,
        pos,
        config,
        shared,
        is_ominous,
    );
}

/// Vanilla parity: `VaultBlockEntity.Server.isValidToInsert`.
fn is_valid_to_insert(config: &VaultConfig, stack_to_insert: &ItemStack) -> bool {
    ItemStack::is_same_item_same_components(stack_to_insert, &config.key_item)
        && stack_to_insert.count() >= config.key_item.count()
}

/// Vanilla parity: `VaultBlockEntity.Server.resolveItemsToEject`.
fn resolve_items_to_eject(
    world: &Arc<World>,
    config: &VaultConfig,
    pos: BlockPos,
    player: &Player,
    inserted: &ItemStack,
) -> Vec<ItemStack> {
    let mut rng = rand::rng();
    let mut context = LootContext::new(&mut rng)
        .with_origin(
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()) + 0.5,
            f64::from(pos.z()) + 0.5,
        )
        .with_luck(player.get_luck())
        .with_this_entity(entity_loot_ref(player))
        .with_tool(inserted)
        .with_game_time(world.game_time());
    config.loot_table.get_random_items(&mut context)
}

/// Vanilla parity: `VaultBlockEntity.Server.playInsertFailSound`, whose buffer
/// stops a player who spams the wrong item from machine-gunning the sound.
fn play_insert_fail_sound(
    world: &Arc<World>,
    server: &mut VaultServerData,
    pos: BlockPos,
    sound: SoundEventRef,
) {
    if world.game_time() < server.last_insert_fail_timestamp() + INSERT_FAIL_SOUND_BUFFER_TICKS {
        return;
    }
    world.play_sound(sound, SoundSource::Blocks, pos, 1.0, 1.0, None);
    server.set_last_insert_fail_timestamp(world.game_time());
}

impl BlockEntity for VaultBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let view: NbtCompoundView<'_, '_> = nbt.into();
        let mut data = self.data.lock();
        if let Some(server) = view.compound("server_data") {
            data.server.load(&server);
        }
        data.config = view
            .compound("config")
            .map_or_else(VaultConfig::default, |config| VaultConfig::load(&config));
        if let Some(shared) = view.compound("shared_data") {
            data.shared.load(&shared);
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let data = self.data.lock();
        nbt.insert("config", data.config.save());
        nbt.insert("shared_data", data.shared.save());
        nbt.insert("server_data", data.server.save());
    }

    /// Vanilla parity: `VaultBlockEntity.getUpdateTag`, which sends the shared
    /// half and nothing else.
    fn get_update_tag(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        nbt.insert("shared_data", self.data.lock().shared.save());
        Some(nbt)
    }

    fn tick(&self, world: &Arc<World>) {
        self.server_tick(world, self.get_block_pos());
    }
}
