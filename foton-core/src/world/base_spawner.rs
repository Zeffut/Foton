//! The machinery behind every spawner block and the spawner minecart.
//!
//! Vanilla parity: `net.minecraft.world.level.BaseSpawner`.
//!
//! Two structural differences, both forced by Foton and both documented where
//! they bite:
//!
//! * Vanilla asks `SpawnPlacements.checkSpawnRules` before it creates anything.
//!   Foton has no path from an entity type to its spawn predicate, so the mob is
//!   created and then asked, exactly as `World::tick_natural_spawn` already does.
//! * Vanilla counts nearby mobs by exact Java class. Foton counts by entity
//!   type, which is the finest identity an entity carries here.

use std::io::Cursor;
use std::ptr;
use std::sync::{Arc, Weak};

use foton_registry::entity_type::EntityTypeRef;
use foton_registry::spawn_data::{CustomSpawnRules, SpawnData};
use foton_registry::{REGISTRY, RegistryExt as _, level_events, vanilla_game_events};
use foton_utils::locks::SyncMutex;
use foton_utils::nbt::NbtNumeric as _;
use foton_utils::random::weighted_list::WeightedList;
use foton_utils::types::Difficulty;
use foton_utils::{BlockPos, Identifier, WorldAabb};
use glam::DVec3;
use simdnbt::borrow::{NbtCompound as NbtCompoundView, read_compound};
use simdnbt::owned::NbtCompound;

use crate::chunk::light::LightLayer;
use crate::entity::{ENTITIES, Entity, EntitySpawnReason, SharedEntity, next_entity_id};
use crate::event::{CreatureSpawnEvent, PreCreatureSpawnEvent};
use crate::physics::{WorldCollisionProvider, has_collision};
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// Vanilla parity: `BaseSpawner.EVENT_SPAWN`.
pub const EVENT_SPAWN: i32 = 1;

/// Vanilla parity: `BaseSpawner.DEFAULT_SPAWN_DELAY`.
const DEFAULT_SPAWN_DELAY: i16 = 20;
/// Vanilla parity: `BaseSpawner.DEFAULT_MIN_SPAWN_DELAY`.
const DEFAULT_MIN_SPAWN_DELAY: i32 = 200;
/// Vanilla parity: `BaseSpawner.DEFAULT_MAX_SPAWN_DELAY`.
const DEFAULT_MAX_SPAWN_DELAY: i32 = 800;
/// Vanilla parity: `BaseSpawner.DEFAULT_SPAWN_COUNT`.
const DEFAULT_SPAWN_COUNT: i32 = 4;
/// Vanilla parity: `BaseSpawner.DEFAULT_MAX_NEARBY_ENTITIES`.
const DEFAULT_MAX_NEARBY_ENTITIES: i32 = 6;
/// Vanilla parity: `BaseSpawner.DEFAULT_REQUIRED_PLAYER_RANGE`.
const DEFAULT_REQUIRED_PLAYER_RANGE: i32 = 16;
/// Vanilla parity: `BaseSpawner.DEFAULT_SPAWN_RANGE`.
const DEFAULT_SPAWN_RANGE: i32 = 4;

/// What the owning block entity or minecart does when the spawner fires.
///
/// Vanilla declares `BaseSpawner.broadcastEvent` abstract and lets
/// `SpawnerBlockEntity` override `setNextSpawnData` on top; both hooks are here
/// because the spawner itself has no way back to whatever owns it.
pub trait SpawnerOwner {
    /// Vanilla parity: `BaseSpawner.broadcastEvent`.
    fn broadcast_spawner_event(&self, world: &Arc<World>, pos: BlockPos, id: i32);

    /// Vanilla parity: the `setNextSpawnData` override of `SpawnerBlockEntity`,
    /// which re-sends the block so the client's spinning mob changes with it.
    fn on_next_spawn_data_set(&self, _world: &Arc<World>, _pos: BlockPos) {}
}

/// Something a spawn egg can retarget.
///
/// Vanilla parity: the `net.minecraft.world.level.Spawner` interface, which the
/// spawner block entity, the trial spawner block entity and the spawner
/// minecart all implement.
pub trait Spawner {
    /// Vanilla parity: `Spawner.setEntityId`.
    fn set_spawner_entity_id(&self, entity_type: EntityTypeRef);
}

/// Everything a spawner remembers between ticks.
struct BaseSpawnerState {
    spawn_delay: i32,
    spawn_potentials: WeightedList<SpawnData>,
    next_spawn_data: Option<SpawnData>,
    min_spawn_delay: i32,
    max_spawn_delay: i32,
    spawn_count: i32,
    max_nearby_entities: i32,
    required_player_range: i32,
    spawn_range: i32,
}

impl Default for BaseSpawnerState {
    fn default() -> Self {
        Self {
            spawn_delay: i32::from(DEFAULT_SPAWN_DELAY),
            spawn_potentials: WeightedList::empty(),
            next_spawn_data: None,
            min_spawn_delay: DEFAULT_MIN_SPAWN_DELAY,
            max_spawn_delay: DEFAULT_MAX_SPAWN_DELAY,
            spawn_count: DEFAULT_SPAWN_COUNT,
            max_nearby_entities: DEFAULT_MAX_NEARBY_ENTITIES,
            required_player_range: DEFAULT_REQUIRED_PLAYER_RANGE,
            spawn_range: DEFAULT_SPAWN_RANGE,
        }
    }
}

/// The spawner behind a spawner block and a spawner minecart.
///
/// Vanilla parity: `BaseSpawner`. The client-side fields (`spin`, `oSpin`,
/// `displayEntity`) are absent: Foton is a server, and vanilla only ever writes
/// them from `clientTick`.
pub struct BaseSpawner {
    state: SyncMutex<BaseSpawnerState>,
}

impl Default for BaseSpawner {
    fn default() -> Self {
        Self::new()
    }
}

/// Why one attempt out of `spawnCount` produced nothing.
///
/// Vanilla expresses this with `continue` and `return` inside one long loop.
/// Naming the outcomes keeps the loop readable and lets the caller distinguish
/// "try the next one" from "stop and re-delay", which vanilla does too.
enum SpawnAttempt {
    Spawned,
    Skipped,
    /// Vanilla's `this.delay(level, pos); return;` -- the whole tick gives up.
    GiveUp,
}

impl BaseSpawner {
    /// Creates a spawner with vanilla's defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: SyncMutex::new(BaseSpawnerState::default()),
        }
    }

    /// Points the spawner at one entity type.
    ///
    /// Vanilla parity: `BaseSpawner.setEntityId`.
    pub fn set_entity_id(
        &self,
        owner: &dyn SpawnerOwner,
        entity_type_key: &Identifier,
        world: Option<&Arc<World>>,
        pos: BlockPos,
    ) {
        let mut state = self.state.lock();
        Self::get_or_create_next_spawn_data(&mut state);
        if let Some(next) = state.next_spawn_data.as_mut() {
            let entity = next.entity_to_spawn_mut();
            while entity.remove("id").is_some() {}
            entity.insert("id", entity_type_key.to_string());
        }
        drop(state);

        if let Some(world) = world {
            owner.on_next_spawn_data_set(world, pos);
        }
    }

    /// Returns the entity type the spawner is currently pointed at.
    #[must_use]
    pub fn next_entity_type_key(&self) -> Option<Identifier> {
        self.state
            .lock()
            .next_spawn_data
            .as_ref()
            .and_then(SpawnData::entity_type_key)
    }

    /// Returns the ticks left before the next attempt.
    ///
    /// Vanilla keeps `spawnDelay` private; Foton exposes it because a test has
    /// no other way to see that a spawner re-armed itself.
    #[must_use]
    pub fn spawn_delay(&self) -> i32 {
        self.state.lock().spawn_delay
    }

    /// Sets the remaining delay before the next spawn attempt.
    pub fn set_spawn_delay(&self, delay: i32) {
        self.state.lock().spawn_delay = delay.max(0);
    }

    /// Returns the minimum delay between spawn attempts.
    #[must_use]
    pub fn min_spawn_delay(&self) -> i32 {
        self.state.lock().min_spawn_delay
    }

    /// Sets the minimum delay, preserving vanilla's non-negative input contract.
    pub fn set_min_spawn_delay(&self, delay: i32) {
        self.state.lock().min_spawn_delay = delay.max(0);
    }

    /// Maximum ticks between spawn attempts.
    pub fn max_spawn_delay(&self) -> i32 {
        self.state.lock().max_spawn_delay
    }

    /// Sets the maximum spawn delay, clamped to a valid non-negative value.
    pub fn set_max_spawn_delay(&self, delay: i32) {
        self.state.lock().max_spawn_delay = delay.max(0);
    }

    /// Returns whether a player is close enough to keep the spawner awake.
    ///
    /// Vanilla parity: `BaseSpawner.isNearPlayer`, which asks
    /// `Level.hasNearbyAlivePlayer`. Foton has no such method; the nearest
    /// non-spectator living player inside the range is the same question.
    #[must_use]
    pub fn is_near_player(&self, world: &Arc<World>, pos: BlockPos) -> bool {
        let range = f64::from(self.state.lock().required_player_range);
        Self::has_nearby_alive_player(world, pos, range)
    }

    fn has_nearby_alive_player(world: &Arc<World>, pos: BlockPos, range: f64) -> bool {
        let center = DVec3::new(
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()) + 0.5,
            f64::from(pos.z()) + 0.5,
        );
        world
            .nearest_player(center, range, |player| {
                !player.is_spectator() && player.is_alive()
            })
            .is_some()
    }

    /// Runs one server tick.
    ///
    /// Vanilla parity: `BaseSpawner.serverTick`.
    pub fn server_tick(&self, owner: &dyn SpawnerOwner, world: &Arc<World>, pos: BlockPos) {
        if !self.is_near_player(world, pos) || !world.is_spawner_block_enabled() {
            return;
        }

        {
            let mut state = self.state.lock();
            if state.spawn_delay == -1 {
                drop(state);
                self.delay(owner, world, pos);
                state = self.state.lock();
            }
            if state.spawn_delay > 0 {
                state.spawn_delay -= 1;
                return;
            }
        }

        let (spawn_count, next_spawn_data) = {
            let mut state = self.state.lock();
            Self::get_or_create_next_spawn_data(&mut state);
            (
                state.spawn_count,
                state.next_spawn_data.clone().unwrap_or_default(),
            )
        };

        let mut spawned_any = false;
        for _ in 0..spawn_count {
            match self.try_spawn_one(world, pos, &next_spawn_data) {
                SpawnAttempt::Spawned => spawned_any = true,
                SpawnAttempt::Skipped => {}
                SpawnAttempt::GiveUp => {
                    self.delay(owner, world, pos);
                    return;
                }
            }
        }

        if spawned_any {
            self.delay(owner, world, pos);
        }
    }

    /// One iteration of vanilla's `for (int c = 0; c < this.spawnCount; c++)`.
    #[expect(
        clippy::too_many_lines,
        reason = "transposes BaseSpawner.serverTick's single attempt, guard for guard"
    )]
    fn try_spawn_one(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        next_spawn_data: &SpawnData,
    ) -> SpawnAttempt {
        let Some(entity_type_key) = next_spawn_data.entity_type_key() else {
            // Vanilla parity: `EntityType.by(input)` empty -> delay and return.
            return SpawnAttempt::GiveUp;
        };
        let Some(entity_type) = REGISTRY.entity_types.by_key(&entity_type_key) else {
            return SpawnAttempt::GiveUp;
        };

        let spawn_range = self.state.lock().spawn_range;
        let spawn_pos = read_spawn_position(next_spawn_data.entity_to_spawn())
            .unwrap_or_else(|| random_spawn_position(pos, spawn_range));

        let spawn_aabb = WorldAabb::entity_box(
            spawn_pos.x,
            spawn_pos.y,
            spawn_pos.z,
            f64::from(entity_type.dimensions.half_width()),
            f64::from(entity_type.dimensions.height),
        );
        if has_collision(&WorldCollisionProvider::new(world), spawn_aabb) {
            return SpawnAttempt::Skipped;
        }

        let spawn_block_pos = BlockPos::new(
            spawn_pos.x.floor() as i32,
            spawn_pos.y.floor() as i32,
            spawn_pos.z.floor() as i32,
        );

        if let Some(rules) = next_spawn_data.custom_spawn_rules() {
            if !entity_type.mob_category.is_friendly() && world.difficulty() == Difficulty::Peaceful
            {
                return SpawnAttempt::Skipped;
            }
            if !custom_spawn_rules_allow(rules, world, spawn_block_pos) {
                return SpawnAttempt::Skipped;
            }
        }

        // Vanilla builds the entity from the tag here. Foton has no dispatch on
        // the tag's `id`, so the type is resolved above and handed in.
        let mut pre_spawn = PreCreatureSpawnEvent::new(
            world.key.to_string(),
            spawn_pos.x,
            spawn_pos.y,
            spawn_pos.z,
            entity_type.key.to_string(),
            "Spawner".to_owned(),
        );
        world.fire_event(&mut pre_spawn);
        if pre_spawn.is_cancelled() {
            return SpawnAttempt::Skipped;
        }
        let Some(entity) = load_spawner_entity(
            world,
            entity_type,
            spawn_pos,
            next_spawn_data,
            EntitySpawnReason::Spawner,
        ) else {
            // Vanilla parity: `loadEntityRecursive` returning null delays the
            // spawner. Foton gets here when the entity type has no factory,
            // which is the same "this spawner cannot build its mob" answer.
            log::debug!(
                "spawner at {pos:?} cannot build {}: no entity factory",
                entity_type.key
            );
            return SpawnAttempt::GiveUp;
        };

        let nearby = count_nearby_of_type(world, pos, spawn_range, &entity);
        if nearby >= self.state.lock().max_nearby_entities {
            return SpawnAttempt::GiveUp;
        }

        entity.set_rotation((rand::random::<f32>() * 360.0, 0.0));

        if let Some(mob) = entity.as_mob() {
            let placement_ok = next_spawn_data.custom_spawn_rules().is_some()
                || mob.check_spawn_rules(world, EntitySpawnReason::Spawner, spawn_block_pos);
            if !placement_ok || !mob.is_free(DVec3::ZERO) {
                return SpawnAttempt::Skipped;
            }

            if next_spawn_data.has_no_configuration() {
                let _ = mob.finalize_spawn(world, EntitySpawnReason::Spawner, None);
            }

            if let Some(equipment) = next_spawn_data.equipment() {
                mob.equip_from_table(world, equipment);
            }
        }

        let mut spawn_event = CreatureSpawnEvent::new(
            entity.uuid(),
            world.key.to_string(),
            entity.position().x,
            entity.position().y,
            entity.position().z,
            "Spawner".to_owned(),
        );
        world.begin_pending_spawn(Arc::clone(&entity));
        world.fire_event(&mut spawn_event);
        world.end_pending_spawn(&entity.uuid());
        if spawn_event.is_cancelled() {
            return SpawnAttempt::Skipped;
        }
        if world.try_add_entity(Arc::clone(&entity)).is_err() {
            return SpawnAttempt::GiveUp;
        }

        world.level_event(level_events::PARTICLES_MOBBLOCK_SPAWN, pos, 0, None);
        world.game_event(
            &vanilla_game_events::ENTITY_PLACE,
            spawn_block_pos,
            &GameEventContext::new(Some(entity.as_ref()), None),
        );
        if let Some(mob) = entity.as_mob() {
            mob.spawn_anim();
        }

        SpawnAttempt::Spawned
    }

    /// Re-arms the spawner and picks the next mob.
    ///
    /// Vanilla parity: `BaseSpawner.delay`.
    fn delay(&self, owner: &dyn SpawnerOwner, world: &Arc<World>, pos: BlockPos) {
        let picked = {
            let mut state = self.state.lock();
            state.spawn_delay = if state.max_spawn_delay <= state.min_spawn_delay {
                state.min_spawn_delay
            } else {
                state.min_spawn_delay
                    + rand::random_range(0..(state.max_spawn_delay - state.min_spawn_delay))
            };
            state.spawn_potentials.get_random().cloned()
        };

        if let Some(next) = picked {
            self.state.lock().next_spawn_data = Some(next);
            owner.on_next_spawn_data_set(world, pos);
        }

        owner.broadcast_spawner_event(world, pos, EVENT_SPAWN);
    }

    /// Vanilla parity: `BaseSpawner.getOrCreateNextSpawnData`, without the
    /// `setNextSpawnData` callback -- callers that can reach the world fire it.
    fn get_or_create_next_spawn_data(state: &mut BaseSpawnerState) {
        if state.next_spawn_data.is_some() {
            return;
        }
        state.next_spawn_data = Some(
            state
                .spawn_potentials
                .get_random()
                .cloned()
                .unwrap_or_default(),
        );
    }

    /// Vanilla parity: `BaseSpawner.load`.
    ///
    /// Every count is read leniently. Vanilla writes them as shorts and reads
    /// them back with `getIntOr`, which accepts any numeric tag; a strict read
    /// would turn every saved spawner into a default one on the next load.
    pub fn load(&self, nbt: &NbtCompoundView<'_, '_>) {
        let mut state = self.state.lock();
        state.spawn_delay = numeric_or(nbt, "Delay", i32::from(DEFAULT_SPAWN_DELAY));
        state.next_spawn_data = nbt.compound("SpawnData").map(|data| SpawnData::load(&data));

        let saved_potentials = nbt.list("SpawnPotentials");
        state.spawn_potentials = match saved_potentials.as_ref() {
            Some(_) => SpawnData::load_list(saved_potentials.as_ref()),
            // Vanilla parity: the `orElseGet` of `load`, which seeds the
            // potentials from the current spawn data so a spawner written
            // without them keeps spawning what it was spawning.
            None => WeightedList::single(state.next_spawn_data.clone().unwrap_or_default()),
        };

        state.min_spawn_delay = numeric_or(nbt, "MinSpawnDelay", DEFAULT_MIN_SPAWN_DELAY);
        state.max_spawn_delay = numeric_or(nbt, "MaxSpawnDelay", DEFAULT_MAX_SPAWN_DELAY);
        state.spawn_count = numeric_or(nbt, "SpawnCount", DEFAULT_SPAWN_COUNT);
        state.max_nearby_entities =
            numeric_or(nbt, "MaxNearbyEntities", DEFAULT_MAX_NEARBY_ENTITIES);
        state.required_player_range =
            numeric_or(nbt, "RequiredPlayerRange", DEFAULT_REQUIRED_PLAYER_RANGE);
        state.spawn_range = numeric_or(nbt, "SpawnRange", DEFAULT_SPAWN_RANGE);
    }

    /// Vanilla parity: `BaseSpawner.save`, which writes every count as a short.
    pub fn save(&self, nbt: &mut NbtCompound) {
        let state = self.state.lock();
        nbt.insert("Delay", state.spawn_delay as i16);
        nbt.insert("MinSpawnDelay", state.min_spawn_delay as i16);
        nbt.insert("MaxSpawnDelay", state.max_spawn_delay as i16);
        nbt.insert("SpawnCount", state.spawn_count as i16);
        nbt.insert("MaxNearbyEntities", state.max_nearby_entities as i16);
        nbt.insert("RequiredPlayerRange", state.required_player_range as i16);
        nbt.insert("SpawnRange", state.spawn_range as i16);
        if let Some(next) = &state.next_spawn_data {
            nbt.insert("SpawnData", next.save());
        }
        nbt.insert(
            "SpawnPotentials",
            SpawnData::save_list(&state.spawn_potentials),
        );
    }

    /// Vanilla parity: `BaseSpawner.onEventTriggered`, whose only server-side
    /// job is to claim the event so the block does not fall through to another
    /// handler. The delay reset it performs is client-only.
    #[must_use]
    pub const fn on_event_triggered(id: i32) -> bool {
        id == EVENT_SPAWN
    }
}

/// Vanilla parity: `ValueInput.getIntOr`, which accepts any numeric tag.
fn numeric_or(nbt: &NbtCompoundView<'_, '_>, name: &str, fallback: i32) -> i32 {
    nbt.get(name)
        .and_then(|tag| tag.codec_i32())
        .unwrap_or(fallback)
}

/// Vanilla parity: the `input.read("Pos", Vec3.CODEC)` of `serverTick`.
fn read_spawn_position(entity: &NbtCompound) -> Option<DVec3> {
    let list = entity.list("Pos")?;
    let coordinates = list.doubles()?;
    let [x, y, z] = coordinates[..] else {
        return None;
    };
    Some(DVec3::new(x, y, z))
}

/// Vanilla parity: the `orElseGet` fallback position of `serverTick`.
fn random_spawn_position(pos: BlockPos, spawn_range: i32) -> DVec3 {
    let range = f64::from(spawn_range);
    DVec3::new(
        (rand::random::<f64>() - rand::random::<f64>()).mul_add(range, f64::from(pos.x()) + 0.5),
        f64::from(pos.y() + rand::random_range(0..3) - 1),
        (rand::random::<f64>() - rand::random::<f64>()).mul_add(range, f64::from(pos.z()) + 0.5),
    )
}

/// Vanilla parity: `SpawnData.CustomSpawnRules.isValidPosition`.
pub(crate) fn custom_spawn_rules_allow(
    rules: &CustomSpawnRules,
    world: &Arc<World>,
    pos: BlockPos,
) -> bool {
    let block_light = i32::from(world.light_value_at(LightLayer::Block, pos));
    let sky_light = i32::from(world.effective_sky_brightness(pos));
    rules.block_light_limit.is_value_in_range(block_light)
        && rules.sky_light_limit.is_value_in_range(sky_light)
}

/// Vanilla parity: the `EntityTypeTest.forExactClass` count of `serverTick`.
///
/// Vanilla counts by exact Java class; Foton counts by entity type, the finest
/// identity an entity carries here. The two differ only where one class backs
/// several registered types.
fn count_nearby_of_type(
    world: &Arc<World>,
    pos: BlockPos,
    spawn_range: i32,
    spawned: &SharedEntity,
) -> i32 {
    let range = f64::from(spawn_range);
    let aabb = WorldAabb::new(
        f64::from(pos.x()) - range,
        f64::from(pos.y()) - range,
        f64::from(pos.z()) - range,
        f64::from(pos.x() + 1) + range,
        f64::from(pos.y() + 1) + range,
        f64::from(pos.z() + 1) + range,
    );
    let spawned_type = spawned.entity_type();
    world
        .get_entities_in_aabb_matching(&aabb, |entity| {
            !entity.is_spectator() && ptr::eq(entity.entity_type(), spawned_type)
        })
        .len() as i32
}

/// Builds one spawner mob from its saved tag.
///
/// Vanilla parity: `EntityType.loadEntityRecursive`. Foton resolves the type
/// before calling this, so passengers declared in the tag are not rebuilt --
/// see the module note.
pub(crate) fn load_spawner_entity(
    world: &Arc<World>,
    entity_type: EntityTypeRef,
    position: DVec3,
    spawn_data: &SpawnData,
    _reason: EntitySpawnReason,
) -> Option<SharedEntity> {
    if !ENTITIES.has_factory(entity_type) {
        return None;
    }

    let entity = ENTITIES.create(
        entity_type,
        next_entity_id(),
        position,
        Arc::downgrade(world) as Weak<World>,
    )?;

    let mut bytes = Vec::new();
    spawn_data.entity_to_spawn().write(&mut bytes);
    match read_compound(&mut Cursor::new(&bytes)) {
        Ok(borrowed) => entity.load_additional((&borrowed).into()),
        Err(error) => log::warn!("spawner entity tag could not be re-read: {error}"),
    }

    Some(entity)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicI32, Ordering};

    use simdnbt::owned::NbtCompound;

    use super::*;
    use crate::test_support::fresh_test_world;

    struct CountingOwner {
        events: AtomicI32,
    }

    impl SpawnerOwner for CountingOwner {
        fn broadcast_spawner_event(&self, _world: &Arc<World>, _pos: BlockPos, _id: i32) {
            self.events.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn reparse(nbt: &NbtCompound) -> Vec<u8> {
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        bytes
    }

    /// A spawner writes itself to disk every save and is read back on every
    /// chunk load. Vanilla writes the counts as shorts and reads them as ints,
    /// so a field written with the wrong width comes back as a default and the
    /// spawner silently changes its rate.
    #[test]
    fn every_tuning_field_survives_the_nbt_round_trip() {
        foton_registry::init_vanilla_registry();
        let spawner = BaseSpawner::new();

        let mut saved = NbtCompound::new();
        saved.insert("Delay", 7i16);
        saved.insert("MinSpawnDelay", 11i32);
        saved.insert("MaxSpawnDelay", 13i32);
        saved.insert("SpawnCount", 3i32);
        saved.insert("MaxNearbyEntities", 5i32);
        saved.insert("RequiredPlayerRange", 9i32);
        saved.insert("SpawnRange", 2i32);
        let mut entity = NbtCompound::new();
        entity.insert("id", "minecraft:zombie");
        let mut spawn_data = NbtCompound::new();
        spawn_data.insert("entity", entity);
        saved.insert("SpawnData", spawn_data);

        let bytes = reparse(&saved);
        let borrowed =
            read_compound(&mut Cursor::new(&bytes)).expect("hand-built spawner nbt must parse");
        spawner.load(&(&borrowed).into());

        let mut written = NbtCompound::new();
        spawner.save(&mut written);
        let bytes = reparse(&written);
        let borrowed =
            read_compound(&mut Cursor::new(&bytes)).expect("saved spawner nbt must parse");
        let reloaded = BaseSpawner::new();
        reloaded.load(&(&borrowed).into());

        let state = reloaded.state.lock();
        assert_eq!(state.spawn_delay, 7);
        assert_eq!(state.min_spawn_delay, 11);
        assert_eq!(state.max_spawn_delay, 13);
        assert_eq!(state.spawn_count, 3);
        assert_eq!(state.max_nearby_entities, 5);
        assert_eq!(state.required_player_range, 9);
        assert_eq!(state.spawn_range, 2);
        assert_eq!(
            state
                .next_spawn_data
                .as_ref()
                .and_then(SpawnData::entity_type_key),
            Some(Identifier::vanilla_static("zombie"))
        );
    }

    /// A spawner saved without `SpawnPotentials` -- which is every spawner a
    /// player places with `/setblock ... {SpawnData:{...}}` -- must keep
    /// spawning what its `SpawnData` names rather than falling back to nothing.
    #[test]
    fn a_spawner_without_potentials_reuses_its_spawn_data() {
        foton_registry::init_vanilla_registry();
        let spawner = BaseSpawner::new();

        let mut entity = NbtCompound::new();
        entity.insert("id", "minecraft:zombie");
        let mut spawn_data = NbtCompound::new();
        spawn_data.insert("entity", entity);
        let mut saved = NbtCompound::new();
        saved.insert("SpawnData", spawn_data);

        let bytes = reparse(&saved);
        let borrowed =
            read_compound(&mut Cursor::new(&bytes)).expect("hand-built spawner nbt must parse");
        spawner.load(&(&borrowed).into());

        let state = spawner.state.lock();
        assert_eq!(state.spawn_potentials.len(), 1);
        assert_eq!(
            state.spawn_potentials.entries()[0].value.entity_type_key(),
            Some(Identifier::vanilla_static("zombie"))
        );
    }

    /// `delay` is the only place a spawner re-arms itself, and vanilla stays
    /// inside `[min, max)`. A `max <= min` spawner must not reach `random_range`
    /// with an empty range, which would panic the tick thread.
    #[test]
    fn the_delay_range_holds_and_an_inverted_range_pins_to_the_minimum() {
        foton_registry::init_vanilla_registry();
        let world = fresh_test_world("base_spawner_delay");
        let owner = CountingOwner {
            events: AtomicI32::new(0),
        };
        let pos = BlockPos::new(8, 64, 8);
        let spawner = BaseSpawner::new();

        {
            let mut state = spawner.state.lock();
            state.min_spawn_delay = 100;
            state.max_spawn_delay = 140;
        }
        for _ in 0..32 {
            spawner.delay(&owner, &world, pos);
            let delay = spawner.spawn_delay();
            assert!((100..140).contains(&delay), "delay {delay} out of range");
        }

        {
            let mut state = spawner.state.lock();
            state.min_spawn_delay = 200;
            state.max_spawn_delay = 200;
        }
        spawner.delay(&owner, &world, pos);
        assert_eq!(spawner.spawn_delay(), 200);

        assert_eq!(owner.events.load(Ordering::Relaxed), 33);
    }
}
