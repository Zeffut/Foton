//! Turtle entity.
//!
//! Vanilla parity: `Turtle`. A turtle is built around one place: the beach it
//! hatched on. It swims wherever it likes, but a pregnant turtle walks back to
//! that beach, digs, and lays the eggs the next generation hatches from. Almost
//! every goal here exists to serve that round trip, and the egg it lays is the
//! `TurtleEggBlock` whose hatch path spawns turtles again.

use std::f64::consts::{FRAC_PI_2, PI};
use std::sync::{Arc, Weak};

use glam::{DVec3, IVec2};
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, IntProperty};
use steel_registry::entity_type::{
    EntityAttachmentPoint, EntityAttachments, EntityDimensions, EntityTypeRef,
};
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::TurtleEntityData;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    REGISTRY, TaggedRegistryExt as _, level_events, sound_events, vanilla_attributes,
    vanilla_blocks, vanilla_damage_types, vanilla_game_events, vanilla_game_rules,
    vanilla_loot_tables,
};
use steel_utils::locks::SyncMutex;
use steel_utils::types::{InteractionHand, UpdateFlags};
use steel_utils::{BlockPos, BlockStateId, ChunkPos, Downcast as _, DowncastType, DowncastTypeKey};

use crate::advancement::triggers;
use crate::behavior::InteractionResult;
use crate::behavior::blocks::TurtleEggBlock;
use crate::entity::ai::control::MoveControlOperation;
use crate::entity::ai::goal::{
    BreedGoal, Goal, GoalControls, LookAtPlayerGoal, MoveToBlockGoal, PanicGoal, RandomStrollGoal,
    TemptGoal, block_pos_corner, default_random_pos_towards, look_for_water,
};
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::entities::ExperienceOrbEntity;
use crate::entity::living_entity::gift_loot_items_with_rng;
use crate::entity::mob::{NavigationKind, rotlerp};
use crate::entity::{
    AgeableMob, AgeableMobBase, Animal, AnimalBase, Entity, EntityBase, EntityBaseLoad, EntityPose,
    EntitySpawnReason, EntitySyncedData, LivingEntity, LivingEntityBase, LivingEntitySyncedData,
    Mob, MobBase, MoveResult, PathfinderMob, SpawnGroupData,
};
use crate::fluid::FluidStateExt as _;
use crate::physics::MoverType;
use crate::player::Player;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader, World};

/// Vanilla `Turtle.BABY_SCALE`.
const BABY_SCALE: f32 = 0.3;

/// The passenger attachment vanilla builds `BABY_DIMENSIONS` with.
///
/// Vanilla parity: `attach(PASSENGER, 0.0F, EntityTypes.TURTLE.getHeight(), -0.25F)`,
/// where the height of an adult turtle is `0.4`.
const BABY_PASSENGER_ATTACHMENTS: [EntityAttachmentPoint; 1] =
    [EntityAttachmentPoint::new(0.0, 0.4, -0.25)];

/// Vanilla `Turtle.getAmbientSoundInterval`.
const AMBIENT_SOUND_INTERVAL: i32 = 200;

/// Speed a panicking turtle flees at.
const PANIC_SPEED_MOD: f64 = 1.2;
/// How far a panicking turtle looks for water.
///
/// Vanilla parity: the `7` of `Turtle.TurtlePanicGoal.canUse`, wider than the
/// `5` every other panicking mob uses.
const PANIC_WATER_SEARCH: i32 = 7;
/// Speed a turtle courts, lays and travels at.
const BREED_SPEED_MOD: f64 = 1.0;
/// Speed a tempted turtle follows at.
const TEMPT_SPEED_MOD: f64 = 1.1;
// Vanilla's `TurtleGoToWaterGoal` constructor reads
// `turtle.isBaby() ? 2.0 : speedModifier`, but goals are registered from the
// `Mob` constructor, before either `finalizeSpawn` or the saved age can make
// the turtle a baby. The `2.0` branch is therefore unreachable in vanilla too,
// so this goal is always built at the plain speed modifier.

/// Vanilla `Turtle.TurtleLayEggGoal`'s search range.
const LAY_EGG_SEARCH_RANGE: i32 = 16;
/// How close to home a turtle must be before it will start digging.
const LAY_EGG_HOME_RANGE: f64 = 9.0;
/// Ticks of digging before the eggs appear.
const LAY_EGG_DURATION_TICKS: i32 = 200;
/// How often the digging turtle kicks up sand.
const LAY_EGG_PARTICLE_INTERVAL: i32 = 5;
/// Love time a turtle is left with after laying.
const LAY_EGG_LOVE_TIME: i32 = 600;

/// Vanilla `Turtle.TurtleGoToWaterGoal`'s search range.
const GO_TO_WATER_SEARCH_RANGE: i32 = 24;
/// Vanilla `Turtle.TurtleGoToWaterGoal.GIVE_UP_TICKS`.
const GO_TO_WATER_GIVE_UP_TICKS: i32 = 1200;
/// How often the water-seeking turtle repaths.
const GO_TO_WATER_RECALCULATE_INTERVAL: i32 = 160;

/// Vanilla `Turtle.TurtleGoHomeGoal.GIVE_UP_TICKS`.
const GO_HOME_GIVE_UP_TICKS: i32 = 600;
/// How often an idle turtle rolls to check whether it has strayed.
const GO_HOME_ROLL_TICKS: i32 = 700;
/// How far a turtle may stray before it heads home.
const GO_HOME_STRAY_RANGE: f64 = 64.0;
/// How close counts as home for the purpose of stopping.
const GO_HOME_ARRIVAL_RANGE: f64 = 7.0;
/// How close counts as home for the purpose of the give-up timer.
const GO_HOME_NEARLY_THERE_RANGE: f64 = 16.0;

/// Vanilla `Turtle.TurtleRandomStrollGoal`'s interval.
const STROLL_INTERVAL_TICKS: i32 = 100;

/// How far a traveling turtle picks a destination.
///
/// Vanilla parity: the `random.nextInt(1025) - 512` of `TurtleTravelGoal.start`.
const TRAVEL_XZ_RANGE: i32 = 512;
/// How far up or down that destination may be.
const TRAVEL_Y_RANGE: i32 = 4;
/// How many blocks around the next step must be loaded.
const TRAVEL_CHUNK_MARGIN: i32 = 34;

/// Fraction of its speed a swimming turtle keeps each tick.
///
/// Vanilla parity: the `scale(0.9)` of `Turtle.travelInWater`.
const SWIM_DRAG: f64 = 0.9;
/// How hard a turtle pushes itself through the water.
const SWIM_ACCELERATION: f32 = 0.1;
/// Downward drift a turtle with nowhere to be settles into.
const IDLE_SINK: f64 = -0.005;
/// How close to home a turtle heading home must be to stop sinking.
const HOME_NO_SINK_RANGE: f64 = 20.0;
/// Upward nudge a submerged turtle gets every tick.
const SUBMERGED_LIFT: f64 = 0.005;
/// Beyond this distance from home a swimming turtle halves its speed.
const AWAY_FROM_HOME_SLOWDOWN_RANGE: f64 = 16.0;
/// Floor the halved swimming speed cannot go below.
const SWIM_SPEED_FLOOR: f32 = 0.08;
/// Floor a baby's thirded swimming speed cannot go below.
const BABY_SWIM_SPEED_FLOOR: f32 = 0.06;
/// Floor the halved walking speed cannot go below.
const WALK_SPEED_FLOOR: f32 = 0.06;
/// How fast the turtle converges on its wanted speed.
const SPEED_LERP: f32 = 0.125;
/// Share of its speed a turtle converts into vertical movement.
const VERTICAL_STEER: f64 = 0.1;
/// Degrees a turtle may turn toward its heading in one tick.
const TURN_RATE: f32 = 90.0;

/// Vanilla `Turtle.nextStep`, which is why a turtle's steps are so frequent.
const STEP_DISTANCE: f32 = 0.15;

/// Vanilla `Turtle.playSwimSound` multiplier.
const SWIM_SOUND_VOLUME_MULTIPLIER: f32 = 1.5;

/// How high above sea level a turtle will still spawn.
///
/// Vanilla parity: the `+ 4` of `Turtle.checkTurtleSpawnRules`.
const SPAWN_HEIGHT_ABOVE_SEA_LEVEL: i32 = 4;

/// The egg-count property of the block a turtle lays.
const EGGS: &IntProperty = &BlockStateProperties::EGGS;
/// Vanilla `TurtleEggBlock.MAX_EGGS`, the top of `random.nextInt(4) + 1`.
const MAX_EGGS: u8 = 4;

/// Runtime turtle fields vanilla keeps on the entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TurtleState {
    /// Vanilla `Turtle.homePos`.
    home_pos: BlockPos,
    /// Vanilla `Turtle.travelPos`.
    travel_pos: Option<BlockPos>,
    /// Vanilla `Turtle.goingHome`.
    going_home: bool,
    /// Vanilla `Turtle.layEggCounter`.
    lay_egg_counter: i32,
}

impl TurtleState {
    const fn new() -> Self {
        Self {
            home_pos: BlockPos::ZERO,
            travel_pos: None,
            going_home: false,
            lay_egg_counter: 0,
        }
    }
}

/// A turtle.
#[entity_behavior(class = "Turtle")]
pub struct TurtleEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    state: SyncMutex<TurtleState>,
    entity_data: SyncMutex<TurtleEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `TurtleEntity`.
unsafe impl DowncastType for TurtleEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/turtle");
}

impl TurtleEntity {
    /// Creates a turtle at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a turtle from saved base data.
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
            // Vanilla parity: the four `setPathfindingMalus` calls of the
            // `Turtle` constructor. Open water is free; every door is a wall.
            let mut malus = mob_base.pathfinding_malus().lock();
            malus.set(PathType::Water, 0.0);
            malus.set(PathType::DoorIronClosed, -1.0);
            malus.set(PathType::DoorWoodClosed, -1.0);
            malus.set(PathType::DoorOpen, -1.0);
        }
        let mut entity_data = TurtleEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla parity: `Turtle.registerGoals`.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(0, TurtlePanicGoal::new(PANIC_SPEED_MOD));
            goals.add_goal(1, TurtleBreedGoal::new(BREED_SPEED_MOD));
            goals.add_goal(1, TurtleLayEggGoal::new(BREED_SPEED_MOD));
            goals.add_goal(
                2,
                TemptGoal::new(
                    TEMPT_SPEED_MOD,
                    |item_stack| {
                        REGISTRY
                            .items
                            .is_in_tag(item_stack.item(), &ItemTag::TURTLE_FOOD)
                    },
                    false,
                ),
            );
            goals.add_goal(3, TurtleGoToWaterGoal::new(BREED_SPEED_MOD));
            goals.add_goal(4, TurtleGoHomeGoal::new(BREED_SPEED_MOD));
            goals.add_goal(7, TurtleTravelGoal::new(BREED_SPEED_MOD));
            goals.add_goal(8, LookAtPlayerGoal::new(8.0));
            goals.add_goal(
                9,
                TurtleRandomStrollGoal::new(BREED_SPEED_MOD, STROLL_INTERVAL_TICKS),
            );
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            state: SyncMutex::new(TurtleState::new()),
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Returns vanilla `Turtle.getHomePos`.
    #[must_use]
    pub fn home_pos(&self) -> BlockPos {
        self.state.lock().home_pos
    }

    /// Sets vanilla `Turtle.setHomePos`.
    pub fn set_home_pos(&self, pos: BlockPos) {
        self.state.lock().home_pos = pos;
    }

    /// Returns vanilla `Turtle.hasEgg`.
    #[must_use]
    pub fn has_egg(&self) -> bool {
        *self.entity_data.lock().has_egg.get()
    }

    /// Sets vanilla `Turtle.setHasEgg`.
    pub fn set_has_egg(&self, has_egg: bool) {
        self.entity_data.lock().has_egg.set(has_egg);
    }

    /// Returns vanilla `Turtle.isLayingEgg`.
    #[must_use]
    pub fn is_laying_egg(&self) -> bool {
        *self.entity_data.lock().laying_egg.get()
    }

    /// Sets vanilla `Turtle.setLayingEgg`, which also resets the dig timer.
    fn set_laying_egg(&self, laying_egg: bool) {
        self.state.lock().lay_egg_counter = i32::from(laying_egg);
        self.entity_data.lock().laying_egg.set(laying_egg);
    }

    /// Returns vanilla `Turtle.goingHome`.
    #[must_use]
    fn is_going_home(&self) -> bool {
        self.state.lock().going_home
    }

    /// Returns whether the stack is vanilla turtle food.
    #[must_use]
    pub fn is_food(item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::TURTLE_FOOD)
    }

    /// Vanilla parity: `Turtle.checkTurtleSpawnRules`.
    #[must_use]
    pub fn check_turtle_spawn_rules(world: &Arc<World>, pos: BlockPos) -> bool {
        pos.y() < world.sea_level + SPAWN_HEIGHT_ABOVE_SEA_LEVEL
            && TurtleEggBlock::on_sand(world.as_ref(), pos)
            && <Self as Animal>::is_bright_enough_to_spawn(world.as_ref(), pos)
    }

    /// Vanilla parity: `Turtle.TurtleMoveControl.updateSpeed`.
    fn update_swim_speed(&self) {
        if self.is_in_water() {
            self.set_velocity(self.velocity() + DVec3::new(0.0, SUBMERGED_LIFT, 0.0));
            if !block_closer_to_center_than(
                self.home_pos(),
                self.position(),
                AWAY_FROM_HOME_SLOWDOWN_RANGE,
            ) {
                self.set_mob_speed((self.get_speed() / 2.0).max(SWIM_SPEED_FLOOR));
            }

            if AgeableMob::is_baby(self) {
                self.set_mob_speed((self.get_speed() / 3.0).max(BABY_SWIM_SPEED_FLOOR));
            }
        } else if self.on_ground() {
            self.set_mob_speed((self.get_speed() / 2.0).max(WALK_SPEED_FLOOR));
        }
    }

    /// Drops the scute a turtle leaves behind when it finishes growing up.
    ///
    /// Vanilla parity: the `dropFromGiftLootTable(TURTLE_GROW)` of
    /// `Turtle.ageBoundaryReached`.
    fn drop_grow_gift(&self) {
        let Some(world) = self.level() else {
            return;
        };
        if !world.get_game_rule(&vanilla_game_rules::MOB_DROPS) {
            return;
        }

        let mut rng = rand::rng();
        let drops =
            gift_loot_items_with_rng(self, &vanilla_loot_tables::GAMEPLAY_TURTLE_GROW, &mut rng);
        for drop in drops {
            self.spawn_at_location(drop, 0.0);
        }
    }
}

impl Entity for TurtleEntity {
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
            baby_dimensions(self.entity_type).scale(scale)
        } else if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `Turtle.isPushedByFluid`; a turtle holds its line in a
    /// current that would sweep any other animal away.
    fn is_pushed_by_fluid(&self) -> bool {
        false
    }

    /// Vanilla parity: `Turtle.nextStep`.
    fn next_step(&self) -> f32 {
        self.base().movement_progress().move_dist() + STEP_DISTANCE
    }

    /// Vanilla parity: `Turtle.getSwimSound`.
    fn swim_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_TURTLE_SWIM
    }

    /// Vanilla parity: `Turtle.playSwimSound`, which is half again as loud as
    /// any other swimmer's.
    fn play_swim_sound(&self, volume: f32) {
        let volume = volume * SWIM_SOUND_VOLUME_MULTIPLIER;
        let pitch = 1.0 + (rand::random::<f32>() - rand::random::<f32>()) * 0.4;
        self.play_sound(self.swim_sound(), volume, pitch);
    }

    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        let sound = if AgeableMob::is_baby(self) {
            &sound_events::ENTITY_TURTLE_SHAMBLE_BABY
        } else {
            &sound_events::ENTITY_TURTLE_SHAMBLE
        };
        self.play_sound(sound, 0.15, 1.0);
    }

    /// Vanilla parity: `Turtle.thunderHit`, which kills the turtle outright.
    fn thunder_hit(&self, world: &World, _bolt: &dyn Entity) {
        self.hurt_server(
            world,
            &DamageSource::environment(&vanilla_damage_types::LIGHTNING_BOLT),
            f32::MAX,
        );
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        let home_pos = self.home_pos();
        nbt.insert(
            "home_pos",
            NbtTag::IntArray(vec![home_pos.x(), home_pos.y(), home_pos.z()]),
        );
        nbt.insert("has_egg", self.has_egg());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        // Vanilla reads `home_pos` before calling super so a turtle without the
        // field falls back to wherever it happens to be standing.
        let home_pos = nbt
            .int_array("home_pos")
            .and_then(|values| {
                let [x, y, z] = values[..] else { return None };
                Some(BlockPos::new(x, y, z))
            })
            .unwrap_or_else(|| self.block_position());
        self.set_home_pos(home_pos);

        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        self.set_has_egg(nbt.byte("has_egg").is_some_and(|flag| flag != 0));
    }
}

impl LivingEntity for TurtleEntity {
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

    /// Vanilla parity: `Turtle.getAgeScale`.
    fn get_age_scale(&self) -> f32 {
        if AgeableMob::is_baby(self) {
            BABY_SCALE
        } else {
            1.0
        }
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(if AgeableMob::is_baby(self) {
            &sound_events::ENTITY_TURTLE_HURT_BABY
        } else {
            &sound_events::ENTITY_TURTLE_HURT
        })
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(if AgeableMob::is_baby(self) {
            &sound_events::ENTITY_TURTLE_DEATH_BABY
        } else {
            &sound_events::ENTITY_TURTLE_DEATH
        })
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    /// Vanilla parity: `Turtle.aiStep`, whose only addition is the sand a
    /// digging turtle kicks up every five ticks.
    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);

        let lay_egg_counter = self.state.lock().lay_egg_counter;
        if !Entity::is_alive(self)
            || !self.is_laying_egg()
            || lay_egg_counter < 1
            || lay_egg_counter % LAY_EGG_PARTICLE_INTERVAL != 0
        {
            return result;
        }

        let Some(world) = self.level() else {
            return result;
        };
        let pos = self.block_position();
        if TurtleEggBlock::on_sand(world.as_ref(), pos) {
            let sand = world.get_block_state(pos.below());
            world.level_event(
                level_events::PARTICLES_DESTROY_BLOCK,
                pos,
                i32::from(sand.0),
                None,
            );
            world.game_event(
                &vanilla_game_events::ENTITY_ACTION,
                pos,
                &GameEventContext::new(Some(self), None),
            );
        }

        result
    }

    /// Vanilla parity: `Turtle.travelInWater`. A turtle that is nearly home
    /// stops sinking so it can hold its depth over the beach.
    fn travel_in_water(
        &self,
        input: DVec3,
        _base_gravity: f64,
        _is_falling: bool,
        _old_y: f64,
    ) -> Option<MoveResult> {
        self.move_relative(SWIM_ACCELERATION, input);
        let result = self.move_entity(MoverType::SelfMovement, self.velocity());
        self.set_velocity(self.velocity() * SWIM_DRAG);

        let holding_position = self.is_going_home()
            && block_closer_to_center_than(self.home_pos(), self.position(), HOME_NO_SINK_RANGE);
        if self.target().is_none() && !holding_position {
            self.set_velocity(self.velocity() + DVec3::new(0.0, IDLE_SINK, 0.0));
        }

        result
    }
}

impl AgeableMob for TurtleEntity {
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

    /// Vanilla parity: `Turtle.ageBoundaryReached`, which is where the scute
    /// comes from: a turtle sheds it the moment it stops being a baby.
    fn age_boundary_changed(&self, baby: bool) {
        self.refresh_dimensions();
        if !baby {
            self.drop_grow_gift();
        }
    }
}

impl Animal for TurtleEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        TurtleEntity::is_food(item_stack)
    }

    /// Vanilla parity: `Turtle.canFallInLove`; a turtle already carrying an egg
    /// will not court again until it has laid it.
    fn can_fall_in_love(&self) -> bool {
        self.in_love_time() <= 0 && !self.has_egg()
    }
}

impl Mob for TurtleEntity {
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

    /// Vanilla parity: `Turtle.getAmbientSound`. Only a grown turtle out of the
    /// water has anything to say.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        (!self.is_in_water() && self.on_ground() && !AgeableMob::is_baby(self))
            .then_some(&sound_events::ENTITY_TURTLE_AMBIENT_LAND)
    }

    fn ambient_sound_interval(&self) -> i32 {
        AMBIENT_SOUND_INTERVAL
    }

    /// Vanilla parity: `Turtle.canBeLeashed`.
    fn can_be_leashed(&self) -> bool {
        false
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        let _ = spawn_reason;
        Self::check_turtle_spawn_rules(world, pos)
    }

    /// Vanilla parity: `Turtle.finalizeSpawn`; where a turtle first appears is
    /// the beach it will come back to.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.set_home_pos(self.block_position());
        self.finalize_spawn_ageable_mob(world, spawn_reason, group_data)
    }

    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        Animal::mob_interact_animal(self, player, hand)
    }

    /// Vanilla parity: `Turtle.TurtleMoveControl.tick`, which replaces the
    /// shared move control outright: a turtle steers in three dimensions and
    /// slows itself down both in the water and on the sand.
    fn tick_move_control(&self) {
        self.update_swim_speed();

        let (operation, wanted_position, speed_modifier) = {
            let controls = self.mob_base().controls().lock();
            let move_control = controls.move_control;
            (
                move_control.operation(),
                move_control.wanted_position(),
                move_control.speed_modifier(),
            )
        };

        let navigating = matches!(operation, MoveControlOperation::MoveTo)
            && !self.mob_base().navigation().lock().is_done();
        if !navigating {
            self.set_mob_speed(0.0);
            return;
        }

        let delta = wanted_position - self.position();
        let distance = delta.length();
        if distance < 1.0e-5 {
            self.set_mob_speed(0.0);
            return;
        }

        let wanted_yaw = (delta.z.atan2(delta.x).to_degrees() as f32) - 90.0;
        let (yaw, pitch) = self.rotation();
        let turned = rotlerp(yaw, wanted_yaw, TURN_RATE);
        self.set_rotation((turned, pitch));
        self.set_y_body_rot(turned);

        let movement_speed = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MOVEMENT_SPEED);
        let target_speed = (speed_modifier * movement_speed) as f32;
        let current_speed = self.get_speed();
        let speed = SPEED_LERP.mul_add(target_speed - current_speed, current_speed);
        self.set_mob_speed(speed);

        let lift = f64::from(speed) * (delta.y / distance) * VERTICAL_STEER;
        self.set_velocity(self.velocity() + DVec3::new(0.0, lift, 0.0));
    }
}

impl PathfinderMob for TurtleEntity {
    /// Navigates as a swimmer while in water and as a walker on land.
    ///
    /// Vanilla parity: `Turtle.TurtlePathNavigation`, which extends
    /// `AmphibiousPathNavigation`. Steel answers this per path request, the
    /// same seam the drowned uses.
    fn navigation_kind(&self) -> NavigationKind {
        if self.is_in_water() {
            NavigationKind::WaterBound {
                allow_breaching: false,
            }
        } else {
            NavigationKind::Ground
        }
    }

    /// Vanilla parity: `Turtle.TurtlePathNavigation.isStableDestination`. A
    /// turtle crossing the ocean only accepts water; one going anywhere else
    /// wants something solid under the node.
    fn is_stable_destination(&self, pos: BlockPos) -> bool {
        let Some(world) = self.level() else {
            return false;
        };

        if self.state.lock().travel_pos.is_some() {
            return world.get_block_state(pos).get_block() == &vanilla_blocks::WATER;
        }

        !world.get_block_state(pos.below()).is_air()
    }

    /// Vanilla parity: `Turtle.getWalkTargetValue`, which makes open water and
    /// sand equally attractive and everything else merely lit.
    fn get_walk_target_value(&self, pos: BlockPos) -> f32 {
        let Some(world) = self.level() else {
            return 0.0;
        };

        // Vanilla asks the fluid state here, not the block, so a waterlogged
        // block counts as water to a turtle looking for a way out to sea.
        if !self.is_going_home() && world.get_block_state(pos).get_fluid_state().is_water() {
            return 10.0;
        }

        if TurtleEggBlock::on_sand(world.as_ref(), pos) {
            return 10.0;
        }

        world.pathfinding_cost_from_light_levels(pos)
    }
}

/// Vanilla `Turtle.BABY_DIMENSIONS`.
fn baby_dimensions(entity_type: EntityTypeRef) -> EntityDimensions {
    EntityDimensions::new_with_attachments(
        entity_type.dimensions.width,
        entity_type.dimensions.height,
        entity_type.dimensions.eye_height,
        EntityAttachments::new(&BABY_PASSENGER_ATTACHMENTS, &[], &[], &[]),
    )
    .scale(BABY_SCALE)
}

fn block_closer_to_center_than(pos: BlockPos, position: DVec3, distance: f64) -> bool {
    let (x, y, z) = pos.get_center();
    DVec3::new(x, y, z).distance_squared(position) < distance * distance
}

fn turtle_of(mob: &dyn PathfinderMob) -> Option<&TurtleEntity> {
    mob.downcast_ref::<TurtleEntity>()
}

/// Runs for water first and only then for anywhere else.
///
/// Vanilla parity: `Turtle.TurtlePanicGoal`, which searches wider than the base
/// goal and does not need to be on fire to want the sea.
struct TurtlePanicGoal {
    inner: PanicGoal,
}

impl TurtlePanicGoal {
    const fn new(speed_modifier: f64) -> Self {
        Self {
            inner: PanicGoal::new(speed_modifier),
        }
    }
}

impl Goal for TurtlePanicGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn is_panic_goal(&self) -> bool {
        true
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if !self.inner.should_panic(mob) {
            return false;
        }

        if let Some(water_pos) = look_for_water(mob, PANIC_WATER_SEARCH) {
            self.inner.set_wanted_position(block_pos_corner(water_pos));
            return true;
        }

        self.inner.find_random_position(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.inner.stop(mob);
    }
}

/// Ends in an egg the turtle carries away, not in a hatchling.
///
/// Vanilla parity: `Turtle.TurtleBreedGoal`.
struct TurtleBreedGoal {
    inner: BreedGoal,
}

impl TurtleBreedGoal {
    fn new(speed_modifier: f64) -> Self {
        Self {
            inner: BreedGoal::with_breed_action(speed_modifier, |world, animal, partner| {
                // Vanilla's `Turtle.TurtleBreedGoal.breed` fires this itself
                // rather than going through `finalizeSpawnChildFromBreeding`,
                // and passes a null offspring: what a turtle leaves behind is
                // an egg, not a hatchling.
                // TODO: award the animals-bred stat here too, once Steel has one.
                if let Some(cause) = animal
                    .love_cause_uuid()
                    .or_else(|| partner.love_cause_uuid())
                    && let Some(entity) = world.get_entity_by_uuid(&cause)
                    && let Some(player) = entity.as_player()
                {
                    triggers::entity::bred_animals(
                        player,
                        animal.as_entity_event_source(),
                        partner.as_entity_event_source(),
                        None,
                    );
                }
                if let Some(turtle) = animal.downcast_ref::<TurtleEntity>() {
                    turtle.set_has_egg(true);
                }
                animal.set_age(PARENT_AGE_AFTER_BREEDING);
                partner.set_age(PARENT_AGE_AFTER_BREEDING);
                animal.reset_love();
                partner.reset_love();

                if world.get_game_rule(&vanilla_game_rules::MOB_DROPS) {
                    let xp = rand::random_range(0..7) + 1;
                    ExperienceOrbEntity::award(world, animal.position(), xp);
                }
            }),
        }
    }
}

/// Vanilla `Animal.PARENT_AGE_AFTER_BREEDING`.
const PARENT_AGE_AFTER_BREEDING: i32 = 6000;

impl Goal for TurtleBreedGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let has_egg = turtle_of(mob).is_some_and(TurtleEntity::has_egg);
        !has_egg && self.inner.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.inner.tick(mob);
    }
}

/// Walks to a patch of sand near home and digs a clutch of eggs into it.
///
/// Vanilla parity: `Turtle.TurtleLayEggGoal`.
struct TurtleLayEggGoal {
    inner: MoveToBlockGoal,
}

impl TurtleLayEggGoal {
    fn new(speed_modifier: f64) -> Self {
        Self {
            inner: MoveToBlockGoal::new(speed_modifier, LAY_EGG_SEARCH_RANGE, |level, pos| {
                level.get_block_state(pos.above()).is_air() && TurtleEggBlock::is_sand(level, pos)
            }),
        }
    }

    fn is_near_home(mob: &dyn PathfinderMob) -> bool {
        turtle_of(mob).is_some_and(|turtle| {
            block_closer_to_center_than(turtle.home_pos(), turtle.position(), LAY_EGG_HOME_RANGE)
        })
    }

    /// Vanilla parity: the egg placement in `TurtleLayEggGoal.tick`.
    fn lay_eggs(&self, turtle: &TurtleEntity, world: &Arc<World>) {
        let egg_pos = self.inner.block_pos().above();
        world.play_sound(
            &sound_events::ENTITY_TURTLE_LAY_EGG,
            SoundSource::Blocks,
            turtle.block_position(),
            0.3,
            0.9 + rand::random::<f32>() * 0.2,
            None,
        );

        let eggs = rand::random_range(0..MAX_EGGS) + 1;
        let egg_state = vanilla_blocks::TURTLE_EGG
            .default_state()
            .set_value(EGGS, eggs);
        world.set_block(
            egg_pos,
            egg_state,
            UpdateFlags::UPDATE_CLIENTS | UpdateFlags::UPDATE_NEIGHBORS,
        );
        world.game_event(
            &vanilla_game_events::BLOCK_PLACE,
            egg_pos,
            &GameEventContext::new(Some(turtle), Some(egg_state)),
        );

        turtle.set_has_egg(false);
        turtle.set_laying_egg(false);
        turtle.set_in_love_time(LAY_EGG_LOVE_TIME);
    }
}

impl Goal for TurtleLayEggGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let has_egg = turtle_of(mob).is_some_and(TurtleEntity::has_egg);
        has_egg && Self::is_near_home(mob) && self.inner.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let has_egg = turtle_of(mob).is_some_and(TurtleEntity::has_egg);
        self.inner.can_continue_to_use(mob) && has_egg && Self::is_near_home(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.inner.tick(mob);

        let Some(turtle) = turtle_of(mob) else {
            return;
        };
        if turtle.is_in_water() || !self.inner.is_reached_target() {
            return;
        }

        let lay_egg_counter = turtle.state.lock().lay_egg_counter;
        if lay_egg_counter < 1 {
            turtle.set_laying_egg(true);
        } else if lay_egg_counter > LAY_EGG_DURATION_TICKS
            && let Some(world) = turtle.level()
        {
            self.lay_eggs(turtle, &world);
        }

        if turtle.is_laying_egg() {
            turtle.state.lock().lay_egg_counter += 1;
        }
    }
}

/// Heads for the nearest water.
///
/// Vanilla parity: `Turtle.TurtleGoToWaterGoal`.
struct TurtleGoToWaterGoal {
    inner: MoveToBlockGoal,
}

impl TurtleGoToWaterGoal {
    fn new(speed_modifier: f64) -> Self {
        Self {
            inner: MoveToBlockGoal::new(speed_modifier, GO_TO_WATER_SEARCH_RANGE, |level, pos| {
                level.get_block_state(pos).get_block() == &vanilla_blocks::WATER
            })
            .with_vertical_search_start(-1)
            .with_recalculate_path_interval(GO_TO_WATER_RECALCULATE_INTERVAL),
        }
    }
}

impl Goal for TurtleGoToWaterGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(turtle) = turtle_of(mob) else {
            return false;
        };

        if AgeableMob::is_baby(turtle) && !turtle.is_in_water() {
            return self.inner.can_use(mob);
        }

        !turtle.is_going_home()
            && !turtle.is_in_water()
            && !turtle.has_egg()
            && self.inner.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(turtle) = turtle_of(mob) else {
            return false;
        };
        let Some(world) = turtle.level() else {
            return false;
        };

        !turtle.is_in_water()
            && self.inner.try_ticks() <= GO_TO_WATER_GIVE_UP_TICKS
            && world.get_block_state(self.inner.block_pos()).get_block() == &vanilla_blocks::WATER
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.inner.tick(mob);
    }
}

/// Walks the turtle back to the beach it hatched on.
///
/// Vanilla parity: `Turtle.TurtleGoHomeGoal`. A turtle carrying an egg starts
/// this immediately; one that is merely far from home rolls for it.
struct TurtleGoHomeGoal {
    speed_modifier: f64,
    stuck: bool,
    close_to_home_try_ticks: i32,
}

impl TurtleGoHomeGoal {
    const fn new(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            stuck: false,
            close_to_home_try_ticks: 0,
        }
    }
}

impl Goal for TurtleGoHomeGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(turtle) = turtle_of(mob) else {
            return false;
        };
        if AgeableMob::is_baby(turtle) {
            return false;
        }
        if turtle.has_egg() {
            return true;
        }
        if rand::random_range(0..reduced_tick_delay(GO_HOME_ROLL_TICKS)) != 0 {
            return false;
        }

        !block_closer_to_center_than(turtle.home_pos(), turtle.position(), GO_HOME_STRAY_RANGE)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(turtle) = turtle_of(mob) else {
            return false;
        };

        !block_closer_to_center_than(turtle.home_pos(), turtle.position(), GO_HOME_ARRIVAL_RANGE)
            && !self.stuck
            && self.close_to_home_try_ticks <= reduced_tick_delay(GO_HOME_GIVE_UP_TICKS)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        if let Some(turtle) = turtle_of(mob) {
            turtle.state.lock().going_home = true;
        }
        self.stuck = false;
        self.close_to_home_try_ticks = 0;
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        if let Some(turtle) = turtle_of(mob) {
            turtle.state.lock().going_home = false;
        }
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(turtle) = turtle_of(mob) else {
            return;
        };
        let home_pos = turtle.home_pos();
        let close_to_home =
            block_closer_to_center_than(home_pos, turtle.position(), GO_HOME_NEARLY_THERE_RANGE);
        if close_to_home {
            self.close_to_home_try_ticks += 1;
        }

        if !turtle.mob_base().navigation().lock().is_done() {
            return;
        }

        let (x, y, z) = home_pos.get_bottom_center();
        let home_vec = DVec3::new(x, y, z);
        let mut next_pos = default_random_pos_towards(mob, 16, 3, home_vec, NARROW_CONE)
            .or_else(|| default_random_pos_towards(mob, 8, 7, home_vec, WIDE_CONE));

        if let Some(candidate) = next_pos
            && !close_to_home
            && let Some(world) = turtle.level()
            && world
                .get_block_state(BlockPos::containing(candidate.x, candidate.y, candidate.z))
                .get_block()
                != &vanilla_blocks::WATER
        {
            next_pos = default_random_pos_towards(mob, 16, 5, home_vec, WIDE_CONE);
        }

        let Some(next_pos) = next_pos else {
            self.stuck = true;
            return;
        };

        mob.move_to_pos(next_pos, self.speed_modifier);
    }
}

/// Sends a turtle on a long swim to nowhere in particular.
///
/// Vanilla parity: `Turtle.TurtleTravelGoal`, which picks a point up to five
/// hundred blocks away and heads for it in short hops.
struct TurtleTravelGoal {
    speed_modifier: f64,
    stuck: bool,
}

impl TurtleTravelGoal {
    const fn new(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            stuck: false,
        }
    }
}

impl Goal for TurtleTravelGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        turtle_of(mob).is_some_and(|turtle| {
            !turtle.is_going_home() && !turtle.has_egg() && turtle.is_in_water()
        })
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(turtle) = turtle_of(mob) else {
            return false;
        };

        !turtle.mob_base().navigation().lock().is_done()
            && !self.stuck
            && !turtle.is_going_home()
            && !turtle.is_in_love()
            && !turtle.has_egg()
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let Some(turtle) = turtle_of(mob) else {
            return;
        };

        let position = turtle.position();
        let xt = rand::random_range(0..(2 * TRAVEL_XZ_RANGE + 1)) - TRAVEL_XZ_RANGE;
        let mut yt = rand::random_range(0..(2 * TRAVEL_Y_RANGE + 1)) - TRAVEL_Y_RANGE;
        let zt = rand::random_range(0..(2 * TRAVEL_XZ_RANGE + 1)) - TRAVEL_XZ_RANGE;
        let sea_level = turtle.level().map_or(0, |world| world.sea_level);
        if f64::from(yt) + position.y > f64::from(sea_level - 1) {
            yt = 0;
        }

        turtle.state.lock().travel_pos = Some(BlockPos::containing(
            f64::from(xt) + position.x,
            f64::from(yt) + position.y,
            f64::from(zt) + position.z,
        ));
        self.stuck = false;
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        if let Some(turtle) = turtle_of(mob) {
            turtle.state.lock().travel_pos = None;
        }
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(turtle) = turtle_of(mob) else {
            return;
        };
        let Some(travel_pos) = turtle.state.lock().travel_pos else {
            self.stuck = true;
            return;
        };
        if !turtle.mob_base().navigation().lock().is_done() {
            return;
        }

        let (x, y, z) = travel_pos.get_bottom_center();
        let target = DVec3::new(x, y, z);
        let mut next_pos = default_random_pos_towards(mob, 16, 3, target, NARROW_CONE)
            .or_else(|| default_random_pos_towards(mob, 8, 7, target, WIDE_CONE));

        if let Some(candidate) = next_pos
            && !has_chunks_around(turtle, candidate)
        {
            next_pos = None;
        }

        let Some(next_pos) = next_pos else {
            self.stuck = true;
            return;
        };

        mob.move_to_pos(next_pos, self.speed_modifier);
    }
}

/// Wanders the beach, but only when there is nothing better to do.
///
/// Vanilla parity: `Turtle.TurtleRandomStrollGoal`.
struct TurtleRandomStrollGoal {
    inner: RandomStrollGoal,
}

impl TurtleRandomStrollGoal {
    const fn new(speed_modifier: f64, interval: i32) -> Self {
        Self {
            inner: RandomStrollGoal::with_interval(speed_modifier, interval),
        }
    }
}

impl Goal for TurtleRandomStrollGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(turtle) = turtle_of(mob) else {
            return false;
        };

        !turtle.is_in_water()
            && !turtle.is_going_home()
            && !turtle.has_egg()
            && self.inner.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.inner.stop(mob);
    }
}

/// The narrow cone `DefaultRandomPos.getPosTowards` is first tried with.
///
/// Vanilla parity: the `(float)(Math.PI / 10)` of the turtle's travel goals.
const NARROW_CONE: f64 = PI / 10.0;
/// The wide cone it falls back to.
const WIDE_CONE: f64 = FRAC_PI_2;

/// Vanilla parity: the `hasChunksAt(x - 34, z - 34, x + 34, z + 34)` guard of
/// `TurtleTravelGoal.tick`, which keeps a turtle from swimming into unloaded
/// world.
fn has_chunks_around(turtle: &TurtleEntity, pos: DVec3) -> bool {
    let Some(world) = turtle.level() else {
        return false;
    };
    let min = BlockPos::containing(
        pos.x - f64::from(TRAVEL_CHUNK_MARGIN),
        pos.y,
        pos.z - f64::from(TRAVEL_CHUNK_MARGIN),
    );
    let max = BlockPos::containing(
        pos.x + f64::from(TRAVEL_CHUNK_MARGIN),
        pos.y,
        pos.z + f64::from(TRAVEL_CHUNK_MARGIN),
    );

    let min_chunk = ChunkPos::from_block_pos(min);
    let max_chunk = ChunkPos::from_block_pos(max);
    for chunk_x in min_chunk.0.x..=max_chunk.0.x {
        for chunk_z in min_chunk.0.y..=max_chunk.0.y {
            if !world.has_full_chunk(ChunkPos(IVec2::new(chunk_x, chunk_z))) {
                return false;
            }
        }
    }

    true
}

/// Vanilla parity: `Goal.reducedTickDelay`.
const fn reduced_tick_delay(ticks: i32) -> i32 {
    (ticks + 1) / 2
}

#[cfg(test)]
mod tests;
