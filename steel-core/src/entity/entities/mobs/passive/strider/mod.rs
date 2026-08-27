//! Strider entity.
//!
//! Vanilla parity: `Strider`. The Nether's transport: it walks on lava, and a
//! saddled one steers with a warped fungus on a stick the way a pig steers with
//! a carrot. What makes it more than a pig on a lake is that it shivers off the
//! lava -- a strider on cold ground is a third slower -- so taking one anywhere
//! useful means either riding another strider or building a lava road.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::entity_type::{
    EntityAttachmentPoint, EntityAttachments, EntityDimensions, EntityTypeRef,
};
use steel_registry::fluid::{FluidState, FluidStateExt as _};
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_entity_data::StriderEntityData;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    REGISTRY, TaggedRegistryExt, sound_events, vanilla_attributes, vanilla_blocks, vanilla_items,
};
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{
    BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey, Identifier,
};

use crate::behavior::{BLOCK_BEHAVIORS, InteractionResult};
use crate::entity::ai::goal::{
    BreedGoal, FollowParentGoal, Goal, GoalControls, LookAtPlayerGoal, MoveToBlockGoal, PanicGoal,
    RandomLookAroundGoal, RandomStrollGoal, TemptGoal,
};
use crate::entity::ai::path::{PathComputationType, PathType};
use crate::entity::attribute::{AttributeModifier, AttributeModifierOperation};
use crate::entity::damage::DamageSource;
use crate::entity::{
    AgeableMob, AgeableMobBase, AgeableMobGroupData, Animal, AnimalBase, Entity, EntityBase,
    EntityBaseLoad, EntityPose, EntitySpawnReason, EntitySyncedData, ItemBasedSteering,
    ItemSteerable, LivingEntity, LivingEntityBase, LivingEntitySyncedData, Mob, MobBase,
    MoveResult, PathfinderMob, SharedEntity, SpawnGroupData,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::player::Player;
use crate::world::{LevelReader, World};

/// Where a rider sits on a baby strider.
const BABY_PASSENGER_ATTACHMENTS: [EntityAttachmentPoint; 1] =
    [EntityAttachmentPoint::new(0.0, 0.656_25, 0.0)];

/// Baby strider hitbox.
///
/// Vanilla parity: `Strider.BABY_DIMENSIONS`.
const BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new_with_attachments(
    0.45,
    0.85,
    0.4375,
    EntityAttachments::new(&BABY_PASSENGER_ATTACHMENTS, &[], &[], &[]),
);

/// Identifier of the speed penalty a shivering strider carries.
///
/// Vanilla parity: `Strider.SUFFOCATING_MODIFIER_ID`.
const SUFFOCATING_MODIFIER_ID: Identifier = Identifier::vanilla_static("suffocating");

/// How much slower a shivering strider moves.
///
/// Vanilla parity: `Strider.SUFFOCATING_MODIFIER`. Minus a third of the base
/// speed is the cost of leaving the lava, and it is what makes a strider a
/// vehicle for the Nether rather than for anywhere.
const SUFFOCATING_MODIFIER_AMOUNT: f64 = -0.34;

/// Share of its speed a steered strider uses on lava.
///
/// Vanilla parity: `Strider.STEERING_MODIFIER`.
const STEERING_MODIFIER: f64 = 0.55;

/// Share of its speed a steered strider uses while shivering.
///
/// Vanilla parity: `Strider.SUFFOCATE_STEERING_MODIFIER`.
const SUFFOCATE_STEERING_MODIFIER: f64 = 0.35;

/// One-in-this-many chance per tick of a happy noise while being tempted.
const HAPPY_SOUND_CHANCE: i32 = 140;

/// One-in-this-many chance per tick of a retreat noise while panicking.
const RETREAT_SOUND_CHANCE: i32 = 60;

/// Speed multiplier while fleeing.
const PANIC_SPEED: f64 = 1.65;

/// Speed multiplier while following a warped fungus.
const TEMPT_SPEED: f64 = 1.4;

/// Speed multiplier while walking back to the lava.
const GO_TO_LAVA_SPEED: f64 = 1.0;

/// How far a stranded strider looks for lava.
///
/// Vanilla parity: the `super(strider, speedModifier, 8, 2)` of
/// `StriderGoToLavaGoal`.
const GO_TO_LAVA_SEARCH_RANGE: i32 = 8;

/// How far up and down that search reaches.
const GO_TO_LAVA_VERTICAL_RANGE: i32 = 2;

/// Ticks between path recalculations while walking to lava.
///
/// Vanilla parity: `StriderGoToLavaGoal.shouldRecalculatePath`.
const GO_TO_LAVA_RECALCULATE_INTERVAL: i32 = 20;

/// Distance at which a strider turns to watch something.
const LOOK_AT_RANGE: f64 = 8.0;

/// Vanilla parity: the default probability of `LookAtPlayerGoal`.
const LOOK_AT_PROBABILITY: f32 = 0.02;

/// How far a strider strolls between pauses.
const STROLL_INTERVAL_TICKS: i32 = 60;

/// Value a lava block scores when the strider looks for somewhere to walk.
///
/// Vanilla parity: `Strider.getWalkTargetValue`.
const LAVA_WALK_TARGET_VALUE: f32 = 10.0;

/// Extra distance a strider covers between step sounds.
///
/// Vanilla parity: `Strider.nextStep`, which spaces the steps closer together
/// than the default whole block.
const NEXT_STEP_DISTANCE: f32 = 0.6;

/// Chance a naturally spawned strider is a baby.
///
/// Vanilla parity: the `new AgeableMob.AgeableMobGroupData(0.5F)` of
/// `Strider.finalizeSpawn` -- ten times the usual animal chance.
const BABY_SPAWN_CHANCE: f32 = 0.5;

/// Height above a block's floor at which a strider's feet clear the lava.
///
/// Vanilla parity: the half-height liquid collision shape a strider reports,
/// `Block.column(16.0, 0.0, 8.0)`.
const LIQUID_COLLISION_HEIGHT: f64 = 0.5;

/// A strider.
#[entity_behavior(class = "Strider")]
pub struct StriderEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    steering: SyncMutex<ItemBasedSteering>,
    entity_data: SyncMutex<StriderEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `StriderEntity`.
unsafe impl DowncastType for StriderEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/strider");
}

impl StriderEntity {
    /// Creates a strider at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a strider from saved base data.
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

        // Vanilla parity: the constructor's pathfinding malus. Water is
        // forbidden outright and lava and fire cost nothing, which is what lets
        // the path finder route a strider straight across a lava lake.
        AnimalBase::initialize_pathfinding_malus(&mob_base);
        {
            let mut malus = mob_base.pathfinding_malus().lock();
            malus.set(PathType::Water, -1.0);
            malus.set(PathType::Lava, 0.0);
            malus.set(PathType::FireInNeighbor, 0.0);
            malus.set(PathType::Fire, 0.0);
        }

        let mut entity_data = StriderEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla parity: the goal order of `Strider.registerGoals`.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(1, PanicGoal::new(PANIC_SPEED));
            goals.add_goal(2, BreedGoal::new(1.0));
            goals.add_goal(
                3,
                TemptGoal::new(
                    TEMPT_SPEED,
                    |item_stack| {
                        REGISTRY
                            .items
                            .is_in_tag(item_stack.item(), &ItemTag::STRIDER_TEMPT_ITEMS)
                    },
                    false,
                ),
            );
            goals.add_goal(4, StriderGoToLavaGoal::new());
            goals.add_goal(5, FollowParentGoal::new(1.0));
            goals.add_goal(
                7,
                RandomStrollGoal::with_interval(1.0, STROLL_INTERVAL_TICKS),
            );
            goals.add_goal(8, LookAtPlayerGoal::new(LOOK_AT_RANGE));
            goals.add_goal(8, RandomLookAroundGoal::new());
            goals.add_goal(
                9,
                LookAtPlayerGoal::new_for_living_entities(
                    LOOK_AT_RANGE,
                    LOOK_AT_PROBABILITY,
                    |_, candidate, _| candidate.downcast_ref::<Self>().is_some(),
                ),
            );
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            steering: SyncMutex::new(ItemBasedSteering::new()),
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Returns whether the strider is shivering off the lava.
    ///
    /// Vanilla parity: `Strider.isSuffocating`.
    #[must_use]
    pub fn is_suffocating(&self) -> bool {
        *self.entity_data.lock().suffocating.get()
    }

    /// Sets the shiver, and the speed penalty that goes with it.
    ///
    /// Vanilla parity: `Strider.setSuffocating`. The penalty is a transient
    /// modifier rather than a new base value, so it lifts cleanly when the
    /// strider gets back on the lava however its base speed was changed
    /// meanwhile.
    pub fn set_suffocating(&self, suffocating: bool) {
        self.entity_data.lock().suffocating.set(suffocating);

        let mut attributes = self.attributes().lock();
        if suffocating {
            attributes.set_modifier(
                vanilla_attributes::MOVEMENT_SPEED,
                AttributeModifier {
                    id: SUFFOCATING_MODIFIER_ID,
                    amount: SUFFOCATING_MODIFIER_AMOUNT,
                    operation: AttributeModifierOperation::AddMultipliedBase,
                },
                false,
            );
        } else {
            attributes
                .remove_modifier(vanilla_attributes::MOVEMENT_SPEED, &SUFFOCATING_MODIFIER_ID);
        }
    }

    /// Returns whether the strider stands somewhere warm enough to stop
    /// shivering.
    ///
    /// Vanilla parity: the `inWarmBlocks || onWarmStrider` of `Strider.tick`.
    /// Riding another strider counts, which is how a pair crosses cold ground.
    fn is_warm(&self, world: &Arc<World>) -> bool {
        let inside_is_warm = world
            .get_block_state(self.block_position())
            .get_block()
            .has_tag(&BlockTag::STRIDER_WARM_BLOCKS);
        let standing_on_warm = self.on_pos_legacy().is_some_and(|pos| {
            world
                .get_block_state(pos)
                .get_block()
                .has_tag(&BlockTag::STRIDER_WARM_BLOCKS)
        });
        let in_lava = self.fluid_contact().lava_height() > 0.0;

        inside_is_warm || standing_on_warm || in_lava || self.is_riding_a_warm_strider()
    }

    fn is_riding_a_warm_strider(&self) -> bool {
        let Some(vehicle) = self.vehicle() else {
            return false;
        };
        vehicle
            .downcast_ref::<Self>()
            .is_some_and(|strider| !strider.is_suffocating())
    }

    /// Keeps the strider riding on top of the lava instead of wading in it.
    ///
    /// Vanilla parity: `Strider.floatStrider`. Vanilla asks whether the
    /// strider's feet clear its own half-height liquid collision shape; Steel
    /// carries no per-entity liquid shape, so the same question is put directly
    /// to the position.
    fn float_strider(&self) {
        if !self.is_in_lava() {
            return;
        }
        let Some(world) = self.level() else {
            return;
        };

        let pos = self.block_position();
        let feet_clear_surface = self.position().y - f64::from(pos.y()) >= LIQUID_COLLISION_HEIGHT;
        let lava_above = world
            .get_block_state(pos.above())
            .get_fluid_state()
            .is_lava();

        if feet_clear_surface && !lava_above {
            self.set_on_ground(true);
        } else {
            self.set_velocity(self.velocity() * 0.5 + DVec3::new(0.0, 0.05, 0.0));
        }
    }

    /// Plays the noises a strider makes about where it is going.
    ///
    /// Vanilla parity: the first half of `Strider.tick`.
    fn tick_mood_sounds(&self) {
        if PathfinderMob::is_being_tempted(self) && rand::random_range(0..HAPPY_SOUND_CHANCE) == 0 {
            self.play_sound(&sound_events::ENTITY_STRIDER_HAPPY, 1.0, 1.0);
        } else if PathfinderMob::is_panicking(self)
            && rand::random_range(0..RETREAT_SOUND_CHANCE) == 0
        {
            self.play_sound(&sound_events::ENTITY_STRIDER_RETREAT, 1.0, 1.0);
        }
    }

    fn set_ridden_rotation(&self, controller_yaw: f32, controller_pitch: f32) {
        self.set_rotation((controller_yaw, controller_pitch * 0.5));
        self.base.set_old_yaw_to_current();
        let yaw = self.rotation().0;
        self.set_y_body_rot(yaw);
        self.set_y_head_rot(yaw);
    }

    /// Returns whether the stack is vanilla strider food.
    #[must_use]
    pub fn is_strider_food(item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::STRIDER_FOOD)
    }

    /// Returns whether a strider may spawn where the spawner put it.
    ///
    /// Vanilla parity: `Strider.checkStriderSpawnRules`. It climbs out of the
    /// lava it was offered and demands open air above, which is what keeps
    /// striders on the surface of a lava sea rather than buried under it.
    #[must_use]
    pub fn check_strider_spawn_rules(world: &dyn LevelReader, pos: BlockPos) -> bool {
        let mut check_pos = pos;
        while world
            .get_block_state(check_pos.above())
            .get_fluid_state()
            .is_lava()
        {
            check_pos = check_pos.above();
        }
        world.get_block_state(check_pos.above()).get_block() == &vanilla_blocks::AIR
    }
}

impl Entity for StriderEntity {
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

    /// Vanilla parity: `Strider.tick`, which decides the shiver before the
    /// living tick spends the speed it just changed, and floats afterwards.
    fn tick(&self) {
        self.tick_mood_sounds();

        if !Mob::is_no_ai(self)
            && let Some(world) = self.level()
        {
            let warm = self.is_warm(&world);
            if self.is_suffocating() == warm {
                self.set_suffocating(!warm);
            }
        }

        LivingEntity::tick_living_entity(self);
        self.float_strider();
    }

    /// Vanilla parity: the `blocksBuilding = true` of the constructor.
    fn blocks_building(&self) -> bool {
        true
    }

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

    /// Vanilla parity: `Strider.isOnFire`, which is always false. A strider
    /// wading in lava is not burning, and the client should not draw it that
    /// way.
    fn is_on_fire(&self) -> bool {
        false
    }

    /// Vanilla parity: `Strider.getControllingPassenger`.
    fn controlling_passenger(&self) -> Option<SharedEntity> {
        if self.is_saddled()
            && let Some(passenger) = self.first_passenger()
            && passenger.as_player().is_some_and(|player| {
                let mut is_holding_warped_fungus = |item_stack: &ItemStack| {
                    item_stack.is(&vanilla_items::WARPED_FUNGUS_ON_A_STICK)
                };
                player.is_holding(&mut is_holding_warped_fungus)
            })
        {
            return Some(passenger);
        }

        self.controlling_passenger_mob()
    }

    /// Vanilla parity: `Strider.canAddPassenger`, which refuses a rider while
    /// the strider's own eyes are under the lava.
    fn can_add_passenger(&self, _passenger: &dyn Entity) -> bool {
        !self.is_vehicle() && !self.is_eye_in_lava()
    }

    /// Vanilla parity: `Strider.nextStep`.
    fn next_step(&self) -> f32 {
        self.base().movement_progress().move_dist() + NEXT_STEP_DISTANCE
    }

    /// Vanilla parity: `Strider.playStepSound`.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        let sound = if self.is_in_lava() {
            &sound_events::ENTITY_STRIDER_STEP_LAVA
        } else {
            &sound_events::ENTITY_STRIDER_STEP
        };
        self.play_sound(sound, 1.0, 1.0);
    }

    /// Vanilla parity: `Strider.checkFallDamage`. Lava is a floor, not a
    /// landing, so a strider that walks off a cliff onto a lava lake is not
    /// hurt by the drop.
    fn check_fall_damage(
        &self,
        vertical_movement: f64,
        on_ground: bool,
        on_state: BlockStateId,
        pos: BlockPos,
        world: &Arc<World>,
    ) {
        if self.is_in_lava() {
            self.reset_fall_distance();
        } else {
            self.living_check_fall_damage(vertical_movement, on_ground, on_state, pos, world);
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
    }
}

impl LivingEntity for StriderEntity {
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

    /// Walks on lava rather than sinking into it.
    ///
    /// Vanilla parity: `Strider.canStandOnFluid`. This one line is the mob: the
    /// physics and the walk node evaluator both ask it, so a strider both
    /// floats on a lava lake and routes across one.
    fn can_stand_on_fluid(&self, fluid_state: FluidState) -> bool {
        fluid_state.is_lava()
    }

    fn can_stand_on_fluid_predicate(&self) -> fn(FluidState) -> bool {
        |fluid_state| fluid_state.is_lava()
    }

    /// Vanilla parity: `Strider.isSensitiveToWater`. Fire's creature, hurt by
    /// rain and by a thrown water bottle.
    fn is_sensitive_to_water(&self) -> bool {
        true
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_STRIDER_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_STRIDER_DEATH)
    }

    fn can_use_slot(&self, slot: EquipmentSlot) -> bool {
        slot != EquipmentSlot::Saddle || (Entity::is_alive(self) && !AgeableMob::is_baby(self))
    }

    fn can_dispenser_equip_into_slot(&self, slot: EquipmentSlot) -> bool {
        slot == EquipmentSlot::Saddle || Mob::can_pick_up_loot(self)
    }

    fn equip_sound(&self, slot: EquipmentSlot, _stack: &ItemStack) -> Option<SoundEventRef> {
        (slot == EquipmentSlot::Saddle).then_some(&sound_events::ENTITY_STRIDER_SADDLE)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    /// Vanilla parity: `Strider.tickRidden`.
    fn tick_ridden(&self, controller: &Player, _ridden_input: DVec3) {
        let (yaw, pitch) = controller.rotation();
        self.set_ridden_rotation(yaw, pitch);
        ItemSteerable::tick_boost(self);
    }

    /// Vanilla parity: `Strider.getRiddenInput`, which throws the rider's
    /// steering away: a strider only ever walks forward, and the rider aims it
    /// by looking.
    fn ridden_input(&self, _controller: &Player, _self_input: DVec3) -> DVec3 {
        DVec3::new(0.0, 0.0, 1.0)
    }

    /// Vanilla parity: `Strider.getRiddenSpeed`.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "vanilla casts the same product to a float"
    )]
    fn ridden_speed(&self, _controller: &Player) -> f32 {
        let movement_speed = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MOVEMENT_SPEED);
        let modifier = if self.is_suffocating() {
            SUFFOCATE_STEERING_MODIFIER
        } else {
            STEERING_MODIFIER
        };
        (movement_speed * modifier) as f32 * ItemSteerable::boost_factor(self)
    }

    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);
        result
    }
}

impl ItemSteerable for StriderEntity {
    fn item_based_steering(&self) -> &SyncMutex<ItemBasedSteering> {
        &self.steering
    }

    fn boost_time_total(&self) -> i32 {
        *self.entity_data.lock().boost_time.get()
    }

    fn set_boost_time_total(&self, boost_time_total: i32) {
        self.entity_data.lock().boost_time.set(boost_time_total);
    }
}

impl AgeableMob for StriderEntity {
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

impl Animal for StriderEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    /// Vanilla parity: `Strider.isFood`.
    fn is_food(&self, item_stack: &ItemStack) -> bool {
        Self::is_strider_food(item_stack)
    }
}

impl Mob for StriderEntity {
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

    /// Vanilla parity: `Strider.getAmbientSound`, which stays quiet while the
    /// strider has something better to say.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        (!PathfinderMob::is_panicking(self) && !PathfinderMob::is_being_tempted(self))
            .then_some(&sound_events::ENTITY_STRIDER_AMBIENT)
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        _spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        Self::check_strider_spawn_rules(world.as_ref(), pos)
    }

    /// Vanilla parity: `Strider.finalizeSpawn`, minus the jockeys.
    ///
    /// TODO: vanilla gives one strider in thirty a saddled zombified piglin
    /// rider and one in ten a baby strider rider. Both need a second mob
    /// created and mounted from inside `finalize_spawn`, and Steel calls this
    /// before the strider joins the world (see `World::try_spawn_at`), so there
    /// is no `SharedEntity` to mount anything onto yet. The rest matches,
    /// including the raised baby chance vanilla uses for the no-jockey case.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let group_data = group_data.unwrap_or(SpawnGroupData::AgeableMob(
            AgeableMobGroupData::with_baby_spawn_chance(BABY_SPAWN_CHANCE),
        ));
        self.finalize_spawn_ageable_mob(world, spawn_reason, Some(group_data))
    }

    /// Vanilla parity: `Strider.mobInteract`. A saddled strider is mounted by
    /// an empty hand, and feeding one makes it chew.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let item_stack = {
            let inventory = player.inventory.lock();
            let item_stack = inventory.get_item_in_hand(hand);
            item_stack.copy_with_count(item_stack.count())
        };
        let has_food = Self::is_strider_food(&item_stack);

        if !has_food && self.is_saddled() && !self.is_vehicle() && !player.is_secondary_use_active()
        {
            if let Some(world) = self.level()
                && let Some(vehicle) = world.get_entity_by_id(self.id())
            {
                player.start_riding(&vehicle);
            }
            return InteractionResult::Success;
        }

        let interaction_result = Animal::mob_interact_animal(self, player, hand);
        if interaction_result.consumes_action() {
            if has_food && !self.is_silent() {
                self.play_sound(
                    &sound_events::ENTITY_STRIDER_EAT,
                    1.0,
                    1.0 + (rand::random::<f32>() - rand::random::<f32>()) * 0.2,
                );
            }
            return interaction_result;
        }

        if LivingEntity::is_equippable_in_slot(self, &item_stack, EquipmentSlot::Saddle) {
            return LivingEntity::interact_living_entity_with_equippable(self, player, hand);
        }

        InteractionResult::Pass
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for StriderEntity {
    /// Prefers lava over anything else underfoot.
    ///
    /// Vanilla parity: `Strider.getWalkTargetValue`. Lava is worth ten, and a
    /// strider already in lava treats dry land as unreachable, which is what
    /// keeps one from wandering off its lake.
    fn get_walk_target_value(&self, pos: BlockPos) -> f32 {
        let Some(world) = self.level() else {
            return 0.0;
        };

        if world.get_block_state(pos).get_fluid_state().is_lava() {
            LAVA_WALK_TARGET_VALUE
        } else if self.is_in_lava() {
            f32::NEG_INFINITY
        } else {
            0.0
        }
    }

    /// Vanilla parity: `Strider.StriderPathNavigation.isStableDestination`,
    /// which adds lava to the blocks a path may end on.
    fn is_stable_destination(&self, pos: BlockPos) -> bool {
        self.level().is_some_and(|world| {
            world.get_block_state(pos).get_block() == &vanilla_blocks::LAVA
                || world.get_block_state(pos.below()).is_solid_render()
        })
    }
}

/// Walks a stranded strider back to the nearest lava.
///
/// Vanilla parity: `Strider.StriderGoToLavaGoal`, a `MoveToBlockGoal` that only
/// runs while the strider is out of the lava. Without it a strider pushed onto
/// the shore of a lava sea would shiver there forever.
struct StriderGoToLavaGoal {
    inner: MoveToBlockGoal,
}

impl StriderGoToLavaGoal {
    fn new() -> Self {
        Self {
            inner: MoveToBlockGoal::with_vertical_search_range(
                GO_TO_LAVA_SPEED,
                GO_TO_LAVA_SEARCH_RANGE,
                GO_TO_LAVA_VERTICAL_RANGE,
                |level, pos| {
                    level.get_block_state(pos).get_block() == &vanilla_blocks::LAVA
                        && is_pathfindable_by_land(level, pos.above())
                },
            )
            .with_recalculate_path_interval(GO_TO_LAVA_RECALCULATE_INTERVAL)
            // Vanilla's `getMoveToTarget` returns the lava block itself rather
            // than the block above it, because walking onto the lava is the
            // point.
            .with_move_to_target(|pos| pos),
        }
    }
}

impl Goal for StriderGoToLavaGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn requires_update_every_tick(&self) -> bool {
        self.inner.requires_update_every_tick()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        !mob.is_in_lava() && self.inner.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        !mob.is_in_lava() && self.inner.can_continue_to_use(mob)
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

fn is_pathfindable_by_land(level: &dyn LevelReader, pos: BlockPos) -> bool {
    let state = level.get_block_state(pos);
    BLOCK_BEHAVIORS
        .get_behavior(state.get_block())
        .is_pathfindable(state, PathComputationType::Land)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Weak};

    use glam::DVec3;
    use steel_registry::fluid::FluidState;
    use steel_registry::{init_vanilla_registry, vanilla_entities, vanilla_fluids};
    use steel_utils::ChunkPos;
    use steel_utils::types::UpdateFlags;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::entity::ai::path::Path;
    use crate::entity::entities::PigEntity;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    fn strider() -> StriderEntity {
        StriderEntity::new(
            &vanilla_entities::STRIDER,
            1,
            DVec3::ZERO,
            Weak::<World>::new(),
        )
    }

    #[test]
    fn downcast_key_identifies_strider() {
        assert_eq!(
            StriderEntity::TYPE_KEY,
            DowncastTypeKey::new("steel:entity/strider")
        );
    }

    #[test]
    fn stands_on_lava_but_not_on_water() {
        init_vanilla_registry();
        let mob = strider();

        assert!(mob.can_stand_on_fluid(FluidState::from_block_level(&vanilla_fluids::LAVA, 0)));
        assert!(!mob.can_stand_on_fluid(FluidState::from_block_level(&vanilla_fluids::WATER, 0)));
    }

    #[test]
    fn the_path_finder_is_told_the_same_thing_as_the_physics() {
        init_vanilla_registry();
        let mob = strider();
        let predicate = mob.can_stand_on_fluid_predicate();

        for fluid in [&vanilla_fluids::LAVA, &vanilla_fluids::WATER] {
            let state = FluidState::from_block_level(fluid, 0);
            assert_eq!(
                mob.can_stand_on_fluid(state),
                predicate(state),
                "the two answers disagree about {:?}",
                fluid.key
            );
        }
    }

    #[test]
    fn water_hurts_a_strider() {
        init_vanilla_registry();
        assert!(strider().is_sensitive_to_water());
    }

    #[test]
    fn a_strider_in_lava_is_never_drawn_on_fire() {
        init_vanilla_registry();
        assert!(!Entity::is_on_fire(&strider()));
    }

    #[test]
    fn shivering_costs_a_third_of_the_walking_speed() {
        init_vanilla_registry();
        let mob = strider();
        let warm_speed = mob
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MOVEMENT_SPEED);

        mob.set_suffocating(true);
        let cold_speed = mob
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MOVEMENT_SPEED);

        assert!(mob.is_suffocating());
        let expected = warm_speed * (1.0 + SUFFOCATING_MODIFIER_AMOUNT);
        assert!(
            (cold_speed - expected).abs() < 1e-9,
            "{cold_speed} != {expected}"
        );

        // Vanilla removes the modifier rather than writing a new base value, so
        // the original speed has to come back exactly.
        mob.set_suffocating(false);
        let thawed_speed = mob
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MOVEMENT_SPEED);
        assert!(!mob.is_suffocating());
        assert!((thawed_speed - warm_speed).abs() < 1e-9);
    }

    #[test]
    fn a_shivering_strider_is_steered_more_slowly() {
        init_vanilla_registry();
        let mob = strider();
        let base = mob
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MOVEMENT_SPEED);

        // `ridden_speed` needs a controller only for its signature, so the two
        // modifiers are compared through the constants the method multiplies by.
        let warm = base * STEERING_MODIFIER;
        mob.set_suffocating(true);
        let cold = mob
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MOVEMENT_SPEED)
            * SUFFOCATE_STEERING_MODIFIER;

        assert!(
            cold < warm,
            "shivering {cold} should be slower than warm {warm}"
        );
    }

    #[test]
    fn a_baby_strider_is_smaller_and_has_its_own_seat() {
        init_vanilla_registry();
        let mob = strider();
        let adult = mob.dimensions_for_pose(EntityPose::Standing);

        Mob::set_baby(&mob, true);
        let baby = mob.dimensions_for_pose(EntityPose::Standing);

        assert!(baby.width < adult.width);
        assert!(baby.height < adult.height);
        assert!((baby.height - 0.85).abs() < f32::EPSILON);
    }

    #[test]
    fn only_a_grown_living_strider_wears_a_saddle() {
        init_vanilla_registry();
        let mob = strider();

        assert!(mob.can_use_slot(EquipmentSlot::Saddle));
        Mob::set_baby(&mob, true);
        assert!(!mob.can_use_slot(EquipmentSlot::Saddle));
    }

    #[test]
    fn the_saddle_slot_has_the_strider_equip_sound() {
        init_vanilla_registry();
        let mob = strider();
        let empty = ItemStack::empty();

        assert_eq!(
            mob.equip_sound(EquipmentSlot::Saddle, &empty)
                .map(|s| &s.key),
            Some(&sound_events::ENTITY_STRIDER_SADDLE.key)
        );
        assert!(mob.equip_sound(EquipmentSlot::Chest, &empty).is_none());
    }

    #[test]
    fn steps_are_spaced_closer_than_a_whole_block() {
        init_vanilla_registry();
        let mob = strider();
        let move_dist = mob.base().movement_progress().move_dist();

        assert!((mob.next_step() - (move_dist + NEXT_STEP_DISTANCE)).abs() < f32::EPSILON);
    }

    #[test]
    fn walk_target_value_without_world_is_zero() {
        init_vanilla_registry();
        assert!(
            strider()
                .get_walk_target_value(BlockPos::new(0, 64, 0))
                .abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn lava_is_the_best_place_to_walk() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("strider_walk_target_value");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let lava_pos = BlockPos::new(8, 64, 8);
        let _ = world.set_block(
            lava_pos,
            vanilla_blocks::LAVA.default_state(),
            UpdateFlags::UPDATE_CLIENTS,
        );

        let mob = StriderEntity::new(
            &vanilla_entities::STRIDER,
            1,
            DVec3::new(8.5, 70.0, 8.5),
            Arc::downgrade(&world),
        );

        assert!(
            (mob.get_walk_target_value(lava_pos) - LAVA_WALK_TARGET_VALUE).abs() < f32::EPSILON
        );
        // Dry air scores nothing while the strider is out of the lava; it only
        // becomes unreachable once the strider is standing in some.
        assert!(mob.get_walk_target_value(BlockPos::new(2, 64, 2)).abs() < f32::EPSILON);
    }

    #[test]
    fn a_path_may_end_on_lava() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("strider_stable_destination");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let lava_pos = BlockPos::new(8, 64, 8);
        let air_pos = BlockPos::new(4, 64, 4);
        let _ = world.set_block(
            lava_pos,
            vanilla_blocks::LAVA.default_state(),
            UpdateFlags::UPDATE_CLIENTS,
        );

        let mob = StriderEntity::new(
            &vanilla_entities::STRIDER,
            1,
            DVec3::new(8.5, 70.0, 8.5),
            Arc::downgrade(&world),
        );

        assert!(mob.is_stable_destination(lava_pos));
        assert!(!mob.is_stable_destination(air_pos));
    }

    #[test]
    fn spawning_needs_open_air_above_the_lava() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("strider_spawn_rules");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        // A column of lava with air on top: the rule walks up out of the lava
        // and finds the air.
        let base = BlockPos::new(8, 64, 8);
        for offset in 0..3 {
            let _ = world.set_block(
                base.offset(0, offset, 0),
                vanilla_blocks::LAVA.default_state(),
                UpdateFlags::UPDATE_CLIENTS,
            );
        }
        assert!(StriderEntity::check_strider_spawn_rules(
            world.as_ref(),
            base
        ));

        // Capping the column turns the same position down.
        let _ = world.set_block(
            base.offset(0, 3, 0),
            vanilla_blocks::NETHERRACK.default_state(),
            UpdateFlags::UPDATE_CLIENTS,
        );
        assert!(!StriderEntity::check_strider_spawn_rules(
            world.as_ref(),
            base
        ));
    }

    /// Builds a netherrack shelf split by a four-block channel of lava.
    ///
    /// ```text
    ///   z=8  N N N L L L L N N N        N netherrack  L lava
    ///        ^ start                ^ goal
    /// ```
    fn lava_channel_world(name: &'static str) -> Arc<World> {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world(name);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        let floor_y = 64;
        for x in 2..=12 {
            for z in 6..=10 {
                let is_channel = (6..=9).contains(&x);
                let block = if is_channel {
                    vanilla_blocks::LAVA.default_state()
                } else {
                    vanilla_blocks::NETHERRACK.default_state()
                };
                let _ = world.set_block(
                    BlockPos::new(x, floor_y, z),
                    block,
                    UpdateFlags::UPDATE_CLIENTS,
                );
                // Clear headroom so the walk evaluator has somewhere to stand.
                for above in 1..=2 {
                    let _ = world.set_block(
                        BlockPos::new(x, floor_y + above, z),
                        vanilla_blocks::AIR.default_state(),
                        UpdateFlags::UPDATE_CLIENTS,
                    );
                }
            }
        }

        world
    }

    /// The whole point of the mob, end to end.
    ///
    /// `can_stand_on_fluid` is only worth anything if the path finder hears
    /// about it, and until this mob it never did: `MobPathSettings::from_mob`
    /// hard-coded the predicate to `false`, so a strider would have floated on
    /// a lava lake while refusing to plan a single step across it. The pig is
    /// the control -- same world, same start, same goal, and it must stay dry.
    #[test]
    fn only_the_strider_plans_a_route_over_the_lava() {
        let world = lava_channel_world("strider_crosses_lava");
        let start = DVec3::new(3.5, 65.0, 8.5);
        let goal = BlockPos::new(11, 65, 8);

        let strider =
            StriderEntity::new(&vanilla_entities::STRIDER, 1, start, Arc::downgrade(&world));
        let pig = PigEntity::new(&vanilla_entities::PIG, 2, start, Arc::downgrade(&world));

        // A freshly built entity has never moved, so `on_ground` is false and
        // `can_update_path` refuses before the evaluator ever runs. Standing
        // them up is what a tick would have done.
        strider.set_on_ground(true);
        pig.set_on_ground(true);

        let strider_lava_nodes = lava_nodes(strider.create_path_to(goal, 0).as_ref());
        let pig_lava_nodes = lava_nodes(pig.create_path_to(goal, 0).as_ref());

        assert!(
            strider_lava_nodes > 0,
            "the strider should route over the lava, crossed {strider_lava_nodes} lava nodes"
        );
        assert_eq!(
            pig_lava_nodes, 0,
            "a pig cannot stand on lava, so no step of its path may be a lava node"
        );
    }

    /// Counts the steps of a path that stand on lava.
    fn lava_nodes(path: Option<&Path>) -> usize {
        path.map_or(0, |path| {
            (0..path.node_count())
                .filter_map(|index| path.node(index))
                .filter(|node| node.path_type == PathType::Lava)
                .count()
        })
    }

    #[test]
    fn the_lava_the_strider_crosses_is_really_lava() {
        // Guards the test above: if the channel ever stopped being lava, both
        // mobs would cross and the comparison would pass for the wrong reason.
        let world = lava_channel_world("strider_channel_is_lava");

        for x in 6..=9 {
            assert!(
                world
                    .get_block_state(BlockPos::new(x, 64, 8))
                    .get_fluid_state()
                    .is_lava(),
                "x={x} should be lava"
            );
        }
    }

    #[test]
    fn warped_fungus_tempts_and_feeds() {
        init_vanilla_registry();
        let mob = strider();

        assert!(Animal::is_food(
            &mob,
            &ItemStack::new(&vanilla_items::WARPED_FUNGUS)
        ));
        assert!(!Animal::is_food(
            &mob,
            &ItemStack::new(&vanilla_items::CARROT)
        ));
    }
}
