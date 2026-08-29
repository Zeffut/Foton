//! Salmon entity.
//!
//! Vanilla parity: `Salmon`, `AbstractFish` and `WaterAnimal`. A salmon behaves
//! exactly like a cod but comes in three sizes, and the size is its hitbox as
//! well as its look.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::entity_type::{EntityDimensions, EntityTypeRef};
use foton_registry::sound_event::SoundEventRef;
use foton_registry::sound_events;
use foton_registry::vanilla_entity_data::SalmonEntityData;
use foton_utils::locks::SyncMutex;
use foton_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::entity::ai::goal::{AvoidEntityGoal, PanicGoal, RandomSwimmingGoal};
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::mob::NavigationKind;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntityPose, EntitySpawnReason, EntitySyncedData,
    LivingEntity, LivingEntityBase, Mob, MobBase, PathfinderMob, SpawnGroupData,
};
use crate::physics::MoveResult;
use crate::world::World;

use super::fish;
use crate::entity::spawn_rules::check_surface_water_animal_spawn_rules;

/// How big a salmon grew.
///
/// Vanilla parity: `Salmon.Variant`, whose scale is applied to the hitbox and
/// not only to the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SalmonVariant {
    /// Half size.
    Small,
    /// The size a cod is.
    Medium,
    /// Half again as large.
    Large,
}

impl SalmonVariant {
    /// Vanilla parity: `Salmon.Variant.DEFAULT`.
    const DEFAULT: Self = Self::Medium;

    /// Returns the synced id, clamping like vanilla rather than failing.
    ///
    /// Vanilla parity: `Salmon.Variant.BY_ID`, built with
    /// `OutOfBoundsStrategy.CLAMP`.
    #[must_use]
    const fn from_id(id: i32) -> Self {
        match id {
            ..=0 => Self::Small,
            1 => Self::Medium,
            _ => Self::Large,
        }
    }

    /// Returns the synced id.
    #[must_use]
    const fn id(self) -> i32 {
        match self {
            Self::Small => 0,
            Self::Medium => 1,
            Self::Large => 2,
        }
    }

    /// Returns the name this variant is saved under.
    #[must_use]
    const fn serialized_name(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }

    /// Returns the variant saved under `name`.
    #[must_use]
    fn from_serialized_name(name: &str) -> Option<Self> {
        match name {
            "small" => Some(Self::Small),
            "medium" => Some(Self::Medium),
            "large" => Some(Self::Large),
            _ => None,
        }
    }

    /// Returns how much this variant scales the hitbox.
    ///
    /// Vanilla parity: `Salmon.Variant.boundingBoxScale`.
    #[must_use]
    const fn bounding_box_scale(self) -> f32 {
        match self {
            Self::Small => 0.5,
            Self::Medium => 1.0,
            Self::Large => 1.5,
        }
    }
}

/// Weights the three sizes are rolled with on spawn.
///
/// Vanilla parity: the weighted list of `Salmon.finalizeSpawn`.
const VARIANT_WEIGHTS: [(SalmonVariant, i32); 3] = [
    (SalmonVariant::Small, 30),
    (SalmonVariant::Medium, 50),
    (SalmonVariant::Large, 15),
];

/// A salmon.
#[entity_behavior(class = "Salmon")]
pub struct SalmonEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<SalmonEntityData>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `SalmonEntity`.
unsafe impl DowncastType for SalmonEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/salmon");
}

impl SalmonEntity {
    /// Creates a salmon at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a salmon from saved base data.
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
        // Vanilla parity: `WaterAnimal` clears the water malus, so open water is
        // free to path through rather than merely allowed.
        mob_base
            .pathfinding_malus()
            .lock()
            .set(PathType::Water, 0.0);
        let mut entity_data = SalmonEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Keep vanilla AbstractFish goal priorities in the same order.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(0, PanicGoal::new(fish::PANIC_SPEED_MODIFIER));
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
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Returns how big this salmon is.
    ///
    /// Vanilla parity: `Salmon.getVariant`.
    #[must_use]
    pub fn variant(&self) -> SalmonVariant {
        SalmonVariant::from_id(*self.entity_data.lock().variant_type.get())
    }

    /// Sets how big this salmon is.
    ///
    /// Vanilla parity: `Salmon.setVariant`, which also refreshes the hitbox
    /// because the size is not only cosmetic.
    pub fn set_variant(&self, variant: SalmonVariant) {
        self.entity_data.lock().variant_type.set(variant.id());
        self.refresh_dimensions();
    }

    /// Rolls a size the way spawning does.
    ///
    /// Vanilla parity: the weighted list of `Salmon.finalizeSpawn`: mostly
    /// medium, a third small, and large is rare.
    #[must_use]
    fn random_variant() -> SalmonVariant {
        let total: i32 = VARIANT_WEIGHTS.iter().map(|(_, weight)| weight).sum();
        let mut roll = rand::random_range(0..total);
        for (variant, weight) in VARIANT_WEIGHTS {
            roll -= weight;
            if roll < 0 {
                return variant;
            }
        }
        SalmonVariant::DEFAULT
    }

    /// Returns whether this salmon came out of a bucket.
    ///
    /// Vanilla parity: `AbstractFish.fromBucket`, whose name this keeps even
    /// though it reads state rather than converting anything.
    #[must_use]
    #[expect(
        clippy::wrong_self_convention,
        reason = "the name mirrors the vanilla accessor"
    )]
    pub fn from_bucket(&self) -> bool {
        *self.entity_data.lock().abstract_fish.from_bucket.get()
    }

    /// Marks this salmon as having come out of a bucket.
    pub fn set_from_bucket(&self, from_bucket: bool) {
        self.entity_data
            .lock()
            .abstract_fish_mut()
            .from_bucket
            .set(from_bucket);
    }
}

impl Entity for SalmonEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `Salmon.getDefaultDimensions`, which scales the hitbox by
    /// the variant so a large salmon is genuinely harder to miss.
    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self) * self.variant().bounding_box_scale();
        self.entity_type.dimensions.scale(scale)
    }

    /// Vanilla parity: `WaterAnimal.baseTick`, which reads the air left before
    /// the shared tick spends it.
    fn base_tick(&self) {
        let air_before_tick = self.air_supply();
        Mob::base_tick_mob(self);
        if let Some(world) = self.level() {
            fish::handle_air_supply(self, &world, air_before_tick);
        }
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    /// Vanilla parity: `AbstractFish.playStepSound` is empty; a fish has no feet.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {}

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        nbt.insert("FromBucket", self.from_bucket());
        nbt.insert("type", self.variant().serialized_name());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.set_from_bucket(nbt.byte("FromBucket").is_some_and(|flag| flag != 0));
        if let Some(raw) = nbt.string("type")
            && let Some(variant) = SalmonVariant::from_serialized_name(raw.to_str().as_ref())
        {
            self.set_variant(variant);
        }
    }
}

impl LivingEntity for SalmonEntity {
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
        Some(&sound_events::ENTITY_SALMON_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_SALMON_DEATH)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        fish::flop(self, &sound_events::ENTITY_SALMON_FLOP);
        self.default_ai_step()
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

impl Mob for SalmonEntity {
    /// Returns whether this mob accepts where the spawner put it.
    ///
    /// Vanilla parity: `WaterAnimal::checkSurfaceWaterAnimalSpawnRules`,
    /// which keeps it in the top thirteen blocks of the sea.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        let _ = spawn_reason;
        check_surface_water_animal_spawn_rules(world, pos)
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

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_SALMON_AMBIENT)
    }

    /// Vanilla parity: `WaterAnimal.getAmbientSoundInterval`.
    fn ambient_sound_interval(&self) -> i32 {
        fish::AMBIENT_SOUND_INTERVAL
    }

    /// Vanilla parity: `AbstractFish.removeWhenFarAway`. A bucketed fish someone
    /// released stays where it was put.
    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        !self.from_bucket()
    }

    /// Vanilla parity: `AbstractFish.FishMoveControl.tick`.
    fn tick_move_control(&self) {
        fish::tick_move_control(self);
    }

    /// Vanilla parity: `Salmon.finalizeSpawn`, which rolls the size.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.set_variant(Self::random_variant());
        self.finalize_spawn_mob_base(world, spawn_reason, group_data)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for SalmonEntity {
    /// Vanilla parity: `AbstractFish.createNavigation` returns a
    /// `WaterBoundPathNavigation`; a salmon never breaches.
    fn navigation_kind(&self) -> NavigationKind {
        NavigationKind::WaterBound {
            allow_breaching: false,
        }
    }
}
