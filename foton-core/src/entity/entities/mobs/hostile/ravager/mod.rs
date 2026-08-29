//! Ravager entity.
//!
//! Vanilla parity: `Ravager`. The siege engine of a raid: a hundred hearts,
//! twelve damage, and a charge that chews through leaves and jumps walls. Its
//! whole rhythm is three timers -- the swing, the stun and the roar -- and the
//! interesting one is the stun: block a ravager's hit with a shield and half
//! the time it staggers for two seconds, then roars and throws everything
//! around it away. That is the only opening a player gets.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::vanilla_entity_data::RavagerEntityData;
use foton_registry::vanilla_game_rules::MOB_GRIEFING;
use foton_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_attributes, vanilla_damage_types,
    vanilla_entities, vanilla_game_events,
};
use foton_utils::entity_events::EntityStatus;
use foton_utils::locks::SyncMutex;
use foton_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, WorldAabb};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::entity::ai::goal::GoalControl;
use crate::entity::ai::goal::{
    FloatGoal, HurtByTargetGoal, LongDistancePatrolGoal, LookAtPlayerGoal, MeleeAttackGoal,
    NearestAttackableTargetGoal, ObtainRaidLeaderBannerGoal, PathfindToRaidGoal,
    RaiderCelebrationGoal, RaiderMoveThroughVillageGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::entity::EntityEventSource as _;
use crate::entity::patrolling_monster::{
    PatrolState, PatrollingMonster, read_patrol_state, write_patrol_state,
};
use crate::entity::raider::{
    Raider, RaiderState, finalize_spawn_raider, read_raider_state, write_raider_state,
};
use crate::entity::spawn_rules::check_monster_spawn_rules;
use crate::entity::{
    Enemy, Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob, SharedEntity, SpawnGroupData,
};
use crate::world::game_event::GameEventContext;
use crate::world::{ClipBlockShape, ClipFluid, World};

/// NBT key vanilla stores the swing timer under.
const TAG_ATTACK_TICK: &str = "AttackTick";
/// NBT key vanilla stores the stun timer under.
const TAG_STUN_TICK: &str = "StunTick";
/// NBT key vanilla stores the roar timer under.
const TAG_ROAR_TICK: &str = "RoarTick";

/// Ticks a swing takes.
///
/// Vanilla parity: `Ravager.ATTACK_DURATION`.
const ATTACK_DURATION: i32 = 10;

/// Ticks a stun lasts.
///
/// Vanilla parity: `Ravager.STUN_DURATION`.
const STUN_DURATION: i32 = 40;

/// Ticks the roar wind-up takes.
///
/// Vanilla parity: the `roarTick = 20` set when a stun runs out.
const ROAR_DURATION: i32 = 20;

/// Point in the roar wind-up at which the roar actually lands.
///
/// Vanilla parity: the `roarTick == 10` of `aiStep`.
const ROAR_TRIGGER_TICK: i32 = 10;

/// Base speed of a ravager with nothing to chase.
///
/// Vanilla parity: `Ravager.BASE_MOVEMENT_SPEED`.
const BASE_MOVEMENT_SPEED: f64 = 0.3;

/// Speed a ravager works up to once it has a target.
///
/// Vanilla parity: `Ravager.ATTACK_MOVEMENT_SPEED`.
const ATTACK_MOVEMENT_SPEED: f64 = 0.35;

/// How fast a ravager accelerates towards its target speed.
///
/// Vanilla parity: the `Mth.lerp(0.1, base, max)` of `aiStep`, which is why a
/// ravager takes a moment to get going.
const SPEED_LERP: f64 = 0.1;

/// How far around itself a ravager clears leaves.
///
/// Vanilla parity: the `inflate(0.2)` of the block-breaking loop.
const LEAF_BREAK_REACH: f64 = 0.2;

/// How far a roar reaches.
///
/// Vanilla parity: the `inflate(4.0)` of `roar`.
const ROAR_RADIUS: f64 = 4.0;

/// Damage a roar deals.
///
/// Vanilla parity: the `hurtServer(.., 6.0F)` of `roar`.
const ROAR_DAMAGE: f32 = 6.0;

/// Horizontal strength of a roar's shove.
///
/// Vanilla parity: the `/ dd * 4.0` of `strongKnockback`.
const ROAR_KNOCKBACK: f64 = 4.0;

/// Upward strength of a roar's shove.
const ROAR_KNOCKBACK_UP: f64 = 0.2;

/// Floor on the squared horizontal distance used to normalize a shove.
///
/// Vanilla parity: the `Math.max(dd, 0.001)` of `strongKnockback`, which keeps
/// an entity standing exactly on the ravager from being launched to infinity.
const KNOCKBACK_DISTANCE_FLOOR: f64 = 0.001;

/// Chance a blocked hit staggers the ravager rather than shoving the blocker.
///
/// Vanilla parity: the `nextDouble() < 0.5` of `blockedByItem`.
const STUN_CHANCE: f64 = 0.5;

/// How far a ravager can turn its head.
///
/// Vanilla parity: `Ravager.getMaxHeadYRot`, which is half a normal mob's and
/// is why a ravager has to turn its whole body to look at you.
const MAX_HEAD_Y_ROT: f32 = 45.0;

/// How much narrower a ravager's reach is than its body.
///
/// Vanilla parity: the `deflate(0.05, 0.0, 0.05)` of `getAttackBoundingBox`.
const ATTACK_BOX_DEFLATION: f64 = 0.05;

/// Speed a ravager closes on its target at.
const MELEE_SPEED_MODIFIER: f64 = 1.0;

/// Speed a ravager wanders at.
const STROLL_SPEED_MODIFIER: f64 = 0.4;

/// Distance at which a ravager watches a player.
const LOOK_AT_PLAYER_RANGE: f64 = 6.0;

/// Distance at which a ravager watches another mob.
const LOOK_AT_MOB_RANGE: f64 = 8.0;

/// How often a ravager bothers to watch something.
const LOOK_AT_PROBABILITY: f32 = 0.02;

/// Speed a follower patrols at.
const PATROL_SPEED: f64 = 0.7;

/// Speed the captain patrols at.
const PATROL_LEADER_SPEED: f64 = 0.595;

/// Experience a ravager drops.
///
/// Vanilla parity: the `xpReward = 20` of the `Ravager` constructor.
const XP_REWARD: i32 = 20;

/// The three timers that drive a ravager.
#[derive(Debug, Default, Clone, Copy)]
struct RavagerTimers {
    /// Ticks left of the current swing.
    attack: i32,
    /// Ticks left of the current stagger.
    stunned: i32,
    /// Ticks left of the roar wind-up.
    roar: i32,
}
/// Speed a raider walks the streets of the village it is raiding at.
///
/// Vanilla parity: the `1.05F` of `new RaiderMoveThroughVillageGoal(this, 1.05F, 1)`.
const VILLAGE_WALK_SPEED_MODIFIER: f64 = 1.05;

/// How close to a house counts as having reached it.
///
/// Vanilla parity: the `1` of the same goal.
const VILLAGE_POI_ARRIVAL_DISTANCE: f64 = 1.0;

/// A ravager.
#[entity_behavior(class = "Ravager")]
pub struct RavagerEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<RavagerEntityData>,
    patrol_state: PatrolState,
    raider_state: RaiderState,
    timers: SyncMutex<RavagerTimers>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `RavagerEntity`.
unsafe impl DowncastType for RavagerEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/ravager");
}

impl RavagerEntity {
    /// Creates a ravager at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a ravager from saved base data.
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
        let mut entity_data = RavagerEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla parity: the goal order of `Ravager.registerGoals`, over
            // the ones `PatrollingMonster` and `Raider` add.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(0, FloatGoal::new(&mob_base));
            goals.add_goal(
                4,
                LongDistancePatrolGoal::new(PATROL_SPEED, PATROL_LEADER_SPEED),
            );
            goals.add_goal(4, MeleeAttackGoal::new(MELEE_SPEED_MODIFIER, true));
            goals.add_goal(1, ObtainRaidLeaderBannerGoal::new());
            goals.add_goal(3, PathfindToRaidGoal::new());
            goals.add_goal(
                4,
                RaiderMoveThroughVillageGoal::new(
                    VILLAGE_WALK_SPEED_MODIFIER,
                    VILLAGE_POI_ARRIVAL_DISTANCE,
                ),
            );
            goals.add_goal(5, RaiderCelebrationGoal::new());
            goals.add_goal(5, WaterAvoidingRandomStrollGoal::new(STROLL_SPEED_MODIFIER));
            goals.add_goal(
                6,
                LookAtPlayerGoal::new_with_probability(LOOK_AT_PLAYER_RANGE, LOOK_AT_PROBABILITY),
            );
            goals.add_goal(
                10,
                LookAtPlayerGoal::new_for_living_entities(
                    LOOK_AT_MOB_RANGE,
                    LOOK_AT_PROBABILITY,
                    |_, target, _| target.as_mob().is_some(),
                ),
            );
            // Vanilla also adds three raid-only goals. Foton has no raid.
        }

        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(
                2,
                HurtByTargetGoal::new()
                    .with_ignored_damage_filter(|entity| entity.as_raider().is_some())
                    .set_alert_others([]),
            );
            targets.add_goal(
                3,
                NearestAttackableTargetGoal::new_for_players(true, |_, _, _| true),
            );
            targets.add_goal(
                4,
                NearestAttackableTargetGoal::new(true, |_, target, _| {
                    target.entity_type() == &vanilla_entities::IRON_GOLEM
                }),
            );
            // Vanilla also hunts adult villagers at priority 4; Foton has none.
        }

        let ravager = Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            patrol_state: PatrolState::new(),
            raider_state: RaiderState::new(),
            timers: SyncMutex::new(RavagerTimers::default()),
        };
        ravager.set_xp_reward(XP_REWARD);
        // Vanilla parity: the `setPathfindingMalus(LEAVES, 0.0F)` of the
        // constructor, which is what lets a ravager path straight through a
        // tree rather than around it.
        ravager.set_pathfinding_malus(PathType::Leaves, 0.0);
        ravager
    }

    /// Returns ticks left of the current swing.
    #[must_use]
    pub fn attack_tick(&self) -> i32 {
        self.timers.lock().attack
    }

    /// Returns ticks left of the current stagger.
    #[must_use]
    pub fn stunned_tick(&self) -> i32 {
        self.timers.lock().stunned
    }

    /// Returns ticks left of the roar wind-up.
    #[must_use]
    pub fn roar_tick(&self) -> i32 {
        self.timers.lock().roar
    }

    /// Throws `entity` away from the ravager.
    ///
    /// Vanilla parity: `Ravager.strongKnockback`.
    fn strong_knockback(&self, entity: &dyn Entity) {
        let position = self.position();
        let target = entity.position();
        let xd = target.x - position.x;
        let zd = target.z - position.z;
        let distance_sqr = xd.mul_add(xd, zd * zd).max(KNOCKBACK_DISTANCE_FLOOR);
        entity.push_impulse(DVec3::new(
            xd / distance_sqr * ROAR_KNOCKBACK,
            ROAR_KNOCKBACK_UP,
            zd / distance_sqr * ROAR_KNOCKBACK,
        ));
    }

    /// Hurts and throws everything nearby.
    ///
    /// Vanilla parity: `Ravager.roar`. Illagers are shoved but never hurt,
    /// which is what lets a ravager roar in the middle of its own raid.
    fn roar(&self) {
        if !Entity::is_alive(self) {
            return;
        }
        let Some(world) = self.level() else {
            return;
        };

        let griefing = world.get_game_rule(&MOB_GRIEFING);
        let self_id = self.id();
        let search_box = self.bounding_box().inflate(ROAR_RADIUS);
        let caught = world.get_entities_in_aabb_matching(&search_box, |entity| {
            if entity.id() == self_id || entity.as_living_entity().is_none() {
                return false;
            }
            if !Entity::is_alive(entity) {
                return false;
            }
            // Vanilla parity: without mob griefing, armor stands are spared.
            griefing || entity.entity_type() != &vanilla_entities::ARMOR_STAND
        });

        let source = DamageSource::environment(&vanilla_damage_types::MOB_ATTACK)
            .with_causing_entity(self_id)
            .with_direct_entity(self_id);
        for entity in caught {
            if entity.as_abstract_illager().is_none()
                && let Some(living) = entity.as_living_entity()
            {
                living.hurt(&world, &source, ROAR_DAMAGE);
            }
            if entity.as_player().is_none() {
                self.strong_knockback(entity.as_ref());
            }
        }

        world.game_event(
            &vanilla_game_events::ENTITY_ACTION,
            self.block_position(),
            &GameEventContext::new(Some(self.as_entity_event_source()), None),
        );
        self.broadcast_entity_event(EntityStatus::RavagerRoared);
    }

    /// Chews through the leaves a charging ravager runs into, or jumps.
    ///
    /// Vanilla parity: the block-breaking half of `Ravager.aiStep`.
    fn break_leaves_or_jump(&self, world: &Arc<World>) {
        if !self.horizontal_collision() || !world.get_game_rule(&MOB_GRIEFING) {
            return;
        }

        let bounds = self.bounding_box().inflate(LEAF_BREAK_REACH);
        let mut destroyed_block = false;
        for x in bounds.min_x().floor() as i32..=bounds.max_x().floor() as i32 {
            for y in bounds.min_y().floor() as i32..=bounds.max_y().floor() as i32 {
                for z in bounds.min_z().floor() as i32..=bounds.max_z().floor() as i32 {
                    let pos = BlockPos::new(x, y, z);
                    let state = world.get_block_state(pos);
                    if !is_leaves(state) {
                        continue;
                    }
                    destroyed_block =
                        world.destroy_block_by_entity(pos, true, self) || destroyed_block;
                }
            }
        }

        if !destroyed_block && self.on_ground() {
            self.jump_from_ground();
        }
    }

    /// Runs the speed, block-breaking and timer half of the ravager's tick.
    ///
    /// Vanilla parity: `Ravager.aiStep`.
    fn ravager_ai_step(&self) {
        if !Entity::is_alive(self) {
            return;
        }

        let target_speed = if self.is_immobile() {
            0.0
        } else {
            let wanted = if self.target().is_some() {
                ATTACK_MOVEMENT_SPEED
            } else {
                BASE_MOVEMENT_SPEED
            };
            let current = self
                .attributes()
                .lock()
                .get_base_value(vanilla_attributes::MOVEMENT_SPEED)
                .unwrap_or(BASE_MOVEMENT_SPEED);
            SPEED_LERP.mul_add(wanted - current, current)
        };
        self.attributes()
            .lock()
            .set_base_value(vanilla_attributes::MOVEMENT_SPEED, target_speed);

        if let Some(world) = self.level() {
            self.break_leaves_or_jump(&world);
        }

        let (should_roar, stun_ended) = {
            let mut timers = self.timers.lock();
            let mut should_roar = false;
            if timers.roar > 0 {
                timers.roar -= 1;
                should_roar = timers.roar == ROAR_TRIGGER_TICK;
            }
            if timers.attack > 0 {
                timers.attack -= 1;
            }
            let mut stun_ended = false;
            if timers.stunned > 0 {
                timers.stunned -= 1;
                if timers.stunned == 0 {
                    timers.roar = ROAR_DURATION;
                    stun_ended = true;
                }
            }
            (should_roar, stun_ended)
        };

        if should_roar {
            self.roar();
        }
        if stun_ended {
            self.play_sound(&sound_events::ENTITY_RAVAGER_ROAR, 1.0, 1.0);
        }
    }
}

/// Returns whether `state` is one of the leaf blocks.
///
/// Vanilla tests `block instanceof LeavesBlock`; Foton reads the block tag,
/// which holds exactly the leaf blocks.
fn is_leaves(state: BlockStateId) -> bool {
    REGISTRY
        .blocks
        .is_in_tag(state.get_block(), &BlockTag::LEAVES)
}

impl Entity for RavagerEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(&sound_events::ENTITY_RAVAGER_STEP, 0.15, 1.0);
    }

    /// Vanilla parity: `Mob.getControllingPassenger`, which the ravager
    /// inherits. A ravager is the one hostile a mob rides into battle, so it
    /// has to report its rider for `updateControlFlags` to have anything to
    /// decide about.
    fn controlling_passenger(&self) -> Option<SharedEntity> {
        Mob::controlling_passenger_mob(self)
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        write_patrol_state(self, nbt);
        write_raider_state(self, nbt);
        let timers = *self.timers.lock();
        nbt.insert(TAG_ATTACK_TICK, timers.attack);
        nbt.insert(TAG_STUN_TICK, timers.stunned);
        nbt.insert(TAG_ROAR_TICK, timers.roar);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        read_patrol_state(self, nbt);
        read_raider_state(self, nbt);
        let mut timers = self.timers.lock();
        timers.attack = nbt.int(TAG_ATTACK_TICK).unwrap_or(0);
        timers.stunned = nbt.int(TAG_STUN_TICK).unwrap_or(0);
        timers.roar = nbt.int(TAG_ROAR_TICK).unwrap_or(0);
    }
}

impl LivingEntity for RavagerEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: `Mob.serverAiStep`, which is where a mob's goals run.
    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
        self.ravager_ai_step();
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

    /// Staggers or shoves when a raised shield eats one of its hits.
    ///
    /// Vanilla parity: `Ravager.blockedByItem`. Half the time the charge stops
    /// dead for two seconds -- the mob's one weakness -- and the other half it
    /// hurls the defender clear.
    fn blocked_by_item(&self, defender: &dyn LivingEntity) {
        if self.roar_tick() != 0 {
            return;
        }

        if rand::random::<f64>() < STUN_CHANCE {
            self.timers.lock().stunned = STUN_DURATION;
            self.play_sound(&sound_events::ENTITY_RAVAGER_STUNNED, 1.0, 1.0);
            self.broadcast_entity_event(EntityStatus::RavagerStunned);
            defender.push_entity(self);
        } else {
            self.strong_knockback(defender);
        }

        defender.mark_hurt();
    }

    /// Vanilla parity: `Ravager.isImmobile`. A ravager mid-swing, staggered or
    /// winding up a roar cannot move, which is the whole of the opening.
    fn is_immobile(&self) -> bool {
        let timers = *self.timers.lock();
        self.default_is_immobile() || timers.attack > 0 || timers.stunned > 0 || timers.roar > 0
    }

    /// Vanilla parity: `Ravager.hasLineOfSight`. A staggered ravager sees
    /// nothing, so it stops tracking and its goals let go.
    fn has_line_of_sight(&self, target: &dyn Entity) -> bool {
        let timers = *self.timers.lock();
        if timers.stunned > 0 || timers.roar > 0 {
            return false;
        }
        // The body of `LivingEntity::has_line_of_sight`, which has no separate
        // base method to call.
        self.has_line_of_sight_with(
            target,
            ClipBlockShape::Collider,
            ClipFluid::None,
            target.get_eye_y(),
        )
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_RAVAGER_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_RAVAGER_DEATH)
    }
}

impl Mob for RavagerEntity {
    /// Vanilla parity: `Ravager` derives from `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    /// Returns whether this mob accepts where the spawner put it.
    ///
    /// Vanilla parity: the `Monster::checkMonsterSpawnRules` `SpawnPlacements`
    /// registers for the ravager.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_monster_spawn_rules(world, spawn_reason, pos)
    }

    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        finalize_spawn_raider(self, spawn_reason);
        self.finalize_spawn_mob_base(world, spawn_reason, group_data)
    }

    /// Vanilla parity: `Ravager.updateControlFlags`. A ravager ridden by a
    /// raider keeps steering itself -- that is how a raid's ravager carries a
    /// pillager into a village without stopping dead -- and only a non-raider
    /// mob rider takes the controls away.
    fn update_control_flags(&self) {
        let no_controller = self
            .controlling_passenger()
            .is_none_or(|passenger| !passenger.is_mob() || passenger.as_raider().is_some());
        let not_in_boat = self
            .vehicle()
            .is_none_or(|vehicle| !vehicle.entity_type().is_abstract_boat);

        let mut selector = self.mob_base().goal_selector().lock();
        selector.set_control(GoalControl::Move, no_controller);
        selector.set_control(GoalControl::Jump, no_controller && not_in_boat);
        selector.set_control(GoalControl::Look, no_controller);
        selector.set_control(GoalControl::Target, no_controller);
    }

    fn max_head_y_rot(&self) -> f32 {
        MAX_HEAD_Y_ROT
    }

    /// Vanilla parity: `Ravager.getAttackBoundingBox`, a touch narrower than
    /// the body so a ravager cannot hit round a corner it is wedged against.
    fn attack_bounding_box(&self, horizontal_expansion: f64) -> WorldAabb {
        self.mob_attack_bounding_box(horizontal_expansion)
            .inflate_xyz(-ATTACK_BOX_DEFLATION, 0.0, -ATTACK_BOX_DEFLATION)
    }

    /// Vanilla parity: `Ravager.doHurtTarget`, which starts the swing timer
    /// before the hit so the ravager freezes for the animation.
    fn do_hurt_target(&self, world: &World, target: &SharedEntity) -> bool {
        self.timers.lock().attack = ATTACK_DURATION;
        self.broadcast_entity_event(EntityStatus::StartAttacking);
        self.play_sound(&sound_events::ENTITY_RAVAGER_ATTACK, 1.0, 1.0);
        self.mob_do_hurt_target(world, target)
    }

    fn remove_when_far_away(&self, dist_sqr: f64) -> bool {
        self.remove_when_far_away_raider(dist_sqr)
    }

    fn requires_custom_persistence(&self) -> bool {
        self.requires_custom_persistence_raider() || self.is_passenger() || self.is_leashed()
    }

    /// Vanilla parity: `Raider.updateNoActionTime`.
    fn update_no_action_time(&self) {
        self.increment_no_action_time();
        self.increment_no_action_time();
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_RAVAGER_AMBIENT)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for RavagerEntity {}

impl PatrollingMonster for RavagerEntity {
    fn patrol_state(&self) -> &PatrolState {
        &self.patrol_state
    }

    /// Vanilla parity: `Ravager.canBeLeader`. A ravager never carries the
    /// banner, which is why a patrol's captain is always an illager.
    fn can_be_leader(&self) -> bool {
        false
    }

    fn can_join_patrol(&self) -> bool {
        self.can_join_patrol_raider()
    }
}

impl Raider for RavagerEntity {
    fn raider_state(&self) -> &RaiderState {
        &self.raider_state
    }

    /// Vanilla parity: `Ravager.applyRaidBuffs`, which is empty.
    fn apply_raid_buffs(&self, _wave: i32, _is_captain: bool) {}

    fn celebrate_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_RAVAGER_CELEBRATE
    }

    fn is_celebrating(&self) -> bool {
        *self.entity_data.lock().raider().is_celebrating.get()
    }

    fn set_celebrating(&self, celebrating: bool) {
        self.entity_data
            .lock()
            .raider_mut()
            .is_celebrating
            .set(celebrating);
    }
}

impl Enemy for RavagerEntity {}

#[cfg(test)]
mod tests;
