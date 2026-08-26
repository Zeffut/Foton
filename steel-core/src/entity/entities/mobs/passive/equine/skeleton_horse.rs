//! The vanilla skeleton horse.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.equine.SkeletonHorse`. A
//! horse that swims instead of drowning, and -- when a thunderstorm spawns it as
//! a trap -- waits for someone to walk close before the sky opens.

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
use steel_registry::vanilla_entity_data::SkeletonHorseEntityData;
use steel_registry::{sound_events, vanilla_attributes, vanilla_entities};
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use crate::behavior::InteractionResult;
use crate::entity::damage::DamageSource;
use crate::entity::entities::mobs::passive::equine::skeleton_trap_goal::SkeletonTrapGoal;
use crate::entity::entities::mobs::passive::equine::{
    add_abstract_horse_goals, sync_mob_effect_entity_data,
};
use crate::entity::{
    AbstractHorse, AbstractHorseBase, AgeableMob, AgeableMobBase, Animal, AnimalBase, BABY_SCALE,
    ENTITIES, Entity, EntityBase, EntityBaseLoad, EntityPose, EntitySpawnReason, EntitySyncedData,
    LivingEntity, LivingEntityBase, LivingEntitySyncedData, Mob, MobBase, MoveResult,
    PathfinderMob, RemovalReason, SharedEntity, SpawnGroupData, generate_jump_strength,
    next_entity_id,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::player::Player;
use crate::world::World;

/// Ticks a trap horse waits before giving up and vanishing.
///
/// Vanilla parity: `SkeletonHorse.TRAP_MAX_LIFE`.
const TRAP_MAX_LIFE: i32 = 18000;

/// Where a skeleton foal carries its rider.
///
/// Vanilla parity: `SkeletonHorse.BABY_DIMENSIONS`, attached at
/// `SKELETON_HORSE.getHeight() - 0.25F` and scaled by `0.7F`.
const SKELETON_HORSE_BABY_PASSENGER_ATTACHMENTS: [EntityAttachmentPoint; 1] =
    [EntityAttachmentPoint::new(
        0.0,
        (1.6 - 0.25) * BABY_SCALE as f64,
        0.0,
    )];

/// A skeleton foal's hitbox.
const SKELETON_HORSE_BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new_with_attachments(
    1.396_484_4 * BABY_SCALE,
    1.6 * BABY_SCALE,
    1.52 * BABY_SCALE,
    EntityAttachments::new(&SKELETON_HORSE_BABY_PASSENGER_ATTACHMENTS, &[], &[], &[]),
);

/// Vanilla `SkeletonHorse.getWaterSlowDown`.
const WATER_SLOW_DOWN: f32 = 0.96;

/// Trap state vanilla keeps on the entity itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SkeletonHorseState {
    is_trap: bool,
    trap_time: i32,
}

/// A vanilla skeleton horse.
#[entity_behavior(class = "SkeletonHorse")]
pub struct SkeletonHorseEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    horse_base: AbstractHorseBase,
    trap: SyncMutex<SkeletonHorseState>,
    entity_data: SyncMutex<SkeletonHorseEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SkeletonHorseEntity`.
unsafe impl DowncastType for SkeletonHorseEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/skeleton_horse");
}

impl SkeletonHorseEntity {
    /// Creates a new skeleton horse.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a skeleton horse from saved base data.
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
        let mut entity_data = SkeletonHorseEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            let mut goals = mob_base.goal_selector().lock();
            add_abstract_horse_goals(&mut goals, true);
            // Vanilla parity: `SkeletonHorse.addBehaviourGoals` is empty, and
            // `setTrap` adds and removes the trap goal as the flag flips. Steel's
            // goal selector has no removal, so the goal is registered once and
            // asks the horse whether it is still a trap -- the same outcome,
            // because the goal clears the flag on its first tick.
            goals.add_goal(1, SkeletonTrapGoal::new());
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            horse_base: AbstractHorseBase::new(0),
            trap: SyncMutex::new(SkeletonHorseState {
                is_trap: false,
                trap_time: 0,
            }),
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Returns vanilla `SkeletonHorse.isTrap`.
    #[must_use]
    pub fn is_trap(&self) -> bool {
        self.trap.lock().is_trap
    }

    /// Applies vanilla `SkeletonHorse.setTrap`.
    pub fn set_trap(&self, is_trap: bool) {
        self.trap.lock().is_trap = is_trap;
    }

    /// Returns vanilla `SkeletonHorse.checkSkeletonHorseSpawnRules`.
    #[must_use]
    pub fn check_skeleton_horse_spawn_rules(
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        if !spawn_reason.is_spawner() {
            return <Self as Animal>::check_animal_spawn_rules(world.as_ref(), spawn_reason, pos);
        }
        spawn_reason.ignores_light_requirements()
            || <Self as Animal>::is_bright_enough_to_spawn(world.as_ref(), pos)
    }

    fn tick_trap_life(&self) {
        if self.is_persistence_required() || !self.is_trap() {
            return;
        }

        let expired = {
            let mut trap = self.trap.lock();
            trap.trap_time += 1;
            trap.trap_time >= TRAP_MAX_LIFE
        };
        if expired {
            self.set_removed(RemovalReason::Discarded);
        }
    }
}

impl Entity for SkeletonHorseEntity {
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
        LivingEntity::tick_living_entity(self);
        self.tick_abstract_horse();
    }

    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if AgeableMob::is_baby(self) {
            SKELETON_HORSE_BABY_DIMENSIONS.scale(scale)
        } else if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    fn controlling_passenger(&self) -> Option<SharedEntity> {
        if Mob::is_saddled(self)
            && let Some(passenger) = self.first_passenger()
            && passenger.as_player().is_some()
        {
            return Some(passenger);
        }
        self.controlling_passenger_mob()
    }

    fn passenger_attachment_point(&self, passenger: &dyn Entity) -> DVec3 {
        let scale = LivingEntity::get_scale(self);
        self.default_passenger_attachment_point(passenger)
            + self.abstract_horse_rearing_rider_offset(scale)
    }

    fn on_climbable(&self) -> bool {
        false
    }

    fn is_pushable(&self) -> bool {
        !self.is_vehicle()
    }

    fn can_jump_while_ridden(&self) -> bool {
        Mob::is_saddled(self)
    }

    fn handle_start_jump(&self, _jump_scale: i32) {
        self.handle_start_jump_abstract_horse();
    }

    fn open_custom_inventory_screen(&self, player: &Player) {
        if (self.is_vehicle() && !self.has_passenger(player)) || !self.is_tamed() {
            return;
        }
        self.open_horse_inventory_screen(player);
    }

    fn cause_fall_damage(
        &self,
        fall_distance: f64,
        damage_modifier: f32,
        source: &DamageSource,
    ) -> bool {
        self.abstract_horse_cause_fall_damage(fall_distance, damage_modifier, source)
    }

    /// Vanilla parity: `SkeletonHorse.getSwimSound`, which gallops through water.
    fn swim_sound(&self) -> SoundEventRef {
        if !self.on_ground() {
            return &sound_events::ENTITY_SKELETON_HORSE_SWIM;
        }
        if !self.is_vehicle() {
            return &sound_events::ENTITY_SKELETON_HORSE_STEP_WATER;
        }

        let gallop_sound_counter = self.bump_gallop_sound_counter();
        if gallop_sound_counter > 5 && gallop_sound_counter % 3 == 0 {
            return &sound_events::ENTITY_SKELETON_HORSE_GALLOP_WATER;
        }
        if gallop_sound_counter <= 5 {
            return &sound_events::ENTITY_SKELETON_HORSE_STEP_WATER;
        }
        &sound_events::ENTITY_SKELETON_HORSE_SWIM
    }

    /// Vanilla parity: `SkeletonHorse.playSwimSound`.
    fn play_swim_sound(&self, volume: f32) {
        if self.on_ground() {
            self.default_play_swim_sound(0.3);
        } else {
            self.default_play_swim_sound((volume * 25.0).min(0.1));
        }
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn update_data_before_sync(&self) {
        sync_mob_effect_entity_data(self, &self.entity_data);
    }

    fn play_step_sound(&self, pos: BlockPos, block_state: BlockStateId) {
        self.abstract_horse_play_step_sound(pos, block_state);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        self.save_abstract_horse(nbt);
        let trap = *self.trap.lock();
        nbt.insert("SkeletonTrap", i8::from(trap.is_trap));
        nbt.insert("SkeletonTrapTime", trap.trap_time);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        self.load_abstract_horse(nbt);
        let mut trap = self.trap.lock();
        trap.is_trap = nbt.byte("SkeletonTrap").is_some_and(|value| value != 0);
        trap.trap_time = nbt.int("SkeletonTrapTime").unwrap_or(0);
    }
}

impl LivingEntity for SkeletonHorseEntity {
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

    fn sound_volume(&self) -> f32 {
        0.8
    }

    fn get_water_slow_down(&self) -> f32 {
        WATER_SLOW_DOWN
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_SKELETON_HORSE_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_SKELETON_HORSE_DEATH)
    }

    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        let was_hurt = self.living_hurt_server(world, source, amount);
        self.abstract_horse_react_to_hurt(was_hurt)
    }

    /// Vanilla parity: `SkeletonHorse.canUseSlot`, which never gates the saddle.
    fn can_use_slot(&self, _slot: EquipmentSlot) -> bool {
        true
    }

    fn can_dispenser_equip_into_slot(&self, slot: EquipmentSlot) -> bool {
        self.abstract_horse_can_dispenser_equip_into_slot(slot)
    }

    fn equip_sound(&self, slot: EquipmentSlot, stack: &ItemStack) -> Option<SoundEventRef> {
        if slot == EquipmentSlot::Saddle {
            return Some(&sound_events::ENTITY_HORSE_SADDLE);
        }
        LivingEntity::default_equip_sound(self, slot, stack)
    }

    fn is_immobile(&self) -> bool {
        self.abstract_horse_is_immobile()
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn tick_ridden(&self, controller: &Player, ridden_input: DVec3) {
        self.tick_ridden_abstract_horse(controller, ridden_input);
    }

    fn ridden_input(&self, controller: &Player, _self_input: DVec3) -> DVec3 {
        self.abstract_horse_ridden_input(controller)
    }

    fn ridden_speed(&self, _controller: &Player) -> f32 {
        self.abstract_horse_ridden_speed()
    }

    fn drop_custom_death_loot(&self, _source: &DamageSource, _killed_by_player: bool) {
        self.drop_abstract_horse_inventory();
    }

    fn ai_step(&self) -> Option<MoveResult> {
        self.ai_step_abstract_horse();
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);
        self.server_ai_step_abstract_horse();
        self.tick_trap_life();
        result
    }
}

impl AgeableMob for SkeletonHorseEntity {
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

    /// Vanilla parity: `SkeletonHorse.canAgeUp`, which is why a skeleton foal
    /// never grows.
    fn can_age_up(&self) -> bool {
        false
    }
}

impl Animal for SkeletonHorseEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        self.is_horse_food(item_stack)
    }

    /// Vanilla parity: `SkeletonHorse.getBreedOffspring`.
    fn get_breed_offspring(
        &self,
        world: &Arc<World>,
        _partner: &dyn Animal,
    ) -> Option<SharedEntity> {
        ENTITIES.create(
            &vanilla_entities::SKELETON_HORSE,
            next_entity_id(),
            self.position(),
            Arc::downgrade(world),
        )
    }
}

impl AbstractHorse for SkeletonHorseEntity {
    fn abstract_horse_base(&self) -> &AbstractHorseBase {
        &self.horse_base
    }

    fn horse_flags(&self) -> i8 {
        *self.entity_data.lock().abstract_horse().id_flags.get()
    }

    fn set_horse_flags(&self, flags: i8) {
        self.entity_data
            .lock()
            .abstract_horse_mut()
            .id_flags
            .set(flags);
    }

    /// Vanilla parity: `SkeletonHorse.playJumpSound`.
    fn play_jump_sound(&self) {
        if self.is_in_water() {
            self.play_sound(&sound_events::ENTITY_SKELETON_HORSE_JUMP_WATER, 0.4, 1.0);
        } else {
            self.play_sound(&sound_events::ENTITY_HORSE_JUMP, 0.4, 1.0);
        }
    }

    /// Vanilla parity: `SkeletonHorse.randomizeAttributes`, jump strength only.
    fn randomize_attributes(&self) {
        self.attributes().lock().set_base_value(
            vanilla_attributes::JUMP_STRENGTH,
            generate_jump_strength(&mut || rand::random::<f64>()),
        );
    }
}

impl Mob for SkeletonHorseEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    /// Vanilla parity: `AbstractHorse.supportQuadLeash`.
    fn support_quad_leash(&self) -> bool {
        true
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        Self::check_skeleton_horse_spawn_rules(world, spawn_reason, pos)
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

    fn ambient_sound_interval(&self) -> i32 {
        400
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(if self.is_eye_in_water() {
            &sound_events::ENTITY_SKELETON_HORSE_AMBIENT_WATER
        } else {
            &sound_events::ENTITY_SKELETON_HORSE_AMBIENT
        })
    }

    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.finalize_spawn_abstract_horse(world, spawn_reason, group_data)
    }

    /// Vanilla parity: `SkeletonHorse.mobInteract`, which ignores an untamed horse.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        if !self.is_tamed() {
            return InteractionResult::Pass;
        }
        self.abstract_horse_mob_interact(player, hand)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for SkeletonHorseEntity {}
