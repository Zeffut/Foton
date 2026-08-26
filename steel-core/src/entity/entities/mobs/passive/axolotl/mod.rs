//! Axolotl entity.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.axolotl.Axolotl`. An
//! axolotl is a brain-driven `Animal` that lives in water, hunts fish and
//! drowned, drowns itself if it is kept out of the water too long, and -- the
//! part nothing else in the game does -- plays dead when it is badly hurt,
//! healing while it floats. A player who helps it finish a kill gets
//! regeneration and loses mining fatigue.

mod axolotl_ai;

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::data_components::vanilla_components;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::entity_variant::AxolotlVariant;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_entity_data::AxolotlEntityData;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_damage_types, vanilla_items,
    vanilla_mob_effects,
};
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, DowncastType, DowncastTypeKey};

use crate::behavior::InteractionResult;
use crate::entity::ai::brain::Brain;
use crate::entity::ai::brain::behavior::TOTAL_PLAYDEAD_TIME;
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::control::{SmoothSwimmingLookControl, SmoothSwimmingMoveControl};
use crate::entity::ai::path::PathType;
use crate::entity::bucketable::{
    Bucketable, bucket_mob_pickup, load_default_data_from_bucket_tag, read_bucket_entity_data,
    save_default_data_to_bucket_tag, set_bucket_entity_data,
};
use crate::entity::damage::DamageSource;
use crate::entity::mob::NavigationKind;
use crate::entity::{
    AgeableMob, AgeableMobBase, Animal, AnimalBase, AxolotlGroupData, Entity, EntityBase,
    EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity, LivingEntityBase,
    LivingEntitySyncedData, Mob, MobBase, MobEffectInstance, MoveResult, PathfinderMob,
    SharedEntity, SpawnGroupData,
};
use crate::physics::MoverType;
use crate::player::Player;
use crate::world::{LevelReader as _, World};

/// How far a player may be from a kill and still be thanked for it.
///
/// Vanilla parity: `Axolotl.PLAYER_REGEN_DETECTION_RANGE`.
const PLAYER_REGEN_DETECTION_RANGE: f64 = 20.0;
/// Vanilla parity: `Axolotl.RARE_VARIANT_CHANCE`, a one-in-1200 blue.
const RARE_VARIANT_CHANCE: i32 = 1200;
/// Vanilla parity: `Axolotl.AXOLOTL_TOTAL_AIR_SUPPLY`, five minutes of air.
const TOTAL_AIR_SUPPLY: i32 = 6000;
/// Vanilla parity: `Axolotl.REHYDRATE_AIR_SUPPLY`, what a bucket gives back.
const REHYDRATE_AIR_SUPPLY: i32 = 1800;
/// Vanilla parity: `Axolotl.REGEN_BUFF_MAX_DURATION`.
const REGEN_BUFF_MAX_DURATION: i32 = 2400;
/// Vanilla parity: `Axolotl.REGEN_BUFF_BASE_DURATION`.
const REGEN_BUFF_BASE_DURATION: i32 = 100;
/// Vanilla parity: the `hurtServer(..., dryOut(), 2.0F)` of `handleAirSupply`.
const DRY_OUT_DAMAGE: f32 = 2.0;
/// Vanilla parity: the age a cluster's third and later axolotls are born at.
const BABY_AGE: i32 = -24_000;
/// Vanilla parity: `Axolotl.getMaxHeadXRot` and `getMaxHeadYRot`, both `1`.
const MAX_HEAD_ROT: f32 = 1.0;
/// Vanilla parity: the `scale(0.9)` of `Axolotl.travelInWater`.
const SWIM_DRAG: f64 = 0.9;

/// Vanilla parity: the `AxolotlMoveControl(this)`, which is a
/// `SmoothSwimmingMoveControl(this, 85, 10, 0.1F, 0.5F, false)`.
const SWIM_MOVE_CONTROL: SmoothSwimmingMoveControl =
    SmoothSwimmingMoveControl::new(85, 10, 0.1, 0.5, false);
/// Vanilla parity: the `AxolotlLookControl(this, 20)`.
const SWIM_LOOK_CONTROL: SmoothSwimmingLookControl = SmoothSwimmingLookControl::new(20);

/// An axolotl.
#[entity_behavior(class = "Axolotl")]
pub struct AxolotlEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    brain: Brain,
    entity_data: SyncMutex<AxolotlEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `AxolotlEntity`.
unsafe impl DowncastType for AxolotlEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/axolotl");
}

impl AxolotlEntity {
    /// Creates an axolotl at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates an axolotl from saved base data.
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
        // Vanilla parity: the `setPathfindingMalus(PathType.WATER, 0.0F)` of the
        // `Axolotl` constructor -- open water is free rather than merely
        // allowed, which is what `Animal` had made it.
        mob_base
            .pathfinding_malus()
            .lock()
            .set(PathType::Water, 0.0);
        let mut entity_data = AxolotlEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            brain: axolotl_ai::make_brain(),
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Returns vanilla `Axolotl.getVariant`.
    #[must_use]
    pub fn variant(&self) -> AxolotlVariant {
        AxolotlVariant::by_id(*self.entity_data.lock().variant.get())
    }

    /// Sets vanilla `Axolotl.setVariant`.
    pub fn set_variant(&self, variant: AxolotlVariant) {
        self.entity_data.lock().variant.set(variant.id());
    }

    /// Returns vanilla `Axolotl.isPlayingDead`.
    #[must_use]
    pub fn is_playing_dead(&self) -> bool {
        *self.entity_data.lock().playing_dead.get()
    }

    /// Sets vanilla `Axolotl.setPlayingDead`.
    pub fn set_playing_dead(&self, playing_dead: bool) {
        self.entity_data.lock().playing_dead.set(playing_dead);
    }

    /// Vanilla parity: `Axolotl.rehydrate`, which is what an axolotl bucket
    /// gives an axolotl that has been out of the water.
    pub fn rehydrate(&self) {
        let air_supply = self.air_supply() + REHYDRATE_AIR_SUPPLY;
        self.set_air_supply(air_supply.min(self.max_air_supply()));
    }

    /// Vanilla parity: `Axolotl.handleAirSupply`.
    ///
    /// An axolotl out of the water spends its five minutes of air and then
    /// takes a heart a tick, which is what makes carrying one overland a race.
    fn handle_air_supply(&self, world: &Arc<World>, pre_tick_air_supply: i32) {
        if !Entity::is_alive(self) || self.is_in_water_or_rain() {
            self.set_air_supply(self.max_air_supply());
            return;
        }

        self.set_air_supply(pre_tick_air_supply - 1);
        if self.should_take_drowning_damage() {
            self.set_air_supply(0);
            self.hurt_server(
                world,
                &DamageSource::environment(&vanilla_damage_types::DRY_OUT),
                DRY_OUT_DAMAGE,
            );
        }
    }

    /// Vanilla parity: `Axolotl.useRareVariant`.
    fn use_rare_variant() -> bool {
        rand::random_range(0..RARE_VARIANT_CHANCE) == 0
    }

    /// Picks one of the four common colors, or the blue one.
    ///
    /// Vanilla parity: `Axolotl.Variant.getSpawnVariant`.
    fn spawn_variant(common: bool) -> AxolotlVariant {
        let valid: Vec<AxolotlVariant> = AxolotlVariant::VALUES
            .into_iter()
            .filter(|variant| variant.is_common() == common)
            .collect();
        valid[rand::random_range(0..valid.len())]
    }

    /// Thanks a player who finished off what this axolotl was fighting.
    ///
    /// Vanilla parity: `Axolotl.onStopAttacking`, the static the fight
    /// activity's `StopAttackingIfTargetInvalid` is built with. The buff lands
    /// only when the target actually died and only on the player who dealt the
    /// killing blow, and only if that player is still nearby.
    pub fn on_stop_attacking(body: &dyn PathfinderMob, target: &SharedEntity) {
        let Some(target_living) = target.as_living_entity() else {
            return;
        };
        if !target_living.is_dead_or_dying() {
            return;
        }
        let Some(world) = body.level() else {
            return;
        };
        let Some(killer_id) = target_living
            .last_damage_source()
            .and_then(|source| source.causing_entity_id)
        else {
            return;
        };

        let area = body.bounding_box().inflate(PLAYER_REGEN_DETECTION_RANGE);
        let in_range = world
            .get_entities_in_aabb_matching(&area, |entity| {
                entity.id() == killer_id && entity.as_player().is_some()
            })
            .into_iter()
            .next();
        let Some(nearby) = in_range else {
            return;
        };
        let Some(player) = nearby.as_player() else {
            return;
        };

        Self::apply_supporting_effects(body, player);
    }

    /// Vanilla parity: `Axolotl.applySupportingEffects`.
    ///
    /// The regeneration stacks in duration rather than in strength, and it
    /// stops stacking at two minutes -- vanilla's `endsWithin(2399)`, which is
    /// `duration <= 2399`, is what caps a player who keeps a shoal of axolotls
    /// at the same buff a player with one gets.
    fn apply_supporting_effects(body: &dyn PathfinderMob, player: &Player) {
        let existing = player.mob_effect(vanilla_mob_effects::REGENERATION);
        let should_apply = existing.as_ref().is_none_or(|effect| {
            !effect.is_infinite_duration() && effect.duration() < REGEN_BUFF_MAX_DURATION
        });
        if should_apply {
            let previous_duration = existing.map_or(0, |effect| effect.duration());
            let duration =
                REGEN_BUFF_MAX_DURATION.min(REGEN_BUFF_BASE_DURATION + previous_duration);
            player.add_mob_effect(MobEffectInstance::with_duration(
                vanilla_mob_effects::REGENERATION,
                duration,
                0,
            ));
        }
        let _ = body;

        player.remove_mob_effect(vanilla_mob_effects::MINING_FATIGUE);
    }

    /// Returns whether the stack is vanilla axolotl food.
    ///
    /// Vanilla parity: `Axolotl.isFood`, which is the `#minecraft:axolotl_food`
    /// tag -- a bucket of tropical fish and nothing else.
    #[must_use]
    pub fn is_axolotl_food(item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::AXOLOTL_FOOD)
    }

    /// Vanilla parity: `Axolotl.checkAxolotlSpawnRules`, which asks only for
    /// clay underfoot -- no light check, unlike every land animal.
    #[must_use]
    pub fn check_axolotl_spawn_rules(world: &Arc<World>, pos: BlockPos) -> bool {
        world
            .get_block_state(pos.below())
            .get_block()
            .has_tag(&BlockTag::AXOLOTLS_SPAWNABLE_ON)
    }
}

impl Entity for AxolotlEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `Axolotl.baseTick`, which reads the air left before the
    /// shared tick spends it and then runs its own drying-out clock.
    fn base_tick(&self) {
        let air_before_tick = self.air_supply();
        Mob::base_tick_mob(self);
        if self.is_no_ai() {
            return;
        }
        if let Some(world) = self.level() {
            self.handle_air_supply(&world, air_before_tick);
        }
    }

    /// Vanilla parity: `Axolotl.getMaxAirSupply`.
    fn max_air_supply(&self) -> i32 {
        TOTAL_AIR_SUPPLY
    }

    /// Vanilla parity: `Axolotl.getSwimSound`.
    fn swim_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_AXOLOTL_SWIM
    }

    /// Vanilla parity: `Axolotl.isPushedByFluid`; an axolotl holds its line in
    /// a current.
    fn is_pushed_by_fluid(&self) -> bool {
        false
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        nbt.insert("Variant", self.variant().id());
        nbt.insert("FromBucket", self.from_bucket());
        self.brain.save(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        self.set_variant(AxolotlVariant::by_id(nbt.int("Variant").unwrap_or(0)));
        self.set_from_bucket(nbt.byte("FromBucket").is_some_and(|flag| flag != 0));
        self.brain.load(nbt);
    }
}

impl LivingEntity for AxolotlEntity {
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
        Some(&sound_events::ENTITY_AXOLOTL_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_AXOLOTL_DEATH)
    }

    /// Vanilla parity: `Axolotl.canBeSeenAsEnemy`. Playing dead is not a
    /// disguise for the player's benefit: a drowned genuinely loses interest.
    ///
    /// Rust has no `super`, so the shared `!isInvulnerable() &&
    /// canBeSeenByAnyone()` of `LivingEntity.canBeSeenAsEnemy` is spelled out.
    fn can_be_seen_as_enemy(&self) -> bool {
        !self.is_playing_dead() && !self.is_invulnerable() && self.can_be_seen_by_anyone()
    }

    /// Vanilla parity: `Axolotl.hurtServer`, the roll that starts the act.
    ///
    /// It needs all of: a one-in-three roll, either a second roll against the
    /// damage or half health already gone, a blow it will survive, water around
    /// it, a real attacker, and not already playing dead.
    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        let current_health = self.get_health();
        if !self.is_no_ai()
            && rand::random_range(0..3) == 0
            && (rand::random_range(0..3) < amount as i32
                || current_health / self.get_max_health() < 0.5)
            && amount < current_health
            && self.is_in_water()
            && (source.causing_entity_id.is_some() || source.direct_entity_id.is_some())
            && !self.is_playing_dead()
        {
            self.brain
                .set_memory(memory_module_types::PLAY_DEAD_TICKS, TOTAL_PLAYDEAD_TIME);
        }

        self.living_hurt_server(world, source, amount)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);
        result
    }

    /// Vanilla parity: `Axolotl.travelInWater`, which swims rather than sinking.
    fn travel_in_water(
        &self,
        input: DVec3,
        _base_gravity: f64,
        _is_falling: bool,
        _old_y: f64,
    ) -> Option<MoveResult> {
        self.move_relative(self.get_speed(), input);
        let result = self.move_entity(MoverType::SelfMovement, self.velocity());
        self.set_velocity(self.velocity() * SWIM_DRAG);
        result
    }
}

impl AgeableMob for AxolotlEntity {
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

impl Animal for AxolotlEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        Self::is_axolotl_food(item_stack)
    }

    /// Vanilla parity: `Axolotl.getBreedOffspring`, where a calf takes one
    /// parent's color at random -- unless the rare roll lands, and then it is
    /// blue whatever its parents were.
    fn initialize_breed_offspring(&self, partner: &dyn Animal, offspring: &dyn Animal) {
        use steel_utils::Downcast as _;

        let Some(calf) = offspring.downcast_ref::<Self>() else {
            return;
        };

        let variant = if Self::use_rare_variant() {
            Self::spawn_variant(false)
        } else if rand::random::<bool>() {
            self.variant()
        } else {
            partner
                .downcast_ref::<Self>()
                .map_or_else(|| self.variant(), Self::variant)
        };

        calf.set_variant(variant);
        calf.set_persistence_required();
    }
}

impl Mob for AxolotlEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn brain(&self) -> Option<&Brain> {
        Some(&self.brain)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    /// Vanilla parity: `Axolotl.getTarget`, which is `getTargetFromBrain` --
    /// an axolotl keeps no target field of its own.
    fn target(&self) -> Option<SharedEntity> {
        self.target_from_brain()
    }

    /// Vanilla parity: `Axolotl.customServerAiStep`, which ticks the brain,
    /// updates the activity, and then mirrors the play-dead clock into the
    /// synced flag the client animates from.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.brain.tick(&world, self);
        axolotl_ai::update_activity(&self.brain);
        if !self.is_no_ai() {
            let playing_dead = self
                .brain
                .get_memory(memory_module_types::PLAY_DEAD_TICKS)
                .is_some_and(|ticks| ticks > 0);
            self.set_playing_dead(playing_dead);
        }
        Animal::custom_server_ai_step_animal(self);
    }

    /// Vanilla parity: `Axolotl.AxolotlMoveControl.tick`, which is the smooth
    /// swimming control with a hard stop while the axolotl is playing dead.
    fn tick_move_control(&self) {
        if self.is_playing_dead() {
            return;
        }
        SWIM_MOVE_CONTROL.tick(self);
    }

    /// Vanilla parity: `Axolotl.AxolotlLookControl.tick`, stopped the same way.
    fn tick_look_control(&self) {
        if self.is_playing_dead() {
            return;
        }
        SWIM_LOOK_CONTROL.tick(self);
    }

    /// Vanilla parity: `Axolotl.getMaxHeadXRot`.
    fn max_head_x_rot(&self) -> f32 {
        MAX_HEAD_ROT
    }

    /// Vanilla parity: `Axolotl.getMaxHeadYRot`.
    fn max_head_y_rot(&self) -> f32 {
        MAX_HEAD_ROT
    }

    /// Vanilla parity: `Axolotl.getAmbientSound`.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(if self.is_in_water() {
            &sound_events::ENTITY_AXOLOTL_IDLE_WATER
        } else {
            &sound_events::ENTITY_AXOLOTL_IDLE_AIR
        })
    }

    /// Vanilla parity: `Axolotl.playAmbientSound`, which is silent while the
    /// axolotl is pretending to be dead.
    fn play_ambient_sound(&self) {
        if self.is_playing_dead() {
            return;
        }
        self.make_sound(self.ambient_sound());
    }

    /// Vanilla parity: `Axolotl.playAttackSound`.
    fn play_attack_sound(&self) {
        self.play_sound(&sound_events::ENTITY_AXOLOTL_ATTACK, 1.0, 1.0);
    }

    /// Vanilla parity: `Axolotl.canBeLeashed`, which is an unconditional yes.
    fn can_be_leashed(&self) -> bool {
        true
    }

    /// Vanilla parity: `Axolotl.requiresCustomPersistence`; one that came out
    /// of a bucket is somebody's, so it never despawns.
    fn requires_custom_persistence(&self) -> bool {
        self.is_passenger() || self.is_leashed() || self.from_bucket()
    }

    /// Vanilla parity: `Axolotl.removeWhenFarAway`.
    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        !self.from_bucket() && self.custom_name().is_none()
    }

    /// Vanilla parity: `Axolotl.mobInteract`, which tries the bucket first and
    /// falls through to the shared animal interaction.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        bucket_mob_pickup(player, hand, self)
            .unwrap_or_else(|| Animal::mob_interact_animal(self, player, hand))
    }

    /// Vanilla parity: `Axolotl.usePlayerItem`, which hands back the water
    /// bucket a bucket of tropical fish leaves behind rather than swallowing it.
    fn use_player_item(&self, player: &Player, hand: InteractionHand) {
        let is_fish_bucket = {
            let inventory = player.inventory.lock();
            inventory
                .get_item_in_hand(hand)
                .is(&vanilla_items::TROPICAL_FISH_BUCKET)
        };
        if !is_fish_bucket {
            player.inventory.lock().shrink_item_in_hand(hand, 1);
            return;
        }

        let overflow = {
            let mut inventory = player.inventory.lock();
            inventory.apply_filled_result(
                hand,
                ItemStack::new(&vanilla_items::WATER_BUCKET),
                player.has_infinite_materials(),
                true,
            )
        };
        if !overflow.is_empty() {
            let _ = player.drop_item(overflow, false, false);
        }
    }

    /// MISSING FOUNDATION: vanilla's `Axolotl.checkSpawnObstruction` narrows
    /// the shared one to `isUnobstructed` alone, dropping the "no liquid in the
    /// bounding box" half -- otherwise an axolotl could never spawn, because it
    /// only ever spawns in water. Steel has no `checkSpawnObstruction` hook on
    /// `Mob` at all, so nothing applies that half either and the narrowing has
    /// nothing to narrow. The same gap is already recorded on the guardian and
    /// the ocelot.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        let _ = spawn_reason;
        Self::check_axolotl_spawn_rules(world, pos)
    }

    /// Vanilla parity: `Axolotl.finalizeSpawn`.
    ///
    /// A cluster shares two colors and every axolotl after the second in it is
    /// born a calf. One out of a bucket returns before any of that, which is
    /// what stops a bucketed axolotl's color being rerolled when it is let out.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        if spawn_reason == EntitySpawnReason::Bucket {
            return group_data;
        }

        let (cluster, is_baby) = match group_data {
            Some(SpawnGroupData::Axolotl(cluster)) => (cluster, cluster.group_size() >= 2),
            _ => (
                AxolotlGroupData::new([Self::spawn_variant(true), Self::spawn_variant(true)]),
                false,
            ),
        };

        self.set_variant(cluster.variant(|len| rand::random_range(0..len)));
        if is_baby {
            self.set_age(BABY_AGE);
        }

        self.finalize_spawn_ageable_mob(world, spawn_reason, Some(SpawnGroupData::Axolotl(cluster)))
    }
}

impl PathfinderMob for AxolotlEntity {
    /// Navigates as a swimmer while in water and as a walker on land.
    ///
    /// Vanilla parity: `Axolotl.createNavigation` returns an
    /// `AmphibiousPathNavigation`. Steel answers this per path request, the
    /// same seam the frog, the turtle and the drowned use.
    fn navigation_kind(&self) -> NavigationKind {
        if self.is_in_water() {
            NavigationKind::WaterBound {
                allow_breaching: false,
            }
        } else {
            NavigationKind::Ground
        }
    }

    /// Vanilla parity: `Axolotl.getWalkTargetValue`, a flat zero -- no block is
    /// more attractive to an axolotl than any other.
    fn get_walk_target_value(&self, _pos: BlockPos) -> f32 {
        0.0
    }
}

impl Bucketable for AxolotlEntity {
    fn from_bucket(&self) -> bool {
        *self.entity_data.lock().from_bucket.get()
    }

    fn set_from_bucket(&self, from_bucket: bool) {
        self.entity_data.lock().from_bucket.set(from_bucket);
    }

    /// Vanilla parity: `Axolotl.saveToBucketTag`.
    ///
    /// The color rides in the item's own `minecraft:axolotl_variant`
    /// component -- that is what makes two buckets of different axolotls
    /// refuse to stack -- while the age and the hunting cooldown ride in the
    /// entity data.
    fn save_to_bucket_tag(&self, bucket: &mut ItemStack) {
        save_default_data_to_bucket_tag(self, bucket);
        bucket.set(vanilla_components::AXOLOTL_VARIANT, self.variant());

        let mut tag = NbtCompound::new();
        read_bucket_entity_data(bucket, |saved| {
            for (key, value) in saved.iter() {
                tag.insert(key.to_string(), value.to_owned());
            }
        });
        tag.insert("Age", self.get_age());
        tag.insert("AgeLocked", self.is_age_locked());
        if self
            .brain
            .has_memory_value(memory_module_types::HAS_HUNTING_COOLDOWN.id())
        {
            tag.insert(
                "HuntingCooldown",
                self.brain
                    .time_until_expiry(memory_module_types::HAS_HUNTING_COOLDOWN),
            );
        }
        set_bucket_entity_data(bucket, tag);
    }

    /// Vanilla parity: `Axolotl.loadFromBucketTag`.
    fn load_from_bucket_tag(&self, tag: BorrowedNbtCompoundView<'_, '_>) {
        load_default_data_from_bucket_tag(self, tag);
        self.set_age(tag.int("Age").unwrap_or(0));
        self.set_age_locked(tag.byte("AgeLocked").is_some_and(|flag| flag != 0));
        match tag.long("HuntingCooldown") {
            Some(cooldown) => self.brain.set_memory_with_expiry(
                memory_module_types::HAS_HUNTING_COOLDOWN,
                true,
                cooldown,
            ),
            None => self
                .brain
                .erase_memory(memory_module_types::HAS_HUNTING_COOLDOWN.id()),
        }
    }

    fn bucket_item_stack(&self) -> ItemStack {
        ItemStack::new(&vanilla_items::AXOLOTL_BUCKET)
    }

    fn pickup_sound(&self) -> SoundEventRef {
        &sound_events::ITEM_BUCKET_FILL_AXOLOTL
    }
}

#[cfg(test)]
mod tests;
