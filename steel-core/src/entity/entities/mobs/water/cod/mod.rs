//! Cod entity.
//!
//! Vanilla parity: `Cod`, `AbstractFish` and `WaterAnimal`. The first mob in
//! Steel that swims: it navigates in three dimensions through water, drowns in
//! air the way a land mob drowns in water, and flops when it lands on a bank.

use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::sound_events;
use steel_registry::vanilla_entity_data::CodEntityData;
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use crate::entity::ai::goal::{AvoidEntityGoal, PanicGoal, RandomSwimmingGoal};
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::mob::NavigationKind;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase, Mob,
    MobBase, PathfinderMob,
};
use crate::physics::MoveResult;
use crate::world::World;

use super::fish;
use crate::entity::EntitySpawnReason;
use crate::entity::spawn_rules::check_surface_water_animal_spawn_rules;
use std::sync::Arc;

/// A cod.
#[entity_behavior(class = "Cod")]
pub struct CodEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<CodEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `CodEntity`.
unsafe impl DowncastType for CodEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/cod");
}

impl CodEntity {
    /// Creates a cod at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a cod from saved base data.
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
        let mut entity_data = CodEntityData::new();
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

    /// Returns whether this cod came out of a bucket.
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

    /// Marks this cod as having come out of a bucket.
    pub fn set_from_bucket(&self, from_bucket: bool) {
        self.entity_data
            .lock()
            .abstract_fish_mut()
            .from_bucket
            .set(from_bucket);
    }
}

impl Entity for CodEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
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
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.set_from_bucket(nbt.byte("FromBucket").is_some_and(|flag| flag != 0));
    }
}

impl LivingEntity for CodEntity {
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
        Some(&sound_events::ENTITY_COD_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_COD_DEATH)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        fish::flop(self, &sound_events::ENTITY_COD_FLOP);
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

impl Mob for CodEntity {
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
        Some(&sound_events::ENTITY_COD_AMBIENT)
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

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for CodEntity {
    /// Vanilla parity: `AbstractFish.createNavigation` returns a
    /// `WaterBoundPathNavigation`; a cod never breaches.
    fn navigation_kind(&self) -> NavigationKind {
        NavigationKind::WaterBound {
            allow_breaching: false,
        }
    }
}
