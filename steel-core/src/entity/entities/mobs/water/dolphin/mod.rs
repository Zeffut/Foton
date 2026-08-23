//! Dolphin entity.
//!
//! Vanilla parity: `Dolphin` and `AgeableWaterCreature`. A dolphin is the one
//! water mob that pays attention to players: it races a rowed boat, gives a
//! swimmer Dolphin's Grace, plays fetch with whatever it finds on the sea bed,
//! and -- once fed a fish -- leads the way to the nearest buried treasure.
//!
//! **Gap: the treasure goal is not here.** `Dolphin.DolphinSwimToTreasureGoal`
//! needs `ServerLevel.findNearestMapStructure(StructureTags.DOLPHIN_LOCATED,
//! pos, 50, false)`, and Steel has no structure-locating call at all -- nothing
//! under `steel-worldgen` or `steel-core` answers "where is the nearest
//! structure of this tag". Everything the goal hangs off is implemented: the
//! `GotFish` flag is set by feeding, saved, loaded and synchronized, and
//! `EntityStatus::DolphinLookingForTreasure` is the event the goal would
//! broadcast. Only the search itself is missing, so a fed dolphin keeps its
//! flag and swims normally instead of setting off.

use std::f32::consts::TAU;
use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::entity_type::{
    EntityAttachmentPoint, EntityAttachments, EntityDimensions, EntityTypeRef,
};
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::DolphinEntityData;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_damage_types, vanilla_entities,
    vanilla_mob_effects,
};
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey};

use crate::behavior::InteractionResult;
use crate::entity::ai::control::{SmoothSwimmingLookControl, SmoothSwimmingMoveControl};
use crate::entity::ai::goal::{
    AvoidEntityGoal, BreathAirGoal, DolphinJumpGoal, FollowPlayerRiddenEntityGoal, Goal,
    GoalControls, HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal, RandomLookAroundGoal,
    RandomSwimmingGoal, TryFindWaterGoal,
};
use crate::entity::ai::path::PathType;
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::damage::DamageSource;
use crate::entity::entities::ItemEntity;
use crate::entity::living_base::MobEffectInstance;
use crate::entity::mob::NavigationKind;
use crate::entity::spawn::AgeableMobGroupData;
use crate::entity::spawn_rules::check_surface_water_animal_spawn_rules;
use crate::entity::{
    AgeableMob, AgeableMobBase, Animal, AnimalBase, Entity, EntityBase, EntityBaseLoad, EntityPose,
    EntitySpawnReason, EntitySyncedData, LivingEntity, LivingEntityBase, LivingEntitySyncedData,
    Mob, MobBase, MoveResult, PathfinderMob, SharedEntity, SpawnGroupData,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::physics::MoverType;
use crate::player::Player;
use crate::world::World;

/// Vanilla `Dolphin.TOTAL_AIR_SUPPLY`.
const TOTAL_AIR_SUPPLY: i32 = 4800;
/// Vanilla `Dolphin.TOTAL_MOISTNESS_LEVEL`.
const TOTAL_MOISTNESS_LEVEL: i32 = 2400;
/// Vanilla `Dolphin.BABY_SCALE`.
const BABY_SCALE: f32 = 0.65;

/// The passenger attachment of `Dolphin.BABY_DIMENSIONS`.
const BABY_PASSENGER_ATTACHMENTS: [EntityAttachmentPoint; 1] =
    [EntityAttachmentPoint::new(0.0, 0.3125, 0.0)];
/// Vanilla `Dolphin.BABY_DIMENSIONS`, an adult scaled to 0.65 with its own eye
/// height and passenger point.
const BABY_EYE_HEIGHT: f32 = 0.093_75;

/// Chance a spawning dolphin is a calf.
///
/// Vanilla parity: the `AgeableMobGroupData(0.1F)` of `Dolphin.finalizeSpawn`.
const BABY_SPAWN_CHANCE: f32 = 0.1;

/// Speed a dolphin swims beside a swimmer at.
const SWIM_WITH_PLAYER_SPEED_MOD: f64 = 4.0;
/// How far a dolphin looks for someone to swim with.
const SWIM_WITH_PLAYER_RANGE: f64 = 10.0;
/// Squared distance beyond which it gives up escorting.
const SWIM_WITH_PLAYER_GIVE_UP_SQR: f64 = 256.0;
/// Squared distance inside which it stops and treads water.
const SWIM_WITH_PLAYER_STOP_SQR: f64 = 6.25;
/// Ticks of Dolphin's Grace a single top-up grants.
const DOLPHINS_GRACE_TICKS: i32 = 100;
/// One in this many ticks the escort tops the effect up again.
const DOLPHINS_GRACE_REFRESH_CHANCE: i32 = 6;

/// Speed a wandering dolphin swims at.
const RANDOM_SWIM_SPEED_MOD: f64 = 1.0;
/// Ticks between two wander attempts.
const RANDOM_SWIM_INTERVAL: i32 = 10;
/// Speed a hunting dolphin charges at.
const ATTACK_SPEED_MOD: f64 = 1.2;
/// How often the dolphin tries to breach.
const JUMP_INTERVAL: i32 = 10;
/// How far a dolphin keeps from a guardian.
const AVOID_GUARDIAN_RANGE: f32 = 8.0;
/// Speed it retreats from one at.
const AVOID_GUARDIAN_SPEED_MOD: f64 = 1.0;

/// How far around itself a dolphin looks for something to play with.
///
/// Vanilla parity: the `inflate(8.0, 8.0, 8.0)` of `PlayWithItemsGoal`.
const PLAY_ITEM_SEARCH_RANGE: f64 = 8.0;
/// Speed it swims to a toy at.
const PLAY_ITEM_SPEED_MOD: f64 = 1.2;
/// Longest extra cooldown after it drops a toy.
const PLAY_ITEM_MAX_COOLDOWN: i32 = 100;
/// Pickup delay the thrown toy gets so the dolphin cannot instantly re-grab it.
const PLAY_ITEM_THROW_PICKUP_DELAY: i32 = 40;
/// How hard a dolphin throws a toy.
const PLAY_ITEM_THROW_POWER: f64 = 0.3;
/// Extra scatter on the throw.
const PLAY_ITEM_THROW_SCATTER: f64 = 0.02;
/// How far below the eyes a thrown toy leaves the mouth.
const PLAY_ITEM_THROW_DROP: f64 = 0.3;

/// Vanilla `Dolphin.getMaxHeadXRot` and `getMaxHeadYRot`, both one degree.
const MAX_HEAD_ROT: f32 = 1.0;

/// Fraction of its speed a swimming dolphin keeps each tick.
const SWIM_DRAG: f64 = 0.9;
/// Downward drift a dolphin with nothing to chase settles into.
const IDLE_SINK: f64 = -0.005;

/// Sideways scatter of a stranded dolphin's flop.
const BEACHED_SCATTER: f64 = 0.2;
/// Upward kick of the same flop.
const BEACHED_LIFT: f64 = 0.5;
/// Damage a dried-out dolphin takes each tick.
const DRY_OUT_DAMAGE: f32 = 1.0;

/// A dolphin.
#[entity_behavior(class = "Dolphin")]
pub struct DolphinEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    entity_data: SyncMutex<DolphinEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `DolphinEntity`.
unsafe impl DowncastType for DolphinEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/dolphin");
}

impl DolphinEntity {
    /// Creates a dolphin at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a dolphin from saved base data.
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
        // Vanilla parity: the `AgeableWaterCreature` constructor.
        mob_base
            .pathfinding_malus()
            .lock()
            .set(PathType::Water, 0.0);
        *mob_base.can_pick_up_loot().lock() = true;
        let mut entity_data = DolphinEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla parity: `Dolphin.registerGoals`. The treasure goal that
            // vanilla puts at priority 1 is absent; see the module comment.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(0, BreathAirGoal::new());
            goals.add_goal(0, TryFindWaterGoal::new());
            goals.add_goal(
                2,
                DolphinSwimWithPlayerGoal::new(SWIM_WITH_PLAYER_SPEED_MOD),
            );
            goals.add_goal(
                4,
                RandomSwimmingGoal::new(RANDOM_SWIM_SPEED_MOD, RANDOM_SWIM_INTERVAL),
            );
            goals.add_goal(4, RandomLookAroundGoal::new());
            goals.add_goal(5, LookAtPlayerGoal::new(6.0));
            goals.add_goal(5, DolphinJumpGoal::new(JUMP_INTERVAL));
            goals.add_goal(6, MeleeAttackGoal::new(ATTACK_SPEED_MOD, true));
            goals.add_goal(8, PlayWithItemsGoal::new());
            goals.add_goal(
                8,
                FollowPlayerRiddenEntityGoal::new(|entity| entity.entity_type().is_abstract_boat),
            );
            // Vanilla also escorts an `AbstractNautilus`; Steel has no nautilus
            // entity, so that second goal has nothing to follow.
            goals.add_goal(
                9,
                AvoidEntityGoal::with_selector(
                    AVOID_GUARDIAN_RANGE,
                    AVOID_GUARDIAN_SPEED_MOD,
                    AVOID_GUARDIAN_SPEED_MOD,
                    |_, target, _| is_guardian(target.entity_type()),
                ),
            );
        }
        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(1, HurtByTargetGoal::new().set_alert_others([]));
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Returns vanilla `Dolphin.gotFish`.
    #[must_use]
    pub fn got_fish(&self) -> bool {
        *self.entity_data.lock().got_fish.get()
    }

    /// Sets vanilla `Dolphin.setGotFish`.
    pub fn set_got_fish(&self, got_fish: bool) {
        self.entity_data.lock().got_fish.set(got_fish);
    }

    /// Returns vanilla `Dolphin.getMoistnessLevel`.
    #[must_use]
    pub fn moistness_level(&self) -> i32 {
        *self.entity_data.lock().moistness_level.get()
    }

    /// Sets vanilla `Dolphin.setMoisntessLevel`, typo and all.
    pub fn set_moistness_level(&self, moistness_level: i32) {
        self.entity_data.lock().moistness_level.set(moistness_level);
    }

    /// Vanilla parity: `Dolphin.BABY_DIMENSIONS`.
    fn baby_dimensions(entity_type: EntityTypeRef) -> EntityDimensions {
        EntityDimensions::new_with_attachments(
            entity_type.dimensions.width * BABY_SCALE,
            entity_type.dimensions.height * BABY_SCALE,
            BABY_EYE_HEIGHT,
            EntityAttachments::new(&BABY_PASSENGER_ATTACHMENTS, &[], &[], &[]),
        )
    }

    /// Vanilla parity: the moistness half of `Dolphin.tick`.
    fn tick_moistness(&self) {
        if self.is_no_ai() {
            self.set_air_supply(TOTAL_AIR_SUPPLY);
            return;
        }

        if self.is_in_water_or_rain() {
            self.set_moistness_level(TOTAL_MOISTNESS_LEVEL);
            return;
        }

        self.set_moistness_level(self.moistness_level() - 1);
        if self.moistness_level() <= 0
            && let Some(world) = self.level()
        {
            self.hurt_server(
                &world,
                &DamageSource::environment(&vanilla_damage_types::DRY_OUT),
                DRY_OUT_DAMAGE,
            );
        }

        if self.on_ground() {
            let scatter = || (rand::random::<f64>() * 2.0 - 1.0) * BEACHED_SCATTER;
            self.set_velocity(self.velocity() + DVec3::new(scatter(), BEACHED_LIFT, scatter()));
            self.set_rotation((rand::random::<f32>() * 360.0, self.rotation().1));
            self.set_on_ground(false);
            self.mark_velocity_sync();
        }
    }

    /// Returns whether the stack is something a dolphin will take.
    ///
    /// Vanilla parity: the `ItemTags.FISHES` test of `Dolphin.mobInteract`.
    #[must_use]
    pub fn is_fish(item_stack: &ItemStack) -> bool {
        !item_stack.is_empty()
            && REGISTRY
                .items
                .is_in_tag(item_stack.item(), &ItemTag::FISHES)
    }
}

/// Returns whether this entity type is a vanilla `Guardian`.
///
/// Vanilla writes `Guardian.class`, which covers the elder guardian too.
fn is_guardian(entity_type: EntityTypeRef) -> bool {
    entity_type == &vanilla_entities::GUARDIAN || entity_type == &vanilla_entities::ELDER_GUARDIAN
}

impl Entity for DolphinEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `AgeableWaterCreature.baseTick` reads the air left
    /// before the shared tick spends it, but `Dolphin.handleAirSupply` is empty
    /// -- a dolphin does not drown, it dries out.
    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    /// Vanilla parity: `Dolphin.tick`.
    fn tick(&self) {
        self.default_tick();
        self.tick_moistness();
    }

    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if AgeableMob::is_baby(self) {
            Self::baby_dimensions(self.entity_type).scale(scale)
        } else if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    /// Vanilla parity: `AgeableWaterCreature.isPushedByFluid`.
    fn is_pushed_by_fluid(&self) -> bool {
        false
    }

    /// Vanilla parity: `Dolphin.getMaxAirSupply`; four minutes rather than the
    /// fifteen seconds every other air breather gets.
    fn max_air_supply(&self) -> i32 {
        TOTAL_AIR_SUPPLY
    }

    fn swim_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_DOLPHIN_SWIM
    }

    // Vanilla `Dolphin.getSwimSplashSound` returns `DOLPHIN_SPLASH`. Steel has
    // no splash-sound seam on `Entity`, so the shared splash plays instead.

    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {}

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        nbt.insert("GotFish", self.got_fish());
        nbt.insert("Moistness", self.moistness_level());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        self.set_got_fish(nbt.byte("GotFish").is_some_and(|flag| flag != 0));
        self.set_moistness_level(nbt.int("Moistness").unwrap_or(TOTAL_MOISTNESS_LEVEL));
    }
}

impl LivingEntity for DolphinEntity {
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

    /// Vanilla parity: `Dolphin.increaseAirSupply`; one breath fills the lungs.
    fn increase_air_supply(&self, _current_supply: i32) -> i32 {
        self.max_air_supply()
    }

    /// Vanilla parity: `Dolphin.getAgeScale`.
    fn get_age_scale(&self) -> f32 {
        if AgeableMob::is_baby(self) {
            BABY_SCALE
        } else {
            1.0
        }
    }

    /// Vanilla parity: `Dolphin.canDispenserEquipIntoSlot`.
    fn can_dispenser_equip_into_slot(&self, slot: EquipmentSlot) -> bool {
        slot == EquipmentSlot::MainHand && Mob::can_pick_up_loot(self)
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_DOLPHIN_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_DOLPHIN_DEATH)
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

    /// Vanilla parity: `Dolphin.travelInWater`, which pushes with the speed the
    /// smooth swimming move control already worked out.
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
        if self.target().is_none() {
            self.set_velocity(self.velocity() + DVec3::new(0.0, IDLE_SINK, 0.0));
        }
        result
    }
}

impl AgeableMob for DolphinEntity {
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

impl Animal for DolphinEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    /// Vanilla `Dolphin` has no `isFood`: a fish feeds a calf and makes an
    /// adult look for treasure, but it never puts one in love mode.
    fn is_food(&self, _item_stack: &ItemStack) -> bool {
        false
    }
}

impl Mob for DolphinEntity {
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

    /// Vanilla parity: `Dolphin.getAmbientSound`.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(if self.is_in_water() {
            &sound_events::ENTITY_DOLPHIN_AMBIENT_WATER
        } else {
            &sound_events::ENTITY_DOLPHIN_AMBIENT
        })
    }

    /// Vanilla parity: `AgeableWaterCreature.getAmbientSoundInterval`.
    fn ambient_sound_interval(&self) -> i32 {
        120
    }

    /// Vanilla parity: `Dolphin.playAttackSound`.
    fn play_attack_sound(&self) {
        self.play_sound(&sound_events::ENTITY_DOLPHIN_ATTACK, 1.0, 1.0);
    }

    /// Vanilla parity: `Dolphin.canAttack`; a calf picks no fights.
    fn can_attack(&self, target: &dyn LivingEntity) -> bool {
        !AgeableMob::is_baby(self) && self.mob_can_attack(target)
    }

    fn max_head_x_rot(&self) -> f32 {
        MAX_HEAD_ROT
    }

    fn max_head_y_rot(&self) -> f32 {
        MAX_HEAD_ROT
    }

    /// Vanilla parity: `Dolphin.canBeLeashed` overrides the
    /// `AgeableWaterCreature` refusal.
    fn can_be_leashed(&self) -> bool {
        true
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        let _ = spawn_reason;
        check_surface_water_animal_spawn_rules(world, pos)
    }

    /// Vanilla parity: `Dolphin.finalizeSpawn`, which fills the lungs and
    /// levels the pitch before the shared ageable half runs.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.set_air_supply(self.max_air_supply());
        let (yaw, _) = self.rotation();
        self.set_rotation((yaw, 0.0));

        let group_data = group_data.unwrap_or(SpawnGroupData::AgeableMob(
            AgeableMobGroupData::with_baby_spawn_chance(BABY_SPAWN_CHANCE),
        ));
        self.finalize_spawn_ageable_mob(world, spawn_reason, Some(group_data))
    }

    /// Vanilla parity: `Dolphin.mobInteract`. A fish either grows a calf up or
    /// puts an adult on the scent of treasure.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let item_stack = {
            let inventory = player.inventory.lock();
            let item_stack = inventory.get_item_in_hand(hand);
            item_stack.copy_with_count(item_stack.count())
        };
        if !Self::is_fish(&item_stack) {
            return Animal::mob_interact_animal(self, player, hand);
        }

        self.play_sound(&sound_events::ENTITY_DOLPHIN_EAT, 1.0, 1.0);

        if self.can_age_up() {
            Mob::use_player_item(self, player, hand);
            self.age_up(
                AgeableMobBase::get_speed_up_seconds_when_feeding(-self.get_age()),
                true,
            );
        } else {
            self.set_got_fish(true);
            Mob::use_player_item(self, player, hand);
        }

        InteractionResult::Success
    }

    /// Vanilla parity: `Dolphin` installs a `SmoothSwimmingMoveControl`.
    fn tick_move_control(&self) {
        SmoothSwimmingMoveControl::new(85, 10, 0.02, 0.1, true).tick(self);
    }

    /// Vanilla parity: `Dolphin` installs a `SmoothSwimmingLookControl`.
    fn tick_look_control(&self) {
        SmoothSwimmingLookControl::new(10).tick(self);
    }
}

impl PathfinderMob for DolphinEntity {
    /// Vanilla parity: `Dolphin.createNavigation` returns a
    /// `WaterBoundPathNavigation`, and the dolphin is the one mob vanilla lets
    /// breach out of it.
    fn navigation_kind(&self) -> NavigationKind {
        NavigationKind::WaterBound {
            allow_breaching: true,
        }
    }
}

/// Swims alongside a swimming player and keeps them in Dolphin's Grace.
///
/// Vanilla parity: `Dolphin.DolphinSwimWithPlayerGoal`.
struct DolphinSwimWithPlayerGoal {
    speed_modifier: f64,
    player: Option<Arc<Player>>,
}

impl DolphinSwimWithPlayerGoal {
    const fn new(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            player: None,
        }
    }

    fn grant_grace(player: &Player) {
        player.add_mob_effect(MobEffectInstance::with_duration(
            vanilla_mob_effects::DOLPHINS_GRACE,
            DOLPHINS_GRACE_TICKS,
            0,
        ));
    }
}

impl Goal for DolphinSwimWithPlayerGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE | GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(world) = mob.level() else {
            return false;
        };

        // Vanilla parity: `SWIM_WITH_PLAYER_TARGETING`.
        let targeting = TargetingConditions::for_non_combat()
            .range(SWIM_WITH_PLAYER_RANGE)
            .ignore_line_of_sight();
        let position = mob.position();
        let origin = DVec3::new(position.x, mob.get_eye_y(), position.z);
        self.player = world.nearest_player(origin, SWIM_WITH_PLAYER_RANGE, |player| {
            targeting.test(world.as_ref(), Some(mob), player)
        });

        let Some(player) = &self.player else {
            return false;
        };
        player.is_swimming()
            && mob
                .target()
                .is_none_or(|target| target.uuid() != player.uuid())
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.player.as_ref().is_some_and(|player| {
            player.is_swimming()
                && mob.position().distance_squared(player.position()) < SWIM_WITH_PLAYER_GIVE_UP_SQR
        })
    }

    fn start(&mut self, _mob: &dyn PathfinderMob) {
        if let Some(player) = self.player.clone() {
            Self::grant_grace(&player);
        }
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.player = None;
        mob.mob_base().navigation().lock().stop();
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(player) = self.player.clone() else {
            return;
        };

        let player_position = player.position();
        mob.mob_base().controls().lock().look_control.set_look_at(
            DVec3::new(player_position.x, player.get_eye_y(), player_position.z),
            mob.max_head_y_rot() + 20.0,
            mob.max_head_x_rot(),
        );

        if mob.position().distance_squared(player_position) < SWIM_WITH_PLAYER_STOP_SQR {
            mob.mob_base().navigation().lock().stop();
        } else {
            mob.move_to_pos(player_position, self.speed_modifier);
        }

        if player.is_swimming() && rand::random_range(0..DOLPHINS_GRACE_REFRESH_CHANCE) == 0 {
            Self::grant_grace(&player);
        }
    }
}

/// Fetches a dropped item, carries it about, and eventually throws it.
///
/// Vanilla parity: `Dolphin.PlayWithItemsGoal`.
struct PlayWithItemsGoal {
    cooldown: i32,
}

impl PlayWithItemsGoal {
    const fn new() -> Self {
        Self { cooldown: 0 }
    }

    /// Vanilla parity: `Dolphin.ALLOWED_ITEMS`.
    fn nearby_toys(mob: &dyn PathfinderMob) -> Vec<SharedEntity> {
        let Some(world) = mob.level() else {
            return Vec::new();
        };

        let search_box = mob.bounding_box().inflate_xyz(
            PLAY_ITEM_SEARCH_RANGE,
            PLAY_ITEM_SEARCH_RANGE,
            PLAY_ITEM_SEARCH_RANGE,
        );
        world.get_entities_in_aabb_matching(&search_box, |entity| {
            entity.downcast_ref::<ItemEntity>().is_some_and(|item| {
                !item.has_pickup_delay() && item.is_alive() && item.is_in_water()
            })
        })
    }

    fn held_item(mob: &dyn PathfinderMob) -> ItemStack {
        let mut held = ItemStack::empty();
        mob.with_equipment_slot(EquipmentSlot::MainHand, &mut |item_stack| {
            held = item_stack.copy_with_count(item_stack.count());
        });
        held
    }

    /// Vanilla parity: `setItemSlot(MAINHAND, ItemStack.EMPTY)`.
    fn clear_held_item(mob: &dyn PathfinderMob) {
        mob.with_equipment_slot_mut(EquipmentSlot::MainHand, &mut |item_stack| {
            *item_stack = ItemStack::empty();
        });
    }

    /// Vanilla parity: the private `drop` of `PlayWithItemsGoal`.
    fn throw_toy(mob: &dyn PathfinderMob, item_stack: ItemStack) {
        if item_stack.is_empty() {
            return;
        }
        let Some(world) = mob.level() else {
            return;
        };

        let position = mob.position();
        let (yaw, pitch) = mob.rotation();
        let yaw_radians = yaw.to_radians();
        let pitch_radians = pitch.to_radians();
        let direction = rand::random::<f32>() * TAU;
        let scatter = PLAY_ITEM_THROW_SCATTER * f64::from(rand::random::<f32>());
        let velocity = DVec3::new(
            PLAY_ITEM_THROW_POWER * f64::from(-yaw_radians.sin() * pitch_radians.cos())
                + f64::from(direction.cos()) * scatter,
            PLAY_ITEM_THROW_POWER * f64::from(pitch_radians.sin()) * 1.5,
            PLAY_ITEM_THROW_POWER * f64::from(yaw_radians.cos() * pitch_radians.cos())
                + f64::from(direction.sin()) * scatter,
        );

        let thrown_position = DVec3::new(
            position.x,
            mob.get_eye_y() - PLAY_ITEM_THROW_DROP,
            position.z,
        );
        if let Some(thrown) = world.spawn_item_with_velocity(thrown_position, item_stack, velocity)
        {
            thrown.set_pickup_delay(PLAY_ITEM_THROW_PICKUP_DELAY);
            thrown.set_thrower(mob.uuid());
        }
    }
}

impl Goal for PlayWithItemsGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if self.cooldown > mob.tick_count() {
            return false;
        }

        !Self::nearby_toys(mob).is_empty() || !Self::held_item(mob).is_empty()
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let toys = Self::nearby_toys(mob);
        if let Some(toy) = toys.first() {
            mob.move_to_pos(toy.position(), PLAY_ITEM_SPEED_MOD);
            mob.play_sound(&sound_events::ENTITY_DOLPHIN_PLAY, 1.0, 1.0);
        }

        self.cooldown = 0;
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        let held = Self::held_item(mob);
        if held.is_empty() {
            return;
        }

        Self::throw_toy(mob, held);
        Self::clear_held_item(mob);
        self.cooldown = mob.tick_count() + rand::random_range(0..PLAY_ITEM_MAX_COOLDOWN);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let held = Self::held_item(mob);
        if !held.is_empty() {
            Self::throw_toy(mob, held);
            Self::clear_held_item(mob);
            return;
        }

        let toys = Self::nearby_toys(mob);
        if let Some(toy) = toys.first() {
            mob.move_to_pos(toy.position(), PLAY_ITEM_SPEED_MOD);
        }
    }
}

#[cfg(test)]
mod tests;
