//! Frog entity.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.frog.Frog`. A frog is an
//! `Animal` driven by a brain rather than a goal selector: it hops in long arcs
//! toward lily pads, swims, croaks, swallows small magma cubes whole -- which is
//! where a froglight comes from -- and, once bred, walks to the bank and lays
//! the frogspawn a tadpole hatches out of.

mod frog_ai;

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::frog_variant::FrogVariantRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::registry_reference::RegistryReference;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_entity_data::FrogEntityData;
use steel_registry::vanilla_entity_type_tags::EntityTypeTag;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    REGISTRY, RegistryExt as _, TaggedRegistryExt as _, sound_events, vanilla_frog_variants,
};
use steel_utils::locks::SyncMutex;
use steel_utils::random::legacy_random::LegacyRandom;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, Identifier};

use std::str::FromStr as _;

use crate::entity::ai::brain::Brain;
use crate::entity::ai::brain::memory::{Unit, memory_module_types};
use crate::entity::ai::control::SmoothSwimmingMoveControl;
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::entities::{MagmaCubeEntity, SlimeEntity};
use crate::entity::mob::NavigationKind;
use crate::entity::{
    AgeableMob, AgeableMobBase, Animal, AnimalBase, Entity, EntityBase, EntityBaseLoad,
    EntitySpawnReason, EntitySyncedData, LivingEntity, LivingEntityBase, LivingEntitySyncedData,
    Mob, MobBase, MoveResult, PathfinderMob, SharedEntity, SpawnGroupData,
};
use crate::physics::MoverType;
use crate::world::{LevelReader as _, World};

/// Vanilla parity: `Frog.FROG_FALL_DAMAGE_REDUCTION`.
const FALL_DAMAGE_REDUCTION: i32 = 5;
// MISSING FOUNDATION: vanilla's `Frog.getHeadRotSpeed` returns 35, which feeds
// `BodyRotationControl`. Steel's body rotation has no head-rotation-speed seam
// -- only `max_head_y_rot` -- so a frog turns its head at the shared rate.
/// Vanilla parity: `Frog.getMaxHeadYRot`.
const MAX_HEAD_Y_ROT: f32 = 5.0;
/// Vanilla parity: the `scale(0.9)` of `Frog.travelInWater`.
const SWIM_DRAG: f64 = 0.9;
/// Vanilla parity: the `0.15F` volume of `Frog.playStepSound`.
const STEP_SOUND_VOLUME: f32 = 0.15;
/// Vanilla parity: the `2.0F` volume of `Frog.playEatingSound`.
const EATING_SOUND_VOLUME: f32 = 2.0;

/// Vanilla parity: the `SmoothSwimmingMoveControl(this, 85, 10, 0.02F, 0.1F, true)`
/// a frog installs.
const SWIM_MOVE_CONTROL: SmoothSwimmingMoveControl =
    SmoothSwimmingMoveControl::new(85, 10, 0.02, 0.1, true);

/// A frog.
#[entity_behavior(class = "Frog")]
pub struct FrogEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    brain: Brain,
    entity_data: SyncMutex<FrogEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `FrogEntity`.
unsafe impl DowncastType for FrogEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/frog");
}

impl FrogEntity {
    /// Creates a frog at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a frog from saved base data.
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
            // Vanilla parity: the two `setPathfindingMalus` calls of the `Frog`
            // constructor. Water is merely expensive; a trapdoor is a wall.
            let mut malus = mob_base.pathfinding_malus().lock();
            malus.set(PathType::Water, 4.0);
            malus.set(PathType::Trapdoor, -1.0);
        }
        let mut entity_data = FrogEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            brain: frog_ai::make_brain(),
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Returns vanilla `Frog.getVariant`.
    #[must_use]
    pub fn variant(&self) -> FrogVariantRef {
        self.entity_data.lock().variant.get().value()
    }

    /// Sets vanilla `Frog.setVariant`.
    pub fn set_variant(&self, variant: FrogVariantRef) {
        self.entity_data
            .lock()
            .variant
            .set(RegistryReference::new(variant));
    }

    fn set_variant_by_key(&self, key: &Identifier) -> bool {
        let Some(variant) = REGISTRY.frog_variants.by_key(key) else {
            return false;
        };
        self.set_variant(variant);
        true
    }

    /// Returns vanilla `Frog.getTongueTarget`.
    #[must_use]
    pub fn tongue_target(&self) -> Option<SharedEntity> {
        let target_id = (*self.entity_data.lock().tongue_target.get())?;
        let world = self.level()?;
        world.get_entity_by_id(i32::try_from(target_id).ok()?)
    }

    /// Sets vanilla `Frog.setTongueTarget`.
    pub fn set_tongue_target(&self, target: &SharedEntity) {
        self.entity_data
            .lock()
            .tongue_target
            .set(u32::try_from(target.id()).ok());
    }

    /// Vanilla parity: `Frog.eraseTongueTarget`.
    pub fn erase_tongue_target(&self) {
        self.entity_data.lock().tongue_target.set(None);
    }

    /// Returns whether a frog will swallow this entity.
    ///
    /// Vanilla parity: `Frog.canEat`, which is the `#minecraft:frog_food` entity
    /// tag minus the cube mobs that are too big -- only a size-one magma cube
    /// fits, and that is what leaves a froglight behind.
    #[must_use]
    pub fn can_eat(entity: &dyn LivingEntity) -> bool {
        if let Some(size) = cube_mob_size(entity)
            && size != 1
        {
            return false;
        }
        REGISTRY
            .entity_types
            .is_in_tag(entity.entity_type(), &EntityTypeTag::FROG_FOOD)
    }

    /// Returns whether the stack is vanilla frog food.
    #[must_use]
    pub fn is_frog_food(item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::FROG_FOOD)
    }

    /// Vanilla parity: `Frog.checkFrogSpawnRules`.
    #[must_use]
    pub fn check_frog_spawn_rules(world: &Arc<World>, pos: BlockPos) -> bool {
        world
            .get_block_state(pos.below())
            .get_block()
            .has_tag(&BlockTag::FROGS_SPAWNABLE_ON)
            && <Self as Animal>::is_bright_enough_to_spawn(world.as_ref(), pos)
    }

    /// Seeds the brain the way a freshly spawned or freshly bred frog needs.
    ///
    /// Vanilla parity: `FrogAi.initMemories`.
    pub fn init_memories(&self) {
        frog_ai::init_memories(&self.brain);
    }
}

/// Returns a cube mob's size, when the entity is one.
///
/// Vanilla parity: the `entity instanceof AbstractCubeMob cubeMob` of
/// `Frog.canEat`. Steel has no shared cube-mob trait object, so the two cube
/// mobs answer through their concrete types.
fn cube_mob_size(entity: &dyn LivingEntity) -> Option<i32> {
    use steel_utils::Downcast as _;

    if let Some(slime) = entity.downcast_ref::<SlimeEntity>() {
        return Some(slime.cube_size());
    }
    if let Some(magma_cube) = entity.downcast_ref::<MagmaCubeEntity>() {
        return Some(magma_cube.cube_size());
    }
    None
}

impl Entity for FrogEntity {
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

    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(&sound_events::ENTITY_FROG_STEP, STEP_SOUND_VOLUME, 1.0);
    }

    /// Vanilla parity: `Frog.isPushedByFluid`; a frog holds its line in a
    /// current.
    fn is_pushed_by_fluid(&self) -> bool {
        false
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        nbt.insert("variant", self.variant().key.to_string());
        self.brain.save(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        if let Some(variant) = nbt.string("variant")
            && let Ok(key) = Identifier::from_str(variant.to_str().as_ref())
        {
            self.set_variant_by_key(&key);
        }
        self.brain.load(nbt);
    }
}

impl LivingEntity for FrogEntity {
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
        Some(&sound_events::ENTITY_FROG_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_FROG_DEATH)
    }

    /// Vanilla parity: `Frog.calculateFallDamage`, which takes five blocks off
    /// every landing -- a frog that jumps four blocks lands unhurt.
    fn calculate_fall_damage(&self, fall_distance: f64, damage_modifier: f32) -> i32 {
        (self.default_calculate_fall_damage(fall_distance, damage_modifier) - FALL_DAMAGE_REDUCTION)
            .max(0)
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

    /// Vanilla parity: `Frog.travelInWater`, which swims rather than sinking.
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

impl AgeableMob for FrogEntity {
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

impl Animal for FrogEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        Self::is_frog_food(item_stack)
    }

    /// Vanilla parity: `Frog.playEatingSound`.
    fn play_eating_sound(&self) {
        self.play_sound(&sound_events::ENTITY_FROG_EAT, EATING_SOUND_VOLUME, 1.0);
    }

    /// Vanilla parity: `Frog.spawnChildFromBreeding`, which produces no child at
    /// all -- the pair get the `IS_PREGNANT` memory instead, and the frogspawn
    /// they lay is what carries the next generation.
    fn spawn_child_from_breeding(&self, world: &Arc<World>, partner: &dyn Animal) {
        self.finalize_spawn_child_from_breeding(world, partner, None);
        self.brain
            .set_memory(memory_module_types::IS_PREGNANT, Unit);
    }
}

impl Mob for FrogEntity {
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

    /// Vanilla parity: `Frog.customServerAiStep`, which is the brain tick and
    /// the activity update, then the shared animal step.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.brain.tick(&world, self);
        frog_ai::update_activity(&self.brain);
        Animal::custom_server_ai_step_animal(self);
    }

    /// Vanilla parity: `Frog.SmoothSwimmingMoveControl`.
    fn tick_move_control(&self) {
        SWIM_MOVE_CONTROL.tick(self);
    }

    /// Vanilla parity: `Frog.getMaxHeadYRot`.
    fn max_head_y_rot(&self) -> f32 {
        MAX_HEAD_Y_ROT
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_FROG_AMBIENT)
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        let _ = spawn_reason;
        Self::check_frog_spawn_rules(world, pos)
    }

    /// Vanilla parity: `Frog.finalizeSpawn`, which picks the variant from the
    /// biome and seeds the jump cooldown.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let variant = world.biome_at(self.block_position()).and_then(|biome| {
            let mut random = LegacyRandom::from_seed(rand::random());
            REGISTRY
                .frog_variants
                .select_spawn_variant(biome, &mut random)
        });
        self.set_variant(variant.unwrap_or(&vanilla_frog_variants::TEMPERATE));
        self.init_memories();
        self.finalize_spawn_ageable_mob(world, spawn_reason, group_data)
    }
}

impl PathfinderMob for FrogEntity {
    /// Navigates as a swimmer while in water and as a walker on land.
    ///
    /// Vanilla parity: `Frog.FrogPathNavigation`, an `AmphibiousPathNavigation`.
    /// Steel answers this per path request, the same seam the turtle and the
    /// drowned use.
    ///
    /// MISSING FOUNDATION: vanilla's `FrogNodeEvaluator` also reports
    /// `PathType.OPEN` above a `#minecraft:frog_prefer_jump_to` block, which is
    /// what lets a frog path *onto* a lily pad rather than only jump to one.
    /// Steel's node evaluators are not per-mob, so a frog reaches lily pads
    /// through the long jump alone.
    fn navigation_kind(&self) -> NavigationKind {
        if self.is_in_water() {
            NavigationKind::WaterBound {
                allow_breaching: false,
            }
        } else {
            NavigationKind::Ground
        }
    }
}

#[cfg(test)]
mod tests;
