//! The camel.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.camel.Camel`. A camel is
//! an `AbstractHorse` that is tame from birth, carries two riders, sits down of
//! its own accord and has to be stood up before it will move, and dashes -- a
//! long forward leap on a fifty-five tick cooldown that is the only way a
//! player crosses a ravine on one.
//!
//! Everything a camel husk shares with it -- which is everything except the
//! sounds, the food tag and the breeding -- lives in
//! [`super::camel_common`], because vanilla's `CamelHusk` is a subclass with no
//! fields of its own.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::entity_type::{EntityDimensions, EntityTypeRef};
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::CamelEntityData;
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use super::camel_ai;
use super::camel_common::{
    BABY_SCALE, CamelBase, CamelLike, MAX_HEAD_Y_ROT, check_camel_spawn_rules, hooks_for,
};
use crate::behavior::InteractionResult;
use crate::entity::ai::brain::Brain;
use crate::entity::damage::DamageSource;
use crate::entity::{
    AbstractHorse, AbstractHorseBase, AgeableMob, AgeableMobBase, Animal, AnimalBase, Entity,
    EntityBase, EntityBaseLoad, EntityPose, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, LivingEntitySyncedData, Mob, MobBase, MoveResult, PathfinderMob,
    SpawnGroupData,
};
use crate::player::Player;
use crate::world::World;

#[cfg(test)]
mod tests;

/// A camel.
#[entity_behavior(class = "Camel")]
pub struct CamelEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    abstract_horse_base: AbstractHorseBase,
    camel_base: CamelBase,
    brain: Brain,
    entity_data: SyncMutex<CamelEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `CamelEntity`.
unsafe impl DowncastType for CamelEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/camel");
}

impl CamelEntity {
    /// Creates a camel at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a camel from saved base data.
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
            // Vanilla parity: the two navigation calls of the `Camel`
            // constructor -- a camel is tall enough to step over a fence.
            let mut navigation = mob_base.navigation().lock();
            navigation.set_can_float(true);
            navigation.set_can_walk_over_fences(true);
        }
        let mut entity_data = CamelEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            abstract_horse_base: AbstractHorseBase::new(0),
            camel_base: CamelBase::new(),
            brain: camel_ai::make_brain(hooks_for::<Self>()),
            entity_data: SyncMutex::new(entity_data),
        }
    }
}

impl Entity for CamelEntity {
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

    /// Vanilla parity: `Camel.tick`.
    fn tick(&self) {
        self.camel_tick();
    }

    /// Vanilla parity: `Camel.getDefaultDimensions`.
    fn dimensions_for_pose(&self, pose: EntityPose) -> EntityDimensions {
        self.camel_dimensions_for_pose(pose)
    }

    /// Vanilla parity: `Camel.playStepSound`.
    fn play_step_sound(&self, _pos: BlockPos, block_state: BlockStateId) {
        self.camel_play_step_sound(block_state);
    }

    /// Vanilla parity: `Camel.canAddPassenger`.
    fn can_add_passenger(&self, _passenger: &dyn Entity) -> bool {
        self.camel_can_add_passenger()
    }

    /// Vanilla parity: `Camel.handleStartJump`.
    fn handle_start_jump(&self, _jump_scale: i32) {
        self.camel_handle_start_jump();
    }

    /// Vanilla parity: `Camel.canJump`.
    fn can_jump_while_ridden(&self) -> bool {
        self.camel_can_jump_while_ridden()
    }

    /// Vanilla parity: `Camel.openCustomInventoryScreen`.
    fn open_custom_inventory_screen(&self, player: &Player) {
        self.open_horse_inventory_screen(player);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_camel(nbt);
        self.brain.save(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_camel(nbt);
        self.brain.load(nbt);
    }
}

impl LivingEntity for CamelEntity {
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
        Some(self.camel_hurt_sound())
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(self.camel_death_sound())
    }

    fn get_age_scale(&self) -> f32 {
        if AgeableMob::is_baby(self) {
            BABY_SCALE
        } else {
            1.0
        }
    }

    /// Vanilla parity: `Camel.actuallyHurt`.
    fn actually_hurt(&self, world: &World, source: &DamageSource, amount: f32) {
        self.camel_actually_hurt(world, source, amount);
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    /// Vanilla parity: `Camel.travel`.
    fn travel(&self, input: DVec3) -> Option<MoveResult> {
        self.camel_travel(input)
    }

    /// Vanilla parity: `Camel.tickRidden`.
    fn tick_ridden(&self, controller: &Player, ridden_input: DVec3) {
        self.camel_tick_ridden(controller, ridden_input);
    }

    /// Vanilla parity: `Camel.getRiddenInput`.
    fn ridden_input(&self, controller: &Player, _self_input: DVec3) -> DVec3 {
        self.camel_ridden_input(controller)
    }

    /// Vanilla parity: `Camel.getRiddenSpeed`.
    fn ridden_speed(&self, controller: &Player) -> f32 {
        self.camel_ridden_speed(controller)
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
        result
    }
}

impl AgeableMob for CamelEntity {
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

impl Animal for CamelEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        self.is_camel_food(item_stack)
    }
}

impl AbstractHorse for CamelEntity {
    fn abstract_horse_base(&self) -> &AbstractHorseBase {
        &self.abstract_horse_base
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

    /// Vanilla parity: `Camel.onPlayerJump`.
    fn on_player_jump(&self, jump_amount: i32) {
        self.camel_on_player_jump(jump_amount);
    }

    /// Vanilla parity: `Camel.executeRidersJump`, which is the dash rather than
    /// the horse's vertical hop.
    fn execute_riders_jump(&self, amount: f32, _input: DVec3) {
        self.execute_camel_dash(amount);
    }

    /// Vanilla parity: `Camel.isTamed`, a flat `true` -- a camel needs no
    /// breaking in, only a saddle.
    fn is_tamed(&self) -> bool {
        true
    }

    /// Vanilla parity: `Camel.canPerformRearing`, which is `false`. A camel
    /// dashes rather than rears.
    fn can_perform_rearing(&self) -> bool {
        false
    }

    /// Vanilla parity: `Camel.handleEating`.
    fn handle_eating(&self, player: &Player, item_stack: &ItemStack) -> bool {
        self.camel_handle_eating(player, item_stack)
    }
}

impl Mob for CamelEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    /// Vanilla parity: `AbstractHorse.supportQuadLeash`.
    fn support_quad_leash(&self) -> bool {
        true
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

    /// Vanilla parity: `Camel.customServerAiStep`.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.brain.tick(&world, self);
        camel_ai::update_activity(&self.brain);
        Animal::custom_server_ai_step_animal(self);
    }

    /// Vanilla parity: `Camel.CamelMoveControl.tick`.
    fn tick_move_control(&self) {
        self.camel_tick_move_control();
    }

    /// Vanilla parity: `Camel.CamelLookControl.tick`.
    fn tick_look_control(&self) {
        self.camel_tick_look_control();
    }

    /// Vanilla parity: `Camel.getMaxHeadYRot`.
    fn max_head_y_rot(&self) -> f32 {
        MAX_HEAD_Y_ROT
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(self.camel_ambient_sound())
    }

    /// Vanilla parity: the `Camel::checkCamelSpawnRules` the entity type is
    /// registered with in `SpawnPlacements`.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        let _ = spawn_reason;
        check_camel_spawn_rules::<Self>(world, pos)
    }

    /// Vanilla parity: `Camel.finalizeSpawn`, which starts every camel fully
    /// stood up rather than mid-animation.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.reset_last_pose_change_tick_to_full_stand(world.game_time());
        self.finalize_spawn_ageable_mob(world, spawn_reason, group_data)
    }

    /// Vanilla parity: `Camel.mobInteract`.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        self.camel_mob_interact(player, hand)
    }
}

impl PathfinderMob for CamelEntity {}

impl CamelLike for CamelEntity {
    fn camel_base(&self) -> &CamelBase {
        &self.camel_base
    }

    fn dash_flag(&self) -> bool {
        *self.entity_data.lock().dash.get()
    }

    fn store_dash_flag(&self, dashing: bool) {
        self.entity_data.lock().dash.set(dashing);
    }

    fn stored_last_pose_change_tick(&self) -> i64 {
        *self.entity_data.lock().last_pose_change_tick.get()
    }

    fn store_last_pose_change_tick(&self, tick: i64) {
        self.entity_data.lock().last_pose_change_tick.set(tick);
    }
}
