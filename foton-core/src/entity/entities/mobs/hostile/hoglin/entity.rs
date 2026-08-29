//! Hoglin entity.
//!
//! Vanilla parity: `net.minecraft.world.entity.monster.hoglin.Hoglin`. The
//! Nether's only breedable animal and one of its nastier hostiles at once: it
//! extends `Animal` and implements `Enemy`, so it falls in love over crimson
//! fungus and gores anything else that comes near.
//!
//! Two things drive it away rather than toward: warped fungus and respawn
//! anchors pacify it, and a crowd of piglins makes it run. Left in the
//! overworld it turns into a [`crate::entity::entities::ZoglinEntity`], which
//! keeps the charge and loses the fear.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::HoglinEntityData;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_attributes, vanilla_blocks,
    vanilla_entities, vanilla_mob_effects,
};
use foton_utils::entity_events::EntityStatus;
use foton_utils::locks::SyncMutex;
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::behavior::InteractionResult;
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::brain::{Activity, Brain};
use crate::entity::conversion::{ConversionParams, convert_to};
use crate::entity::damage::DamageSource;
use crate::entity::entities::ZoglinEntity;
use crate::entity::hoglin_base::{
    self, ATTACK_ANIMATION_DURATION, HoglinBase, PROBABILITY_OF_SPAWNING_AS_BABY,
};
use crate::entity::{
    AgeableMob, AgeableMobBase, Animal, AnimalBase, Enemy, Entity, EntityBase, EntityBaseLoad,
    EntitySpawnReason, EntitySyncedData, LivingEntity, LivingEntityBase, Mob, MobBase,
    MobEffectInstance, MoveResult, PathfinderMob, SharedEntity, SpawnGroupData,
};
use crate::player::Player;
use crate::world::{LevelReader as _, World};

use super::hoglin_ai;
use crate::entity::ai::brain::behavior::utils;
use foton_registry::entity_data::EntityPose;
use foton_registry::entity_type::{EntityAttachmentPoint, EntityAttachments, EntityDimensions};

/// Experience a grown hoglin drops.
///
/// Vanilla parity: the `this.xpReward = 5` of the `Hoglin` constructor.
const XP_REWARD: i32 = 5;

/// Experience a baby hoglin drops.
///
/// Vanilla parity: the `this.xpReward = 3` of `ageBoundaryReached`.
const BABY_XP_REWARD: i32 = 3;

/// Attack damage a grown hoglin has.
///
/// Vanilla parity: `Hoglin.ATTACK_DAMAGE`.
const ATTACK_DAMAGE: f64 = 6.0;

/// Attack damage a baby hoglin has.
///
/// Vanilla parity: `Hoglin.BABY_ATTACK_DAMAGE`.
const BABY_ATTACK_DAMAGE: f64 = 0.5;

/// How long a hoglin has to stand in the overworld before it turns.
///
/// Vanilla parity: `Hoglin.CONVERSION_TIME`.
pub const CONVERSION_TIME: i32 = 300;

/// How long the nausea lasts after the change.
///
/// Vanilla parity: the `new MobEffectInstance(MobEffects.NAUSEA, 200, 0)` of
/// `Hoglin.finishConversion`.
const CONVERSION_NAUSEA_TICKS: i32 = 200;

/// How strongly a hoglin prefers crimson nylium underfoot.
///
/// Vanilla parity: the `10.0F` of `Hoglin.getWalkTargetValue`.
const CRIMSON_NYLIUM_WALK_VALUE: f32 = 10.0;

/// How strongly a hoglin refuses to walk near a repellent.
///
/// Vanilla parity: the `-1.0F` of the same method.
const REPELLENT_WALK_VALUE: f32 = -1.0;

/// Where a baby piglin sits on a baby hoglin.
///
/// Vanilla parity: the `EntityAttachment.PASSENGER` of `Hoglin.BABY_DIMENSIONS`.
const BABY_PASSENGER_ATTACHMENTS: [EntityAttachmentPoint; 1] =
    [EntityAttachmentPoint::new(0.0, 0.875, 0.0)];

/// Vanilla parity: `Hoglin.BABY_DIMENSIONS`.
const BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new_with_attachments(
    0.75,
    0.85,
    0.625,
    EntityAttachments::new(&BABY_PASSENGER_ATTACHMENTS, &[], &[], &[]),
);

/// Fields a hoglin keeps that are neither synced nor on a base.
struct HoglinState {
    /// Vanilla parity: `Hoglin.attackAnimationRemainingTicks`.
    attack_animation_remaining_ticks: i32,
    /// Vanilla parity: `Hoglin.timeInOverworld`.
    time_in_overworld: i32,
    /// Vanilla parity: `Hoglin.cannotBeHunted`.
    cannot_be_hunted: bool,
}

/// A hoglin.
#[entity_behavior(class = "Hoglin")]
pub struct HoglinEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    entity_data: SyncMutex<HoglinEntityData>,
    state: SyncMutex<HoglinState>,
    brain: Brain,
}

// SAFETY: This key is owned by Foton and uniquely identifies `HoglinEntity`.
unsafe impl DowncastType for HoglinEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/hoglin");
}

impl HoglinEntity {
    /// Creates a hoglin at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a hoglin from saved base data.
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
        mob_base.set_xp_reward(XP_REWARD);
        let ageable_base = AgeableMobBase::new();
        let animal_base = AnimalBase::new();
        AnimalBase::initialize_pathfinding_malus(&mob_base);
        let mut entity_data = HoglinEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            entity_data: SyncMutex::new(entity_data),
            state: SyncMutex::new(HoglinState {
                attack_animation_remaining_ticks: 0,
                time_in_overworld: 0,
                cannot_be_hunted: false,
            }),
            brain: hoglin_ai::make_brain(),
        }
    }

    /// Vanilla parity: `Hoglin.isAdult`.
    #[must_use]
    pub fn is_adult(&self) -> bool {
        !AgeableMob::is_baby(self)
    }

    /// Whether a piglin may pick a fight with this hoglin.
    ///
    /// Vanilla parity: `Hoglin.canBeHunted`. A baby is never hunted, and
    /// neither is one saved data marked off-limits -- which is what stops a
    /// bastion's piglins wiping out its stable in one go.
    #[must_use]
    pub fn can_be_hunted(&self) -> bool {
        self.is_adult() && !self.state.lock().cannot_be_hunted
    }

    /// Vanilla parity: the private `Hoglin.setCannotBeHunted`.
    pub fn set_cannot_be_hunted(&self, cannot_be_hunted: bool) {
        self.state.lock().cannot_be_hunted = cannot_be_hunted;
    }

    /// Vanilla parity: `Hoglin.setImmuneToZombification`.
    pub fn set_immune_to_zombification(&self, immune: bool) {
        self.entity_data.lock().immune_to_zombification.set(immune);
    }

    /// Vanilla parity: the private `Hoglin.isImmuneToZombification`.
    #[must_use]
    pub fn is_immune_to_zombification(&self) -> bool {
        *self.entity_data.lock().immune_to_zombification.get()
    }

    /// Whether this hoglin is standing somewhere that turns it into a zoglin.
    ///
    /// Vanilla parity: `Hoglin.isConverting`, which reads the
    /// `EnvironmentAttributes.PIGLINS_ZOMBIFY` attribute. No vanilla biome or
    /// timeline overrides that attribute, so the dimension type's own
    /// `piglins_zombify` flag is the whole answer.
    #[must_use]
    pub fn is_converting(&self) -> bool {
        let Some(world) = self.level() else {
            return false;
        };
        !self.is_immune_to_zombification()
            && !self.is_no_ai()
            && world.dimension_type.piglins_zombify
    }

    /// Sets how long this hoglin has stood in the overworld.
    ///
    /// Vanilla parity: the `@VisibleForTesting Hoglin.setTimeInOverworld`.
    pub fn set_time_in_overworld(&self, time_in_overworld: i32) {
        self.state.lock().time_in_overworld = time_in_overworld;
    }

    /// Returns how long this hoglin has stood in the overworld.
    #[must_use]
    pub fn time_in_overworld(&self) -> i32 {
        self.state.lock().time_in_overworld
    }

    /// Turns this hoglin into a zoglin.
    ///
    /// Vanilla parity: the private `Hoglin.finishConversion`, whose
    /// `ConversionParams.single(this, true, false)` keeps the equipment -- a
    /// hoglin wears none -- and drops `canPickUpLoot`.
    pub fn finish_conversion(&self) -> Option<Arc<ZoglinEntity>> {
        convert_to(
            self,
            ConversionParams::single(true, false),
            |id, position, world| ZoglinEntity::new(&vanilla_entities::ZOGLIN, id, position, world),
            |zoglin| {
                // The age comes across in `copy_common_state`, which runs
                // before this callback.
                zoglin
                    .living_base()
                    .add_mob_effect(MobEffectInstance::with_duration(
                        vanilla_mob_effects::NAUSEA,
                        CONVERSION_NAUSEA_TICKS,
                        0,
                    ));
            },
        )
    }

    /// Vanilla parity: `Hoglin.ageBoundaryReached`.
    fn hoglin_age_boundary_reached(&self, baby: bool) {
        let (xp_reward, attack_damage) = if baby {
            (BABY_XP_REWARD, BABY_ATTACK_DAMAGE)
        } else {
            (XP_REWARD, ATTACK_DAMAGE)
        };
        self.mob_base.set_xp_reward(xp_reward);
        self.attributes()
            .lock()
            .set_base_value(vanilla_attributes::ATTACK_DAMAGE, attack_damage);
    }

    /// Vanilla parity: `Hoglin.getAmbientSound`, whose sound follows the
    /// activity: a retreat call while fleeing or converting, angry while
    /// fighting, and the plain ambient otherwise.
    fn activity_sound(&self) -> SoundEventRef {
        if self.brain.is_active(Activity::Avoid) || self.is_converting() {
            return &sound_events::ENTITY_HOGLIN_RETREAT;
        }
        if self.brain.is_active(Activity::Fight) {
            return &sound_events::ENTITY_HOGLIN_ANGRY;
        }
        if self
            .brain
            .has_memory_value(memory_module_types::NEAREST_REPELLENT.id())
        {
            return &sound_events::ENTITY_HOGLIN_RETREAT;
        }
        &sound_events::ENTITY_HOGLIN_AMBIENT
    }
}

/// Returns whether a hoglin may appear at `pos`.
///
/// Vanilla parity: `Hoglin.checkHoglinSpawnRules`, which keeps bastion floors
/// of nether wart block clear.
#[must_use]
fn check_hoglin_spawn_rules(world: &Arc<World>, pos: BlockPos) -> bool {
    use foton_registry::blocks::block_state_ext::BlockStateExt as _;

    world.get_block_state(pos.below()).get_block() != &vanilla_blocks::NETHER_WART_BLOCK
}

impl Entity for HoglinEntity {
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

    /// Vanilla parity: `Hoglin.getDefaultDimensions`.
    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if AgeableMob::is_baby(self) {
            BABY_DIMENSIONS.scale(scale)
        } else if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    /// Vanilla parity: `Hoglin.getSoundSource`, which is `HOSTILE` even though
    /// the mob extends `Animal`.
    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    /// Vanilla parity: `Hoglin.playStepSound`.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(&sound_events::ENTITY_HOGLIN_STEP, 0.15, 1.0);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        let state = self.state.lock();
        nbt.insert("IsImmuneToZombification", self.is_immune_to_zombification());
        nbt.insert("TimeInOverworld", state.time_in_overworld);
        nbt.insert("CannotBeHunted", state.cannot_be_hunted);
        drop(state);
        self.brain.save(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        self.set_immune_to_zombification(nbt.byte("IsImmuneToZombification").unwrap_or(0) != 0);
        {
            let mut state = self.state.lock();
            state.time_in_overworld = nbt.int("TimeInOverworld").unwrap_or(0);
            state.cannot_be_hunted = nbt.byte("CannotBeHunted").unwrap_or(0) != 0;
        }
        self.brain.load(nbt);
    }
}

impl LivingEntity for HoglinEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: the `Mob.serverAiStep` a hoglin inherits, which is the
    /// only path to [`Mob::custom_server_ai_step`] and so to the brain.
    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    /// Vanilla parity: `Hoglin.aiStep`, which runs down the attack animation,
    /// on top of the ageing and love clocks every animal ticks.
    fn ai_step(&self) -> Option<MoveResult> {
        {
            let mut state = self.state.lock();
            if state.attack_animation_remaining_ticks > 0 {
                state.attack_animation_remaining_ticks -= 1;
            }
        }
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);
        result
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

    fn is_baby(&self) -> bool {
        AgeableMob::is_baby(self)
    }

    /// Vanilla parity: `Hoglin.hurtServer`, which tells the brain who hit it.
    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        let was_hurt = self.living_hurt_server(world, source, amount);
        if !was_hurt {
            return false;
        }
        let Some(attacker) = source
            .causing_entity_id
            .and_then(|id| world.get_entity_by_id(id))
        else {
            return true;
        };
        if attacker.as_living_entity().is_some() {
            hoglin_ai::was_hurt_by(world, &self.brain, self, &attacker);
        }
        true
    }

    /// Vanilla parity: `Hoglin.getBaseExperienceReward`, which returns the flat
    /// `xpReward` rather than `Mob`'s equipment-scaled one -- a hoglin wears
    /// nothing to scale by.
    fn base_experience_reward(&self) -> i32 {
        Mob::xp_reward(self)
    }

    /// Vanilla parity: `Hoglin.shouldDropExperience`, which is unconditionally
    /// true -- unlike every other baby, a baby hoglin is still worth killing.
    fn should_drop_experience(&self) -> bool {
        true
    }

    /// Vanilla parity: `Hoglin.blockedByItem`. A grown hoglin whose charge is
    /// blocked launches the blocker anyway, which is why a shield does not save
    /// you from being thrown.
    fn blocked_by_item(&self, defender: &dyn LivingEntity) {
        // Vanilla's override does not call super, so a baby's blocked charge
        // produces no knockback at all rather than the default nudge.
        if !self.is_adult() {
            return;
        }
        let Some(world) = self.level() else {
            return;
        };
        let Some(defender_entity) = world.get_entity_by_id(defender.id()) else {
            return;
        };
        hoglin_base::throw_target(self, &defender_entity);
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_HOGLIN_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_HOGLIN_DEATH)
    }
}

impl AgeableMob for HoglinEntity {
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

    /// Vanilla parity: `Hoglin.ageBoundaryReached`.
    fn age_boundary_changed(&self, baby: bool) {
        self.hoglin_age_boundary_reached(baby);
    }
}

impl Animal for HoglinEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    /// Vanilla parity: `Hoglin.isFood`, the `hoglin_food` tag -- crimson fungus.
    fn is_food(&self, item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::HOGLIN_FOOD)
    }

    /// Vanilla parity: `Hoglin.canFallInLove`, which a pacified hoglin cannot.
    fn can_fall_in_love(&self) -> bool {
        !hoglin_ai::brain_is_pacified(&self.brain) && self.animal_base.in_love_time() <= 0
    }
}

impl Mob for HoglinEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn brain(&self) -> Option<&Brain> {
        Some(&self.brain)
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    /// Vanilla parity: `Hoglin.customServerAiStep`, which is the brain tick and
    /// the overworld conversion clock.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.brain.tick(&world, self);
        hoglin_ai::update_activity(&self.brain);

        if !self.is_converting() {
            self.state.lock().time_in_overworld = 0;
            return;
        }

        let time_in_overworld = {
            let mut state = self.state.lock();
            state.time_in_overworld += 1;
            state.time_in_overworld
        };
        if time_in_overworld > CONVERSION_TIME {
            self.make_sound(Some(&sound_events::ENTITY_HOGLIN_CONVERTED_TO_ZOMBIFIED));
            self.finish_conversion();
        }
    }

    /// Vanilla parity: `Hoglin.doHurtTarget`, the gore-and-throw.
    fn do_hurt_target(&self, world: &World, target: &SharedEntity) -> bool {
        if target.as_living_entity().is_none() {
            return false;
        }
        self.set_attack_animation_remaining_ticks(ATTACK_ANIMATION_DURATION);
        self.broadcast_entity_event(EntityStatus::StartAttacking);
        self.make_sound(Some(&sound_events::ENTITY_HOGLIN_ATTACK));
        hoglin_ai::on_hit_target(&self.brain, self, target);
        hoglin_base::hurt_and_throw_target(world, self, target)
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(self.activity_sound())
    }

    /// Vanilla parity: `Hoglin.removeWhenFarAway`, which is unconditionally
    /// true -- unlike every other animal, a bred hoglin still despawns.
    fn remove_when_far_away(&self, _distance_squared: f64) -> bool {
        true
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        _spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_hoglin_spawn_rules(world, pos)
    }

    /// Vanilla parity: `Hoglin.finalizeSpawn`, one in five of which is a baby.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        if rand::random::<f32>() < PROBABILITY_OF_SPAWNING_AS_BABY {
            Mob::set_baby(self, true);
        }
        self.finalize_spawn_ageable_mob(world, spawn_reason, group_data)
    }

    /// Vanilla parity: `Hoglin.mobInteract`, which pins a fed hoglin in place.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let interaction_result = Animal::mob_interact_animal(self, player, hand);
        if interaction_result.consumes_action() {
            Mob::set_persistence_required(self);
        }
        interaction_result
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for HoglinEntity {
    /// Vanilla parity: `Hoglin.getWalkTargetValue`, which is what keeps a
    /// hoglin on crimson nylium and off a respawn anchor.
    fn get_walk_target_value(&self, pos: BlockPos) -> f32 {
        use foton_registry::blocks::block_state_ext::BlockStateExt as _;

        let Some(world) = self.level() else {
            return 0.0;
        };
        if let Some(repellent) = self
            .brain
            .get_memory(memory_module_types::NEAREST_REPELLENT)
            && utils::block_closer_than(repellent, pos, hoglin_ai::REPELLENT_AVOID_RANGE)
        {
            return REPELLENT_WALK_VALUE;
        }
        if world.get_block_state(pos.below()).get_block() == &vanilla_blocks::CRIMSON_NYLIUM {
            return CRIMSON_NYLIUM_WALK_VALUE;
        }
        0.0
    }
}

impl HoglinBase for HoglinEntity {
    fn attack_animation_remaining_ticks(&self) -> i32 {
        self.state.lock().attack_animation_remaining_ticks
    }

    fn set_attack_animation_remaining_ticks(&self, ticks: i32) {
        self.state.lock().attack_animation_remaining_ticks = ticks;
    }
}

impl Enemy for HoglinEntity {}
