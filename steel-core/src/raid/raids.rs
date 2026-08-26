//! Every raid one loaded world is running.
//!
//! Vanilla parity: `net.minecraft.world.entity.raid.Raids`, a `SavedData`
//! stored per dimension under `data/raids.dat`. Steel writes the same thing to
//! `data/raids.toml` through [`SavedDataManager`](steel_utils::saved_data::SavedDataManager),
//! the way `chunk_tickets` is written -- one file per loaded world, not one per
//! domain, because a raid belongs to the dimension its village is in.
//!
//! What is persisted is exactly what vanilla's codec persists: the raid's
//! counters, its center and the players who earned Hero of the Village. The
//! raiders themselves are not, because each raider saves its own `RaidId` and
//! puts itself back into its wave when its chunk loads.

use std::sync::Arc;

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use steel_registry::vanilla_game_rules::RAIDS;
use steel_utils::BlockPos;
use steel_utils::locks::SyncMutex;
use uuid::Uuid;

use super::raid::{DEFAULT_MAX_RAID_OMEN_LEVEL, Raid, RaidPhase, block_pos_dist_sqr};
use crate::entity::Entity as _;
use crate::player::Player;
use crate::poi::poi_storage::{OccupationStatus, is_village_type};
use crate::world::World;

/// Radius in blocks searched for the village POIs a raid centers on.
///
/// Vanilla parity: the `getInRange(.., 64, ..)` of `Raids.createOrExtendRaid`.
const VILLAGE_POI_SEARCH_RADIUS: i32 = 64;

/// The mutable half of [`Raids`].
#[derive(Debug)]
struct RaidsState {
    raids: FxHashMap<i32, Arc<Raid>>,
    /// Vanilla parity: `Raids.nextId`, pre-incremented, so the first raid is 2.
    next_id: i32,
    /// Vanilla parity: `Raids.tick`.
    tick: i32,
}

/// The raids of one loaded world.
///
/// Vanilla parity: `Raids`.
#[derive(Debug)]
pub struct Raids {
    state: SyncMutex<RaidsState>,
}

impl Raids {
    /// Creates an empty set of raids.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: SyncMutex::new(RaidsState {
                raids: FxHashMap::default(),
                next_id: 1,
                tick: 0,
            }),
        }
    }

    /// Rebuilds the raids of a world from its saved data.
    #[must_use]
    pub(crate) fn from_persistent(persistent: PersistentRaids) -> Self {
        let mut raids = FxHashMap::default();
        for saved in persistent.raids {
            let raid = Raid::from_saved(
                saved.id,
                BlockPos::new(saved.center_x, saved.center_y, saved.center_z),
                saved.group_count,
                saved.status,
                saved.active,
                saved.started,
                saved.ticks_active,
                saved.raid_omen_level,
                saved.groups_spawned,
                saved.cooldown_ticks,
                saved.post_raid_ticks,
                saved.total_health,
                saved.heroes_of_the_village.into_iter().collect(),
            );
            raids.insert(saved.id, Arc::new(raid));
        }

        Self {
            state: SyncMutex::new(RaidsState {
                raids,
                next_id: persistent.next_id,
                tick: persistent.tick,
            }),
        }
    }

    /// Snapshots the raids of a world for its saved data.
    #[must_use]
    pub(crate) fn to_persistent(&self) -> PersistentRaids {
        let state = self.state.lock();
        let mut raids: Vec<PersistentRaid> = state
            .raids
            .values()
            .map(|raid| {
                let fields = raid.saved_fields();
                PersistentRaid {
                    id: raid.id(),
                    started: fields.started,
                    active: fields.active,
                    ticks_active: fields.ticks_active,
                    raid_omen_level: fields.raid_omen_level,
                    groups_spawned: fields.groups_spawned,
                    cooldown_ticks: fields.raid_cooldown_ticks,
                    post_raid_ticks: fields.post_raid_ticks,
                    total_health: fields.total_health,
                    group_count: fields.num_groups,
                    status: fields.phase,
                    center_x: fields.center.x(),
                    center_y: fields.center.y(),
                    center_z: fields.center.z(),
                    heroes_of_the_village: fields.heroes_of_the_village,
                }
            })
            .collect();
        raids.sort_unstable_by_key(|raid| raid.id);

        PersistentRaids {
            raids,
            next_id: state.next_id,
            tick: state.tick,
        }
    }

    /// Returns the raid filed under `raid_id`.
    ///
    /// Vanilla parity: `Raids.get`.
    #[must_use]
    pub fn get(&self, raid_id: i32) -> Option<Arc<Raid>> {
        self.state.lock().raids.get(&raid_id).map(Arc::clone)
    }

    /// Files a raid under its own id.
    ///
    /// Vanilla parity: the `raidMap.put(getUniqueId(), raid)` of
    /// `Raids.createOrExtendRaid`; the id is already on the raid here because
    /// Steel hands it out before the raid is built.
    pub fn insert(&self, raid: Raid) -> Arc<Raid> {
        let raid = Arc::new(raid);
        self.state.lock().raids.insert(raid.id(), Arc::clone(&raid));
        raid
    }

    /// Returns the closest running raid within `max_dist_sqr` of `pos`.
    ///
    /// Vanilla parity: `Raids.getNearbyRaid`.
    #[must_use]
    pub fn nearby_raid(&self, pos: BlockPos, max_dist_sqr: f64) -> Option<Arc<Raid>> {
        let state = self.state.lock();
        let mut closest: Option<Arc<Raid>> = None;
        let mut closest_distance = max_dist_sqr;
        for raid in state.raids.values() {
            let distance = block_pos_dist_sqr(raid.center(), pos);
            if raid.is_active() && distance < closest_distance {
                closest = Some(Arc::clone(raid));
                closest_distance = distance;
            }
        }
        closest
    }

    /// Runs one tick of every raid, dropping the ones that ended.
    ///
    /// Vanilla parity: `Raids.tick`.
    pub fn tick(&self, world: &Arc<World>) {
        let raids_enabled = world.get_game_rule(&RAIDS);
        // Vanilla marks the saved data dirty every two hundred ticks; Steel
        // writes its saved data on shutdown, so the counter is kept only
        // because vanilla persists it and a reload would otherwise restart it.
        let raids = {
            let mut state = self.state.lock();
            state.tick = state.tick.wrapping_add(1);
            state.raids.values().map(Arc::clone).collect::<Vec<_>>()
        };

        let mut stopped = Vec::new();
        for raid in raids {
            if !raids_enabled {
                raid.stop();
            }
            if raid.is_stopped() {
                stopped.push(raid.id());
            } else {
                raid.tick(world);
                // A raid that stopped itself inside its own tick is dropped on
                // the next one, which is also what vanilla's iterator does.
            }
        }

        if stopped.is_empty() {
            return;
        }
        let mut state = self.state.lock();
        for raid_id in stopped {
            state.raids.remove(&raid_id);
        }
    }

    /// Starts a raid on the village around `raid_position`, or feeds an existing one.
    ///
    /// Vanilla parity: `Raids.createOrExtendRaid`.
    pub fn create_or_extend_raid(
        &self,
        world: &World,
        player: &Player,
        raid_position: BlockPos,
    ) -> Option<Arc<Raid>> {
        if player.is_spectator() {
            return None;
        }
        if !world.get_game_rule(&RAIDS) {
            return None;
        }
        // Vanilla parity: `EnvironmentAttributes.CAN_START_RAID`. No biome or
        // timeline overrides it, so the dimension type is the whole attribute.
        if !world.dimension_type.can_start_raid {
            return None;
        }

        let center = village_center(world, raid_position);
        let raid = match world.get_raid_at(center) {
            Some(existing) => existing,
            None => self.insert(Raid::new(self.next_unique_id(), center, world.difficulty())),
        };

        if !raid.is_started() || raid.raid_omen_level() < DEFAULT_MAX_RAID_OMEN_LEVEL {
            raid.absorb_raid_omen(player);
        }
        Some(raid)
    }

    /// Vanilla parity: `Raids.getUniqueId`, a pre-increment -- so the first
    /// raid a world ever runs is filed under two rather than one.
    pub fn next_unique_id(&self) -> i32 {
        let mut state = self.state.lock();
        state.next_id += 1;
        state.next_id
    }
}

/// Averages the occupied village POIs around `raid_position`.
///
/// Vanilla parity: the `getInRange(.., IS_OCCUPIED)` averaging of
/// `Raids.createOrExtendRaid`, which falls back to the player's own position
/// when nothing claimed a bed or a workstation nearby.
fn village_center(world: &World, raid_position: BlockPos) -> BlockPos {
    let village_pois = world.poi_storage.lock().get_in_range(
        &is_village_type,
        raid_position,
        VILLAGE_POI_SEARCH_RADIUS,
        OccupationStatus::Occupied,
    );
    if village_pois.is_empty() {
        return raid_position;
    }

    let mut total = (0.0_f64, 0.0_f64, 0.0_f64);
    for (pos, _) in &village_pois {
        total.0 += f64::from(pos.x());
        total.1 += f64::from(pos.y());
        total.2 += f64::from(pos.z());
    }
    let count = village_pois.len() as f64;
    BlockPos::containing(total.0 / count, total.1 / count, total.2 / count)
}

impl Default for Raids {
    fn default() -> Self {
        Self::new()
    }
}

/// The saved form of one world's raids.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PersistentRaids {
    #[serde(default)]
    raids: Vec<PersistentRaid>,
    next_id: i32,
    tick: i32,
}

impl Default for PersistentRaids {
    fn default() -> Self {
        Self {
            raids: Vec::new(),
            next_id: 1,
            tick: 0,
        }
    }
}

/// The saved form of one raid.
///
/// The field names follow vanilla's `Raid.MAP_CODEC` so a reader who knows the
/// vanilla file knows this one.
#[derive(Debug, Serialize, Deserialize)]
struct PersistentRaid {
    id: i32,
    started: bool,
    active: bool,
    ticks_active: i64,
    raid_omen_level: i32,
    groups_spawned: i32,
    cooldown_ticks: i32,
    post_raid_ticks: i32,
    total_health: f32,
    group_count: i32,
    status: RaidPhase,
    center_x: i32,
    center_y: i32,
    center_z: i32,
    #[serde(default)]
    heroes_of_the_village: Vec<Uuid>,
}
