//! Pufferfish entity.
//!
//! Vanilla parity: `Pufferfish`, `AbstractFish` and `WaterAnimal`. A pufferfish
//! is a cod with a threat display: anything scary within two blocks inflates
//! it over two stages, and while it is inflated it stings whatever touches it
//! for damage and poison that both scale with how puffed up it is. Deflating
//! is on a timer of its own, which is why it stays a ball for a while after
//! the swimmer has gone.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::{CGameEvent, GameEventType, SoundSource};
use steel_registry::entity_type::{EntityDimensions, EntityTypeRef};
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::PufferfishEntityData;
use steel_registry::vanilla_entity_type_tags::EntityTypeTag;
use steel_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_damage_types, vanilla_mob_effects,
};
use steel_utils::locks::SyncMutex;
use steel_utils::types::GameType;
use steel_utils::{BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey};

use crate::entity::ai::goal::{AvoidEntityGoal, Goal, GoalControls, PanicGoal, RandomSwimmingGoal};
use crate::entity::ai::path::PathType;
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::damage::DamageSource;
use crate::entity::living_base::MobEffectInstance;
use crate::entity::mob::NavigationKind;
use crate::entity::spawn_rules::check_surface_water_animal_spawn_rules;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntityPose, EntitySpawnReason, EntitySyncedData,
    LivingEntity, LivingEntityBase, Mob, MobBase, PathfinderMob,
};
use crate::physics::MoveResult;
use crate::player::Player;
use crate::world::World;

use super::fish;

/// Vanilla `Pufferfish.STATE_SMALL`.
const STATE_SMALL: i32 = 0;
/// Vanilla `Pufferfish.STATE_MID`.
const STATE_MID: i32 = 1;
/// Vanilla `Pufferfish.STATE_FULL`.
const STATE_FULL: i32 = 2;

/// Ticks of inflating before the second stage.
///
/// Vanilla parity: the `inflateCounter > 40` of `Pufferfish.tick`.
const MID_TO_FULL_TICKS: i32 = 40;
/// Ticks of calm before the fish drops back to half puffed.
///
/// Vanilla parity: the `deflateTimer > 60` of the same method.
const FULL_TO_MID_TICKS: i32 = 60;
/// Ticks of calm before it is flat again.
const MID_TO_SMALL_TICKS: i32 = 100;

/// How far a pufferfish notices something worth puffing up at.
///
/// Vanilla parity: the `inflate(2.0)` of `PufferfishPuffGoal.canUse`.
const PUFF_SEARCH_RANGE: f64 = 2.0;
/// How far past its own box a puffed fish stings.
///
/// Vanilla parity: the `inflate(0.3)` of `Pufferfish.aiStep`.
const STING_REACH: f64 = 0.3;
/// Poison ticks a sting inflicts per puff stage.
///
/// Vanilla parity: the `60 * puffState` of `Pufferfish.touch`.
const POISON_TICKS_PER_STATE: i32 = 60;

/// A pufferfish.
#[entity_behavior(class = "Pufferfish")]
pub struct PufferfishEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    inflate_counter: SyncMutex<i32>,
    deflate_timer: SyncMutex<i32>,
    entity_data: SyncMutex<PufferfishEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `PufferfishEntity`.
unsafe impl DowncastType for PufferfishEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/pufferfish");
}

impl PufferfishEntity {
    /// Creates a pufferfish at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a pufferfish from saved base data.
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
        // Vanilla parity: `WaterAnimal` clears the water malus.
        mob_base
            .pathfinding_malus()
            .lock()
            .set(PathType::Water, 0.0);
        let mut entity_data = PufferfishEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla parity: `AbstractFish.registerGoals` plus the puff goal.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(0, PanicGoal::new(fish::PANIC_SPEED_MODIFIER));
            goals.add_goal(1, PufferfishPuffGoal::new());
            goals.add_goal(
                2,
                AvoidEntityGoal::with_selector(
                    fish::AVOID_PLAYER_RANGE,
                    fish::AVOID_WALK_SPEED,
                    fish::AVOID_SPRINT_SPEED,
                    |_, target, _| fish::is_player_to_flee(target),
                ),
            );
            goals.add_goal(
                4,
                RandomSwimmingGoal::new(fish::SWIM_SPEED_MODIFIER, fish::SWIM_INTERVAL_TICKS),
            );
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            inflate_counter: SyncMutex::new(0),
            deflate_timer: SyncMutex::new(0),
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Returns vanilla `Pufferfish.getPuffState`.
    #[must_use]
    pub fn puff_state(&self) -> i32 {
        *self.entity_data.lock().puff_state.get()
    }

    /// Sets vanilla `Pufferfish.setPuffState`.
    ///
    /// Vanilla refreshes the hitbox from `onSyncedDataUpdated`; Steel has no
    /// such hook, so the refresh happens here, which is the only place the
    /// value changes.
    pub fn set_puff_state(&self, puff_state: i32) {
        self.entity_data.lock().puff_state.set(puff_state);
        self.refresh_dimensions();
    }

    /// Returns whether this pufferfish came out of a bucket.
    ///
    /// Vanilla parity: `AbstractFish.fromBucket`.
    #[must_use]
    #[expect(
        clippy::wrong_self_convention,
        reason = "the name mirrors the vanilla accessor"
    )]
    pub fn from_bucket(&self) -> bool {
        *self.entity_data.lock().abstract_fish.from_bucket.get()
    }

    /// Marks this pufferfish as having come out of a bucket.
    pub fn set_from_bucket(&self, from_bucket: bool) {
        self.entity_data
            .lock()
            .abstract_fish_mut()
            .from_bucket
            .set(from_bucket);
    }

    /// Vanilla parity: `Pufferfish.getScale`.
    #[must_use]
    const fn puff_scale(puff_state: i32) -> f32 {
        match puff_state {
            STATE_SMALL => 0.5,
            STATE_MID => 0.7,
            _ => 1.0,
        }
    }

    /// Returns whether this entity is one a pufferfish inflates at.
    ///
    /// Vanilla parity: the `SCARY_MOB` selector, which spares creative players
    /// and everything in `not_scary_for_pufferfish` -- the other pufferfish,
    /// the guardians and the axolotl among them.
    fn is_scary(target: &dyn LivingEntity) -> bool {
        if target
            .as_player()
            .is_some_and(|player| player.game_mode() == GameType::Creative)
        {
            return false;
        }

        !REGISTRY.entity_types.is_in_tag(
            target.entity_type(),
            &EntityTypeTag::NOT_SCARY_FOR_PUFFERFISH,
        )
    }

    /// Vanilla parity: the `TARGETING_CONDITIONS` of `Pufferfish`.
    fn targeting_conditions() -> TargetingConditions {
        TargetingConditions::for_non_combat()
            .ignore_invisibility_testing()
            .ignore_line_of_sight()
            .selector(|_, target, _| Self::is_scary(target))
    }

    /// Vanilla parity: `Pufferfish.tick`, the inflate and deflate clocks.
    fn tick_puff_state(&self) {
        if !Entity::is_alive(self) || !self.is_effective_ai() {
            return;
        }

        let inflate_counter = *self.inflate_counter.lock();
        if inflate_counter > 0 {
            let puff_state = self.puff_state();
            if puff_state == STATE_SMALL {
                self.make_sound(Some(&sound_events::ENTITY_PUFFER_FISH_BLOW_UP));
                self.set_puff_state(STATE_MID);
            } else if inflate_counter > MID_TO_FULL_TICKS && puff_state == STATE_MID {
                self.make_sound(Some(&sound_events::ENTITY_PUFFER_FISH_BLOW_UP));
                self.set_puff_state(STATE_FULL);
            }

            *self.inflate_counter.lock() += 1;
            return;
        }

        if self.puff_state() == STATE_SMALL {
            return;
        }

        let deflate_timer = *self.deflate_timer.lock();
        if deflate_timer > FULL_TO_MID_TICKS && self.puff_state() == STATE_FULL {
            self.make_sound(Some(&sound_events::ENTITY_PUFFER_FISH_BLOW_OUT));
            self.set_puff_state(STATE_MID);
        } else if deflate_timer > MID_TO_SMALL_TICKS && self.puff_state() == STATE_MID {
            self.make_sound(Some(&sound_events::ENTITY_PUFFER_FISH_BLOW_OUT));
            self.set_puff_state(STATE_SMALL);
        }

        *self.deflate_timer.lock() += 1;
    }

    /// Vanilla parity: the sting sweep of `Pufferfish.aiStep`.
    fn sting_nearby_mobs(&self) {
        let puff_state = self.puff_state();
        if puff_state <= STATE_SMALL || !Entity::is_alive(self) {
            return;
        }
        let Some(world) = self.level() else {
            return;
        };

        let targeting = Self::targeting_conditions();
        let search_box = self.bounding_box().inflate(STING_REACH);
        for entity in world.get_entities_in_aabb_matching(&search_box, |entity| {
            entity.is_mob()
                && entity
                    .as_living_entity()
                    .is_some_and(|target| targeting.test(world.as_ref(), Some(self), target))
        }) {
            if !entity.is_alive() {
                continue;
            }
            let Some(target) = entity.as_living_entity() else {
                continue;
            };
            self.touch(&world, target, puff_state);
        }
    }

    /// Vanilla parity: `Pufferfish.touch`.
    fn touch(&self, world: &Arc<World>, target: &dyn LivingEntity, puff_state: i32) {
        let damage_source = DamageSource::environment(&vanilla_damage_types::MOB_ATTACK)
            .with_causing_entity(self.id())
            .with_direct_entity(self.id())
            .with_source_position(self.position());
        if !target.hurt_server(world, &damage_source, (1 + puff_state) as f32) {
            return;
        }

        target.add_mob_effect(MobEffectInstance::with_duration(
            vanilla_mob_effects::POISON,
            POISON_TICKS_PER_STATE * puff_state,
            0,
        ));
        self.play_sound(&sound_events::ENTITY_PUFFER_FISH_STING, 1.0, 1.0);
    }
}

impl Entity for PufferfishEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `Pufferfish.tick` runs the puff clocks before the shared
    /// tick, and `WaterAnimal.baseTick` reads the air left before it is spent.
    fn tick(&self) {
        self.tick_puff_state();
        self.default_tick();
    }

    fn base_tick(&self) {
        let air_before_tick = self.air_supply();
        Mob::base_tick_mob(self);
        if let Some(world) = self.level() {
            fish::handle_air_supply(self, &world, air_before_tick);
        }
    }

    /// Vanilla parity: `Pufferfish.getDefaultDimensions`, which scales with how
    /// puffed up the fish is.
    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self) * Self::puff_scale(self.puff_state());
        if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    /// Vanilla parity: `AbstractFish.playStepSound` is empty.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {}

    /// Vanilla parity: `Pufferfish.playerTouch`, which is the sting a swimmer
    /// gets for brushing past, complete with the client-side screen event.
    fn player_touch(self: Arc<Self>, player: &Arc<Player>) {
        let puff_state = self.puff_state();
        if puff_state <= STATE_SMALL {
            return;
        }
        let Some(world) = self.level() else {
            return;
        };

        let damage_source = DamageSource::environment(&vanilla_damage_types::MOB_ATTACK)
            .with_causing_entity(self.id())
            .with_direct_entity(self.id())
            .with_source_position(self.position());
        if !player.hurt_server(&world, &damage_source, (1 + puff_state) as f32) {
            return;
        }

        if !self.is_silent() {
            player.send_packet(CGameEvent {
                event: GameEventType::PufferFishSting,
                data: 0.0,
            });
        }

        player.add_mob_effect(MobEffectInstance::with_duration(
            vanilla_mob_effects::POISON,
            POISON_TICKS_PER_STATE * puff_state,
            0,
        ));
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        nbt.insert("FromBucket", self.from_bucket());
        nbt.insert("PuffState", self.puff_state());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.set_from_bucket(nbt.byte("FromBucket").is_some_and(|flag| flag != 0));
        self.set_puff_state(nbt.int("PuffState").unwrap_or(STATE_SMALL).min(STATE_FULL));
    }
}

impl LivingEntity for PufferfishEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
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
        Some(&sound_events::ENTITY_PUFFER_FISH_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_PUFFER_FISH_DEATH)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        fish::flop(self, &sound_events::ENTITY_PUFFER_FISH_FLOP);
        let result = self.default_ai_step();
        self.sting_nearby_mobs();
        result
    }

    /// Vanilla parity: `AbstractFish.travelInWater`.
    fn travel_in_water(
        &self,
        input: DVec3,
        _base_gravity: f64,
        _is_falling: bool,
        _old_y: f64,
    ) -> Option<MoveResult> {
        fish::travel_in_water(self, input)
    }
}

impl Mob for PufferfishEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        let _ = spawn_reason;
        check_surface_water_animal_spawn_rules(world, pos)
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn ambient_sound_interval(&self) -> i32 {
        fish::AMBIENT_SOUND_INTERVAL
    }

    /// Vanilla parity: `AbstractFish.removeWhenFarAway`.
    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        !self.from_bucket()
    }

    /// Vanilla parity: `AbstractFish.FishMoveControl.tick`.
    fn tick_move_control(&self) {
        fish::tick_move_control(self);
    }
}

impl PathfinderMob for PufferfishEntity {
    /// Vanilla parity: `AbstractFish.createNavigation`.
    fn navigation_kind(&self) -> NavigationKind {
        NavigationKind::WaterBound {
            allow_breaching: false,
        }
    }
}

/// Inflates the fish while something frightening is beside it.
///
/// Vanilla parity: `Pufferfish.PufferfishPuffGoal`. The goal only starts and
/// stops the inflate counter; the two-stage clock itself is in `tick`.
struct PufferfishPuffGoal;

impl PufferfishPuffGoal {
    const fn new() -> Self {
        Self
    }
}

impl Goal for PufferfishPuffGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(world) = mob.level() else {
            return false;
        };

        let targeting = PufferfishEntity::targeting_conditions();
        let search_box = mob.bounding_box().inflate(PUFF_SEARCH_RANGE);
        !world
            .get_entities_in_aabb_matching(&search_box, |entity| {
                entity
                    .as_living_entity()
                    .is_some_and(|target| targeting.test(world.as_ref(), Some(mob), target))
            })
            .is_empty()
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let Some(pufferfish) = mob.downcast_ref::<PufferfishEntity>() else {
            return;
        };
        *pufferfish.inflate_counter.lock() = 1;
        *pufferfish.deflate_timer.lock() = 0;
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        if let Some(pufferfish) = mob.downcast_ref::<PufferfishEntity>() {
            *pufferfish.inflate_counter.lock() = 0;
        }
    }
}

#[cfg(test)]
mod tests;
