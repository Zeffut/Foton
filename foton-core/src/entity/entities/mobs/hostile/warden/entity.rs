//! The warden.
//!
//! Vanilla parity: `net.minecraft.world.entity.monster.warden.Warden`. The warden is
//! blind: everything it knows about the world arrives through a
//! [`VibrationListener`], which is why it could not exist in Foton before the vibration
//! layer did. What it hears raises anger against whoever caused it, and the anger --
//! not a target field -- decides whether it wanders, investigates, roars or fights.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::entity_data::EntityPose;
use foton_registry::entity_type::{EntityDimensions, EntityTypeRef};
use foton_registry::game_events::GameEventRef;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_damage_type_tags::DamageTypeTag;
use foton_registry::vanilla_entity_data::WardenEntityData;
use foton_registry::vanilla_game_event_tags::GameEventTag;
use foton_registry::{sound_events, vanilla_entities, vanilla_mob_effects};
use foton_utils::entity_events::EntityStatus;
use foton_utils::locks::SyncMutex;
use foton_utils::{
    BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey, Identifier,
};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::entity::ai::brain::Brain;
use crate::entity::ai::brain::memory::{EntityMemory, Unit, memory_module_types};
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::{
    Enemy, Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, MobEffectInstance, PathfinderMob, SharedEntity, SpawnGroupData,
};
use crate::world::World;
use crate::world::game_event::vibrations::{
    VIBRATION_DATA_TAG, VibrationListener, VibrationPositionSource, VibrationUser,
};
use crate::world::game_event::{
    DynamicGameEventListener, DynamicListenerAction, GameEventContext, SharedGameEventListener,
};

use super::anger::{AngerLevel, AngerManagement};
use super::warden_ai;

/// Vanilla `Warden.VIBRATION_COOLDOWN_TICKS`.
pub const VIBRATION_COOLDOWN_TICKS: i64 = 40;
/// Vanilla `Warden.TIME_TO_USE_MELEE_UNTIL_SONIC_BOOM`.
pub const TIME_TO_USE_MELEE_UNTIL_SONIC_BOOM: i32 = 200;
/// Vanilla `Warden.xpReward = 5` of the constructor.
const XP_REWARD: i32 = 5;
/// Vanilla `Warden.DARKNESS_DISPLAY_LIMIT`.
const DARKNESS_DISPLAY_LIMIT: i32 = 200;
/// Vanilla `Warden.DARKNESS_DURATION`.
pub const DARKNESS_DURATION: i32 = 260;
/// Vanilla `Warden.DARKNESS_RADIUS`.
const DARKNESS_RADIUS: f64 = 20.0;
/// Vanilla `Warden.DARKNESS_INTERVAL`.
const DARKNESS_INTERVAL: i32 = 120;
/// Vanilla `Warden.ANGERMANAGEMENT_TICK_DELAY`.
const ANGER_MANAGEMENT_TICK_DELAY: i32 = 20;
/// Vanilla `Warden.DEFAULT_ANGER`.
const DEFAULT_ANGER: i32 = 35;
/// Vanilla `Warden.PROJECTILE_ANGER`.
const PROJECTILE_ANGER: i32 = 10;
/// Vanilla `Warden.ON_HURT_ANGER_BOOST`.
const ON_HURT_ANGER_BOOST: i32 = 20;
/// Vanilla `Warden.RECENT_PROJECTILE_TICK_THRESHOLD`.
const RECENT_PROJECTILE_TICK_THRESHOLD: i64 = 100;
/// Vanilla `Warden.TOUCH_COOLDOWN_TICKS`.
const TOUCH_COOLDOWN_TICKS: i64 = 20;
/// Vanilla `Warden.PROJECTILE_ANGER_DISTANCE`.
const PROJECTILE_ANGER_DISTANCE: f64 = 30.0;
/// Vanilla `Warden.VibrationUser.GAME_EVENT_LISTENER_RANGE`.
const GAME_EVENT_LISTENER_RANGE: i32 = 16;
/// Vanilla `Warden.nextStep`, which steps more often than the default `+ 1.0F`.
const STEP_DISTANCE: f32 = 0.55;

/// Fields a warden keeps that are neither synced nor on a base.
struct WardenState {
    anger_management: AngerManagement,
    listener: Option<Arc<VibrationListener>>,
    dynamic_listener: Option<DynamicGameEventListener>,
}

/// A warden.
#[entity_behavior(class = "Warden")]
pub struct WardenEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<WardenEntityData>,
    state: SyncMutex<WardenState>,
    brain: Brain,
}

// SAFETY: This key is owned by Foton and uniquely identifies `WardenEntity`.
unsafe impl DowncastType for WardenEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/warden");
}

impl WardenEntity {
    /// Creates a warden at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a warden from saved base data.
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
        mob_base.set_xp_reward(XP_REWARD);
        let mut entity_data = WardenEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        let warden = Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            state: SyncMutex::new(WardenState {
                anger_management: AngerManagement::default(),
                listener: None,
                dynamic_listener: None,
            }),
            brain: warden_ai::make_brain(),
        };
        // Vanilla parity: the pathfinding maluses of the `Warden` constructor. A warden
        // walks into fire and lava rather than around them, which is what lets it follow
        // a player across the deep dark's hazards.
        warden.set_pathfinding_malus(PathType::UnpassableRail, 0.0);
        warden.set_pathfinding_malus(PathType::Damaging, 8.0);
        warden.set_pathfinding_malus(PathType::PowderSnow, 8.0);
        warden.set_pathfinding_malus(PathType::Lava, 8.0);
        warden.set_pathfinding_malus(PathType::Fire, 0.0);
        warden.set_pathfinding_malus(PathType::FireInNeighbor, 0.0);
        warden
    }

    /// The brain, without going through [`Mob::brain`].
    #[must_use]
    pub const fn brain_ref(&self) -> &Brain {
        &self.brain
    }

    /// Vanilla parity: `Warden.isDiggingOrEmerging`.
    #[must_use]
    pub fn is_digging_or_emerging(&self) -> bool {
        matches!(self.pose(), EntityPose::Digging | EntityPose::Emerging)
    }

    /// Vanilla parity: `Warden.getClientAngerLevel`.
    #[must_use]
    pub fn client_anger_level(&self) -> i32 {
        *self.entity_data.lock().client_anger_level.get()
    }

    /// Vanilla parity: `Warden.syncClientAngerLevel`.
    fn sync_client_anger_level(&self) {
        let anger = self.active_anger();
        self.entity_data.lock().client_anger_level.set(anger);
    }

    /// Vanilla parity: `Warden.getActiveAnger`.
    fn active_anger(&self) -> i32 {
        let target = self.target_from_brain();
        self.state
            .lock()
            .anger_management
            .active_anger(target.as_deref())
    }

    /// Vanilla parity: `Warden.getAngerLevel`.
    #[must_use]
    pub fn anger_level(&self) -> AngerLevel {
        AngerLevel::by_anger(self.active_anger())
    }

    /// Returns how angry this warden is at one entity in particular.
    ///
    /// Vanilla parity: the `@VisibleForTesting Warden.getAngerManagement`, narrowed to the
    /// one question a caller outside the warden has any business asking. Handing out the
    /// manager itself would hand out the lock it lives behind.
    #[must_use]
    pub fn anger_at(&self, entity: &dyn Entity) -> i32 {
        self.state.lock().anger_management.anger_at(entity)
    }

    /// Vanilla parity: `Warden.clearAnger`.
    pub fn clear_anger(&self, entity: &dyn Entity) {
        self.state.lock().anger_management.clear_anger(entity);
    }

    /// Vanilla parity: `Warden.increaseAngerAt(entity)`, the 35-point default.
    pub fn increase_anger_at(&self, entity: Option<&dyn Entity>) {
        self.increase_anger_at_by(entity, DEFAULT_ANGER, true);
    }

    /// Vanilla parity: `Warden.increaseAngerAt(entity, amount, playSound)`.
    pub fn increase_anger_at_by(&self, entity: Option<&dyn Entity>, amount: i32, play_sound: bool) {
        let Some(entity) = entity else {
            return;
        };
        if self.is_no_ai() || !self.can_target_entity(Some(entity)) {
            return;
        }

        warden_ai::set_dig_cooldown(&self.brain);
        let may_switch_target = self
            .target_from_brain()
            .is_none_or(|target| target.as_player().is_none());
        let new_anger = self
            .state
            .lock()
            .anger_management
            .increase_anger(entity, amount);
        if entity.as_player().is_some()
            && may_switch_target
            && AngerLevel::by_anger(new_anger).is_angry()
        {
            self.brain
                .erase_memory(memory_module_types::ATTACK_TARGET.id());
        }

        if play_sound {
            self.play_listening_sound();
        }
    }

    /// Vanilla parity: `Warden.playListeningSound`.
    fn play_listening_sound(&self) {
        if self.pose() == EntityPose::Roaring {
            return;
        }
        self.play_sound(
            self.anger_level().listening_sound(),
            10.0,
            self.voice_pitch(),
        );
    }

    /// Vanilla parity: `Warden.getEntityAngryAt`.
    #[must_use]
    pub fn entity_angry_at(&self) -> Option<SharedEntity> {
        if !self.anger_level().is_angry() {
            return None;
        }
        self.state
            .lock()
            .anger_management
            .active_entity(&|entity| self.can_target_entity(Some(entity)))
    }

    /// Vanilla parity: `Warden.canTargetEntity`.
    ///
    /// Not implemented: the world-border bounds check. Foton's border is reachable from
    /// the world but not as a bounding-box test on an arbitrary entity, and a warden that
    /// ignores the border only differs outside it.
    #[must_use]
    pub fn can_target_entity(&self, entity: Option<&dyn Entity>) -> bool {
        let Some(entity) = entity else {
            return false;
        };
        let Some(living) = entity.as_living_entity() else {
            return false;
        };
        if entity
            .as_player()
            .is_some_and(|player| player.has_infinite_materials() || player.is_spectator())
        {
            return false;
        }
        !self.is_allied_to(entity)
            && entity.entity_type() != &vanilla_entities::ARMOR_STAND
            && entity.entity_type() != &vanilla_entities::WARDEN
            && !living.is_invulnerable()
            && !living.is_dead_or_dying()
    }

    /// Vanilla parity: `Warden.setAttackTarget`.
    pub fn set_attack_target(&self, target: &SharedEntity) {
        self.brain
            .erase_memory(memory_module_types::ROAR_TARGET.id());
        self.brain.set_memory(
            memory_module_types::ATTACK_TARGET,
            EntityMemory::new(target),
        );
        self.brain
            .erase_memory(memory_module_types::CANT_REACH_WALK_TARGET_SINCE.id());
        warden_ai::set_sonic_boom_cooldown(&self.brain, TIME_TO_USE_MELEE_UNTIL_SONIC_BOOM.into());
    }

    /// Vanilla parity: `Warden.applyDarknessAround`.
    pub fn apply_darkness_around(
        world: &Arc<World>,
        position: DVec3,
        source: Option<&dyn Entity>,
        darkness_radius: f64,
    ) {
        // Vanilla parity: `new MobEffectInstance(MobEffects.DARKNESS, 260, 0, false, false)`
        // -- invisible and without an icon, because the darkness itself is the tell.
        let darkness =
            MobEffectInstance::with_duration(vanilla_mob_effects::DARKNESS, DARKNESS_DURATION, 0)
                .with_visible(false)
                .with_show_icon(false);
        world.add_effect_to_players_around(
            source,
            position,
            darkness_radius,
            &darkness,
            DARKNESS_DISPLAY_LIMIT,
        );
    }

    /// Attaches the game-event listener a warden carries.
    ///
    /// The listener cannot hold the warden without a cycle, so it holds the world and the
    /// entity id and looks the warden back up -- the same route the allay's listeners take.
    fn ensure_listener(&self, world: &Arc<World>) {
        let mut state = self.state.lock();
        if state.listener.is_some() {
            return;
        }
        let user = Arc::new(WardenVibrationUser {
            world: Arc::downgrade(world),
            entity_id: self.id(),
        });
        let listener = Arc::new(VibrationListener::new(user));
        state.dynamic_listener = Some(DynamicGameEventListener::new(
            Arc::clone(&listener) as SharedGameEventListener
        ));
        state.listener = Some(listener);
    }

    fn listener(&self) -> Option<Arc<VibrationListener>> {
        self.state.lock().listener.clone()
    }
}

impl Entity for WardenEntity {
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

    /// Vanilla parity: the server half of `Warden.tick`.
    fn tick(&self) {
        // Vanilla guards this half with `level() instanceof ServerLevel` and then falls
        // through to `super.tick()` either way.
        if let Some(world) = self.level() {
            self.ensure_listener(&world);
            if let Some(listener) = self.listener() {
                listener.tick(&world);
            }
            // A warden a player has kept loaded never digs itself back into the ground.
            if self.is_persistence_required() || self.requires_custom_persistence() {
                warden_ai::set_dig_cooldown(&self.brain);
            }
        }

        LivingEntity::tick_living_entity(self);
    }

    /// Vanilla parity: `Warden.dampensVibrations`, which is what stops a warden's own
    /// footsteps setting off every sensor it walks past.
    fn dampens_vibrations(&self) -> bool {
        true
    }

    /// Vanilla parity: `Warden.nextStep`.
    fn next_step(&self) -> f32 {
        self.base().movement_progress().move_dist() + STEP_DISTANCE
    }

    /// Vanilla parity: `Warden.isPushable`.
    fn is_pushable(&self) -> bool {
        !self.is_digging_or_emerging() && self.default_is_pushable()
    }

    /// Vanilla parity: `Warden.canRide`, which is always false.
    fn can_ride(&self, _vehicle: &dyn Entity) -> bool {
        false
    }

    /// Vanilla parity: `Warden.getDefaultDimensions`, which flattens the warden while it
    /// is in the ground so it does not stick out of the floor it is digging through.
    fn dimensions_for_pose(&self, pose: EntityPose) -> EntityDimensions {
        let dimensions = self.entity_type.dimensions;
        if matches!(pose, EntityPose::Digging | EntityPose::Emerging) {
            // Vanilla `EntityDimensions.fixed(width, 1.0F)`, whose eye height is 85% of
            // the height it is given.
            return EntityDimensions::new(dimensions.width, 1.0, 0.85);
        }
        dimensions
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    /// Vanilla parity: `Warden.playStepSound`.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(&sound_events::ENTITY_WARDEN_STEP, 10.0, 1.0);
    }

    fn update_dynamic_game_event_listener(
        &self,
        action: DynamicListenerAction,
        world: &Arc<World>,
    ) {
        if action == DynamicListenerAction::Add {
            self.ensure_listener(world);
        }
        if let Some(listener) = &self.state.lock().dynamic_listener {
            listener.apply(action, world);
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.state.lock().anger_management.save(nbt);
        if let Some(listener) = self.listener() {
            let mut data = NbtCompound::new();
            listener.save(&mut data);
            nbt.insert(VIBRATION_DATA_TAG, data);
        }
        self.brain.save(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.state.lock().anger_management = AngerManagement::load(&nbt);
        self.sync_client_anger_level();
        if let Some(world) = self.level() {
            self.ensure_listener(&world);
        }
        if let Some(listener) = self.listener() {
            listener.load(nbt.compound(VIBRATION_DATA_TAG).as_ref());
        }
        self.brain.load(nbt);
    }
}

impl LivingEntity for WardenEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
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

    /// Vanilla parity: `Warden.isInvulnerableTo`. A warden in the ground cannot be hit,
    /// which is what stops a player killing it during the emerge animation.
    fn is_invulnerable_to(&self, world: &World, source: &DamageSource) -> bool {
        if self.is_digging_or_emerging() && !source.is(&DamageTypeTag::BYPASSES_INVULNERABILITY) {
            return true;
        }
        self.living_is_invulnerable_to(world, source)
    }

    /// Vanilla parity: `Warden.hurtServer`.
    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        let was_hurt = self.living_hurt_server(world, source, amount);
        if self.is_no_ai() || self.is_digging_or_emerging() {
            return was_hurt;
        }

        let Some(level) = self.level() else {
            return was_hurt;
        };
        let attacker = source
            .causing_entity_id
            .and_then(|id| level.get_entity_by_id(id));
        self.increase_anger_at_by(
            attacker.as_deref(),
            AngerLevel::Angry.minimum_anger() + ON_HURT_ANGER_BOOST,
            false,
        );

        if self
            .brain
            .has_memory_value(memory_module_types::ATTACK_TARGET.id())
        {
            return was_hurt;
        }
        let Some(attacker) = attacker else {
            return was_hurt;
        };
        if attacker.as_living_entity().is_none() {
            return was_hurt;
        }
        if source.is_direct() || self.position().distance(attacker.position()) < 5.0 {
            self.set_attack_target(&attacker);
        }
        was_hurt
    }

    /// Vanilla parity: `Warden.doPush`, which is what makes walking into a warden a bad
    /// idea even before it has heard you.
    fn do_push(&self, entity: &SharedEntity) {
        if !self.is_no_ai()
            && !self
                .brain
                .has_memory_value(memory_module_types::TOUCH_COOLDOWN.id())
        {
            self.brain.set_memory_with_expiry(
                memory_module_types::TOUCH_COOLDOWN,
                Unit,
                TOUCH_COOLDOWN_TICKS,
            );
            self.increase_anger_at(Some(entity.as_ref()));
            warden_ai::set_disturbance_location(self, entity.block_position());
        }
        self.living_do_push(entity);
    }

    /// Vanilla parity: `Warden.getSoundVolume`.
    fn sound_volume(&self) -> f32 {
        4.0
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_WARDEN_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_WARDEN_DEATH)
    }
}

impl Mob for WardenEntity {
    fn is_monster(&self) -> bool {
        true
    }

    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn brain(&self) -> Option<&Brain> {
        Some(&self.brain)
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    /// Vanilla parity: `Warden.getTarget`, which is `getTargetFromBrain`.
    fn target(&self) -> Option<SharedEntity> {
        self.target_from_brain()
    }

    /// Vanilla parity: `Warden.canAttack`.
    fn can_attack(&self, target: &dyn LivingEntity) -> bool {
        self.can_target_entity(Some(target))
    }

    /// Vanilla parity: `Warden.removeWhenFarAway`, which is always false: a warden that
    /// was summoned stays until it digs itself back down.
    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        false
    }

    /// Vanilla parity: `Warden.getAmbientSound`, which is silent while roaring or digging.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        if self.pose() == EntityPose::Roaring || self.is_digging_or_emerging() {
            return None;
        }
        Some(self.anger_level().ambient_sound())
    }

    /// Vanilla parity: `Warden.doHurtTarget`, which resets the sonic boom so a warden that
    /// can reach its target keeps hitting it instead.
    fn do_hurt_target(&self, world: &World, target: &SharedEntity) -> bool {
        self.broadcast_entity_event(EntityStatus::StartAttacking);
        self.play_sound(
            &sound_events::ENTITY_WARDEN_ATTACK_IMPACT,
            10.0,
            self.voice_pitch(),
        );
        warden_ai::set_sonic_boom_cooldown(&self.brain, VIBRATION_COOLDOWN_TICKS);
        self.mob_do_hurt_target(world, target)
    }

    /// Vanilla parity: `Warden.customServerAiStep`.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.brain.tick(&world, self);

        let tick_count = self.tick_count();
        if (tick_count + self.id()) % DARKNESS_INTERVAL == 0 {
            Self::apply_darkness_around(&world, self.position(), Some(self), DARKNESS_RADIUS);
        }

        if tick_count % ANGER_MANAGEMENT_TICK_DELAY == 0 {
            self.state
                .lock()
                .anger_management
                .tick(&world, &|entity| self.can_target_entity(Some(entity)));
            self.sync_client_anger_level();
        }

        warden_ai::update_activity(&self.brain);
    }

    /// Vanilla parity: `Warden.finalizeSpawn`, which is where a summoned warden gets its
    /// emerge animation and the twenty-minute cooldown before it may dig away again.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.brain.set_memory_with_expiry(
            memory_module_types::DIG_COOLDOWN,
            Unit,
            warden_ai::DIGGING_COOLDOWN.into(),
        );
        if spawn_reason == EntitySpawnReason::Triggered {
            self.set_pose(EntityPose::Emerging);
            self.brain.set_memory_with_expiry(
                memory_module_types::IS_EMERGING,
                Unit,
                warden_ai::EMERGE_DURATION.into(),
            );
            self.play_sound(&sound_events::ENTITY_WARDEN_AGITATED, 5.0, 1.0);
        }
        self.finalize_spawn_mob_base(world, spawn_reason, group_data)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for WardenEntity {}

impl Enemy for WardenEntity {}

/// Vanilla `Warden.VibrationUser`.
struct WardenVibrationUser {
    world: Weak<World>,
    entity_id: i32,
}

impl WardenVibrationUser {
    fn with_warden<R>(&self, action: impl FnOnce(&WardenEntity) -> R) -> Option<R> {
        let world = self.world.upgrade()?;
        let entity = world.get_entity_by_id(self.entity_id)?;
        let warden = entity.downcast_ref::<WardenEntity>()?;
        Some(action(warden))
    }
}

impl VibrationUser for WardenVibrationUser {
    fn listener_radius(&self) -> i32 {
        GAME_EVENT_LISTENER_RANGE
    }

    fn position_source(&self) -> VibrationPositionSource {
        // Vanilla listens from the warden's eye, not its feet.
        let y_offset = self
            .with_warden(|warden| warden.get_eye_height() as f32)
            .unwrap_or(0.0);
        VibrationPositionSource::Entity {
            world: Weak::clone(&self.world),
            entity_id: self.entity_id,
            y_offset,
        }
    }

    fn listenable_events(&self) -> Identifier {
        GameEventTag::WARDEN_CAN_LISTEN
    }

    fn can_trigger_avoid_vibration(&self) -> bool {
        true
    }

    /// Vanilla `Warden.VibrationUser.canReceiveVibration`.
    fn can_receive_vibration(
        &self,
        _world: &Arc<World>,
        _pos: BlockPos,
        _event: GameEventRef,
        context: &GameEventContext<'_>,
    ) -> bool {
        self.with_warden(|warden| {
            if warden.is_no_ai()
                || warden.is_dead_or_dying()
                || warden
                    .brain_ref()
                    .has_memory_value(memory_module_types::VIBRATION_COOLDOWN.id())
                || warden.is_digging_or_emerging()
            {
                return false;
            }
            // A warden ignores anything it could not attack anyway; a vibration from
            // another warden is the case this exists for.
            context.source_entity().is_none_or(|source| {
                source.as_living_entity().is_none() || warden.can_target_entity(Some(source))
            })
        })
        .unwrap_or(false)
    }

    /// Vanilla `Warden.VibrationUser.onReceiveVibration`.
    fn on_receive_vibration(
        &self,
        _world: &Arc<World>,
        pos: BlockPos,
        _event: GameEventRef,
        source_entity: Option<&dyn Entity>,
        projectile_owner: Option<&dyn Entity>,
        _receiving_distance: f32,
    ) {
        self.with_warden(|warden| {
            if warden.is_dead_or_dying() {
                return;
            }
            warden.brain_ref().set_memory_with_expiry(
                memory_module_types::VIBRATION_COOLDOWN,
                Unit,
                VIBRATION_COOLDOWN_TICKS,
            );
            warden.broadcast_entity_event(EntityStatus::TendrilsShiver);
            warden.play_sound(
                &sound_events::ENTITY_WARDEN_TENDRIL_CLICKS,
                5.0,
                warden.voice_pitch(),
            );

            let mut suspicious_pos = pos;
            match projectile_owner {
                Some(owner) => {
                    if warden.position().distance(owner.position()) < PROJECTILE_ANGER_DISTANCE {
                        if warden
                            .brain_ref()
                            .has_memory_value(memory_module_types::RECENT_PROJECTILE.id())
                        {
                            // A second arrow from the same place is what convinces a warden
                            // to go for the shooter rather than the arrow.
                            if warden.can_target_entity(Some(owner)) {
                                suspicious_pos = owner.block_position();
                            }
                            warden.increase_anger_at(Some(owner));
                        } else {
                            warden.increase_anger_at_by(Some(owner), PROJECTILE_ANGER, true);
                        }
                    }
                    warden.brain_ref().set_memory_with_expiry(
                        memory_module_types::RECENT_PROJECTILE,
                        Unit,
                        RECENT_PROJECTILE_TICK_THRESHOLD,
                    );
                }
                None => warden.increase_anger_at(source_entity),
            }

            if warden.anger_level().is_angry() {
                return;
            }
            let active_entity = warden
                .state
                .lock()
                .anger_management
                .active_entity(&|entity| warden.can_target_entity(Some(entity)));
            let is_the_same_suspect = match (&active_entity, source_entity) {
                (Some(active), Some(source)) => active.id() == source.id(),
                _ => false,
            };
            if projectile_owner.is_some() || active_entity.is_none() || is_the_same_suspect {
                warden_ai::set_disturbance_location(warden, suspicious_pos);
            }
        });
    }
}
