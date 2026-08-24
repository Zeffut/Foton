//! Vex entity.
//!
//! Vanilla parity: `Vex`, `Vex.VexChargeAttackGoal`, `Vex.VexRandomMoveGoal`
//! and `Vex.VexCopyOwnerTargetGoal`. A vex ignores blocks entirely, drifts
//! around the point it was summoned at, and charges straight through walls at
//! whatever its summoner is fighting until its borrowed life runs out.
//!
//! Summoning is the evoker's job; this is only the vex itself.
//!
//! **Gap**: `Vex.getLightLevelDependentMagicValue` returns a flat `1.0F`.
//! Steel's `PathfinderMob::get_walk_target_value` default only implements
//! vanilla's darkness formula for animals, so the override has nothing to
//! change yet.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::VexEntityData;
use steel_registry::{sound_events, vanilla_damage_types, vanilla_items};
use steel_utils::locks::SyncMutex;
use steel_utils::uuid_ext::UuidExt as _;
use steel_utils::{BlockPos, Downcast as _, DowncastType, DowncastTypeKey};
use uuid::Uuid;

use crate::entity::Enemy;
use crate::entity::EntitySpawnReason;
use crate::entity::ai::control::VexMoveControl;
use crate::entity::ai::goal::{
    FloatGoal, Goal, GoalControls, HurtByTargetGoal, LookAtPlayerGoal, NearestAttackableTargetGoal,
    reduced_tick_delay,
};
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::damage::DamageSource;
use crate::entity::entities::WitchEntity;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase, Mob,
    MobBase, PathfinderMob, SharedEntity, SpawnGroupData,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::world::World;

/// Experience a vex drops.
///
/// Vanilla parity: the `this.xpReward = 3` of the constructor.
const XP_REWARD: i32 = 3;

/// Ticks between two wingbeats.
///
/// Vanilla parity: `Vex.TICKS_PER_FLAP`, `Mth.ceil(Math.PI * 5.0 / 4.0)`.
const TICKS_PER_FLAP: i32 = 4;

/// The synchronized bit that says a vex is mid-charge.
///
/// Vanilla parity: `Vex.FLAG_IS_CHARGING`.
const FLAG_IS_CHARGING: i8 = 1;

/// Ticks between two hits once a vex's life has run out.
///
/// Vanilla parity: the `this.limitedLifeTicks = 20` of `Vex.tick`, which is
/// what makes a spent vex flicker away over several seconds rather than pop.
const LIMITED_LIFE_DEATH_INTERVAL: i32 = 20;

/// Damage a vex takes each time its spent life ticks over.
///
/// Vanilla parity: the `hurt(damageSources().starve(), 1.0F)` of `Vex.tick`.
const LIMITED_LIFE_DAMAGE: f32 = 1.0;

/// Distance at which a vex watches a player.
///
/// Vanilla parity: `new LookAtPlayerGoal(this, Player.class, 3.0F, 1.0F)`.
const LOOK_AT_PLAYER_RANGE: f64 = 3.0;

/// How often a vex bothers to look at a player.
const LOOK_AT_PLAYER_PROBABILITY: f32 = 1.0;

/// How often a vex bothers to look at another mob.
///
/// Vanilla parity: the `DEFAULT_PROBABILITY` of the two-argument
/// `LookAtPlayerGoal` constructor.
const LOOK_AT_MOB_PROBABILITY: f32 = 0.02;

/// Distance at which a vex watches another mob.
///
/// Vanilla parity: `new LookAtPlayerGoal(this, Mob.class, 8.0F)`.
const LOOK_AT_MOB_RANGE: f64 = 8.0;

/// One chance in this many ticks that a vex picks a new move.
///
/// Vanilla parity: the `reducedTickDelay(7)` shared by both movement goals.
const MOVE_ATTEMPT_INTERVAL_TICKS: i32 = 7;

/// Squared distance a target has to be beyond before a vex bothers charging.
///
/// Vanilla parity: the `distanceToSqr(target) > 4.0` of
/// `VexChargeAttackGoal.canUse`.
const CHARGE_MIN_RANGE_SQR: f64 = 4.0;

/// Squared distance within which a charging vex keeps re-aiming.
///
/// Vanilla parity: the `distance < 9.0` of `VexChargeAttackGoal.tick`.
const CHARGE_REAIM_RANGE_SQR: f64 = 9.0;

/// Speed multiplier of a charge.
const CHARGE_SPEED_MODIFIER: f64 = 1.0;

/// Speed multiplier of an idle drift.
///
/// Vanilla parity: the `0.25` of `VexRandomMoveGoal.tick`.
const WANDER_SPEED_MODIFIER: f64 = 0.25;

/// How many spots a drifting vex tries before it gives up for this attempt.
const WANDER_ATTEMPTS: i32 = 3;

/// Horizontal span of a drift target, in blocks.
///
/// Vanilla parity: the `random.nextInt(15) - 7` of `VexRandomMoveGoal.tick`.
const WANDER_HORIZONTAL_SPAN: i32 = 15;

/// Horizontal offset subtracted from the span, centering it on the origin.
const WANDER_HORIZONTAL_OFFSET: i32 = 7;

/// Vertical span of a drift target, in blocks.
const WANDER_VERTICAL_SPAN: i32 = 11;

/// Vertical offset subtracted from the span.
const WANDER_VERTICAL_OFFSET: i32 = 5;

/// A vex.
#[entity_behavior(class = "Vex")]
pub struct VexEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<VexEntityData>,
    /// The mob that summoned this vex (vanilla `Vex.owner`).
    owner: SyncMutex<Option<Uuid>>,
    /// The point this vex drifts around (vanilla `Vex.boundOrigin`).
    bound_origin: SyncMutex<Option<BlockPos>>,
    /// The borrowed lifetime, if this vex was given one.
    ///
    /// Vanilla keeps `hasLimitedLife` and `limitedLifeTicks` apart; `None` here
    /// is the `hasLimitedLife == false` case, which is the only state in which
    /// the counter is meaningless.
    limited_life_ticks: SyncMutex<Option<i32>>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `VexEntity`.
unsafe impl DowncastType for VexEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/vex");
}

impl VexEntity {
    /// Creates a vex at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a vex from saved base data.
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
        let mut entity_data = VexEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);
        mob_base.set_xp_reward(XP_REWARD);

        {
            // Keep vanilla Vex goal priorities in the same order.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(0, FloatGoal::new(&mob_base));
            goals.add_goal(4, VexChargeAttackGoal::new());
            goals.add_goal(8, VexRandomMoveGoal);
            goals.add_goal(
                9,
                LookAtPlayerGoal::new_with_probability(
                    LOOK_AT_PLAYER_RANGE,
                    LOOK_AT_PLAYER_PROBABILITY,
                ),
            );
            goals.add_goal(
                10,
                LookAtPlayerGoal::new_for_living_entities(
                    LOOK_AT_MOB_RANGE,
                    LOOK_AT_MOB_PROBABILITY,
                    |_, target, _| target.is_mob(),
                ),
            );
        }

        {
            let mut targets = mob_base.target_selector().lock();
            // Vanilla passes `Raider.class` so a vex never turns on the
            // illagers it fights beside.
            //
            // TODO: Steel's ignore list is by concrete type, and the witch is
            // the only raider it has; the rest have to be added here as they
            // land.
            targets.add_goal(
                1,
                HurtByTargetGoal::new()
                    .with_ignored_damage_types([WitchEntity::TYPE_KEY])
                    .set_alert_others([]),
            );
            targets.add_goal(2, VexCopyOwnerTargetGoal::new());
            targets.add_goal(
                3,
                NearestAttackableTargetGoal::new_for_players(true, |_, _, _| true),
            );
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            owner: SyncMutex::new(None),
            bound_origin: SyncMutex::new(None),
            limited_life_ticks: SyncMutex::new(None),
        }
    }

    /// Returns the point this vex drifts around, if it was given one.
    ///
    /// Vanilla parity: `Vex.getBoundOrigin`.
    #[must_use]
    pub fn bound_origin(&self) -> Option<BlockPos> {
        *self.bound_origin.lock()
    }

    /// Vanilla parity: `Vex.setBoundOrigin`, called by the evoker that summons
    /// the vex so the swarm stays around the summoning spot.
    pub fn set_bound_origin(&self, bound_origin: Option<BlockPos>) {
        *self.bound_origin.lock() = bound_origin;
    }

    /// Returns the mob that summoned this vex.
    ///
    /// Vanilla parity: `Vex.getOwnerReference`.
    #[must_use]
    pub fn owner_uuid(&self) -> Option<Uuid> {
        *self.owner.lock()
    }

    /// Vanilla parity: `Vex.setOwner`.
    pub fn set_owner(&self, owner: &dyn Entity) {
        *self.owner.lock() = Some(owner.uuid());
    }

    /// Resolves the summoner, if it is still loaded.
    ///
    /// Vanilla parity: `OwnableEntity.getOwner`.
    #[must_use]
    pub fn owner(&self) -> Option<SharedEntity> {
        let uuid = self.owner_uuid()?;
        self.level()?.get_entity_by_uuid(&uuid)
    }

    /// Gives this vex a borrowed lifetime.
    ///
    /// Vanilla parity: `Vex.setLimitedLife`.
    pub fn set_limited_life(&self, life_ticks: i32) {
        *self.limited_life_ticks.lock() = Some(life_ticks);
    }

    /// Returns whether this vex is mid-charge.
    ///
    /// Vanilla parity: `Vex.isCharging`.
    #[must_use]
    pub fn is_charging(&self) -> bool {
        *self.entity_data.lock().flags.get() & FLAG_IS_CHARGING != 0
    }

    /// Vanilla parity: `Vex.setIsCharging`.
    pub fn set_is_charging(&self, charging: bool) {
        let mut data = self.entity_data.lock();
        let flags = *data.flags.get();
        let updated = if charging {
            flags | FLAG_IS_CHARGING
        } else {
            flags & !FLAG_IS_CHARGING
        };
        data.flags.set(updated);
    }

    /// Burns down the borrowed lifetime and starves the vex once it is spent.
    ///
    /// Vanilla parity: the tail of `Vex.tick`.
    fn tick_limited_life(&self) {
        let expired = {
            let mut ticks = self.limited_life_ticks.lock();
            let Some(remaining) = ticks.as_mut() else {
                return;
            };
            *remaining -= 1;
            if *remaining > 0 {
                return;
            }
            *remaining = LIMITED_LIFE_DEATH_INTERVAL;
            true
        };

        if expired && let Some(world) = self.level() {
            self.hurt(
                world.as_ref(),
                &DamageSource::environment(&vanilla_damage_types::STARVE),
                LIMITED_LIFE_DAMAGE,
            );
        }
    }
}

/// Rushes the target in a straight line and hits whatever it lands on.
///
/// Vanilla parity: `Vex.VexChargeAttackGoal`.
struct VexChargeAttackGoal;

impl VexChargeAttackGoal {
    const fn new() -> Self {
        Self
    }

    /// Points the move control at the target's eyes.
    fn aim_at(mob: &dyn PathfinderMob, target: &SharedEntity) {
        let position = target.position();
        let eye_position = DVec3::new(position.x, target.get_eye_y(), position.z);
        mob.mob_base()
            .controls()
            .lock()
            .move_control
            .set_wanted_position(eye_position, CHARGE_SPEED_MODIFIER);
    }
}

impl Goal for VexChargeAttackGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    /// Vanilla parity: `VexChargeAttackGoal.canUse`.
    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(target) = mob.target() else {
            return false;
        };
        if !target
            .as_living_entity()
            .is_some_and(LivingEntity::is_alive)
        {
            return false;
        }
        if mob.mob_base().controls().lock().move_control.has_wanted() {
            return false;
        }
        if rand::random_range(0..reduced_tick_delay(MOVE_ATTEMPT_INTERVAL_TICKS)) != 0 {
            return false;
        }

        mob.position().distance_squared(target.position()) > CHARGE_MIN_RANGE_SQR
    }

    /// Vanilla parity: `VexChargeAttackGoal.canContinueToUse`.
    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if !mob.mob_base().controls().lock().move_control.has_wanted() {
            return false;
        }
        if !mob
            .downcast_ref::<VexEntity>()
            .is_some_and(VexEntity::is_charging)
        {
            return false;
        }

        mob.target()
            .and_then(|target| target.as_living_entity().map(LivingEntity::is_alive))
            .unwrap_or(false)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        if let Some(target) = mob.target() {
            Self::aim_at(mob, &target);
        }
        if let Some(vex) = mob.downcast_ref::<VexEntity>() {
            vex.set_is_charging(true);
        }
        mob.play_sound(&sound_events::ENTITY_VEX_CHARGE, 1.0, 1.0);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        if let Some(vex) = mob.downcast_ref::<VexEntity>() {
            vex.set_is_charging(false);
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    /// Vanilla parity: `VexChargeAttackGoal.tick`.
    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(target) = mob.target() else {
            return;
        };

        if mob.bounding_box().intersects(target.bounding_box()) {
            if let Some(world) = mob.level() {
                let _ = mob.do_hurt_target(world.as_ref(), &target);
            }
            if let Some(vex) = mob.downcast_ref::<VexEntity>() {
                vex.set_is_charging(false);
            }
            return;
        }

        if mob.position().distance_squared(target.position()) < CHARGE_REAIM_RANGE_SQR {
            Self::aim_at(mob, &target);
        }
    }
}

/// Drifts around the point the vex was summoned at.
///
/// Vanilla parity: `Vex.VexRandomMoveGoal`.
struct VexRandomMoveGoal;

impl Goal for VexRandomMoveGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    /// Vanilla parity: `VexRandomMoveGoal.canUse`.
    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        !mob.mob_base().controls().lock().move_control.has_wanted()
            && rand::random_range(0..reduced_tick_delay(MOVE_ATTEMPT_INTERVAL_TICKS)) == 0
    }

    /// Vanilla parity: `VexRandomMoveGoal.canContinueToUse` returns false, so
    /// the goal runs for exactly one tick each time it starts.
    fn can_continue_to_use(&mut self, _mob: &dyn PathfinderMob) -> bool {
        false
    }

    /// Vanilla parity: `VexRandomMoveGoal.tick`.
    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(world) = mob.level() else {
            return;
        };
        let origin = mob
            .downcast_ref::<VexEntity>()
            .and_then(VexEntity::bound_origin)
            .unwrap_or_else(|| mob.block_position());

        for _ in 0..WANDER_ATTEMPTS {
            let test_pos = origin.offset(
                rand::random_range(0..WANDER_HORIZONTAL_SPAN) - WANDER_HORIZONTAL_OFFSET,
                rand::random_range(0..WANDER_VERTICAL_SPAN) - WANDER_VERTICAL_OFFSET,
                rand::random_range(0..WANDER_HORIZONTAL_SPAN) - WANDER_HORIZONTAL_OFFSET,
            );
            // Vanilla parity: .
            // Vanilla parity: `Level.isEmptyBlock`.
            if !world.get_block_state(test_pos).is_air() {
                continue;
            }

            let (x, y, z) = test_pos.get_center();
            let center = DVec3::new(x, y, z);
            mob.mob_base()
                .controls()
                .lock()
                .move_control
                .set_wanted_position(center, WANDER_SPEED_MODIFIER);
            if mob.target().is_none() {
                mob.mob_base()
                    .controls()
                    .lock()
                    .look_control
                    .set_look_at(center, 180.0, 20.0);
            }
            return;
        }
    }
}

/// Takes over whatever the summoner is fighting.
///
/// Vanilla parity: `Vex.VexCopyOwnerTargetGoal`.
struct VexCopyOwnerTargetGoal {
    targeting: TargetingConditions,
}

impl VexCopyOwnerTargetGoal {
    fn new() -> Self {
        Self {
            targeting: TargetingConditions::for_non_combat()
                .ignore_line_of_sight()
                .ignore_invisibility_testing(),
        }
    }

    /// Returns what the summoner is currently fighting.
    fn owner_target(mob: &dyn PathfinderMob) -> Option<SharedEntity> {
        let vex = mob.downcast_ref::<VexEntity>()?;
        let owner = vex.owner()?;
        owner.as_mob()?.target()
    }
}

impl Goal for VexCopyOwnerTargetGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::TARGET
    }

    /// Vanilla parity: `VexCopyOwnerTargetGoal.canUse`.
    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(world) = mob.level() else {
            return false;
        };
        let Some(target) = Self::owner_target(mob) else {
            return false;
        };
        let Some(living) = target.as_living_entity() else {
            return false;
        };

        self.targeting.test(world.as_ref(), Some(mob), living)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let target = Self::owner_target(mob);
        mob.set_target(target.as_ref());
    }
}

impl Entity for VexEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `Vex.tick`, which turns collision off around the whole
    /// tick and forces gravity off after it, so a vex drifts through walls and
    /// never falls.
    fn tick(&self) {
        self.set_no_physics(true);
        self.tick_living_entity();
        self.set_no_physics(false);
        self.set_no_gravity(true);
        self.tick_limited_life();
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    /// Vanilla parity: `Vex.isFlapping`.
    fn is_flapping(&self) -> bool {
        self.tick_count() % TICKS_PER_FLAP == 0
    }

    /// Vanilla parity: `Vex.isAffectedByBlocks`. A vex passes through blocks
    /// without being slowed, burned or webbed by them, right up until it is
    /// removed.
    fn is_affected_by_blocks(&self) -> bool {
        !self.is_removed()
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        if let Some(origin) = self.bound_origin() {
            nbt.insert(
                "bound_pos",
                NbtTag::IntArray(vec![origin.x(), origin.y(), origin.z()]),
            );
        }
        if let Some(life_ticks) = *self.limited_life_ticks.lock() {
            nbt.insert("life_ticks", life_ticks);
        }
        if let Some(owner) = self.owner_uuid() {
            nbt.insert("owner", NbtTag::IntArray(owner.to_int_array().to_vec()));
        }
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        *self.bound_origin.lock() = nbt.int_array("bound_pos").and_then(|coords| match *coords {
            [x, y, z] => Some(BlockPos::new(x, y, z)),
            _ => None,
        });
        *self.limited_life_ticks.lock() = nbt.int("life_ticks");
        *self.owner.lock() = nbt
            .int_array("owner")
            .and_then(|array| Uuid::from_int_array(&array));
    }
}

impl LivingEntity for VexEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: `Mob.serverAiStep`, which is where a mob's goals run.
    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
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
        Some(&sound_events::ENTITY_VEX_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_VEX_DEATH)
    }
}

impl Mob for VexEntity {
    /// Vanilla parity: `Vex` derives from `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    /// Vanilla parity: `Vex` installs a `VexMoveControl`.
    fn tick_move_control(&self) {
        VexMoveControl.tick(self);
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_VEX_AMBIENT)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    /// Vanilla parity: `Vex.finalizeSpawn`, whose only work is the sword.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        // Vanilla parity: `Vex.populateDefaultEquipmentSlots`. The sword never
        // drops, which is why a vex swarm leaves no loot behind.
        self.living_base().equipment().lock().set(
            EquipmentSlot::MainHand,
            ItemStack::new(&vanilla_items::IRON_SWORD),
        );
        self.set_drop_chance(EquipmentSlot::MainHand, 0.0);

        self.finalize_spawn_mob_base(world, spawn_reason, group_data)
    }
}

impl PathfinderMob for VexEntity {}

impl Enemy for VexEntity {}

#[cfg(test)]
mod tests;
