//! Beehive block entity implementation.
//!
//! Vanilla parity: `BeehiveBlockEntity`. The hive is a container for bees: it
//! swallows one whole, keeps its saved NBT for as long as the bee stays in, and
//! puts it back out of the front face when its timer runs out -- raising the
//! honey level if the bee came home carrying nectar.

use std::io::Cursor;
use std::mem;
use std::sync::{Arc, Weak};

use simdnbt::borrow::{
    BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView,
    read_compound as read_borrowed_compound,
};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::IntProperty;
use steel_registry::blocks::properties::{BlockStateProperties, Direction, EnumProperty};
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_block_entity_types, vanilla_blocks,
    vanilla_entities, vanilla_game_events,
};
use steel_utils::types::UpdateFlags;
use steel_utils::{
    BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey, locks::SyncMutex,
};

use crate::behavior::blocks::building::campfire_block::is_smokey_pos;
use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::entity::entities::BeeEntity;
use crate::entity::{
    AgeableMob, Animal, ENTITIES, Entity, Mob, RemovalReason, SharedEntity, next_entity_id,
};
use crate::player::Player;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader, World};

/// Maximum number of occupants in a vanilla beehive.
pub const BEEHIVE_MAX_OCCUPANTS: usize = 3;
/// Minimum occupation time for bees without nectar.
pub const BEEHIVE_MIN_OCCUPATION_TICKS_NECTARLESS: i32 = 600;
/// Vanilla `BeehiveBlockEntity.MIN_OCCUPATION_TICKS_NECTAR`.
pub const BEEHIVE_MIN_OCCUPATION_TICKS_NECTAR: i32 = 2400;
/// Vanilla `BeehiveBlockEntity.MIN_TICKS_BEFORE_REENTERING_HIVE`.
pub const BEEHIVE_MIN_TICKS_BEFORE_REENTERING: i32 = 400;

/// Vanilla `BeehiveBlock.MAX_HONEY_LEVELS`.
const MAX_HONEY_LEVEL: u8 = 5;
/// One release in this many raises the honey level by two instead of one.
///
/// Vanilla parity: the `random.nextInt(100) == 0` of `releaseOccupant`.
const DOUBLE_HONEY_CHANCE: i32 = 100;
/// How far a released bee is pushed clear of the hive's front face.
///
/// Vanilla parity: the `0.55 + bbWidth / 2.0F` of `releaseOccupant`.
const RELEASE_CLEARANCE: f64 = 0.55;
/// How close a harvesting player must be for a released bee to turn on them.
///
/// Vanilla parity: the `distanceToSqr(...) <= 16.0` of `emptyAllLivingFromHive`.
const ANGER_RANGE_SQR: f64 = 16.0;
/// Chance a released bee inherits the hive's remembered flower.
const INHERIT_FLOWER_CHANCE: f32 = 0.9;
/// One in this many ticks a busy hive buzzes.
///
/// Vanilla parity: the `random.nextDouble() < 0.005` of `serverTick`.
const WORK_SOUND_CHANCE: f64 = 0.005;

/// Entity tags vanilla strips before storing a bee, and again before rebuilding
/// it.
///
/// Vanilla parity: `BeehiveBlockEntity.IGNORED_BEE_TAGS`. Steel's bee saves a
/// subset of these, but the list is applied whole so a hand-written or imported
/// hive behaves the same as vanilla's.
const IGNORED_BEE_TAGS: [&str; 25] = [
    "Air",
    "drop_chances",
    "equipment",
    "Brain",
    "CanPickUpLoot",
    "DeathTime",
    "fall_distance",
    "FallFlying",
    "Fire",
    "HurtTime",
    "LeftHanded",
    "Motion",
    "NoGravity",
    "OnGround",
    "PortalCooldown",
    "Pos",
    "Rotation",
    "sleeping_pos",
    "CannotEnterHiveTicks",
    "TicksSincePollination",
    "CropsGrownSincePollination",
    "hive_pos",
    "Passengers",
    "leash",
    "UUID",
];

const HORIZONTAL_FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;
const LEVEL_HONEY: &IntProperty = &BlockStateProperties::LEVEL_HONEY;

/// Why a hive is letting a bee out.
///
/// Vanilla parity: `BeehiveBlockEntity.BeeReleaseStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeeReleaseStatus {
    /// The bee came home with nectar; the hive gains honey.
    HoneyDelivered,
    /// The bee's timer simply ran out.
    BeeReleased,
    /// Fire nearby; the bees leave regardless of weather or a blocked face.
    Emergency,
}

struct BeeOccupant {
    entity_data: NbtCompound,
    ticks_in_hive: i32,
    min_ticks_in_hive: i32,
}

impl BeeOccupant {
    fn worldgen(ticks_in_hive: i32) -> Self {
        Self {
            entity_data: default_bee_entity_data(),
            ticks_in_hive,
            min_ticks_in_hive: BEEHIVE_MIN_OCCUPATION_TICKS_NECTARLESS,
        }
    }

    /// Vanilla parity: `BeehiveBlockEntity.Occupant.of`.
    fn of_bee(bee: &BeeEntity) -> Self {
        let mut entity_data = NbtCompound::new();
        bee.save_additional(&mut entity_data);
        for key in IGNORED_BEE_TAGS {
            while entity_data.remove(key).is_some() {}
        }
        entity_data.insert("id", vanilla_entities::BEE.key.to_string());

        let has_nectar = bee.has_nectar();
        Self {
            entity_data,
            ticks_in_hive: 0,
            min_ticks_in_hive: if has_nectar {
                BEEHIVE_MIN_OCCUPATION_TICKS_NECTAR
            } else {
                BEEHIVE_MIN_OCCUPATION_TICKS_NECTARLESS
            },
        }
    }

    fn load(nbt: NbtCompoundView<'_, '_>) -> Self {
        let entity_data = nbt
            .compound("entity_data")
            .map_or_else(default_bee_entity_data, |entity_data| {
                entity_data.to_owned()
            });
        let ticks_in_hive = nbt.int("ticks_in_hive").unwrap_or(0);
        let min_ticks_in_hive = nbt
            .int("min_ticks_in_hive")
            .unwrap_or(BEEHIVE_MIN_OCCUPATION_TICKS_NECTARLESS);

        Self {
            entity_data,
            ticks_in_hive,
            min_ticks_in_hive,
        }
    }

    fn save(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.insert("entity_data", self.entity_data.clone());
        nbt.insert("ticks_in_hive", self.ticks_in_hive);
        nbt.insert("min_ticks_in_hive", self.min_ticks_in_hive);
        nbt
    }

    /// Vanilla parity: `BeehiveBlockEntity.BeeData.tick`.
    const fn tick(&mut self) -> bool {
        let done = self.ticks_in_hive > self.min_ticks_in_hive;
        self.ticks_in_hive += 1;
        done
    }

    /// Vanilla parity: `BeehiveBlockEntity.BeeData.hasNectar`.
    fn has_nectar(&self) -> bool {
        self.entity_data
            .byte("HasNectar")
            .is_some_and(|flag| flag != 0)
    }
}

fn default_bee_entity_data() -> NbtCompound {
    let mut entity_data = NbtCompound::new();
    entity_data.insert("id", vanilla_entities::BEE.key.to_string());
    entity_data
}

struct BeehiveState {
    stored: Vec<BeeOccupant>,
    /// Vanilla `BeehiveBlockEntity.savedFlowerPos`.
    saved_flower_pos: Option<BlockPos>,
}

impl BeehiveState {
    fn push_occupant(&mut self, occupant: BeeOccupant) -> bool {
        if self.stored.len() >= BEEHIVE_MAX_OCCUPANTS {
            return false;
        }

        self.stored.push(occupant);
        true
    }
}

/// Beehive and bee nest block entity.
pub struct BeehiveBlockEntity {
    base: BlockEntityBase,
    state: SyncMutex<BeehiveState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `BeehiveBlockEntity`.
unsafe impl DowncastType for BeehiveBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/beehive");
}

impl BeehiveBlockEntity {
    /// Creates a new beehive block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(&vanilla_block_entity_types::BEEHIVE, level, pos, state),
            state: SyncMutex::new(BeehiveState {
                stored: Vec::new(),
                saved_flower_pos: None,
            }),
        }
    }

    /// Stores a vanilla worldgen bee occupant.
    ///
    /// Mirrors `BeehiveBlockEntity.Occupant.create(ticksInHive)`.
    pub fn store_worldgen_bee(&self, ticks_in_hive: i32) {
        let stored = {
            self.state
                .lock()
                .push_occupant(BeeOccupant::worldgen(ticks_in_hive))
        };
        if stored {
            BlockEntity::set_changed(self);
        }
    }

    /// Returns the number of stored occupants.
    #[must_use]
    pub fn occupant_count(&self) -> usize {
        self.state.lock().stored.len()
    }

    /// Returns whether the hive currently stores no occupants.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.state.lock().stored.is_empty()
    }

    /// Returns vanilla `BeehiveBlockEntity.isFull`.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.state.lock().stored.len() >= BEEHIVE_MAX_OCCUPANTS
    }

    /// Returns vanilla `BeehiveBlockEntity.isFireNearby`.
    ///
    /// A hive with fire in the surrounding three-by-three-by-three empties
    /// itself, which is what makes burning a nest release its bees at once.
    #[must_use]
    pub fn is_fire_nearby(&self) -> bool {
        let Some(world) = self.get_level() else {
            return false;
        };
        let origin = self.get_block_pos();
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let pos = origin.offset(dx, dy, dz);
                    if world.get_block_state(pos).get_block() == &vanilla_blocks::FIRE {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Returns vanilla `BeehiveBlockEntity.isSedated`.
    ///
    /// A hive with campfire smoke under it is calm: a player may harvest it and
    /// the bees it lets out only stay away for a while instead of stinging.
    #[must_use]
    pub fn is_sedated(&self) -> bool {
        self.get_level()
            .is_some_and(|world| is_smokey_pos(&world, self.get_block_pos()))
    }

    /// Returns the flower the hive remembers on its occupants' behalf.
    #[must_use]
    pub fn saved_flower_pos(&self) -> Option<BlockPos> {
        self.state.lock().saved_flower_pos
    }

    /// Vanilla parity: `BeehiveBlockEntity.addOccupant`.
    ///
    /// The bee stops being an entity here: its save data goes into the hive and
    /// the entity is discarded.
    pub fn add_occupant(&self, bee: &BeeEntity) {
        if self.is_full() {
            return;
        }

        bee.stop_riding();
        bee.eject_passengers();
        bee.drop_leash();

        let occupant = BeeOccupant::of_bee(bee);
        let bee_flower_pos = bee.saved_flower_pos();
        {
            let mut state = self.state.lock();
            if !state.push_occupant(occupant) {
                return;
            }
            if let Some(flower_pos) = bee_flower_pos
                && (state.saved_flower_pos.is_none() || rand::random::<bool>())
            {
                state.saved_flower_pos = Some(flower_pos);
            }
        }

        if let Some(world) = self.get_level() {
            let pos = self.get_block_pos();
            world.play_sound(
                &sound_events::BLOCK_BEEHIVE_ENTER,
                SoundSource::Blocks,
                pos,
                1.0,
                1.0,
                None,
            );
            world.game_event(
                &vanilla_game_events::BLOCK_CHANGE,
                pos,
                &GameEventContext::new(Some(bee), None),
            );
        }

        bee.set_removed(RemovalReason::Discarded);
        BlockEntity::set_changed(self);
    }

    /// Vanilla parity: `BeehiveBlockEntity.emptyAllLivingFromHive`.
    ///
    /// This is what a player harvesting an unsmoked hive sets off: every bee it
    /// let out turns on them, unless a campfire below has sedated the hive, in
    /// which case they are merely told to stay out for a while.
    pub fn empty_all_living_from_hive(
        &self,
        player: Option<&Player>,
        state: BlockStateId,
        release_reason: BeeReleaseStatus,
    ) {
        let released = self.release_all_occupants(state, release_reason);
        let Some(player) = player else {
            return;
        };
        let target = self
            .get_level()
            .and_then(|world| world.get_entity_by_uuid(&player.uuid()));

        for entity in released {
            if player.position().distance_squared(entity.position()) > ANGER_RANGE_SQR {
                continue;
            }
            let Some(bee) = entity.downcast_ref::<BeeEntity>() else {
                continue;
            };
            if self.is_sedated() {
                bee.set_stay_out_of_hive_countdown(BEEHIVE_MIN_TICKS_BEFORE_REENTERING);
            } else if let Some(target) = target.as_ref() {
                bee.set_target(Some(target));
            }
        }
    }

    /// Vanilla parity: `BeehiveBlockEntity.releaseAllOccupants`.
    fn release_all_occupants(
        &self,
        block_state: BlockStateId,
        release_status: BeeReleaseStatus,
    ) -> Vec<SharedEntity> {
        let Some(world) = self.get_level() else {
            return Vec::new();
        };
        let pos = self.get_block_pos();

        let (occupants, saved_flower_pos) = {
            let mut state = self.state.lock();
            (mem::take(&mut state.stored), state.saved_flower_pos)
        };

        let mut spawned = Vec::new();
        let mut kept = Vec::new();
        for occupant in occupants {
            match release_occupant(
                &world,
                pos,
                block_state,
                &occupant,
                release_status,
                saved_flower_pos,
            ) {
                Some(bee) => spawned.push(bee),
                None => kept.push(occupant),
            }
        }

        let released_any = !spawned.is_empty();
        {
            let mut state = self.state.lock();
            // Anything the hive swallowed while this ran keeps its place behind
            // the occupants that refused to leave.
            let mut restored = kept;
            restored.append(&mut state.stored);
            state.stored = restored;
        }

        if released_any {
            BlockEntity::set_changed(self);
        }
        spawned
    }

    /// Vanilla parity: `BeehiveBlockEntity.tickOccupants`.
    fn tick_occupants(&self, world: &Arc<World>) {
        let block_state = world.get_block_state(self.get_block_pos());
        let saved_flower_pos = self.saved_flower_pos();
        let pos = self.get_block_pos();

        let due: Vec<BeeOccupant> = {
            let mut state = self.state.lock();
            let mut due = Vec::new();
            state.stored.retain_mut(|occupant| {
                if occupant.tick() {
                    due.push(BeeOccupant {
                        entity_data: occupant.entity_data.clone(),
                        ticks_in_hive: occupant.ticks_in_hive,
                        min_ticks_in_hive: occupant.min_ticks_in_hive,
                    });
                    return false;
                }
                true
            });
            due
        };

        if due.is_empty() {
            return;
        }

        let mut released_any = false;
        let mut refused = Vec::new();
        for occupant in due {
            let status = if occupant.has_nectar() {
                BeeReleaseStatus::HoneyDelivered
            } else {
                BeeReleaseStatus::BeeReleased
            };
            if release_occupant(world, pos, block_state, &occupant, status, saved_flower_pos)
                .is_some()
            {
                released_any = true;
            } else {
                refused.push(occupant);
            }
        }

        if !refused.is_empty() {
            let mut state = self.state.lock();
            for occupant in refused {
                state.stored.insert(0, occupant);
            }
        }
        if released_any {
            BlockEntity::set_changed(self);
        }
    }
}

/// Vanilla parity: the static `BeehiveBlockEntity.releaseOccupant`.
///
/// Returns the bee that left, or `None` when it stayed in -- because the weather
/// says so, or because the front of the hive is blocked.
fn release_occupant(
    world: &Arc<World>,
    pos: BlockPos,
    block_state: BlockStateId,
    occupant: &BeeOccupant,
    release_status: BeeReleaseStatus,
    saved_flower_pos: Option<BlockPos>,
) -> Option<SharedEntity> {
    if world.bees_stay_in_hive() && release_status != BeeReleaseStatus::Emergency {
        return None;
    }

    let facing = block_state.try_get_value(HORIZONTAL_FACING)?;
    let facing_pos = pos.relative(facing);
    let front_blocked =
        world.is_collision_shape_full_block_at(facing_pos, world.get_block_state(facing_pos));
    if front_blocked && release_status != BeeReleaseStatus::Emergency {
        return None;
    }

    let dimensions = vanilla_entities::BEE.dimensions;
    let clearance = if front_blocked {
        0.0
    } else {
        RELEASE_CLEARANCE + f64::from(dimensions.width) / 2.0
    };
    let (step_x, _, step_z) = facing.offset();
    let spawn = glam::DVec3::new(
        f64::from(pos.x()) + 0.5 + clearance * f64::from(step_x),
        f64::from(pos.y()) + 0.5 - f64::from(dimensions.height) / 2.0,
        f64::from(pos.z()) + 0.5 + clearance * f64::from(step_z),
    );

    let entity = ENTITIES.create(
        &vanilla_entities::BEE,
        next_entity_id(),
        spawn,
        Arc::downgrade(world),
    )?;
    let bee = entity.downcast_ref::<BeeEntity>()?;
    load_stored_bee(bee, &occupant.entity_data);

    if let Some(flower_pos) = saved_flower_pos
        && !bee.has_saved_flower_pos()
        && rand::random::<f32>() < INHERIT_FLOWER_CHANCE
    {
        bee.set_saved_flower_pos(flower_pos);
    }

    bee.set_hive_pos(pos);
    bee.set_no_gravity(true);
    set_bee_release_data(occupant.ticks_in_hive, bee);

    if release_status == BeeReleaseStatus::HoneyDelivered {
        bee.drop_off_nectar();
        raise_honey_level(world, pos, block_state);
    }

    bee.set_old_position_to_current();
    world.play_sound(
        &sound_events::BLOCK_BEEHIVE_EXIT,
        SoundSource::Blocks,
        pos,
        1.0,
        1.0,
        None,
    );
    world.game_event(
        &vanilla_game_events::BLOCK_CHANGE,
        pos,
        &GameEventContext::new(Some(bee), None),
    );

    if world.try_add_entity(Arc::clone(&entity)).is_err() {
        return None;
    }
    Some(entity)
}

/// Vanilla parity: the honey-level half of `releaseOccupant`.
fn raise_honey_level(world: &Arc<World>, pos: BlockPos, block_state: BlockStateId) {
    if !REGISTRY
        .blocks
        .is_in_tag(block_state.get_block(), &BlockTag::BEEHIVES)
    {
        return;
    }
    let Some(honey_level) = block_state.try_get_value(LEVEL_HONEY) else {
        return;
    };
    if honey_level >= MAX_HONEY_LEVEL {
        return;
    }

    let mut increase = if rand::random_range(0..DOUBLE_HONEY_CHANCE) == 0 {
        2
    } else {
        1
    };
    if honey_level + increase > MAX_HONEY_LEVEL {
        increase -= 1;
    }
    world.set_block(
        pos,
        block_state.set_value(LEVEL_HONEY, honey_level + increase),
        UpdateFlags::UPDATE_ALL,
    );
}

/// Vanilla parity: `BeehiveBlockEntity.Occupant.setBeeReleaseData`, which ages
/// the bee and burns down its love timer by the time it spent inside.
fn set_bee_release_data(ticks_in_hive: i32, bee: &BeeEntity) {
    if !bee.is_age_locked() {
        let age = bee.get_age();
        if age < 0 {
            bee.set_age((age + ticks_in_hive).min(0));
        } else if age > 0 {
            bee.set_age((age - ticks_in_hive).max(0));
        }
    }
    bee.set_in_love_time((bee.in_love_time() - ticks_in_hive).max(0));
}

/// Restores a stored bee's save data onto a freshly created entity.
///
/// Vanilla parity: the `EntityType.loadEntityRecursive` of
/// `Occupant.createEntity`. Steel builds the entity from the registry first and
/// then replays the compound through the same `load_additional` the chunk loader
/// uses, so a stored bee comes back with the age, name and nectar it went in
/// with.
fn load_stored_bee(bee: &BeeEntity, entity_data: &NbtCompound) {
    let mut trimmed = entity_data.clone();
    for key in IGNORED_BEE_TAGS {
        while trimmed.remove(key).is_some() {}
    }

    let mut bytes = Vec::new();
    trimmed.write(&mut bytes);
    let Ok(borrowed) = read_borrowed_compound(&mut Cursor::new(&bytes)) else {
        return;
    };
    let view: NbtCompoundView<'_, '_> = (&borrowed).into();
    bee.load_additional(view);
}

impl BlockEntity for BeehiveBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla parity: `BeehiveBlockEntity.setChanged`, which is where an
    /// emergency release comes from -- lighting a fire next to a hive changes it,
    /// and the change itself empties it.
    fn set_changed(&self) {
        if self.is_fire_nearby()
            && let Some(world) = self.get_level()
        {
            let block_state = world.get_block_state(self.get_block_pos());
            self.empty_all_living_from_hive(None, block_state, BeeReleaseStatus::Emergency);
        }
        self.base().set_changed();
    }

    /// Vanilla parity: `BeehiveBlockEntity.serverTick`.
    fn tick(&self, world: &Arc<World>) {
        self.tick_occupants(world);

        if self.is_empty() {
            return;
        }
        if rand::random::<f64>() >= WORK_SOUND_CHANCE {
            return;
        }
        world.play_sound(
            &sound_events::BLOCK_BEEHIVE_WORK,
            SoundSource::Blocks,
            self.get_block_pos(),
            1.0,
            1.0,
            None,
        );
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt: NbtCompoundView<'_, '_> = nbt.into();
        let mut stored = Vec::new();

        if let Some(bees) = nbt.list("bees")
            && let Some(compounds) = bees.compounds()
        {
            for compound in compounds {
                if stored.len() >= BEEHIVE_MAX_OCCUPANTS {
                    break;
                }
                stored.push(BeeOccupant::load(compound));
            }
        }

        let saved_flower_pos = nbt.int_array("flower_pos").and_then(|values| {
            let [x, y, z] = values[..] else { return None };
            Some(BlockPos::new(x, y, z))
        });

        let mut state = self.state.lock();
        state.stored = stored;
        state.saved_flower_pos = saved_flower_pos;
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let state = self.state.lock();
        let bees = state
            .stored
            .iter()
            .map(BeeOccupant::save)
            .collect::<Vec<_>>();
        nbt.insert("bees", NbtList::Compound(bees));
        if let Some(flower_pos) = state.saved_flower_pos {
            nbt.insert(
                "flower_pos",
                NbtTag::IntArray(vec![flower_pos.x(), flower_pos.y(), flower_pos.z()]),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_blocks};
    use steel_utils::ChunkPos;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::block_entity::init_block_entities;
    use crate::entity::init_entities;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    /// Places a beehive and hands back its block entity.
    fn hive_at(world: &Arc<World>, pos: BlockPos) -> Arc<World> {
        assert!(world.set_block(
            pos,
            vanilla_blocks::BEEHIVE.default_state(),
            UpdateFlags::UPDATE_ALL
        ));
        Arc::clone(world)
    }

    #[test]
    fn a_bee_goes_into_a_hive_and_comes_back_out_with_the_honey_it_carried() {
        // The whole point of the bee: Steel has had beehives since the start and
        // nothing has ever lived in one. This is a bee entering, the hive keeping
        // it, and the hive letting it back out with the honey level raised.
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
        init_entities();
        let world = fresh_test_world("beehive_round_trip");
        let pos = BlockPos::new(8, 64, 8);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
        let world = hive_at(&world, pos);

        let block_entity = world
            .get_block_entity(pos)
            .unwrap_or_else(|| panic!("placing a beehive should create its block entity"));
        let hive = block_entity
            .downcast_ref::<BeehiveBlockEntity>()
            .unwrap_or_else(|| panic!("the beehive's block entity should be a beehive"));

        let bee = Arc::new(BeeEntity::new(
            &vanilla_entities::BEE,
            next_entity_id(),
            glam::DVec3::new(8.5, 65.0, 8.5),
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(Arc::clone(&bee) as SharedEntity)
            .unwrap_or_else(|error| panic!("bee should enter the test world: {error:?}"));
        bee.set_has_nectar(true);

        hive.add_occupant(bee.as_ref());

        assert_eq!(hive.occupant_count(), 1, "the hive swallowed the bee");
        assert!(bee.is_removed(), "the bee stopped being an entity");

        // A bee carrying nectar owes the hive two full minutes, four times what a
        // bee that came home empty owes it. Ticking a little past the shorter
        // deadline is what tells the two apart.
        for _ in 0..(BEEHIVE_MIN_OCCUPATION_TICKS_NECTARLESS + 5) {
            hive.tick(&world);
        }
        assert_eq!(
            hive.occupant_count(),
            1,
            "a bee with nectar stays in far longer than one without"
        );

        for _ in 0..BEEHIVE_MIN_OCCUPATION_TICKS_NECTAR {
            hive.tick(&world);
        }

        assert!(hive.is_empty(), "the hive let the bee back out");
        let released = world.get_entities_in_aabb_matching(
            &steel_utils::WorldAabb::new(4.0, 60.0, 4.0, 13.0, 69.0, 13.0),
            |entity| entity.entity_type() == &vanilla_entities::BEE,
        );
        assert_eq!(released.len(), 1, "exactly one bee came back out");
        assert!(
            world.get_block_state(pos).get_value(LEVEL_HONEY) > 0,
            "a bee that came home with nectar leaves honey behind"
        );
        let released_bee = released[0]
            .downcast_ref::<BeeEntity>()
            .unwrap_or_else(|| panic!("the released entity should be a bee"));
        assert!(
            !released_bee.has_nectar(),
            "the bee handed its nectar over on the way out"
        );
        assert_eq!(
            released_bee.hive_pos(),
            Some(pos),
            "a released bee remembers the hive it came out of"
        );
    }

    #[test]
    fn a_full_hive_takes_no_more_bees() {
        // Vanilla's `MAX_OCCUPANTS` is what stops a hive being an infinite bee
        // store, and it is also what makes `BeeLocateHiveGoal` look elsewhere.
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
        let world = fresh_test_world("beehive_capacity");
        let pos = BlockPos::new(8, 64, 8);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
        let world = hive_at(&world, pos);

        let block_entity = world
            .get_block_entity(pos)
            .unwrap_or_else(|| panic!("placing a beehive should create its block entity"));
        let hive = block_entity
            .downcast_ref::<BeehiveBlockEntity>()
            .unwrap_or_else(|| panic!("the beehive's block entity should be a beehive"));

        for _ in 0..(BEEHIVE_MAX_OCCUPANTS + 2) {
            hive.store_worldgen_bee(0);
        }

        assert_eq!(hive.occupant_count(), BEEHIVE_MAX_OCCUPANTS);
        assert!(hive.is_full());
    }
}
