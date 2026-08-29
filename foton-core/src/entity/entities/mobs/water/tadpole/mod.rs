//! Tadpole entity.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.frog.Tadpole`. A tadpole
//! is a fish with a clock: it swims, follows a player holding a slime ball, and
//! after twenty minutes turns into a frog. It is what a `FrogspawnBlock` hatches
//! into, which is the loop this closes.

mod tadpole_ai;

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::TadpoleEntityData;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_entities};
use foton_utils::locks::SyncMutex;
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::behavior::InteractionResult;
use crate::entity::ai::brain::Brain;
use crate::entity::ai::path::PathType;
use crate::entity::conversion::{ConversionParams, convert_to};
use crate::entity::damage::DamageSource;
use crate::entity::entities::FrogEntity;
use crate::entity::mob::NavigationKind;
use crate::entity::{
    AgeableMobBase, ENTITIES, Entity, EntityBase, EntityBaseLoad, EntitySpawnReason,
    EntitySyncedData, LivingEntity, LivingEntityBase, Mob, MobBase, PathfinderMob, next_entity_id,
};
use crate::physics::MoveResult;
use crate::player::Player;
use crate::world::World;

use super::fish;

/// How long a tadpole takes to become a frog.
///
/// Vanilla parity: `Tadpole.ticksToBeFrog`, which is `Math.abs(-24000)`.
pub const TICKS_TO_BE_FROG: i32 = 24_000;

/// How far a fed tadpole's age jumps per second of feeding.
///
/// Vanilla parity: the `* 20` of `Tadpole.ageUp`.
const TICKS_PER_FED_SECOND: i32 = 20;

/// Vanilla parity: the `0.15F` volume of `Tadpole.ageUp`'s grow-up sound.
const GROW_UP_SOUND_VOLUME: f32 = 0.15;

/// Vanilla parity: `FrogspawnBlock.MIN_TADPOLES_SPAWN`.
pub const MIN_TADPOLES_SPAWN: i32 = 2;
/// Vanilla parity: `FrogspawnBlock.MAX_TADPOLES_SPAWN`, exclusive in the
/// `random.nextInt(2, 6)` the block rolls.
pub const MAX_TADPOLES_SPAWN_EXCLUSIVE: i32 = 6;

/// A tadpole.
#[entity_behavior(class = "Tadpole")]
pub struct TadpoleEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    brain: Brain,
    /// Vanilla `Tadpole.age`, a plain field rather than the `AgeableMob` clock:
    /// a tadpole is a fish and has no baby form.
    age: SyncMutex<i32>,
    entity_data: SyncMutex<TadpoleEntityData>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `TadpoleEntity`.
unsafe impl DowncastType for TadpoleEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/tadpole");
}

impl TadpoleEntity {
    /// Creates a tadpole at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a tadpole from saved base data.
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
        let mut entity_data = TadpoleEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            brain: tadpole_ai::make_brain(),
            age: SyncMutex::new(0),
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Returns vanilla `Tadpole.getAge`.
    #[must_use]
    pub fn age(&self) -> i32 {
        *self.age.lock()
    }

    /// Returns vanilla `Tadpole.isAgeLocked`.
    #[must_use]
    pub fn is_age_locked(&self) -> bool {
        *self.entity_data.lock().age_locked.get()
    }

    /// Sets vanilla `Tadpole.setAgeLocked`.
    pub fn set_age_locked(&self, locked: bool) {
        self.entity_data.lock().age_locked.set(locked);
    }

    /// Sets vanilla `Tadpole.setAge`, which is also where a tadpole notices it
    /// has run out of time and turns into a frog.
    pub fn set_age(&self, age: i32) {
        *self.age.lock() = age;
        if age >= TICKS_TO_BE_FROG {
            self.grow_up();
        }
    }

    /// Returns vanilla `Tadpole.getTicksLeftUntilAdult`.
    #[must_use]
    pub fn ticks_left_until_adult(&self) -> i32 {
        (TICKS_TO_BE_FROG - self.age()).max(0)
    }

    /// Vanilla parity: `Tadpole.ageUp`, the conversion into a frog.
    ///
    /// This is the far end of the frogspawn loop: the block hatches tadpoles and
    /// twenty minutes later each of them is a frog that can lay spawn of its own.
    fn grow_up(&self) {
        let Some(world) = self.level() else {
            return;
        };

        let converted = convert_to(
            self,
            ConversionParams::single(false, false),
            |id, position, level| FrogEntity::new(&vanilla_entities::FROG, id, position, level),
            |frog| {
                frog.finalize_spawn(&world, EntitySpawnReason::Conversion, None);
                frog.set_persistence_required();
            },
        );

        if converted.is_some() {
            self.play_sound(
                &sound_events::ENTITY_TADPOLE_GROW_UP,
                GROW_UP_SOUND_VOLUME,
                1.0,
            );
        }
    }

    /// Returns whether the stack is vanilla frog food.
    ///
    /// Vanilla parity: `Tadpole.isFood`, which reads `#minecraft:frog_food`
    /// rather than an `Animal.isFood` a fish does not have.
    #[must_use]
    pub fn is_food(item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::FROG_FOOD)
    }

    /// Vanilla parity: `Tadpole.feed`.
    ///
    /// The happy-villager puff vanilla adds here is a client-local
    /// `Level.addParticle`, so there is no server-side work for it.
    fn feed(&self, player: &Player, hand: InteractionHand) {
        Mob::use_player_item(self, player, hand);
        let speed_up =
            AgeableMobBase::get_speed_up_seconds_when_feeding(self.ticks_left_until_adult());
        self.set_age(self.age() + speed_up * TICKS_PER_FED_SECOND);
    }
}

impl Entity for TadpoleEntity {
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
        nbt.insert("Age", self.age());
        nbt.insert("AgeLocked", self.is_age_locked());
        self.brain.save(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        // The age is assigned directly rather than through `set_age`: a tadpole
        // loaded at or past its deadline must not convert while the chunk is
        // still being read in. Its next `ai_step` grows it up instead.
        *self.age.lock() = nbt.int("Age").unwrap_or(0);
        self.set_age_locked(nbt.byte("AgeLocked").is_some_and(|flag| flag != 0));
        self.brain.load(nbt);
    }
}

impl LivingEntity for TadpoleEntity {
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
        Some(&sound_events::ENTITY_TADPOLE_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_TADPOLE_DEATH)
    }

    /// Vanilla parity: `Tadpole.shouldDropExperience`, which is `false` -- a
    /// tadpole is worth nothing, unlike every other fish.
    fn should_drop_experience(&self) -> bool {
        false
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    /// Vanilla parity: `Tadpole.aiStep`, whose only addition to the fish tick is
    /// the clock that eventually makes it a frog.
    fn ai_step(&self) -> Option<MoveResult> {
        fish::flop(self, &sound_events::ENTITY_TADPOLE_FLOP);
        let result = self.default_ai_step();

        if !self.is_age_locked() {
            self.set_age(self.age() + 1);
        }

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

impl Mob for TadpoleEntity {
    /// Vanilla parity: `Mob.serverAiStep` ticks the goal selector for every
    /// mob it runs, brain-driven or not. `Mob::tick_goal_selectors` has an
    /// empty default, so leaving it out is how a registered goal set never
    /// runs.
    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

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

    /// Vanilla parity: `Tadpole.customServerAiStep`, which is the brain tick and
    /// nothing else -- a tadpole has no goal selector at all.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.brain.tick(&world, self);
        tadpole_ai::update_activity(&self.brain);
    }

    /// Vanilla parity: `Tadpole.getAmbientSound`, which is null.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        None
    }

    fn ambient_sound_interval(&self) -> i32 {
        fish::AMBIENT_SOUND_INTERVAL
    }

    /// Vanilla parity: `Tadpole.fromBucket` is a hard `true`, so a tadpole never
    /// despawns -- vanilla treats every one of them as if it had been released
    /// from a bucket.
    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        false
    }

    /// Vanilla parity: `AbstractFish.FishMoveControl.tick`.
    fn tick_move_control(&self) {
        fish::tick_move_control(self);
    }

    /// Vanilla parity: `Tadpole.mobInteract`. Feeding hurries it toward being a
    /// frog.
    ///
    /// MISSING FOUNDATION: vanilla then falls through to
    /// `Bucketable.bucketMobPickup` and to the golden-dandelion age lock. Foton's
    /// `MobBucketItem` does not carry a tadpole yet, so a water bucket used on
    /// one does nothing rather than picking it up.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let item_stack = {
            let inventory = player.inventory.lock();
            let item_stack = inventory.get_item_in_hand(hand);
            item_stack.copy_with_count(item_stack.count())
        };

        if Self::is_food(&item_stack) && !self.is_age_locked() {
            self.feed(player, hand);
            return InteractionResult::Success;
        }

        InteractionResult::Pass
    }
}

impl PathfinderMob for TadpoleEntity {
    /// Vanilla parity: `Tadpole.createNavigation` returns a
    /// `WaterBoundPathNavigation`; a tadpole never breaches.
    fn navigation_kind(&self) -> NavigationKind {
        NavigationKind::WaterBound {
            allow_breaching: false,
        }
    }
}

/// Spawns the tadpoles a hatching frogspawn block releases.
///
/// Vanilla parity: `FrogspawnBlock.spawnTadpoles`, which lives here rather than
/// in the block so the entity owns its own construction.
pub fn spawn_tadpoles_from_frogspawn(world: &Arc<World>, pos: BlockPos) {
    let count = rand::random_range(MIN_TADPOLES_SPAWN..MAX_TADPOLES_SPAWN_EXCLUSIVE);

    for _ in 0..count {
        let x = f64::from(pos.x()) + random_tadpole_position_offset();
        let z = f64::from(pos.z()) + random_tadpole_position_offset();
        let y = f64::from(pos.y()) - 0.5;

        let Some(entity) = ENTITIES.create(
            &vanilla_entities::TADPOLE,
            next_entity_id(),
            DVec3::new(x, y, z),
            Arc::downgrade(world),
        ) else {
            return;
        };

        // Vanilla parity: the `random.nextInt(1, 361)` degrees of yaw.
        entity.set_rotation((rand::random_range(1..361) as f32, 0.0));
        entity.set_old_position_to_current();
        if let Some(mob) = entity.as_mob() {
            mob.set_persistence_required();
        }
        let _added = world.try_add_entity(entity);
    }
}

/// Vanilla parity: `FrogspawnBlock.getRandomTadpolePositionOffset`, which keeps
/// a hatchling's hitbox inside the block it came out of.
fn random_tadpole_position_offset() -> f64 {
    rand::random::<f64>().clamp(0.2, 0.799_999_997_019_767_8)
}

#[cfg(test)]
mod tests;
