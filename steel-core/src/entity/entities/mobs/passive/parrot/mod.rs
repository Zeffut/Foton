//! Parrot entity.
//!
//! Vanilla parity: `Parrot` and `ShoulderRidingEntity`. The only tameable mob
//! that flies, the only one that rides a shoulder, and the only one that tells
//! you what is hiding in the dark by imitating it.
//!
//! **Gap**: `Parrot.doPush` stops a parrot shoving players around. Steel
//! resolves pushing on the pushed entity rather than the pusher, so there is no
//! per-pusher hook to override yet.

mod imitation;
mod wander;

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_entity_data::ParrotEntityData;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    REGISTRY, TaggedRegistryExt, sound_events, vanilla_damage_types, vanilla_mob_effects,
};
use steel_utils::locks::SyncMutex;
use steel_utils::types::{Difficulty, InteractionHand};
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use uuid::Uuid;

use crate::behavior::InteractionResult;
use crate::entity::ai::goal::{
    FloatGoal, FollowMobGoal, FollowOwnerGoal, LandOnOwnersShoulderGoal, LookAtPlayerGoal,
    SitWhenOrderedToGoal, TamableAnimalPanicGoal, WaterAvoidingRandomFlyingGoal,
};
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::{
    AgeableMob, AgeableMobBase, AgeableMobGroupData, Animal, AnimalBase, Entity, EntityBase,
    EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity, LivingEntityBase,
    LivingEntitySyncedData, Mob, MobBase, MobEffectInstance, MoveControlKind, NavigationKind,
    PathfinderMob, RemovalReason, SpawnGroupData, TamableAnimal, TamableAnimalBase,
};
use crate::physics::MoveResult;
use crate::player::Player;
use crate::world::World;

use imitation::{imitated_sound, is_imitable};
use wander::parrot_wander_position;

/// How long after landing before the parrot may sit on a shoulder again.
///
/// Vanilla parity: `ShoulderRidingEntity.RIDE_COOLDOWN`.
const RIDE_COOLDOWN_TICKS: i32 = 100;

/// One chance in this many ticks that a parrot imitates something nearby.
///
/// Vanilla parity: the `random.nextInt(400) == 0` of `Parrot.aiStep`.
const IMITATE_CHANCE: i32 = 400;

/// One chance in this many that an imitation attempt goes ahead at all.
///
/// Vanilla parity: the `random.nextInt(2) == 0` of `Parrot.imitateNearbyMobs`.
const IMITATE_COIN_FLIP: i32 = 2;

/// How far a parrot listens for something to imitate.
///
/// Vanilla parity: the `inflate(20.0)` of `Parrot.imitateNearbyMobs`.
const IMITATE_RANGE: f64 = 20.0;

/// One chance in this many that an idle parrot picks a mob sound to chirp.
///
/// Vanilla parity: the `random.nextInt(1000) == 0` of `Parrot.getAmbient`.
const AMBIENT_IMITATION_CHANCE: i32 = 1000;

/// One chance in this many that seeds tame the parrot.
///
/// Vanilla parity: the `random.nextInt(10) == 0` of `Parrot.mobInteract`.
const TAME_CHANCE: i32 = 10;

/// How long a cookie poisons a parrot for, in ticks.
///
/// Vanilla parity: the `new MobEffectInstance(MobEffects.POISON, 900)` of
/// `Parrot.mobInteract`. The parrot dies immediately after anyway; the effect
/// is what the client sees for the instant before it does.
const COOKIE_POISON_TICKS: i32 = 900;

/// How fast a falling parrot's descent is damped.
///
/// Vanilla parity: the `multiply(1.0, 0.6, 1.0)` of `Parrot.calculateFlapping`,
/// which is what makes a parrot glide down rather than drop.
const FALL_DAMPING: f64 = 0.6;

/// The five colors a parrot comes in.
///
/// Vanilla parity: `Parrot.Variant`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParrotVariant {
    /// The default red parrot.
    #[default]
    RedBlue,
    /// The blue parrot.
    Blue,
    /// The green parrot.
    Green,
    /// The yellow parrot.
    YellowBlue,
    /// The grey parrot.
    Gray,
}

impl ParrotVariant {
    /// Every variant, in vanilla's id order.
    pub const VALUES: [Self; 5] = [
        Self::RedBlue,
        Self::Blue,
        Self::Green,
        Self::YellowBlue,
        Self::Gray,
    ];

    /// Returns the synchronized id.
    #[must_use]
    pub const fn id(self) -> i32 {
        match self {
            Self::RedBlue => 0,
            Self::Blue => 1,
            Self::Green => 2,
            Self::YellowBlue => 3,
            Self::Gray => 4,
        }
    }

    /// Returns the variant for a synchronized id.
    ///
    /// Vanilla parity: `Parrot.Variant.byId`, which clamps rather than wraps.
    #[must_use]
    pub const fn by_id(id: i32) -> Self {
        match id {
            ..=0 => Self::RedBlue,
            1 => Self::Blue,
            2 => Self::Green,
            3 => Self::YellowBlue,
            _ => Self::Gray,
        }
    }
}

/// Vanilla parrot entity.
#[entity_behavior(class = "Parrot")]
pub struct ParrotEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    tamable_base: TamableAnimalBase,
    /// Ticks since this parrot last left a shoulder.
    ///
    /// Vanilla parity: `ShoulderRidingEntity.rideCooldownCounter`.
    ride_cooldown_counter: SyncMutex<i32>,
    entity_data: SyncMutex<ParrotEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `ParrotEntity`.
unsafe impl DowncastType for ParrotEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/parrot");
}

impl ParrotEntity {
    /// Creates a new parrot entity.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a parrot entity from saved base data.
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
        let mut entity_data = ParrotEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        let parrot = Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base: AgeableMobBase::new(),
            animal_base: AnimalBase::new(),
            tamable_base: TamableAnimalBase::new(),
            ride_cooldown_counter: SyncMutex::new(0),
            entity_data: SyncMutex::new(entity_data),
        };

        // Vanilla parity: the three pathfinding maluses of the constructor.
        // `Animal` sets the first two; the parrot adds cocoa on top.
        parrot.set_pathfinding_malus(PathType::FireInNeighbor, -1.0);
        parrot.set_pathfinding_malus(PathType::Fire, -1.0);
        parrot.set_pathfinding_malus(PathType::Cocoa, -1.0);
        parrot.mob_base.navigation().lock().set_can_float(true);
        parrot.register_goals();
        parrot
    }

    /// Vanilla parity: `Parrot.registerGoals`.
    fn register_goals(&self) {
        let mut goals = self.mob_base.goal_selector().lock();
        goals.add_goal(0, TamableAnimalPanicGoal::new(1.25));
        goals.add_goal(0, FloatGoal::new(&self.mob_base));
        goals.add_goal(1, LookAtPlayerGoal::new(8.0));
        goals.add_goal(2, SitWhenOrderedToGoal::new());
        goals.add_goal(2, FollowOwnerGoal::new(1.0, 5.0, 1.0));
        goals.add_goal(
            2,
            WaterAvoidingRandomFlyingGoal::new(1.0).with_position(parrot_wander_position),
        );
        goals.add_goal(3, LandOnOwnersShoulderGoal::new());
        // Vanilla parity: the default `FollowMobGoal` predicate, which
        // follows any mob that is not of the follower's own kind.
        goals.add_goal(
            3,
            FollowMobGoal::new(1.0, 3.0, 7.0, |follower, candidate| {
                follower.entity_type() != candidate.entity_type()
            }),
        );
    }

    /// Returns the current parrot variant.
    #[must_use]
    pub fn variant(&self) -> ParrotVariant {
        ParrotVariant::by_id(*self.entity_data.lock().variant.get())
    }

    /// Sets the current parrot variant.
    pub fn set_variant(&self, variant: ParrotVariant) {
        self.entity_data.lock().variant.set(variant.id());
    }

    /// Returns vanilla `ShoulderRidingEntity.canSitOnShoulder`.
    #[must_use]
    pub fn can_sit_on_shoulder(&self) -> bool {
        *self.ride_cooldown_counter.lock() > RIDE_COOLDOWN_TICKS
    }

    /// Hands this parrot to a player's shoulder.
    ///
    /// Vanilla parity: `ShoulderRidingEntity.setEntityOnShoulder`, which saves
    /// the entity into the player and discards the live one.
    pub fn set_entity_on_shoulder(&self, player: &Player) -> bool {
        let Some(world) = self.level() else {
            return false;
        };
        let Some(shared) = world.get_entity_by_id(self.id()) else {
            return false;
        };
        if !player.set_entity_on_shoulder(&shared) {
            return false;
        }

        self.set_removed(RemovalReason::Discarded);
        true
    }

    /// Returns vanilla `Parrot.isFlying`.
    #[must_use]
    pub fn is_flying(&self) -> bool {
        !self.on_ground()
    }

    /// Returns whether the stack is vanilla parrot food.
    ///
    /// Vanilla parity: the `ItemTags.PARROT_FOOD` of `Parrot.mobInteract`. Note
    /// that `Parrot.isFood` returns false: seeds tame a parrot but never breed
    /// it, so the `Animal` food path must not see them.
    #[must_use]
    pub fn is_taming_food(item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::PARROT_FOOD)
    }

    /// Returns vanilla `Parrot.getPitch`.
    #[must_use]
    pub fn parrot_pitch() -> f32 {
        (rand::random::<f32>() - rand::random::<f32>()).mul_add(0.2, 1.0)
    }

    /// Plays the sound an idle parrot makes.
    ///
    /// Vanilla parity: `Parrot.getAmbient`, which is also what a shoulder-riding
    /// parrot uses through the player.
    #[must_use]
    pub fn ambient_sound_for(world: &Arc<World>) -> SoundEventRef {
        if world.difficulty() != Difficulty::Peaceful
            && rand::random_range(0..AMBIENT_IMITATION_CHANCE) == 0
        {
            let index = rand::random_range(0..imitation::MOB_SOUND_MAP.len());
            if let Some((_, sound)) = imitation::MOB_SOUND_MAP.get(index) {
                return sound;
            }
        }

        &sound_events::ENTITY_PARROT_AMBIENT
    }

    /// Makes something nearby's noise, if anything nearby is worth imitating.
    ///
    /// Vanilla parity: `Parrot.imitateNearbyMobs`.
    pub fn imitate_nearby_mobs(entity: &dyn Entity) -> bool {
        let Some(world) = entity.level() else {
            return false;
        };
        if !entity.is_alive() || entity.is_silent() || rand::random_range(0..IMITATE_COIN_FLIP) != 0
        {
            return false;
        }

        let search = entity.bounding_box().inflate(IMITATE_RANGE);
        let candidates = world.get_entities_in_aabb_matching(&search, |candidate| {
            candidate.is_mob() && is_imitable(candidate.entity_type())
        });
        if candidates.is_empty() {
            return false;
        }

        let index = rand::random_range(0..candidates.len());
        let Some(mob) = candidates.get(index) else {
            return false;
        };
        if mob.is_silent() {
            return false;
        }
        let Some(sound) = imitated_sound(mob.entity_type()) else {
            return false;
        };

        let position = entity.position();
        world.play_sound(
            sound,
            entity.sound_source(),
            BlockPos::containing(position.x, position.y, position.z),
            0.7,
            Self::parrot_pitch(),
            None,
        );
        true
    }

    /// Speaks for a parrot riding a player's shoulder.
    ///
    /// Vanilla parity: the body of `ServerPlayer.playShoulderEntityAmbientSound`
    /// once it has decided the rider is a parrot.
    pub fn imitate_nearby_mobs_or_chirp(player: &Player) {
        if Self::imitate_nearby_mobs(player) {
            return;
        }
        let Some(world) = player.level() else {
            return;
        };

        let position = player.position();
        world.play_sound(
            Self::ambient_sound_for(&world),
            player.sound_source(),
            BlockPos::containing(position.x, position.y, position.z),
            1.0,
            Self::parrot_pitch(),
            None,
        );
    }

    /// Vanilla parity: the descent damping of `Parrot.calculateFlapping`.
    fn damp_descent(&self) {
        let movement = self.velocity();
        if self.on_ground() || movement.y >= 0.0 {
            return;
        }
        self.set_velocity(DVec3::new(
            movement.x,
            movement.y * FALL_DAMPING,
            movement.z,
        ));
    }

    /// Vanilla parity: the poisonous-food branch of `Parrot.mobInteract`.
    fn eat_cookie(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        Mob::use_player_item(self, player, hand);
        self.add_mob_effect(MobEffectInstance::new(
            vanilla_mob_effects::POISON,
            COOKIE_POISON_TICKS,
        ));

        if let Some(world) = self.level() {
            let source = DamageSource::environment(&vanilla_damage_types::PLAYER_ATTACK)
                .with_causing_entity(player.id())
                .with_direct_entity(player.id())
                .with_source_position(player.position());
            self.hurt(&world, &source, f32::MAX);
        }

        InteractionResult::Success
    }

    fn update_dirty_mob_effect_entity_data(&self) {
        if !self.living_base.take_effects_dirty() {
            return;
        }

        let display = self.living_base.mob_effect_display_state();

        {
            let mut entity_data = self.entity_data.lock();
            let living = entity_data.living_entity_mut();
            living.effect_particles.set(display.particles);
            living.effect_ambience.set(display.ambient);
        }

        self.entity_data.set_base_invisible_flag(display.invisible);
        self.entity_data
            .set_base_glowing_flag(self.has_glowing_tag() || display.glowing);
    }
}

/// Returns whether a parrot may appear at `pos`.
///
/// Vanilla parity: `Parrot.checkParrotSpawnRules`.
#[must_use]
fn check_parrot_spawn_rules(world: &Arc<World>, pos: BlockPos) -> bool {
    world
        .get_block_state(pos.below())
        .get_block()
        .has_tag(&BlockTag::PARROTS_SPAWNABLE_ON)
        && <ParrotEntity as Animal>::is_bright_enough_to_spawn(world.as_ref(), pos)
}

impl Entity for ParrotEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn tick(&self) {
        // Vanilla parity: the `rideCooldownCounter++` of
        // `ShoulderRidingEntity.tick`, which runs before the living tick.
        *self.ride_cooldown_counter.lock() += 1;
        LivingEntity::tick_living_entity(self);
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn update_data_before_sync(&self) {
        self.update_dirty_mob_effect_entity_data();
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(&sound_events::ENTITY_PARROT_STEP, 0.15, 1.0);
    }

    /// Vanilla parity: `Parrot.checkFallDamage`, which is empty: a parrot
    /// never lands hard.
    fn check_fall_damage(
        &self,
        _vertical_movement: f64,
        _on_ground: bool,
        _on_state: BlockStateId,
        _pos: BlockPos,
        _world: &Arc<World>,
    ) {
    }

    fn is_pushable(&self) -> bool {
        true
    }

    fn is_allied_to(&self, other: &dyn Entity) -> bool {
        self.considers_entity_as_ally_tamable(other)
    }

    fn is_tame_owned_by(&self, owner: &dyn LivingEntity) -> bool {
        self.is_tame() && self.is_owned_by(owner.as_entity_event_source())
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        self.save_tamable_animal(nbt);
        nbt.insert("Variant", self.variant().id());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        self.load_tamable_animal(nbt);
        self.set_variant(ParrotVariant::by_id(nbt.int("Variant").unwrap_or(0)));
    }
}

impl LivingEntity for ParrotEntity {
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
        Some(&sound_events::ENTITY_PARROT_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_PARROT_DEATH)
    }

    fn voice_pitch(&self) -> f32 {
        Self::parrot_pitch()
    }

    /// Vanilla parity: `Parrot.omnidirectionalAirMover`, which is what keeps a
    /// parrot from sinking as fast as it drifts.
    fn air_travel_vertical_friction(&self, air_drag: f32) -> f32 {
        air_drag
    }

    /// Vanilla parity: the `Parrot.hurtServer` override.
    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        if self.is_invulnerable_to(world, source) {
            return false;
        }

        self.set_ordered_to_sit(false);
        self.living_hurt_server(world, source, amount)
    }

    fn die(&self, source: &DamageSource) {
        self.notify_owner_of_death(source);
        self.living_die(source);
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    /// Vanilla parity: `Parrot.aiStep`.
    fn ai_step(&self) -> Option<MoveResult> {
        if rand::random_range(0..IMITATE_CHANCE) == 0 {
            Self::imitate_nearby_mobs(self);
        }

        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);
        self.damp_descent();
        // VANILLA CLIENT-LOCAL: the rest of `calculateFlapping` and the
        // jukebox party state only drive the wing animation.
        result
    }
}

impl AgeableMob for ParrotEntity {
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

    /// Vanilla parity: `Parrot.canBeABaby`, which is false.
    fn get_baby_start_age(&self) -> i32 {
        0
    }
}

impl Animal for ParrotEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    /// Vanilla parity: `Parrot.isFood`, which is false: a parrot never breeds.
    fn is_food(&self, _item_stack: &ItemStack) -> bool {
        false
    }

    /// Vanilla parity: `Parrot.canMate`, which is false.
    fn can_mate(&self, _partner: &dyn Animal) -> bool {
        false
    }
}

impl TamableAnimal for ParrotEntity {
    fn tamable_base(&self) -> &TamableAnimalBase {
        &self.tamable_base
    }

    fn tamable_flags(&self) -> i8 {
        *self.entity_data.lock().tamable_animal().flags.get()
    }

    fn set_tamable_flags(&self, flags: i8) {
        self.entity_data
            .lock()
            .tamable_animal_mut()
            .flags
            .set(flags);
    }

    fn owner_uuid(&self) -> Option<Uuid> {
        *self.entity_data.lock().tamable_animal().owneruuid.get()
    }

    fn set_owner_uuid(&self, owner: Option<Uuid>) {
        self.entity_data
            .lock()
            .tamable_animal_mut()
            .owneruuid
            .set(owner);
    }

    /// Vanilla parity: `Parrot.canFlyToOwner`, which is true and is why a
    /// parrot may teleport onto a leaf block its owner is standing under.
    fn can_fly_to_owner(&self) -> bool {
        true
    }
}

impl Mob for ParrotEntity {
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

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn custom_server_ai_step(&self) {
        Animal::custom_server_ai_step_animal(self);
    }

    /// Vanilla parity: the `FlyingMoveControl` the constructor installs, with a
    /// ten-degree pitch limit and no hovering.
    fn move_control_kind(&self) -> MoveControlKind {
        MoveControlKind::Flying {
            max_turn: 10.0,
            hovers_in_place: false,
        }
    }

    fn can_attack(&self, target: &dyn LivingEntity) -> bool {
        self.can_attack_tamable(target)
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        self.level().map(|world| Self::ambient_sound_for(&world))
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        _spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_parrot_spawn_rules(world, pos)
    }

    /// Vanilla parity: `Parrot.finalizeSpawn`.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let index = rand::random_range(0..ParrotVariant::VALUES.len());
        if let Some(variant) = ParrotVariant::VALUES.get(index) {
            self.set_variant(*variant);
        }

        let group_data = group_data.unwrap_or(SpawnGroupData::AgeableMob(
            AgeableMobGroupData::with_should_spawn_baby(false),
        ));
        self.finalize_spawn_ageable_mob(world, spawn_reason, Some(group_data))
    }

    /// Vanilla parity: `Parrot.mobInteract`.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let item_stack = {
            let inventory = player.inventory.lock();
            let item_stack = inventory.get_item_in_hand(hand);
            item_stack.copy_with_count(item_stack.count())
        };

        if !self.is_tame() && Self::is_taming_food(&item_stack) {
            Mob::use_player_item(self, player, hand);
            if !self.is_silent() {
                self.play_sound(&sound_events::ENTITY_PARROT_EAT, 1.0, Self::parrot_pitch());
            }

            if rand::random_range(0..TAME_CHANCE) == 0 {
                self.tame(player);
                self.spawn_taming_particles(true);
            } else {
                self.spawn_taming_particles(false);
            }
            return InteractionResult::Success;
        }

        if REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::PARROT_POISONOUS_FOOD)
        {
            return self.eat_cookie(player, hand);
        }

        if !self.is_flying() && self.is_tame() && self.is_owned_by(player) {
            self.set_ordered_to_sit(!self.is_ordered_to_sit());
            return InteractionResult::Success;
        }

        Animal::mob_interact_animal(self, player, hand)
    }
}

impl PathfinderMob for ParrotEntity {
    /// Vanilla parity: `Parrot.createNavigation`, a `FlyingPathNavigation`.
    fn navigation_kind(&self) -> NavigationKind {
        NavigationKind::Flying
    }
}

#[cfg(test)]
mod tests;
