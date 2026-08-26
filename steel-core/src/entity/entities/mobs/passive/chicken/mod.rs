//! Chicken entity.
//!
//! Vanilla parity: `Chicken`. A chicken flaps hard enough to break its own fall,
//! and lays an egg every five to ten minutes as long as it is grown and free.

use std::str::FromStr as _;
use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::chicken_sound_variant::{ChickenAge, ChickenSoundVariantRef};
use steel_registry::chicken_variant::ChickenVariantRef;
use steel_registry::entity_type::{
    EntityAttachmentPoint, EntityAttachments, EntityDimensions, EntityTypeRef,
};
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::ChickenEntityData;
use steel_registry::vanilla_game_events;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    REGISTRY, RegistryExt, RegistryReference, TaggedRegistryExt, sound_events, vanilla_loot_tables,
};
use steel_utils::locks::SyncMutex;
use steel_utils::random::legacy_random::LegacyRandom;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, Identifier};

use crate::entity::ai::goal::{
    BreedGoal, FloatGoal, FollowParentGoal, LookAtPlayerGoal, PanicGoal, RandomLookAroundGoal,
    TemptGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::living_entity::gift_loot_items_with_rng;
use crate::entity::{
    AgeableMob, AgeableMobBase, Animal, AnimalBase, Entity, EntityBase, EntityBaseLoad, EntityPose,
    EntitySpawnReason, EntitySyncedData, LivingEntity, LivingEntityBase, Mob, MobBase,
    PathfinderMob, SpawnGroupData,
};
use crate::physics::MoveResult;
use crate::world::World;
use crate::world::game_event::GameEventContext;

const CHICKEN_BABY_PASSENGER_ATTACHMENTS: [EntityAttachmentPoint; 1] =
    [EntityAttachmentPoint::new(0.0, 0.375, 0.0)];
const CHICKEN_BABY_WIDTH: f32 = 0.3;
const CHICKEN_BABY_HEIGHT: f32 = 0.4;
const CHICKEN_BABY_EYE_HEIGHT: f32 = 0.281_25;

const CHICKEN_BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new_with_attachments(
    CHICKEN_BABY_WIDTH,
    CHICKEN_BABY_HEIGHT,
    CHICKEN_BABY_EYE_HEIGHT,
    EntityAttachments::new(&CHICKEN_BABY_PASSENGER_ATTACHMENTS, &[], &[], &[]),
);

/// Shortest wait between two eggs, in ticks.
///
/// Vanilla parity: the `6000` floor of `this.random.nextInt(6000) + 6000`.
const EGG_TIME_MIN: i32 = 6000;

/// Width of the random window added on top of [`EGG_TIME_MIN`].
const EGG_TIME_SPREAD: i32 = 6000;

/// Fraction of downward speed a flapping chicken keeps each tick.
///
/// Vanilla parity: the `0.6` of `Chicken.aiStep`, which is why a chicken never
/// takes fall damage.
const FALL_SPEED_RETAINED: f64 = 0.6;

/// Experience a chicken jockey drops instead of the usual animal reward.
///
/// Vanilla parity: `Chicken.getBaseExperienceReward`.
const JOCKEY_EXPERIENCE_REWARD: i32 = 10;

/// State a chicken keeps to itself.
struct ChickenState {
    /// Ticks left before the next egg.
    egg_time: i32,
    /// Whether this chicken carries a baby zombie.
    is_chicken_jockey: bool,
}

/// A chicken.
#[entity_behavior(class = "Chicken")]
pub struct ChickenEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    entity_data: SyncMutex<ChickenEntityData>,
    state: SyncMutex<ChickenState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `ChickenEntity`.
unsafe impl DowncastType for ChickenEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/chicken");
}

impl ChickenEntity {
    /// Creates a chicken at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a chicken from saved base data.
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
        // Vanilla parity: a chicken is happy to walk into water, unlike the other
        // farm animals.
        mob_base
            .pathfinding_malus()
            .lock()
            .set(PathType::Water, 0.0);
        let mut entity_data = ChickenEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Keep vanilla Chicken goal priorities and speeds in the same order.
            let mut goal_selector = mob_base.goal_selector().lock();
            goal_selector.add_goal(0, FloatGoal::new(&mob_base));
            goal_selector.add_goal(1, PanicGoal::new(1.4));
            goal_selector.add_goal(2, BreedGoal::new(1.0));
            goal_selector.add_goal(
                3,
                TemptGoal::new(
                    1.0,
                    |item_stack| {
                        REGISTRY
                            .items
                            .is_in_tag(item_stack.item(), &ItemTag::CHICKEN_FOOD)
                    },
                    false,
                ),
            );
            goal_selector.add_goal(4, FollowParentGoal::new(1.1));
            goal_selector.add_goal(5, WaterAvoidingRandomStrollGoal::new(1.0));
            goal_selector.add_goal(6, LookAtPlayerGoal::new(6.0));
            goal_selector.add_goal(7, RandomLookAroundGoal::new());
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            entity_data: SyncMutex::new(entity_data),
            state: SyncMutex::new(ChickenState {
                egg_time: next_egg_time(),
                is_chicken_jockey: false,
            }),
        }
    }

    /// Sets the chicken variant by registry entry.
    pub fn set_variant(&self, variant: ChickenVariantRef) {
        self.entity_data
            .lock()
            .variant
            .set(RegistryReference::new(variant));
    }

    /// Returns the chicken variant, falling back to temperate when invalid.
    #[must_use]
    pub fn variant(&self) -> ChickenVariantRef {
        self.entity_data.lock().variant.get().value()
    }

    /// Sets the chicken sound variant by registry entry.
    pub fn set_sound_variant(&self, sound_variant: ChickenSoundVariantRef) {
        self.entity_data
            .lock()
            .sound_variant
            .set(RegistryReference::new(sound_variant));
    }

    /// Returns the chicken sound variant, falling back to classic when invalid.
    #[must_use]
    pub fn sound_variant(&self) -> ChickenSoundVariantRef {
        self.entity_data.lock().sound_variant.get().value()
    }

    /// Returns the sound set matching this chicken's age.
    ///
    /// Vanilla parity: `Chicken.getSoundSet`, which is why a chick peeps rather
    /// than clucks.
    fn sound_set(&self) -> &'static ChickenAge {
        let sound_variant = self.sound_variant();
        if AgeableMob::is_baby(self) {
            &sound_variant.baby_sounds
        } else {
            &sound_variant.adult_sounds
        }
    }

    /// Returns whether this chicken is carrying a baby zombie.
    ///
    /// Vanilla parity: `Chicken.isChickenJockey`.
    #[must_use]
    pub fn is_chicken_jockey(&self) -> bool {
        self.state.lock().is_chicken_jockey
    }

    /// Marks this chicken as a jockey mount.
    ///
    /// Vanilla parity: `Chicken.setChickenJockey`.
    pub fn set_chicken_jockey(&self, is_chicken_jockey: bool) {
        self.state.lock().is_chicken_jockey = is_chicken_jockey;
    }

    fn set_variant_by_key(&self, key: &Identifier) -> bool {
        let Some(variant) = REGISTRY.chicken_variants.by_key(key) else {
            return false;
        };
        self.set_variant(variant);
        true
    }

    fn set_sound_variant_by_key(&self, key: &Identifier) {
        if let Some(sound_variant) = REGISTRY.chicken_sound_variants.by_key(key) {
            self.set_sound_variant(sound_variant);
        }
    }

    /// Slows the chicken's descent so it lands unharmed.
    ///
    /// Vanilla parity: the delta-movement branch of `Chicken.aiStep`.
    fn break_own_fall(&self) {
        if self.on_ground() {
            return;
        }
        let movement = self.velocity();
        if movement.y < 0.0 {
            self.set_velocity(DVec3::new(
                movement.x,
                movement.y * FALL_SPEED_RETAINED,
                movement.z,
            ));
        }
    }

    /// Counts down to the next egg and lays it when the timer runs out.
    ///
    /// Vanilla parity: the egg branch of `Chicken.aiStep`.
    fn tick_egg_laying(&self) {
        if !Entity::is_alive(self) || AgeableMob::is_baby(self) || self.is_chicken_jockey() {
            return;
        }

        let ready = {
            let mut state = self.state.lock();
            state.egg_time -= 1;
            state.egg_time <= 0
        };
        if !ready {
            return;
        }

        if self.lay_egg() {
            self.play_sound(
                &sound_events::ENTITY_CHICKEN_EGG,
                1.0,
                0.2f32.mul_add(rand::random::<f32>() - rand::random::<f32>(), 1.0),
            );
        }
        self.state.lock().egg_time = next_egg_time();
    }

    /// Drops whatever the chicken-lay loot table rolls.
    ///
    /// Vanilla parity: `Mob.dropFromGiftLootTable` with
    /// `BuiltInLootTables.CHICKEN_LAY`.
    fn lay_egg(&self) -> bool {
        let mut rng = rand::rng();
        let drops =
            gift_loot_items_with_rng(self, &vanilla_loot_tables::GAMEPLAY_CHICKEN_LAY, &mut rng);
        if drops.is_empty() {
            return false;
        }

        let mut dropped = false;
        for drop in drops {
            if self.spawn_at_location(drop, 0.0).is_some() {
                dropped = true;
            }
        }
        if dropped && let Some(world) = self.level() {
            // Vanilla parity: the `gameEvent(GameEvent.ENTITY_PLACE)` of
            // `Chicken.aiStep`, which is what lets a sculk sensor hear an egg
            // land.
            world.game_event_at(
                &vanilla_game_events::ENTITY_PLACE,
                self.position(),
                &GameEventContext::new(Some(self as &dyn Entity), None),
            );
        }
        dropped
    }

    /// Returns whether an item matches the vanilla chicken food tag.
    #[must_use]
    pub fn is_food(item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::CHICKEN_FOOD)
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

/// Rolls the wait until the next egg.
///
/// Vanilla parity: `this.random.nextInt(6000) + 6000`.
fn next_egg_time() -> i32 {
    rand::random_range(0..EGG_TIME_SPREAD) + EGG_TIME_MIN
}

impl Entity for ChickenEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if AgeableMob::is_baby(self) {
            CHICKEN_BABY_DIMENSIONS.scale(scale)
        } else if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        }
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
        self.play_sound(self.sound_set().step_sound, 0.15, 1.0);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        let state = self.state.lock();
        nbt.insert("IsChickenJockey", state.is_chicken_jockey);
        nbt.insert("EggLayTime", state.egg_time);
        drop(state);
        nbt.insert("variant", self.variant().key.to_string());
        nbt.insert("sound_variant", self.sound_variant().key.to_string());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);

        {
            let mut state = self.state.lock();
            state.is_chicken_jockey = nbt.byte("IsChickenJockey").is_some_and(|flag| flag != 0);
            if let Some(egg_time) = nbt.int("EggLayTime") {
                state.egg_time = egg_time;
            }
        }

        if let Some(variant) = nbt.string("variant")
            && let Ok(key) = Identifier::from_str(variant.to_str().as_ref())
        {
            self.set_variant_by_key(&key);
        }
        if let Some(sound_variant) = nbt.string("sound_variant")
            && let Ok(key) = Identifier::from_str(sound_variant.to_str().as_ref())
        {
            self.set_sound_variant_by_key(&key);
        }
    }
}

impl LivingEntity for ChickenEntity {
    fn chicken_loot_variant(&self) -> Option<&'static Identifier> {
        Some(&self.variant().key)
    }

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
        Some(self.sound_set().hurt_sound)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(self.sound_set().death_sound)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    /// Vanilla parity: `Chicken.getBaseExperienceReward`, which pays out more for
    /// the chicken a baby zombie rode in on.
    fn base_experience_reward(&self) -> i32 {
        if self.is_chicken_jockey() {
            JOCKEY_EXPERIENCE_REWARD
        } else {
            Animal::base_experience_reward_animal(self)
        }
    }

    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();

        self.break_own_fall();
        self.tick_egg_laying();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);
        result
    }
}

impl AgeableMob for ChickenEntity {
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

    fn age_boundary_changed(&self, _baby: bool) {
        self.refresh_dimensions();
    }
}

impl Animal for ChickenEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        ChickenEntity::is_food(item_stack)
    }

    fn breed_variant_key(&self) -> Option<&Identifier> {
        Some(&self.variant().key)
    }

    fn set_breed_variant_key(&self, key: &Identifier) -> bool {
        self.set_variant_by_key(key)
    }

    /// Vanilla parity: `Chicken.getBreedOffspring`, which flips a coin between
    /// the two parents rather than mixing them.
    fn initialize_breed_offspring(&self, partner: &dyn Animal, offspring: &dyn Animal) {
        let use_self_variant = rand::random::<bool>();
        let variant_key = if use_self_variant {
            self.breed_variant_key()
        } else {
            partner.breed_variant_key()
        };
        let Some(variant_key) = variant_key else {
            return;
        };

        if !offspring.set_breed_variant_key(variant_key) {
            log::error!("chicken offspring could not inherit breeding variant {variant_key}");
        }
    }
}

impl Mob for ChickenEntity {
    /// Returns whether this mob accepts where the spawner put it.
    ///
    /// Vanilla parity: `Animal::checkAnimalSpawnRules`. Animals want light
    /// and a block their own tag allows, which is why a field fills with
    /// cows by day and a cave never does.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        <Self as Animal>::check_animal_spawn_rules(world.as_ref(), spawn_reason, pos)
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

    fn custom_server_ai_step(&self) {
        Animal::custom_server_ai_step_animal(self);
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(self.sound_set().ambient_sound)
    }

    /// Vanilla parity: `Chicken.removeWhenFarAway`. Only a jockey mount despawns;
    /// a farmed chicken stays where it was left.
    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        self.is_chicken_jockey()
    }

    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let biome = world.biome_at(self.block_position());
        let (variant, sound_variant) = {
            let mut random = LegacyRandom::from_seed(rand::random());
            let variant = biome.and_then(|biome| {
                REGISTRY
                    .chicken_variants
                    .select_spawn_variant(biome, &mut random)
            });
            let sound_variant = REGISTRY.chicken_sound_variants.pick_random(&mut random);
            (variant, sound_variant)
        };

        if let Some(variant) = variant {
            self.set_variant(variant);
        }
        if let Some(sound_variant) = sound_variant {
            self.set_sound_variant(sound_variant);
        }

        self.finalize_spawn_ageable_mob(world, spawn_reason, group_data)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for ChickenEntity {}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_entities, vanilla_items};

    use super::*;

    fn test_chicken() -> ChickenEntity {
        init_vanilla_registry();
        ChickenEntity::new(
            &vanilla_entities::CHICKEN,
            1,
            DVec3::ZERO,
            Weak::<World>::new(),
        )
    }

    /// Vanilla parity: `this.random.nextInt(6000) + 6000`, so five to ten minutes.
    #[test]
    fn the_egg_timer_always_starts_inside_the_vanilla_window() {
        for _ in 0..64 {
            let egg_time = next_egg_time();
            assert!(
                (EGG_TIME_MIN..EGG_TIME_MIN + EGG_TIME_SPREAD).contains(&egg_time),
                "egg time {egg_time} outside the vanilla window"
            );
        }
    }

    /// Vanilla parity: the `multiply(1.0, 0.6, 1.0)` of `Chicken.aiStep`, which is
    /// what stops a chicken ever hitting the ground hard.
    #[test]
    fn falling_speed_is_cut_but_horizontal_speed_is_not() {
        let chicken = test_chicken();
        chicken.set_velocity(DVec3::new(0.4, -1.0, -0.2));

        chicken.break_own_fall();

        let velocity = chicken.velocity();
        assert!(
            (velocity.y - -FALL_SPEED_RETAINED).abs() < 1e-9,
            "y was {}",
            velocity.y
        );
        assert!((velocity.x - 0.4).abs() < 1e-9);
        assert!((velocity.z - -0.2).abs() < 1e-9);
    }

    /// Rising chickens are left alone, so a chicken kicked upward still travels.
    #[test]
    fn upward_speed_is_left_alone() {
        let chicken = test_chicken();
        chicken.set_velocity(DVec3::new(0.0, 0.5, 0.0));

        chicken.break_own_fall();

        assert!((chicken.velocity().y - 0.5).abs() < 1e-9);
    }

    /// Vanilla parity: `Chicken.removeWhenFarAway`, which keeps farmed chickens
    /// around and lets jockey mounts despawn.
    #[test]
    fn only_a_jockey_mount_despawns_when_far_away() {
        let chicken = test_chicken();
        assert!(!Mob::remove_when_far_away(&chicken, 4096.0));

        chicken.set_chicken_jockey(true);
        assert!(Mob::remove_when_far_away(&chicken, 4096.0));
    }

    /// Vanilla parity: `ItemTags.CHICKEN_FOOD`, which is the seeds and nothing else.
    #[test]
    fn chickens_eat_seeds_and_not_wheat() {
        init_vanilla_registry();
        assert!(ChickenEntity::is_food(&ItemStack::new(
            &vanilla_items::WHEAT_SEEDS
        )));
        assert!(ChickenEntity::is_food(&ItemStack::new(
            &vanilla_items::PUMPKIN_SEEDS
        )));
        assert!(!ChickenEntity::is_food(&ItemStack::new(
            &vanilla_items::WHEAT
        )));
    }

    /// A chick is smaller than a grown chicken, which is what the client draws and
    /// what its hitbox measures.
    #[test]
    fn a_chick_is_smaller_than_a_grown_chicken() {
        let chicken = test_chicken();
        let adult = Entity::dimensions_for_pose(&chicken, EntityPose::Standing);

        Mob::set_baby(&chicken, true);
        let baby = Entity::dimensions_for_pose(&chicken, EntityPose::Standing);

        assert!(
            baby.width < adult.width,
            "{} vs {}",
            baby.width,
            adult.width
        );
        assert!(baby.height < adult.height);
    }
}
