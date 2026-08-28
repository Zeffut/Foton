//! Persistent player data structures.
//!
//! This module defines the data format for saving and loading player state.

use std::io::Cursor;
use std::str::FromStr as _;

use rustc_hash::FxHashSet;
use simdnbt::borrow::read_compound as read_borrowed_compound;
use simdnbt::owned::NbtCompound;
use steel_registry::item_stack::ItemStack;
use steel_registry::stat::{Stat, StatValueRegistry};
use steel_registry::{REGISTRY, RegistryEntry as _, RegistryExt as _};
use steel_utils::Identifier;
use steel_utils::types::GameType;

use crate::{
    chunk_saver::{ChunkStorage, PersistentEntity},
    entity::entities::WardenSpawnTracker,
    entity::{Entity, EntityFireFreezeState, LivingEntity},
    inventory::container::Container,
};

use super::{
    ENDER_CHEST_SLOTS, Player, PlayerRespawnConfig, abilities::Abilities, experience::Experience,
    food_data::FoodData, player_inventory::PlayerInventory,
};

/// Current data version for player saves.
/// Increment when making breaking changes to the format.
pub const PLAYER_DATA_VERSION: i32 = 11;

/// Persistent player data saved by Steel's storage backend.
///
/// This is Steel's runtime save snapshot. Vanilla import/export should live outside
/// server runtime storage so compatibility logic does not constrain the native format.
#[derive(Debug, Clone)]
pub struct PersistentPlayerData {
    /// Position (x, y, z) in absolute world coordinates.
    pub pos: [f64; 3],

    /// Velocity (x, y, z) in blocks per tick.
    pub motion: [f64; 3],

    /// Rotation (yaw, pitch) in degrees.
    pub rotation: [f32; 2],

    /// Whether the player is on the ground.
    pub on_ground: bool,

    /// Whether the player is elytra gliding.
    pub fall_flying: bool,

    /// Vanilla `remainingFireTicks`.
    pub remaining_fire_ticks: i32,

    /// Synchronized vanilla `TicksFrozen`.
    pub ticks_frozen: i32,

    /// Vanilla `isInPowderSnow`.
    pub is_in_powder_snow: bool,

    /// Vanilla `wasInPowderSnow`.
    pub was_in_powder_snow: bool,

    /// Vanilla `hasVisualFire`.
    pub has_visual_fire: bool,

    /// Current health points.
    pub health: f32,

    /// Current game mode (0=survival, 1=creative, 2=adventure, 3=spectator).
    pub game_mode: i32,

    /// Previous game mode of the player, or `None` if vanilla has not recorded one yet.
    pub prev_game_mode: Option<i32>,

    /// Player abilities (flight, invulnerability, etc.).
    pub abilities: PersistentAbilities,

    /// Inventory items with slot indices.
    pub inventory: Vec<PersistentSlot>,

    /// Ender chest items with slot indices.
    ///
    /// Vanilla parity: the `EnderItems` tag. These belong to the player rather
    /// than to any block, so they are saved here and nowhere else.
    pub ender_items: Vec<PersistentSlot>,

    /// Currently selected hotbar slot (0-8).
    pub selected_slot: i32,

    /// Loaded world identifier (e.g., "minecraft:overworld").
    pub world: String,

    /// Current food level (0–20, default 20).
    pub food_level: i32,

    /// Food saturation level (0.0–`food_level`, default 5.0).
    pub food_saturation_level: f32,

    /// Accumulated food exhaustion (0.0–40.0, default 0.0).
    pub food_exhaustion_level: f32,

    /// Internal tick timer for regen/starvation (default 0).
    pub food_tick_timer: i32,

    /// Data version for format migrations.
    pub data_version: i32,

    /// Current experience level
    pub experience_level: i32,

    /// Vanilla progress toward the next experience level.
    pub experience_progress: f32,

    /// Vanilla `Player.totalExperience`, updated by point grants but independent of level/progress.
    pub experience_total: i32,

    /// Vanilla `XpSeed`, the seed an enchanting table draws its three offers
    /// from. Without it a relog silently reshuffles what the table shows.
    pub enchantment_seed: i32,

    /// Vanilla death-screen score. Point grants change it with Java `int` wrapping.
    pub score: i32,

    /// Vanilla `ServerPlayer.seenCredits`.
    pub seen_credits: bool,

    /// Vanilla `ServerPlayer.wardenSpawnTracker`, as its three saved counters.
    ///
    /// The count is what a shrieker escalates and what a warden costs, so losing it on a
    /// reload would mean the deep dark forgave every player every log-out.
    pub warden_spawn_tracker: [i32; 3],

    /// Vanilla one-player root vehicle tree stored with the player instead of chunk data.
    pub root_vehicle: Option<PersistentRootVehicle>,

    /// Vanilla per-player respawn configuration set by beds and respawn anchors.
    pub respawn_config: Option<PlayerRespawnConfig>,

    /// Vanilla in-flight ender pearls stored with the player (`ServerPlayer.enderPearls`).
    pub ender_pearls: Vec<PersistentEnderPearl>,

    /// Advancement progress, one entry per advancement with anything to save.
    ///
    /// Vanilla writes this to its own `advancements/<uuid>.json` because it has
    /// one world; Steel scopes it to the domain, beside the experience and the
    /// inventory it was earned with.
    pub advancements: Vec<PersistentAdvancement>,

    /// Statistics, keyed by the two registry keys a statistic is made of.
    ///
    /// The keys travel rather than the registry ids: an id is only meaningful
    /// against the registry that produced it, and a save has to survive a
    /// registry growing a new entry in the middle.
    pub statistics: Vec<PersistentStatistic>,

    /// The half of the save every living entity has, as a written NBT compound.
    ///
    /// Vanilla parity: the `LivingEntity.addAdditionalSaveData` a player's own
    /// `addAdditionalSaveData` sits on top of. Absorption, potion effects and
    /// attribute modifiers live nowhere else in this file, so without it a
    /// player logged back in with the shield, the effects and the modifiers
    /// gone. The keys it shares with a field above -- health, equipment -- are
    /// applied first and then overwritten by that field, which is the order
    /// vanilla's `super`-first read runs in.
    pub living_nbt: Vec<u8>,
}

/// One advancement's saved progress.
///
/// Vanilla parity: one entry of `PlayerAdvancements.Data`, whose value is an
/// `AdvancementProgress` -- and the only part of that worth keeping is which
/// criteria were met and when.
#[derive(Debug, Clone)]
pub struct PersistentAdvancement {
    /// The advancement's registry key.
    pub key: String,
    /// The criteria that have been met.
    pub criteria: Vec<PersistentCriterion>,
}

/// One met criterion and the moment it was met.
#[derive(Debug, Clone)]
pub struct PersistentCriterion {
    /// The criterion's name within its advancement.
    pub name: String,
    /// When it was met, in epoch milliseconds.
    pub obtained_epoch_millis: i64,
}

/// One statistic and its value.
///
/// Vanilla parity: one entry of the `stats` object `ServerStatsCounter` writes,
/// which is likewise keyed by the two names rather than by ids.
#[derive(Debug, Clone)]
pub struct PersistentStatistic {
    /// The stat type's registry key.
    pub stat_type: String,
    /// The value's key in the registry that stat type names.
    pub value: String,
    /// What the counter stands at.
    pub count: i32,
}

/// A vanilla `RootVehicle` tree persisted with player data.
#[derive(Debug, Clone)]
pub struct PersistentRootVehicle {
    /// UUID of the direct vehicle the player should reattach to.
    pub attach: [u8; 16],
    /// Root vehicle entity tree.
    pub entity: PersistentEntity,
}

/// A thrown ender pearl persisted with its owning player.
///
/// Mirrors a vanilla `ender_pearls` list entry: the pearl entity plus the world
/// it lives in (`ender_pearl_dimension`), so it re-spawns in its original world.
#[derive(Debug, Clone)]
pub struct PersistentEnderPearl {
    /// Key of the world the pearl lives in.
    pub world: String,
    /// Serialized pearl entity.
    pub entity: PersistentEntity,
}

/// Persistent abilities data.
#[derive(Debug, Clone)]
pub struct PersistentAbilities {
    /// Whether the player is invulnerable to damage.
    pub invulnerable: bool,
    /// Whether the player is currently flying.
    pub flying: bool,
    /// Whether the player is allowed to fly.
    pub may_fly: bool,
    /// Whether the player can instantly break blocks (creative mode).
    pub instabuild: bool,
    /// Whether the player can place/break blocks.
    pub may_build: bool,
    /// Flying speed (default 0.05).
    pub flying_speed: f32,
    /// Walking speed (default 0.1).
    pub walking_speed: f32,
}

/// An inventory slot with its index.
#[derive(Debug, Clone)]
pub struct PersistentSlot {
    /// Slot index in the inventory.
    pub slot: i8,
    /// The item stack in this slot.
    pub item: ItemStack,
}

impl PersistentPlayerData {
    /// Extracts persistent data from a live player.
    #[must_use]
    pub fn from_player(player: &Player) -> Self {
        // Before the inventory lock below, not after: a player's equipment is a
        // view onto its own inventory, so `save_living` takes that same lock to
        // read the worn items and would hang waiting on this function.
        let mut living = NbtCompound::new();
        player.save_living(&mut living);
        let mut living_nbt = Vec::new();
        living.write(&mut living_nbt);

        let pos = player.position();
        let (yaw, pitch) = player.rotation();
        let delta = player.velocity();
        let on_ground = player.on_ground();
        let fall_flying = player.is_fall_flying();
        let fire_freeze = player.fire_freeze_state();
        let abilities = player.abilities.lock();
        let inventory = player.inventory.lock();
        let food_data = player.food_data.lock();

        // Collect non-empty inventory slots
        let mut slots = Vec::new();
        // Main inventory (0-35) and equipment (36-42)
        for slot in 0..PlayerInventory::CONTAINER_SIZE {
            let item = inventory.get_item(slot);
            if !item.is_empty() {
                slots.push(PersistentSlot {
                    slot: slot as i8,
                    item: item.clone(),
                });
            }
        }

        let mut ender_slots = Vec::new();
        {
            let ender_chest = player.ender_chest.lock();
            for slot in 0..ENDER_CHEST_SLOTS {
                let item = ender_chest.get_item(slot);
                if !item.is_empty() {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "there are twenty-seven slots"
                    )]
                    ender_slots.push(PersistentSlot {
                        slot: slot as i8,
                        item: item.clone(),
                    });
                }
            }
        }

        let (experience_level, experience_progress, experience_total) = {
            let lock = player.experience.lock();
            (lock.level(), lock.progress(), lock.total_points())
        };
        let score = player.score();
        let advancements = Self::advancements_from_player(player);
        let statistics = Self::statistics_from_player(player);
        let root_vehicle = Self::root_vehicle_from_player(player)
            .or_else(|| player.pending_root_vehicle_for_current_world());
        let ender_pearls = Self::ender_pearls_from_player(player);

        Self {
            pos: [pos.x, pos.y, pos.z],
            motion: [delta.x, delta.y, delta.z],
            rotation: [yaw, pitch],
            on_ground,
            fall_flying,
            remaining_fire_ticks: fire_freeze.remaining_fire_ticks(),
            ticks_frozen: fire_freeze.ticks_frozen(),
            is_in_powder_snow: fire_freeze.is_in_powder_snow(),
            was_in_powder_snow: fire_freeze.was_in_powder_snow(),
            has_visual_fire: fire_freeze.has_visual_fire(),
            health: player.get_health(),
            game_mode: player.game_mode() as i32,
            prev_game_mode: player
                .previous_game_mode()
                .map(|game_mode| game_mode as i32),
            abilities: PersistentAbilities {
                invulnerable: abilities.invulnerable,
                flying: abilities.flying,
                may_fly: abilities.may_fly,
                instabuild: abilities.instabuild,
                may_build: abilities.may_build,
                flying_speed: abilities.flying_speed,
                walking_speed: abilities.walking_speed,
            },
            inventory: slots,
            ender_items: ender_slots,
            selected_slot: i32::from(inventory.get_selected_slot()),
            world: player.get_world().key.to_string(),
            food_level: food_data.food_level,
            food_saturation_level: food_data.saturation_level,
            food_exhaustion_level: food_data.exhaustion_level,
            food_tick_timer: food_data.tick_timer,
            data_version: PLAYER_DATA_VERSION,
            experience_level,
            experience_progress,
            experience_total,
            enchantment_seed: player.enchantment_seed(),
            score,
            seen_credits: player.has_seen_credits(),
            warden_spawn_tracker: warden_spawn_tracker_fields(player.warden_spawn_tracker()),
            root_vehicle,
            respawn_config: player.respawn_config(),

            ender_pearls,
            advancements,
            statistics,
            living_nbt,
        }
    }

    /// Restores the shared living half before the player's own fields.
    ///
    /// The compound is empty for a player who has never been saved, and
    /// [`LivingEntity::load_living`] reads an absent key as a *default* rather
    /// than as "leave it alone" -- so handing it nothing would clear the
    /// effects a domain switch is carrying over instead of preserving them.
    fn apply_living_nbt(&self, player: &Player) {
        if self.living_nbt.is_empty() {
            return;
        }
        let Ok(living) = read_borrowed_compound(&mut Cursor::new(&self.living_nbt)) else {
            tracing::warn!(
                uuid = ?player.uuid(),
                "Failed to parse saved player living state, leaving it at its defaults"
            );
            return;
        };
        player.load_living((&living).into());
    }

    /// Snapshots the player's advancement progress for persistence.
    fn advancements_from_player(player: &Player) -> Vec<PersistentAdvancement> {
        player
            .saved_advancements()
            .into_iter()
            .map(|(key, criteria)| PersistentAdvancement {
                key: key.to_string(),
                criteria: criteria
                    .into_iter()
                    .map(|(name, obtained_epoch_millis)| PersistentCriterion {
                        name: name.to_owned(),
                        obtained_epoch_millis,
                    })
                    .collect(),
            })
            .collect()
    }

    /// Restores the saved advancement progress.
    ///
    /// A key that will not parse any more is dropped with a warning rather than
    /// failing the load, which is what
    /// [`crate::advancement::PlayerAdvancements::load`] already does for an
    /// advancement or a criterion the tree no longer declares.
    fn apply_advancements(&self, player: &Player) {
        let restored = self.advancements.iter().filter_map(|advancement| {
            let key = match Identifier::from_str(&advancement.key) {
                Ok(key) => key,
                Err(error) => {
                    tracing::warn!(
                        key = advancement.key,
                        %error,
                        "Ignoring saved advancement progress under an unparsable key"
                    );
                    return None;
                }
            };
            let criteria = advancement
                .criteria
                .iter()
                .map(|criterion| (criterion.name.clone(), criterion.obtained_epoch_millis))
                .collect();
            Some((key, criteria))
        });
        player.load_advancements(restored);
    }

    /// Snapshots the player's statistics for persistence.
    fn statistics_from_player(player: &Player) -> Vec<PersistentStatistic> {
        player
            .saved_statistics()
            .into_iter()
            .filter_map(|(stat, count)| {
                let stat_type = REGISTRY.stat_types.by_id(stat.stat_type)?;
                let value = stat_value_key(stat_type.value_registry, stat.value)?;
                Some(PersistentStatistic {
                    stat_type: stat_type.key.to_string(),
                    value: value.to_string(),
                    count,
                })
            })
            .collect()
    }

    /// Restores the saved statistics.
    ///
    /// A statistic naming something no longer registered is dropped rather
    /// than failing the load, which is what vanilla's codec does with an
    /// unknown key.
    fn apply_statistics(&self, player: &Player) {
        let restored = self.statistics.iter().filter_map(|entry| {
            let stat_type_key = Identifier::from_str(&entry.stat_type).ok()?;
            let stat_type = REGISTRY.stat_types.by_key(&stat_type_key)?;
            let value_key = Identifier::from_str(&entry.value).ok()?;
            let value = stat_value_id(stat_type.value_registry, &value_key)?;
            Some((
                Stat {
                    stat_type: stat_type.try_id()?,
                    value,
                },
                entry.count,
            ))
        });
        player.load_statistics(restored);
    }

    /// Snapshots the player's live in-flight ender pearls for persistence.
    fn ender_pearls_from_player(player: &Player) -> Vec<PersistentEnderPearl> {
        let mut seen = FxHashSet::default();
        let mut pearls = player
            .ender_pearls()
            .iter()
            .filter_map(|pearl| {
                let world = pearl.level()?.key.to_string();
                let entity = ChunkStorage::entity_tree_to_persistent(pearl)?;
                seen.insert(entity.uuid);
                Some(PersistentEnderPearl { world, entity })
            })
            .collect::<Vec<_>>();
        pearls.extend(
            player
                .pending_ender_pearls()
                .into_iter()
                .filter(|pearl| seen.insert(pearl.entity.uuid)),
        );
        pearls
    }

    fn root_vehicle_from_player(player: &Player) -> Option<PersistentRootVehicle> {
        let vehicle = player.vehicle()?;
        let root_vehicle = player.root_vehicle()?;
        if root_vehicle.id() == player.id() || !root_vehicle.has_exactly_one_player_passenger() {
            return None;
        }

        let entity = ChunkStorage::entity_tree_to_persistent(&root_vehicle)?;
        Some(PersistentRootVehicle {
            attach: *vehicle.uuid().as_bytes(),
            entity,
        })
    }
}

impl Player {
    /// Resets domain-scoped gameplay data to the defaults used for a new player.
    pub(crate) fn reset_domain_data_for_first_visit(&self) {
        use glam::DVec3;

        self.set_velocity(DVec3::ZERO);
        self.set_on_ground(false);
        self.set_fall_flying(false);
        self.base()
            .set_fire_freeze_state(EntityFireFreezeState::new());
        self.sync_base_fire_freeze_entity_data();
        self.set_health(self.get_max_health());
        *self.abilities.lock() = Abilities::default();
        *self.inventory.lock() = PlayerInventory::new();
        *self.food_data.lock() = FoodData::new();

        let mut experience = Experience::default();
        experience.dirty = true;
        *self.experience.lock() = experience;

        self.set_score(0);
        self.set_seen_credits(false);
        self.reset_advancements();
        self.load_statistics([]);
        // Vanilla parity: the `this.wardenSpawnTracker.reset()` of `ServerPlayer.reset`,
        // which is what makes dying to a warden clear the way to the next one.
        let mut tracker = self.warden_spawn_tracker();
        tracker.reset();
        self.set_warden_spawn_tracker(tracker);
    }
}

/// The registry key one statistic value stands for.
///
/// Vanilla dispatches on the stat type's own registry; Steel names the four
/// registries the vanilla stat types range over, so this is that dispatch.
fn stat_value_key(registry: StatValueRegistry, id: usize) -> Option<&'static Identifier> {
    match registry {
        StatValueRegistry::Block => REGISTRY.blocks.by_id(id).map(|block| &block.key),
        StatValueRegistry::Item => REGISTRY.items.by_id(id).map(|item| &item.key),
        StatValueRegistry::EntityType => REGISTRY
            .entity_types
            .by_id(id)
            .map(|entity_type| &entity_type.key),
        StatValueRegistry::CustomStat => REGISTRY.custom_stats.by_id(id).map(|stat| &stat.key),
    }
}

/// The id a statistic value's key resolves to.
fn stat_value_id(registry: StatValueRegistry, key: &Identifier) -> Option<usize> {
    match registry {
        StatValueRegistry::Block => REGISTRY.blocks.id_from_key(key),
        StatValueRegistry::Item => REGISTRY.items.id_from_key(key),
        StatValueRegistry::EntityType => REGISTRY.entity_types.id_from_key(key),
        StatValueRegistry::CustomStat => REGISTRY.custom_stats.id_from_key(key),
    }
}

/// Vanilla parity: `WardenSpawnTracker.CODEC`, whose three fields are all it holds.
const fn warden_spawn_tracker_fields(tracker: WardenSpawnTracker) -> [i32; 3] {
    [
        tracker.ticks_since_last_warning(),
        tracker.warning_level(),
        tracker.cooldown_ticks(),
    ]
}

impl Default for PersistentAbilities {
    fn default() -> Self {
        Self {
            invulnerable: false,
            flying: false,
            may_fly: false,
            instabuild: false,
            may_build: true,
            flying_speed: 0.05,
            walking_speed: 0.1,
        }
    }
}

impl From<&Abilities> for PersistentAbilities {
    fn from(abilities: &Abilities) -> Self {
        Self {
            invulnerable: abilities.invulnerable,
            flying: abilities.flying,
            may_fly: abilities.may_fly,
            instabuild: abilities.instabuild,
            may_build: abilities.may_build,
            flying_speed: abilities.flying_speed,
            walking_speed: abilities.walking_speed,
        }
    }
}

impl From<PersistentAbilities> for Abilities {
    fn from(persistent: PersistentAbilities) -> Self {
        Self {
            invulnerable: persistent.invulnerable,
            flying: persistent.flying,
            may_fly: persistent.may_fly,
            instabuild: persistent.instabuild,
            may_build: persistent.may_build,
            flying_speed: persistent.flying_speed,
            walking_speed: persistent.walking_speed,
        }
    }
}

impl PersistentPlayerData {
    /// Applies the saved data to a player.
    ///
    /// This restores position, rotation, inventory, abilities, etc.
    pub fn apply_to_player(&self, player: &Player) {
        self.apply_to_player_inner(player, true);
    }

    /// Applies saved gameplay state without restoring world-local location data.
    ///
    /// Used when the saved world is unavailable or differs from an explicitly
    /// selected world, which must use the target spawn instead.
    pub fn apply_to_player_without_location(&self, player: &Player) {
        self.apply_to_player_inner(player, false);
    }

    fn apply_to_player_inner(&self, player: &Player, restore_location: bool) {
        use glam::DVec3;

        self.apply_living_nbt(player);

        if restore_location {
            // Position
            player
                .base()
                .set_position_local(DVec3::new(self.pos[0], self.pos[1], self.pos[2]));

            // Rotation
            player.set_rotation((self.rotation[0], self.rotation[1]));

            // Motion/velocity
            player.set_velocity(DVec3::new(self.motion[0], self.motion[1], self.motion[2]));

            // Ground state
            player.set_fall_flying(self.fall_flying);
            player.set_on_ground(self.on_ground);
        }

        player
            .base()
            .set_fire_freeze_state(EntityFireFreezeState::from_parts(
                self.remaining_fire_ticks,
                self.ticks_frozen,
                self.is_in_powder_snow,
                self.was_in_powder_snow,
                self.has_visual_fire,
            ));
        player.sync_base_fire_freeze_entity_data();
        player.set_respawn_position(self.respawn_config.clone(), false);

        // Health
        player.set_health(self.health);

        // Game mode
        player.restore_game_modes(
            self.game_mode.into(),
            self.prev_game_mode.map(GameType::from),
        );

        // Abilities
        *player.abilities.lock() = self.abilities.clone().into();

        // Inventory
        {
            let mut inventory = player.inventory.lock();
            // Clear existing inventory first
            for slot in 0..PlayerInventory::CONTAINER_SIZE {
                inventory.set_item(slot, ItemStack::empty());
            }
            // Restore saved items
            for slot_data in &self.inventory {
                let slot_index = slot_data.slot as usize;
                if slot_index < PlayerInventory::CONTAINER_SIZE {
                    inventory.set_item(slot_index, slot_data.item.clone());
                }
            }
            // Restore selected slot
            let selected = self.selected_slot.clamp(0, 8) as u8;
            inventory.set_selected_slot(selected);
        }

        // Ender chest
        {
            let mut ender_chest = player.ender_chest.lock();
            for slot in 0..ENDER_CHEST_SLOTS {
                ender_chest.set_item(slot, ItemStack::empty());
            }
            for slot_data in &self.ender_items {
                let slot_index = slot_data.slot as usize;
                if slot_index < ENDER_CHEST_SLOTS {
                    ender_chest.set_item(slot_index, slot_data.item.clone());
                }
            }
        }

        // Food data
        {
            let mut food = player.food_data.lock();
            food.food_level = self.food_level;
            food.saturation_level = self.food_saturation_level;
            food.exhaustion_level = self.food_exhaustion_level;
            food.tick_timer = self.food_tick_timer;
        }

        {
            let mut experience = player.experience.lock();
            *experience = Experience::from_parts(
                self.experience_level,
                self.experience_progress,
                self.experience_total,
            );
        }
        self.apply_advancements(player);
        self.apply_statistics(player);
        player.set_enchantment_seed(self.enchantment_seed);
        player.set_score(self.score);
        player.set_seen_credits(self.seen_credits);
        let [ticks_since_last_warning, warning_level, cooldown_ticks] = self.warden_spawn_tracker;
        player.set_warden_spawn_tracker(WardenSpawnTracker::new(
            ticks_since_last_warning,
            warning_level,
            cooldown_ticks,
        ));
    }
}
