//! Bee entity.
//!
//! Vanilla parity: `Bee`. A bee is built around two block positions it
//! remembers -- a hive and a flower -- and everything it does is a trip between
//! them. It leaves the hive, finds a flower, hovers over it until it is carrying
//! nectar, grows whatever crop it happens to fly over on the way, and comes back
//! to raise the hive's honey level. Provoke it and it stings once, which fills
//! the target with poison and kills the bee within the minute.
//!
//! Steel already had beehives, bee nests, honey levels, comparator output and
//! harvesting; this is the occupant those blocks were written for.

mod goals;

use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_macros::entity_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, DoubleBlockHalf};
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_entity_data::BeeEntityData;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_attributes, vanilla_blocks,
    vanilla_damage_types, vanilla_mob_effects,
};
use steel_utils::locks::SyncMutex;
use steel_utils::types::{Difficulty, InteractionHand};
use steel_utils::{
    BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey, Identifier, UuidExt,
};
use uuid::Uuid;

use crate::behavior::{BLOCK_BEHAVIORS, ITEM_BEHAVIORS, InteractionResult};
use crate::block_entity::entities::BeehiveBlockEntity;
use crate::entity::ai::goal::{
    BreedGoal, FloatGoal, FollowParentGoal, ResetUniversalAngerTargetGoal, TemptGoal,
};
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::mob::{MoveControlKind, NavigationKind};
use crate::entity::neutral_mob::{NeutralMob, PersistentAnger, read_persistent_anger};
use crate::entity::{
    AgeableMob, AgeableMobBase, Animal, AnimalBase, Entity, EntityBase, EntityBaseLoad,
    EntitySyncedData, LivingEntity, LivingEntityBase, LivingEntitySyncedData, Mob, MobBase,
    MobEffectInstance, MoveResult, PathfinderMob, SharedEntity,
};
use crate::player::Player;
use crate::world::{LevelReader, World};

use goals::{
    BeeAttackGoal, BeeBecomeAngryTargetGoal, BeeEnterHiveGoal, BeeGoToHiveGoal,
    BeeGoToKnownFlowerGoal, BeeGrowCropGoal, BeeHurtByOtherGoal, BeeLocateHiveGoal,
    BeePollinateGoal, BeeWanderGoal, ValidateFlowerGoal, ValidateHiveGoal,
};

/// Vanilla `Bee.FLAG_ROLL`.
const FLAG_ROLL: i8 = 2;
/// Vanilla `Bee.FLAG_HAS_STUNG`.
const FLAG_HAS_STUNG: i8 = 4;
/// Vanilla `Bee.FLAG_HAS_NECTAR`.
const FLAG_HAS_NECTAR: i8 = 8;

/// Vanilla `Bee.STING_DEATH_COUNTDOWN`.
const STING_DEATH_COUNTDOWN: i32 = 1200;
/// How often a bee that has stung rolls to die.
///
/// Vanilla parity: the `timeSinceSting % 5 == 0` of `customServerAiStep`.
const STING_DEATH_ROLL_INTERVAL: i32 = 5;
/// Vanilla `Bee.TICKS_BEFORE_GOING_TO_KNOWN_FLOWER`.
const TICKS_BEFORE_GOING_TO_KNOWN_FLOWER: i32 = 600;
/// Vanilla `Bee.TICKS_WITHOUT_NECTAR_BEFORE_GOING_HOME`.
const TICKS_WITHOUT_NECTAR_BEFORE_GOING_HOME: i32 = 3600;
/// Vanilla `Bee.MIN_ATTACK_DIST`, squared: how close its target must be before a
/// bee curls into the roll the client animates.
const MIN_ATTACK_DIST_SQR: f64 = 4.0;
/// Vanilla `Bee.MAX_CROPS_GROWABLE`.
const MAX_CROPS_GROWABLE: i32 = 10;
/// Vanilla `Bee.POISON_SECONDS_NORMAL`.
const POISON_SECONDS_NORMAL: i32 = 10;
/// Vanilla `Bee.POISON_SECONDS_HARD`.
const POISON_SECONDS_HARD: i32 = 18;
/// Vanilla `Bee.TOO_FAR_DISTANCE`, past which a bee forgets a remembered block.
const TOO_FAR_DISTANCE: i32 = 48;
/// Vanilla `Bee.HIVE_CLOSE_ENOUGH_DISTANCE`.
const HIVE_CLOSE_ENOUGH_DISTANCE: f64 = 2.0;
/// Vanilla `Bee.COOLDOWN_BEFORE_LOCATING_NEW_HIVE`.
const COOLDOWN_BEFORE_LOCATING_NEW_HIVE: i32 = 200;
/// Vanilla `Bee.COOLDOWN_BEFORE_LOCATING_NEW_FLOWER`.
const COOLDOWN_BEFORE_LOCATING_NEW_FLOWER: i32 = 200;
/// Vanilla `Bee.MIN_FIND_FLOWER_RETRY_COOLDOWN`.
const MIN_FIND_FLOWER_RETRY_COOLDOWN: i32 = 20;
/// Vanilla `Bee.MAX_FIND_FLOWER_RETRY_COOLDOWN`.
const MAX_FIND_FLOWER_RETRY_COOLDOWN: i32 = 60;

/// Shortest time a provoked bee stays angry.
///
/// Vanilla parity: `Bee.PERSISTENT_ANGER_TIME`, twenty to thirty-nine seconds.
const ANGER_MIN_TICKS: i64 = 20 * 20;
/// Longest time a provoked bee stays angry.
const ANGER_MAX_TICKS: i64 = 39 * 20;

/// How long a bee may hold its breath.
///
/// Vanilla parity: the `underWaterTicks > 20` of `Bee.customServerAiStep`.
const UNDERWATER_DROWN_TICKS: i32 = 20;
/// Damage a drowning bee takes each tick past that.
const DROWN_DAMAGE: f32 = 1.0;

/// Vanilla `Bee.getSoundVolume`.
const SOUND_VOLUME: f32 = 0.4;

/// The pitch limit of the `FlyingMoveControl` a bee installs.
const MOVE_CONTROL_MAX_TURN: f32 = 20.0;
/// How far a bee's navigation will path.
///
/// Vanilla parity: `setRequiredPathLength(48.0F)` in `Bee.createNavigation`.
const REQUIRED_PATH_LENGTH: f32 = 48.0;

/// Nudge a bee gets when it jumps in a fluid.
///
/// Vanilla parity: `Bee.jumpInLiquid`, a tenth of the shared push.
const LIQUID_JUMP_LIFT: f64 = 0.01;

/// Speed a bee attacks at.
const ATTACK_SPEED_MOD: f64 = 1.4;
/// Speed a bee courts at.
const BREED_SPEED_MOD: f64 = 1.0;
/// Speed a tempted bee follows at.
const TEMPT_SPEED_MOD: f64 = 1.25;
/// Speed a bee follows its parent at.
const FOLLOW_PARENT_SPEED_MOD: f64 = 1.25;

/// Runtime bee fields vanilla keeps on the entity.
///
/// Two of these -- `pollinating` and `blacklisted_hives` -- live on the goals in
/// vanilla, where an inner class reaches its sibling's fields directly. Steel's
/// goals are boxed behind the goal selector's mutex, so a goal that read another
/// goal's field would have to re-lock the selector from inside its own tick.
/// They are shared state, so they sit here instead.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BeeState {
    /// Vanilla `Bee.hivePos`.
    hive_pos: Option<BlockPos>,
    /// Vanilla `Bee.savedFlowerPos`.
    saved_flower_pos: Option<BlockPos>,
    /// Vanilla `Bee.timeSinceSting`.
    time_since_sting: i32,
    /// Vanilla `Bee.ticksWithoutNectarSinceExitingHive`.
    ticks_without_nectar_since_exiting_hive: i32,
    /// Vanilla `Bee.stayOutOfHiveCountdown`.
    stay_out_of_hive_countdown: i32,
    /// Vanilla `Bee.numCropsGrownSincePollination`.
    num_crops_grown_since_pollination: i32,
    /// Vanilla `Bee.remainingCooldownBeforeLocatingNewHive`.
    remaining_cooldown_before_locating_new_hive: i32,
    /// Vanilla `Bee.remainingCooldownBeforeLocatingNewFlower`.
    remaining_cooldown_before_locating_new_flower: i32,
    /// Vanilla `Bee.underWaterTicks`.
    under_water_ticks: i32,
    /// Vanilla `Bee.BeePollinateGoal.pollinating`.
    pollinating: bool,
    /// Vanilla `Bee.BeeGoToHiveGoal.blacklistedTargets`.
    blacklisted_hives: Vec<BlockPos>,
}

impl BeeState {
    fn new() -> Self {
        Self {
            hive_pos: None,
            saved_flower_pos: None,
            time_since_sting: 0,
            ticks_without_nectar_since_exiting_hive: 0,
            stay_out_of_hive_countdown: 0,
            num_crops_grown_since_pollination: 0,
            remaining_cooldown_before_locating_new_hive: 0,
            // Vanilla seeds this at construction so a whole hive released at once
            // does not go looking for the same flower on the same tick.
            remaining_cooldown_before_locating_new_flower: rand::random_range(
                MIN_FIND_FLOWER_RETRY_COOLDOWN..MAX_FIND_FLOWER_RETRY_COOLDOWN,
            ),
            under_water_ticks: 0,
            pollinating: false,
            blacklisted_hives: Vec::new(),
        }
    }
}

/// A bee.
#[entity_behavior(class = "Bee")]
pub struct BeeEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    anger: PersistentAnger,
    state: SyncMutex<BeeState>,
    entity_data: SyncMutex<BeeEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `BeeEntity`.
unsafe impl DowncastType for BeeEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/bee");
}

impl BeeEntity {
    /// Creates a bee at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a bee from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self::new_with_base(
            EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
        )
    }

    fn new_with_base(base: EntityBase, entity_type: EntityTypeRef) -> Self {
        let living_base = LivingEntityBase::new(entity_type);
        let mob_base = MobBase::new();
        let ageable_base = AgeableMobBase::new();
        let animal_base = AnimalBase::new();
        AnimalBase::initialize_pathfinding_malus(&mob_base);
        {
            // Vanilla parity: the five `setPathfindingMalus` calls of the `Bee`
            // constructor. Fire and water are walls, the waterline is merely
            // expensive, and a bee will not squeeze past cocoa or a fence.
            let mut malus = mob_base.pathfinding_malus().lock();
            malus.set(PathType::Fire, -1.0);
            malus.set(PathType::Water, -1.0);
            malus.set(PathType::WaterBorder, 16.0);
            malus.set(PathType::Cocoa, -1.0);
            malus.set(PathType::Fence, -1.0);
        }
        {
            // Vanilla parity: `Bee.createNavigation`.
            let mut navigation = mob_base.navigation().lock();
            navigation.set_can_open_doors(false);
            navigation.set_can_float(false);
            navigation
                .set_required_path_length(REQUIRED_PATH_LENGTH, f64::from(REQUIRED_PATH_LENGTH));
        }

        let mut entity_data = BeeEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla parity: `Bee.registerGoals`.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(0, BeeAttackGoal::new(ATTACK_SPEED_MOD));
            goals.add_goal(1, BeeEnterHiveGoal::new());
            goals.add_goal(2, BreedGoal::new(BREED_SPEED_MOD));
            goals.add_goal(3, TemptGoal::new(TEMPT_SPEED_MOD, Self::is_bee_food, false));
            goals.add_goal(3, ValidateHiveGoal::new());
            goals.add_goal(3, ValidateFlowerGoal::new());
            goals.add_goal(4, BeePollinateGoal::new());
            goals.add_goal(5, FollowParentGoal::new(FOLLOW_PARENT_SPEED_MOD));
            goals.add_goal(5, BeeLocateHiveGoal::new());
            goals.add_goal(5, BeeGoToHiveGoal::new());
            goals.add_goal(6, BeeGoToKnownFlowerGoal::new());
            goals.add_goal(7, BeeGrowCropGoal::new());
            goals.add_goal(8, BeeWanderGoal::new());
            goals.add_goal(9, FloatGoal::new(&mob_base));
        }
        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(1, BeeHurtByOtherGoal::new());
            targets.add_goal(2, BeeBecomeAngryTargetGoal::new());
            targets.add_goal(3, ResetUniversalAngerTargetGoal::new(true));
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            anger: PersistentAnger::new(),
            state: SyncMutex::new(BeeState::new()),
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Returns vanilla `Bee.hasNectar`.
    #[must_use]
    pub fn has_nectar(&self) -> bool {
        self.get_flag(FLAG_HAS_NECTAR)
    }

    /// Sets vanilla `Bee.setHasNectar`, which also restarts the hunger clock.
    pub fn set_has_nectar(&self, has_nectar: bool) {
        if has_nectar {
            self.reset_ticks_without_nectar_since_exiting_hive();
        }
        self.set_flag(FLAG_HAS_NECTAR, has_nectar);
    }

    /// Returns vanilla `Bee.hasStung`.
    #[must_use]
    pub fn has_stung(&self) -> bool {
        self.get_flag(FLAG_HAS_STUNG)
    }

    fn set_has_stung(&self, has_stung: bool) {
        self.set_flag(FLAG_HAS_STUNG, has_stung);
    }

    /// Returns vanilla `Bee.isRolling`.
    #[must_use]
    pub fn is_rolling(&self) -> bool {
        self.get_flag(FLAG_ROLL)
    }

    fn set_rolling(&self, rolling: bool) {
        self.set_flag(FLAG_ROLL, rolling);
    }

    fn get_flag(&self, flag: i8) -> bool {
        *self.entity_data.lock().flags.get() & flag != 0
    }

    fn set_flag(&self, flag: i8, value: bool) {
        let mut entity_data = self.entity_data.lock();
        let flags = *entity_data.flags.get();
        let updated = if value { flags | flag } else { flags & !flag };
        entity_data.flags.set(updated);
    }

    /// Returns vanilla `Bee.getHivePos`.
    #[must_use]
    pub fn hive_pos(&self) -> Option<BlockPos> {
        self.state.lock().hive_pos
    }

    /// Sets vanilla `Bee.setHivePos`.
    pub fn set_hive_pos(&self, hive_pos: BlockPos) {
        self.state.lock().hive_pos = Some(hive_pos);
    }

    /// Forgets the hive without starting the relocation cooldown.
    ///
    /// Vanilla parity: the bare `Bee.this.hivePos = null` assignments, as opposed
    /// to [`Self::drop_hive`], which also sets the cooldown.
    pub(super) fn clear_hive_pos(&self) {
        self.state.lock().hive_pos = None;
    }

    /// Returns vanilla `Bee.hasHive`.
    #[must_use]
    pub fn has_hive(&self) -> bool {
        self.state.lock().hive_pos.is_some()
    }

    /// Returns vanilla `Bee.getSavedFlowerPos`.
    #[must_use]
    pub fn saved_flower_pos(&self) -> Option<BlockPos> {
        self.state.lock().saved_flower_pos
    }

    /// Sets vanilla `Bee.setSavedFlowerPos`.
    pub fn set_saved_flower_pos(&self, pos: BlockPos) {
        self.state.lock().saved_flower_pos = Some(pos);
    }

    /// Returns vanilla `Bee.hasSavedFlowerPos`.
    #[must_use]
    pub fn has_saved_flower_pos(&self) -> bool {
        self.state.lock().saved_flower_pos.is_some()
    }

    /// Vanilla parity: `Bee.dropHive`.
    pub(super) fn drop_hive(&self) {
        let mut state = self.state.lock();
        state.hive_pos = None;
        state.remaining_cooldown_before_locating_new_hive = COOLDOWN_BEFORE_LOCATING_NEW_HIVE;
    }

    /// Vanilla parity: `Bee.dropFlower`.
    pub(super) fn drop_flower(&self) {
        let mut state = self.state.lock();
        state.saved_flower_pos = None;
        state.remaining_cooldown_before_locating_new_flower =
            rand::random_range(MIN_FIND_FLOWER_RETRY_COOLDOWN..MAX_FIND_FLOWER_RETRY_COOLDOWN);
    }

    /// Vanilla parity: `Bee.BeeGoToHiveGoal.blacklistTarget`, which remembers the
    /// last three hives that disappointed this bee.
    pub(super) fn blacklist_hive(&self, max_blacklisted: usize) {
        let mut state = self.state.lock();
        let Some(hive_pos) = state.hive_pos else {
            return;
        };
        state.blacklisted_hives.push(hive_pos);
        while state.blacklisted_hives.len() > max_blacklisted {
            state.blacklisted_hives.remove(0);
        }
    }

    /// Takes the closest hive that is not blacklisted, clearing the blacklist if
    /// they all are.
    ///
    /// Vanilla parity: the body of `Bee.BeeLocateHiveGoal.start`.
    pub(super) fn adopt_first_unblacklisted_hive(&self, hives: &[BlockPos]) {
        let mut state = self.state.lock();
        for candidate in hives {
            if !state.blacklisted_hives.contains(candidate) {
                state.hive_pos = Some(*candidate);
                return;
            }
        }

        // Everything nearby has already disappointed this bee once. Vanilla wipes
        // the slate rather than leaving it homeless.
        state.blacklisted_hives.clear();
        state.hive_pos = hives.first().copied();
    }

    /// Returns vanilla `Bee.remainingCooldownBeforeLocatingNewHive`.
    #[must_use]
    pub(super) fn hive_locate_cooldown(&self) -> i32 {
        self.state
            .lock()
            .remaining_cooldown_before_locating_new_hive
    }

    pub(super) fn set_hive_locate_cooldown(&self, ticks: i32) {
        self.state
            .lock()
            .remaining_cooldown_before_locating_new_hive = ticks;
    }

    /// Returns vanilla `Bee.remainingCooldownBeforeLocatingNewFlower`.
    #[must_use]
    pub(super) fn flower_locate_cooldown(&self) -> i32 {
        self.state
            .lock()
            .remaining_cooldown_before_locating_new_flower
    }

    pub(super) fn set_flower_locate_cooldown(&self, ticks: i32) {
        self.state
            .lock()
            .remaining_cooldown_before_locating_new_flower = ticks;
    }

    /// Returns vanilla `Bee.ticksWithoutNectarSinceExitingHive`.
    #[must_use]
    pub(super) fn ticks_without_nectar(&self) -> i32 {
        self.state.lock().ticks_without_nectar_since_exiting_hive
    }

    /// Returns vanilla `Bee.getCropsGrownSincePollination`.
    #[must_use]
    pub(super) fn crops_grown_since_pollination(&self) -> i32 {
        self.state.lock().num_crops_grown_since_pollination
    }

    /// Vanilla parity: `Bee.incrementNumCropsGrownSincePollination`.
    pub(super) fn increment_crops_grown_since_pollination(&self) {
        self.state.lock().num_crops_grown_since_pollination += 1;
    }

    /// Returns vanilla `Bee.BeePollinateGoal.isPollinating`.
    #[must_use]
    pub fn is_pollinating(&self) -> bool {
        self.state.lock().pollinating
    }

    pub(super) fn set_pollinating(&self, pollinating: bool) {
        self.state.lock().pollinating = pollinating;
    }

    /// Vanilla parity: `Bee.setStayOutOfHiveCountdown`, which is how a harvester
    /// with a campfire under the hive keeps the bees out rather than angry.
    pub fn set_stay_out_of_hive_countdown(&self, ticks: i32) {
        self.state.lock().stay_out_of_hive_countdown = ticks;
    }

    /// Vanilla parity: `Bee.resetTicksWithoutNectarSinceExitingHive`.
    pub(super) fn reset_ticks_without_nectar_since_exiting_hive(&self) {
        self.state.lock().ticks_without_nectar_since_exiting_hive = 0;
    }

    /// Vanilla parity: `Bee.isTiredOfLookingForNectar`.
    fn is_tired_of_looking_for_nectar(&self) -> bool {
        self.ticks_without_nectar() > TICKS_WITHOUT_NECTAR_BEFORE_GOING_HOME
    }

    /// Vanilla parity: `Bee.dropOffNectar`, called by the hive once the bee has
    /// been inside long enough to have handed the nectar over.
    pub fn drop_off_nectar(&self) {
        self.set_has_nectar(false);
        self.state.lock().num_crops_grown_since_pollination = 0;
    }

    /// Vanilla parity: `Bee.isTooFarAway`.
    pub(super) fn is_too_far_away(&self, target: BlockPos) -> bool {
        !self.closer_than(target, TOO_FAR_DISTANCE)
    }

    /// Vanilla parity: the private `Bee.closerThan`, which measures block to
    /// block rather than from the bee's exact position.
    pub(super) fn closer_than(&self, target: BlockPos, distance: i32) -> bool {
        let mob_pos = self.block_position();
        let dx = f64::from(target.x() - mob_pos.x());
        let dy = f64::from(target.y() - mob_pos.y());
        let dz = f64::from(target.z() - mob_pos.z());
        let distance = f64::from(distance);
        dx.mul_add(dx, dy.mul_add(dy, dz * dz)) < distance * distance
    }

    /// Runs `read` against this bee's hive, if it still has a reachable one.
    ///
    /// Vanilla parity: `Bee.getBeehiveBlockEntity`, which refuses a hive the bee
    /// has drifted too far from rather than reaching into an unloaded chunk.
    /// Steel hands the block entity to a closure rather than returning it,
    /// because a `SharedBlockEntity` can only be downcast by reference.
    pub(super) fn with_beehive<R>(&self, read: impl FnOnce(&BeehiveBlockEntity) -> R) -> Option<R> {
        let hive_pos = self.hive_pos()?;
        if self.is_too_far_away(hive_pos) {
            return None;
        }

        let world = self.level()?;
        let block_entity = world.get_block_entity(hive_pos)?;
        let hive = block_entity.downcast_ref::<BeehiveBlockEntity>()?;
        Some(read(hive))
    }

    /// Vanilla parity: `Bee.isHiveValid`.
    pub(super) fn is_hive_valid(&self) -> bool {
        self.with_beehive(|_| ()).is_some()
    }

    /// Vanilla parity: `Bee.doesHiveHaveSpace`.
    pub(super) fn does_hive_have_space(&self, hive_pos: BlockPos) -> bool {
        let Some(world) = self.level() else {
            return false;
        };
        world
            .get_block_entity(hive_pos)
            .and_then(|block_entity| {
                block_entity
                    .downcast_ref::<BeehiveBlockEntity>()
                    .map(|hive| !hive.is_full())
            })
            .unwrap_or(false)
    }

    /// Vanilla parity: `Bee.isHiveNearFire`.
    fn is_hive_near_fire(&self) -> bool {
        self.with_beehive(BeehiveBlockEntity::is_fire_nearby)
            .unwrap_or(false)
    }

    /// Vanilla parity: `Bee.wantsToEnterHive`.
    ///
    /// In 26.2 the "is it night or raining" test the bee used to make itself is
    /// the `gameplay/bees_stay_in_hive` environment attribute, so a datapack
    /// timeline decides the bees' curfew rather than the entity.
    pub(super) fn wants_to_enter_hive(&self) -> bool {
        let (stay_out, pollinating) = {
            let state = self.state.lock();
            (state.stay_out_of_hive_countdown, state.pollinating)
        };
        if stay_out > 0 || pollinating || self.has_stung() || self.target().is_some() {
            return false;
        }

        let curfew = self.level().is_some_and(|world| world.bees_stay_in_hive());
        let wants = self.has_nectar() || self.is_tired_of_looking_for_nectar() || curfew;
        wants && !self.is_hive_near_fire()
    }

    /// Vanilla parity: `Bee.attractsBees`, the block test behind both the
    /// pollination search and the flower the bee remembers.
    #[must_use]
    pub fn attracts_bees(state: BlockStateId) -> bool {
        if !state.get_block().has_tag(&BlockTag::BEE_ATTRACTIVE) {
            return false;
        }
        if state
            .try_get_value(&BlockStateProperties::WATERLOGGED)
            .unwrap_or(false)
        {
            return false;
        }
        if state.get_block() == &vanilla_blocks::SUNFLOWER {
            // Only the upper half of a sunflower is a flower to a bee.
            return state
                .try_get_value(&BlockStateProperties::DOUBLE_BLOCK_HALF)
                .is_some_and(|half| half == DoubleBlockHalf::Upper);
        }
        true
    }

    /// Returns whether the stack is vanilla bee food.
    #[must_use]
    pub fn is_bee_food(item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::BEE_FOOD)
    }

    /// Vanilla parity: the `setRolling` half of `Bee.aiStep`.
    ///
    /// The interpolated `rollAmount` itself is client-local; the server owns only
    /// the flag that drives it.
    fn update_rolling(&self) {
        let target_close = self.target().is_some_and(|target| {
            target.position().distance_squared(self.position()) < MIN_ATTACK_DIST_SQR
        });
        self.set_rolling(self.is_angry() && !self.has_stung() && target_close);
    }

    /// Vanilla parity: the `flower.getBeeInteractionEffect()` branch of
    /// `Bee.mobInteract`, which is live for the wither rose and the eyeblossom
    /// because both are in `#minecraft:bee_food`.
    fn held_flower_effect(
        &self,
        player: &Player,
        hand: InteractionHand,
    ) -> Option<MobEffectInstance> {
        let item_stack = {
            let inventory = player.inventory.lock();
            let item_stack = inventory.get_item_in_hand(hand);
            item_stack.copy_with_count(item_stack.count())
        };
        if !Self::is_bee_food(&item_stack) {
            return None;
        }

        let block = ITEM_BEHAVIORS
            .get_behavior(item_stack.item())
            .placed_block()?;
        BLOCK_BEHAVIORS.get_behavior(block).bee_interaction_effect()
    }
}

impl Entity for BeeEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `Bee.playStepSound`, which is empty -- a bee never touches
    /// the ground long enough to make a noise doing it.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {}

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);

        {
            let state = self.state.lock();
            if let Some(hive_pos) = state.hive_pos {
                nbt.insert("hive_pos", block_pos_tag(hive_pos));
            }
            if let Some(flower_pos) = state.saved_flower_pos {
                nbt.insert("flower_pos", block_pos_tag(flower_pos));
            }
            nbt.insert(
                "TicksSincePollination",
                state.ticks_without_nectar_since_exiting_hive,
            );
            nbt.insert("CannotEnterHiveTicks", state.stay_out_of_hive_countdown);
            nbt.insert(
                "CropsGrownSincePollination",
                state.num_crops_grown_since_pollination,
            );
        }

        nbt.insert("HasNectar", self.has_nectar());
        nbt.insert("HasStung", self.has_stung());
        nbt.insert("anger_end_time", self.persistent_anger_end_time());
        if let Some(target) = self.persistent_anger_target() {
            nbt.insert("angry_at", NbtTag::IntArray(target.to_int_array().to_vec()));
        }
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);

        self.set_has_nectar(nbt.byte("HasNectar").is_some_and(|flag| flag != 0));
        self.set_has_stung(nbt.byte("HasStung").is_some_and(|flag| flag != 0));

        {
            let mut state = self.state.lock();
            state.ticks_without_nectar_since_exiting_hive =
                nbt.int("TicksSincePollination").unwrap_or(0);
            state.stay_out_of_hive_countdown = nbt.int("CannotEnterHiveTicks").unwrap_or(0);
            state.num_crops_grown_since_pollination =
                nbt.int("CropsGrownSincePollination").unwrap_or(0);
            state.hive_pos = read_block_pos(nbt, "hive_pos");
            state.saved_flower_pos = read_block_pos(nbt, "flower_pos");
        }

        read_persistent_anger(
            self,
            nbt.long("anger_end_time"),
            nbt.int("AngerTime"),
            nbt.int_array("angry_at")
                .and_then(|values| Uuid::from_int_array(&values)),
        );
    }
}

impl LivingEntity for BeeEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    fn living_synced_data(&self) -> Option<&dyn LivingEntitySyncedData> {
        Some(&self.entity_data)
    }

    fn get_health(&self) -> f32 {
        *self.entity_data.lock().living_entity().health.get()
    }

    fn set_health(&self, health: f32) {
        let max_health = self.get_max_health();
        let clamped = health.clamp(0.0, max_health);
        self.entity_data
            .lock()
            .living_entity_mut()
            .health
            .set(clamped);
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_BEE_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_BEE_DEATH)
    }

    fn sound_volume(&self) -> f32 {
        SOUND_VOLUME
    }

    /// Vanilla parity: `Bee.jumpInLiquid`, a much smaller push than the shared
    /// one so a bee that fell in does not rocket out.
    fn jump_in_liquid(&self, _fluid_tag: &Identifier) {
        self.set_velocity(self.velocity() + DVec3::new(0.0, LIQUID_JUMP_LIFT, 0.0));
    }

    /// Vanilla parity: `Bee.hurtServer`, which knocks the bee off its flower
    /// before anything else so the pollination does not resume mid-fight.
    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        if self.is_invulnerable_to(world, source) {
            return false;
        }
        self.set_pollinating(false);
        self.living_hurt_server(world, source, amount)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    /// Vanilla parity: `Bee.aiStep`, where the three cooldowns run down and where
    /// a bee notices its hive has been mined out.
    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);

        {
            let mut state = self.state.lock();
            state.stay_out_of_hive_countdown = state.stay_out_of_hive_countdown.saturating_sub(1);
            state.remaining_cooldown_before_locating_new_hive = state
                .remaining_cooldown_before_locating_new_hive
                .saturating_sub(1);
            state.remaining_cooldown_before_locating_new_flower = state
                .remaining_cooldown_before_locating_new_flower
                .saturating_sub(1);
        }

        self.update_rolling();
        if self.tick_count() % 20 == 0 && !self.is_hive_valid() {
            self.clear_hive_pos();
        }

        result
    }
}

impl AgeableMob for BeeEntity {
    fn ageable_base(&self) -> &AgeableMobBase {
        &self.ageable_base
    }

    fn is_age_locked(&self) -> bool {
        *self.entity_data.lock().ageable_mob().age_locked.get()
    }

    fn set_age_locked(&self, age_locked: bool) {
        self.entity_data
            .lock()
            .ageable_mob_mut()
            .age_locked
            .set(age_locked);
    }

    fn set_synced_baby(&self, baby: bool) {
        self.entity_data.lock().ageable_mob_mut().baby.set(baby);
    }
}

impl Animal for BeeEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        Self::is_bee_food(item_stack)
    }
}

impl NeutralMob for BeeEntity {
    fn persistent_anger(&self) -> &PersistentAnger {
        &self.anger
    }

    /// Vanilla parity: `Bee.getPersistentAngerEndTime`, which reads the
    /// synchronized field so the client can draw an angry bee.
    fn persistent_anger_end_time(&self) -> i64 {
        *self.entity_data.lock().anger_end_time.get()
    }

    fn set_persistent_anger_end_time(&self, end_time: i64) {
        self.entity_data.lock().anger_end_time.set(end_time);
    }

    /// Vanilla parity: `Bee.startPersistentAngerTimer`.
    fn start_persistent_anger_timer(&self) {
        self.set_time_to_remain_angry(rand::random_range(ANGER_MIN_TICKS..=ANGER_MAX_TICKS));
    }
}

impl Mob for BeeEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    /// Vanilla parity: the anonymous `FlyingPathNavigation` of
    /// `Bee.createNavigation`, whose whole override is to stand still while the
    /// bee is hovering over a flower.
    fn tick_path_navigation(&self) {
        if self.is_pollinating() {
            return;
        }
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    /// Vanilla parity: `Bee.customServerAiStep` -- drowning, the sting clock, the
    /// hunger clock, and the anger reconciliation.
    fn custom_server_ai_step(&self) {
        Animal::custom_server_ai_step_animal(self);

        let Some(world) = self.level() else {
            return;
        };

        let under_water_ticks = {
            let mut state = self.state.lock();
            state.under_water_ticks = if self.is_in_water() {
                state.under_water_ticks + 1
            } else {
                0
            };
            state.under_water_ticks
        };
        if under_water_ticks > UNDERWATER_DROWN_TICKS {
            self.hurt_server(
                &world,
                &DamageSource::environment(&vanilla_damage_types::DROWN),
                DROWN_DAMAGE,
            );
        }

        if self.has_stung() {
            let time_since_sting = {
                let mut state = self.state.lock();
                state.time_since_sting += 1;
                state.time_since_sting
            };
            // Vanilla parity: a bee that has stung rolls every fifth tick against
            // an ever-shortening window, so it dies somewhere inside the minute
            // rather than exactly at the end of one.
            let window = (STING_DEATH_COUNTDOWN - time_since_sting).clamp(1, STING_DEATH_COUNTDOWN);
            if time_since_sting % STING_DEATH_ROLL_INTERVAL == 0
                && rand::random_range(0..window) == 0
            {
                self.hurt_server(
                    &world,
                    &DamageSource::environment(&vanilla_damage_types::GENERIC),
                    self.get_health(),
                );
            }
        }

        if !self.has_nectar() {
            self.state.lock().ticks_without_nectar_since_exiting_hive += 1;
        }

        self.update_persistent_anger(&world, false);
    }

    /// Vanilla parity: `Bee.doHurtTarget`, the sting: poison scaled by difficulty
    /// and the bee's own death sentence.
    fn do_hurt_target(&self, world: &World, target: &SharedEntity) -> bool {
        let damage = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::ATTACK_DAMAGE) as f32;
        let source = DamageSource::environment(&vanilla_damage_types::STING)
            .with_causing_entity(self.id())
            .with_direct_entity(self.id());

        let Some(living) = target.as_living_entity() else {
            return false;
        };
        if !living.hurt_server(world, &source, damage) {
            return false;
        }

        let poison_seconds = match world.difficulty() {
            Difficulty::Normal => POISON_SECONDS_NORMAL,
            Difficulty::Hard => POISON_SECONDS_HARD,
            Difficulty::Peaceful | Difficulty::Easy => 0,
        };
        if poison_seconds > 0 {
            living.add_mob_effect(MobEffectInstance::with_duration(
                vanilla_mob_effects::POISON,
                poison_seconds * 20,
                0,
            ));
        }

        self.set_has_stung(true);
        self.stop_being_angry();
        self.play_sound(&sound_events::ENTITY_BEE_STING, 1.0, 1.0);
        true
    }

    /// Vanilla parity: `Bee.getAmbientSound`, which is null -- the client plays
    /// the buzzing loop itself from the flying flag.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        None
    }

    /// Vanilla parity: `Bee.mobInteract`, which lets a flower that carries a bee
    /// effect be fed to the bee instead of breeding it.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        if let Some(effect) = self.held_flower_effect(player, hand) {
            self.use_player_item(player, hand);
            self.add_mob_effect(effect);
            return InteractionResult::Success;
        }

        Animal::mob_interact_animal(self, player, hand)
    }

    fn move_control_kind(&self) -> MoveControlKind {
        MoveControlKind::Flying {
            max_turn: MOVE_CONTROL_MAX_TURN,
            hovers_in_place: true,
        }
    }

    /// Vanilla parity: `Bee.BeeLookControl.tick`, which stops turning the head
    /// while the bee is angry so it keeps facing what it is diving at.
    fn tick_look_control(&self) {
        if self.is_angry() {
            return;
        }
        self.default_tick_look_control();
    }

    /// Vanilla parity: `Bee.BeeLookControl.resetXRotOnTick`, which holds the
    /// pitch a hovering bee has tilted to.
    fn look_control_resets_pitch(&self) -> bool {
        !self.is_pollinating()
    }
}

impl PathfinderMob for BeeEntity {
    fn navigation_kind(&self) -> NavigationKind {
        NavigationKind::Flying
    }

    /// Vanilla parity: `Bee.getWalkTargetValue`, the inverse of every walker's: a
    /// bee wants the air, not the ground.
    fn get_walk_target_value(&self, pos: BlockPos) -> f32 {
        let Some(world) = self.level() else {
            return 0.0;
        };
        if world.get_block_state(pos).is_air() {
            10.0
        } else {
            0.0
        }
    }

    /// Vanilla parity: the `isStableDestination` override of
    /// `Bee.createNavigation`, which wants something -- anything -- under the
    /// node rather than the solid face the shared flier asks for.
    fn is_stable_destination(&self, pos: BlockPos) -> bool {
        self.level()
            .is_some_and(|world| !world.get_block_state(pos.below()).is_air())
    }
}

/// Writes a vanilla `BlockPos.CODEC` int array.
fn block_pos_tag(pos: BlockPos) -> NbtTag {
    NbtTag::IntArray(vec![pos.x(), pos.y(), pos.z()])
}

/// Reads a vanilla `BlockPos.CODEC` int array.
fn read_block_pos(nbt: BorrowedNbtCompoundView<'_, '_>, key: &str) -> Option<BlockPos> {
    let values = nbt.int_array(key)?;
    let [x, y, z] = values[..] else { return None };
    Some(BlockPos::new(x, y, z))
}

#[cfg(test)]
mod tests;
