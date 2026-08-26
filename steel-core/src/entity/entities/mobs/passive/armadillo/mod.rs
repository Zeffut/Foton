//! Armadillo entity.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.armadillo.Armadillo`. An
//! armadillo balls up when something frightens it -- an undead, whatever hit
//! it, or a player sprinting or riding past -- and while it is balled up it
//! takes half the damage, cannot be bred, and refuses every interaction but a
//! brush. It sheds a scute every five to ten minutes on its own, and a brush
//! takes one early.

mod armadillo_ai;

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::entity_data::ArmadilloState;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::game_events::GameEventRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_damage_type_tags::DamageTypeTag;
use steel_registry::vanilla_entity_data::ArmadilloEntityData;
use steel_registry::vanilla_entity_type_tags::EntityTypeTag;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_game_events, vanilla_items,
};
use steel_utils::entity_events::EntityStatus;
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use crate::behavior::InteractionResult;
use crate::entity::ai::brain::Brain;
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::damage::DamageSource;
use crate::entity::{
    AgeableMob, AgeableMobBase, Animal, AnimalBase, Entity, EntityBase, EntityBaseLoad,
    EntityEventSource as _, EntitySpawnReason, EntitySyncedData, LivingEntity, LivingEntityBase,
    LivingEntitySyncedData, LivingTravelInput, Mob, MobBase, MoveResult, PathfinderMob,
};
use crate::player::Player;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader as _, World};

/// Vanilla parity: `Armadillo.SCARE_CHECK_INTERVAL`, which is also how long the
/// danger memory lives.
pub const SCARE_CHECK_INTERVAL: i64 = 80;
/// Vanilla parity: `Armadillo.SCARE_DISTANCE_HORIZONTAL`.
const SCARE_DISTANCE_HORIZONTAL: f64 = 7.0;
/// Vanilla parity: `Armadillo.SCARE_DISTANCE_VERTICAL`.
const SCARE_DISTANCE_VERTICAL: f64 = 2.0;
/// Vanilla parity: `Armadillo.MAX_HEAD_ROTATION_EXTENT`; the `getMaxHeadYRot`
/// beside it is the integer `32`.
const MAX_HEAD_Y_ROT: f32 = 32.0;
/// Vanilla parity: the `hurtAndBreak(16, ...)` a brush costs.
const BRUSH_DURABILITY_COST: i32 = 16;
/// Vanilla parity: the `0.15F` volume of `Armadillo.playStepSound`.
const STEP_SOUND_VOLUME: f32 = 0.15;
/// Vanilla parity: the `20 * SECONDS_PER_MINUTE * 5` of `pickNextScuteDropTime`,
/// which is five minutes plus up to five more.
const SCUTE_DROP_MIN_TICKS: i32 = 20 * 60 * 5;

/// Vanilla parity: the `(damage - 1.0F) / 2.0F` of `Armadillo.hurtServer`.
const SHELL_DAMAGE_OFFSET: f32 = 1.0;
const SHELL_DAMAGE_DIVISOR: f32 = 2.0;

/// How long each armadillo state's animation lasts.
///
/// Vanilla parity: the `animationDuration` column of `ArmadilloState`. These
/// are not decoration: the ball-up behavior compares the danger memory's
/// remaining life against the unrolling duration, which is what makes an
/// armadillo start opening up before the danger is fully gone.
pub(super) const fn armadillo_state_animation_duration(state: ArmadilloState) -> i64 {
    match state {
        ArmadilloState::Idle => 0,
        ArmadilloState::Rolling => 10,
        ArmadilloState::Scared => 50,
        ArmadilloState::Unrolling => 30,
    }
}

/// Vanilla parity: `ArmadilloState.isThreatened`.
pub(super) const fn armadillo_state_is_threatened(state: ArmadilloState) -> bool {
    !matches!(state, ArmadilloState::Idle)
}

/// Vanilla parity: `ArmadilloState.shouldHideInShell`, whose four bodies are
/// what decide when the shell is actually closed rather than closing.
const fn should_hide_in_shell(state: ArmadilloState, ticks_in_state: i64) -> bool {
    match state {
        ArmadilloState::Idle => false,
        ArmadilloState::Rolling => ticks_in_state > 5,
        ArmadilloState::Scared => true,
        ArmadilloState::Unrolling => ticks_in_state < 26,
    }
}

/// Vanilla parity: `ArmadilloState.getSerializedName`.
const fn state_name(state: ArmadilloState) -> &'static str {
    match state {
        ArmadilloState::Idle => "idle",
        ArmadilloState::Rolling => "rolling",
        ArmadilloState::Scared => "scared",
        ArmadilloState::Unrolling => "unrolling",
    }
}

/// Vanilla parity: the `StringRepresentable.fromEnum` decode of the same.
fn state_from_name(name: &str) -> Option<ArmadilloState> {
    match name {
        "idle" => Some(ArmadilloState::Idle),
        "rolling" => Some(ArmadilloState::Rolling),
        "scared" => Some(ArmadilloState::Scared),
        "unrolling" => Some(ArmadilloState::Unrolling),
        _ => None,
    }
}

/// An armadillo.
#[entity_behavior(class = "Armadillo")]
pub struct ArmadilloEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    brain: Brain,
    /// Vanilla parity: `Armadillo.inStateTicks`.
    in_state_ticks: SyncMutex<i64>,
    /// Vanilla parity: `Armadillo.scuteTime`.
    scute_time: SyncMutex<i32>,
    entity_data: SyncMutex<ArmadilloEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `ArmadilloEntity`.
unsafe impl DowncastType for ArmadilloEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/armadillo");
}

impl ArmadilloEntity {
    /// Creates an armadillo at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates an armadillo from saved base data.
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
        // Vanilla parity: the `getNavigation().setCanFloat(true)` of the
        // constructor.
        mob_base.navigation().lock().set_can_float(true);
        let mut entity_data = ArmadilloEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            brain: armadillo_ai::make_brain(),
            in_state_ticks: SyncMutex::new(0),
            scute_time: SyncMutex::new(Self::pick_next_scute_drop_time()),
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Vanilla parity: `Armadillo.pickNextScuteDropTime`.
    fn pick_next_scute_drop_time() -> i32 {
        rand::random_range(0..SCUTE_DROP_MIN_TICKS) + SCUTE_DROP_MIN_TICKS
    }

    /// Returns vanilla `Armadillo.getState`.
    #[must_use]
    pub fn state(&self) -> ArmadilloState {
        *self.entity_data.lock().armadillo_state.get()
    }

    /// Sets vanilla `Armadillo.switchToState`.
    ///
    /// Vanilla resets `inStateTicks` from `onSyncedDataUpdated`; Steel does it
    /// here, which is the only place the value changes.
    pub fn switch_to_state(&self, state: ArmadilloState) {
        if self.state() == state {
            return;
        }
        self.entity_data.lock().armadillo_state.set(state);
        *self.in_state_ticks.lock() = 0;
    }

    /// Returns how long this armadillo has been in its current state.
    #[must_use]
    pub fn in_state_ticks(&self) -> i64 {
        *self.in_state_ticks.lock()
    }

    /// Returns vanilla `Armadillo.isScared`.
    #[must_use]
    pub fn is_scared(&self) -> bool {
        self.state() != ArmadilloState::Idle
    }

    /// Returns vanilla `Armadillo.shouldHideInShell`.
    #[must_use]
    pub fn should_hide_in_shell(&self) -> bool {
        should_hide_in_shell(self.state(), self.in_state_ticks())
    }

    /// Returns vanilla `Armadillo.shouldSwitchToScaredState`.
    #[must_use]
    pub fn should_switch_to_scared_state(&self) -> bool {
        self.state() == ArmadilloState::Rolling
            && self.in_state_ticks() > armadillo_state_animation_duration(ArmadilloState::Rolling)
    }

    /// Vanilla parity: `Armadillo.rollUp`.
    pub fn roll_up(&self) {
        if self.is_scared() {
            return;
        }
        // Vanilla parity: `Entity.stopInPlace`, which is what makes an
        // armadillo stop dead rather than skidding into its ball.
        self.mob_base.navigation().lock().stop();
        self.set_travel_input(LivingTravelInput::ZERO);
        self.set_mob_speed(0.0);
        let velocity = self.velocity();
        self.set_velocity(DVec3::new(0.0, velocity.y, 0.0));
        self.reset_love();
        self.armadillo_game_event(&vanilla_game_events::ENTITY_ACTION);
        self.make_sound(Some(&sound_events::ENTITY_ARMADILLO_ROLL));
        self.switch_to_state(ArmadilloState::Rolling);
    }

    /// Vanilla parity: `Armadillo.rollOut`.
    pub fn roll_out(&self) {
        if !self.is_scared() {
            return;
        }
        self.armadillo_game_event(&vanilla_game_events::ENTITY_ACTION);
        self.make_sound(Some(&sound_events::ENTITY_ARMADILLO_UNROLL_FINISH));
        self.switch_to_state(ArmadilloState::Idle);
    }

    fn armadillo_game_event(&self, event: GameEventRef) {
        let Some(world) = self.level() else {
            return;
        };
        world.game_event(
            event,
            self.block_position(),
            &GameEventContext::new(Some(self.as_entity_event_source()), None),
        );
    }

    /// Returns vanilla `Armadillo.canStayRolledUp`.
    ///
    /// An armadillo that is panicking, swimming, leashed, riding or ridden
    /// cannot stay in its shell -- which is also why picking one up unrolls it.
    #[must_use]
    pub fn can_stay_rolled_up(&self) -> bool {
        !(self.is_panicking()
            || self.is_in_water()
            || self.is_in_lava()
            || self.is_leashed()
            || self.is_passenger()
            || self.is_vehicle())
    }

    /// Returns vanilla `Armadillo.isScaredBy`.
    ///
    /// Three things frighten an armadillo: an undead, whatever last hit it, and
    /// a player who is sprinting or riding something. A player walking past is
    /// ignored, which is what makes sneaking up on one possible.
    #[must_use]
    pub fn is_scared_by(&self, other: &dyn LivingEntity) -> bool {
        let scare_box = self.bounding_box().inflate_xyz(
            SCARE_DISTANCE_HORIZONTAL,
            SCARE_DISTANCE_VERTICAL,
            SCARE_DISTANCE_HORIZONTAL,
        );
        if !scare_box.intersects(other.bounding_box()) {
            return false;
        }
        if REGISTRY
            .entity_types
            .is_in_tag(other.entity_type(), &EntityTypeTag::UNDEAD)
        {
            return true;
        }
        if self
            .last_hurt_by_mob()
            .is_some_and(|hurt_by| hurt_by.id() == other.id())
        {
            return true;
        }
        let Some(player) = other.as_player() else {
            return false;
        };
        !player.is_spectator() && (player.is_sprinting() || player.is_passenger())
    }

    /// Vanilla parity: `Armadillo.brushOffScute`.
    ///
    /// Steel has no loot tables, so the single-entry `brush/armadillo` table is
    /// the scute written out.
    pub fn brush_off_scute(&self) -> bool {
        if AgeableMob::is_baby(self) {
            return false;
        }
        if self.level().is_some() {
            self.spawn_at_location(ItemStack::new(&vanilla_items::ARMADILLO_SCUTE), 0.0);
            self.play_sound(&sound_events::ENTITY_ARMADILLO_BRUSH, 1.0, 1.0);
            self.armadillo_game_event(&vanilla_game_events::ENTITY_INTERACT);
        }
        true
    }

    /// Vanilla parity: the scute half of `Armadillo.customServerAiStep`.
    fn tick_scute_shed(&self, world: &Arc<World>) {
        if !Entity::is_alive(self) {
            return;
        }
        let due = {
            let mut scute_time = self.scute_time.lock();
            *scute_time -= 1;
            *scute_time <= 0
        };
        if !due || !self.should_drop_loot(world) {
            return;
        }

        // Vanilla parity: the one-entry `gameplay/armadillo_shed` gift table,
        // which always pays out; Steel has no loot tables.
        self.spawn_at_location(ItemStack::new(&vanilla_items::ARMADILLO_SCUTE), 0.0);
        self.play_sound(
            &sound_events::ENTITY_ARMADILLO_SCUTE_DROP,
            1.0,
            (rand::random::<f32>() - rand::random::<f32>()) * 0.2 + 1.0,
        );
        self.armadillo_game_event(&vanilla_game_events::ENTITY_PLACE);
        *self.scute_time.lock() = Self::pick_next_scute_drop_time();
    }

    /// Returns whether the stack is vanilla armadillo food.
    #[must_use]
    pub fn is_armadillo_food(item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::ARMADILLO_FOOD)
    }

    /// Vanilla parity: `Armadillo.checkArmadilloSpawnRules`.
    #[must_use]
    pub fn check_armadillo_spawn_rules(world: &Arc<World>, pos: BlockPos) -> bool {
        world
            .get_block_state(pos.below())
            .get_block()
            .has_tag(&BlockTag::ARMADILLO_SPAWNABLE_ON)
            && <Self as Animal>::is_bright_enough_to_spawn(world.as_ref(), pos)
    }

    /// Vanilla parity: the `(byte)64` entity event, which is what makes a
    /// balled-up armadillo peek out and play the sound.
    pub fn broadcast_peek(&self) {
        self.broadcast_entity_event(EntityStatus::ArmadilloPeek);
    }
}

impl Entity for ArmadilloEntity {
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

    /// Vanilla parity: `Armadillo.tick`. The head clamp is what stops a balled
    /// armadillo swiveling to watch what frightened it.
    fn tick(&self) {
        LivingEntity::tick_living_entity(self);
        if self.is_scared() {
            self.set_y_head_rot(self.y_body_rot());
        }
        *self.in_state_ticks.lock() += 1;
    }

    /// Vanilla parity: `Armadillo.playStepSound`.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(&sound_events::ENTITY_ARMADILLO_STEP, STEP_SOUND_VOLUME, 1.0);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        nbt.insert("state", state_name(self.state()));
        nbt.insert("scute_time", *self.scute_time.lock());
        self.brain.save(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        let state = nbt
            .string("state")
            .and_then(|name| state_from_name(name.to_str().as_ref()))
            .unwrap_or(ArmadilloState::Idle);
        self.switch_to_state(state);
        if let Some(scute_time) = nbt.int("scute_time") {
            *self.scute_time.lock() = scute_time;
        }
        self.brain.load(nbt);
    }
}

impl LivingEntity for ArmadilloEntity {
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

    /// Vanilla parity: `Armadillo.getHurtSound`.
    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(if self.is_scared() {
            &sound_events::ENTITY_ARMADILLO_HURT_REDUCED
        } else {
            &sound_events::ENTITY_ARMADILLO_HURT
        })
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ARMADILLO_DEATH)
    }

    /// Vanilla parity: `Armadillo.hurtServer`, which is the shell: a blow that
    /// lands on a balled-up armadillo is halved after a point is taken off it.
    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        let amount = if self.is_scared() {
            (amount - SHELL_DAMAGE_OFFSET) / SHELL_DAMAGE_DIVISOR
        } else {
            amount
        };
        self.living_hurt_server(world, source, amount)
    }

    /// Vanilla parity: `Armadillo.actuallyHurt`, which is where being hit balls
    /// an armadillo up -- and where fire or a fall un-balls it.
    fn actually_hurt(&self, world: &World, source: &DamageSource, amount: f32) {
        self.living_actually_hurt(world, source, amount);
        if self.is_no_ai() || self.is_dead_or_dying() {
            return;
        }

        let hurt_by_living = source
            .causing_entity_id
            .and_then(|id| world.get_entity_by_id(id))
            .is_some_and(|entity| entity.is_living_entity());
        if hurt_by_living {
            self.brain.set_memory_with_expiry(
                memory_module_types::DANGER_DETECTED_RECENTLY,
                true,
                SCARE_CHECK_INTERVAL,
            );
            if self.can_stay_rolled_up() {
                self.roll_up();
            }
        } else if source.is(&DamageTypeTag::PANIC_ENVIRONMENTAL_CAUSES) {
            self.roll_out();
        }
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
}

impl AgeableMob for ArmadilloEntity {
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

impl Animal for ArmadilloEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        Self::is_armadillo_food(item_stack)
    }

    /// Vanilla parity: `Armadillo.playEatingSound`.
    fn play_eating_sound(&self) {
        self.make_sound(Some(&sound_events::ENTITY_ARMADILLO_EAT));
    }

    /// Vanilla parity: `Armadillo.canFallInLove`, which a balled-up armadillo
    /// cannot.
    fn can_fall_in_love(&self) -> bool {
        self.in_love_time() <= 0 && !self.is_scared()
    }
}

impl Mob for ArmadilloEntity {
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

    /// Vanilla parity: `Armadillo.customServerAiStep`.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.brain.tick(&world, self);
        armadillo_ai::update_activity(&self.brain);
        self.tick_scute_shed(&world);
        Animal::custom_server_ai_step_animal(self);
    }

    /// Vanilla parity: `Armadillo.getAmbientSound`, silent while balled up.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        if self.is_scared() {
            return None;
        }
        Some(&sound_events::ENTITY_ARMADILLO_AMBIENT)
    }

    /// Vanilla parity: `Armadillo.getMaxHeadYRot`.
    fn max_head_y_rot(&self) -> f32 {
        if self.is_scared() {
            0.0
        } else {
            MAX_HEAD_Y_ROT
        }
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        let _ = spawn_reason;
        Self::check_armadillo_spawn_rules(world, pos)
    }

    /// Vanilla parity: `Armadillo.mobInteract`. A brush works on a balled-up
    /// armadillo; nothing else does.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let is_brush = {
            let inventory = player.inventory.lock();
            inventory.get_item_in_hand(hand).is(&vanilla_items::BRUSH)
        };

        if is_brush && self.brush_off_scute() {
            let mut inventory = player.inventory.lock();
            inventory
                .get_item_in_hand_mut(hand)
                .hurt_and_break(BRUSH_DURABILITY_COST, false);
            return InteractionResult::Success;
        }

        if self.is_scared() {
            return InteractionResult::Fail;
        }
        Animal::mob_interact_animal(self, player, hand)
    }
}

impl PathfinderMob for ArmadilloEntity {}

#[cfg(test)]
mod tests;
