//! Creaking entity.
//!
//! Vanilla parity: `net.minecraft.world.entity.monster.creaking.Creaking`. The
//! pale-garden mob, and the only one in the game that a player stops simply by
//! looking at it. A creaking spawned by a `CreakingHeartBlockEntity` is
//! *heart-bound*: it has one point of health, nothing can hurt it, and the only
//! way to be rid of it is to break the heart that is keeping it alive -- every
//! blow landed on the creaking is paid by the heart instead.
//!
//! Two deviations, both named rather than hidden:
//!
//! * Vanilla's `HomeNodeEvaluator` blocks any path node more than 32 blocks
//!   from the heart, which keeps a creaking near its tree. Foton's navigation
//!   has no per-mob node evaluator, so that fence is not built; the heart's own
//!   34-block tether still pulls a wanderer down, so a creaking cannot end up
//!   arbitrarily far away, it just takes the tether rather than the pathfinder
//!   to stop it.
//! * `hasGlowingEyes` / `checkEyeBlink` are the death-twitch eye flicker, which
//!   the client drives off `deathTime`; there is nothing for a server to send.

mod creaking_ai;

#[cfg(test)]
mod tests;

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::CreakingHeartState;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::equipment::EquipmentSlot;
use foton_registry::particle_type::{BlockParticleOption, ParticleData};
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::CreakingEntityData;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_blocks, vanilla_damage_type_tags,
    vanilla_entities, vanilla_game_events, vanilla_particle_types,
};
use foton_utils::entity_events::EntityStatus;
use foton_utils::locks::SyncMutex;
use foton_utils::{BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::behavior::blocks::CREAKING_HEART_STATE;
use crate::block_entity::entities::CreakingHeartBlockEntity;
use crate::entity::LivingEntitySyncedData;
use crate::entity::ai::brain::Brain;
use crate::entity::ai::brain::memory::{EntityMemory, memory_module_types};
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::living_entity::is_looking_at;
use crate::entity::spawn_rules::check_monster_spawn_rules;
use crate::entity::{
    Enemy, Entity, EntityBase, EntityBaseLoad, EntityEventSource as _, EntitySpawnReason,
    EntitySyncedData, LivingEntity, LivingEntityBase, Mob, MobBase, MoveResult, PathfinderMob,
    RemovalReason, SharedEntity,
};
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader as _, World};

/// Experience a creaking drops.
///
/// Vanilla parity: the `this.xpReward = 0` of the `Creaking` constructor. It is
/// worth nothing on purpose: the heart it protects is what pays out, and a
/// creaking that dropped experience would be a farm.
const XP_REWARD: i32 = 0;

/// Vanilla parity: `Creaking.ATTACK_ANIMATION_DURATION`.
const ATTACK_ANIMATION_DURATION: i32 = 15;

/// Vanilla parity: `Creaking.INVULNERABILITY_ANIMATION_DURATION`.
const INVULNERABILITY_ANIMATION_DURATION: i32 = 8;

/// Vanilla parity: `Creaking.TWITCH_DEATH_DURATION`.
const TWITCH_DEATH_DURATION: i32 = 45;

/// Vanilla parity: `Creaking.ACTIVATION_RANGE_SQ`.
const ACTIVATION_RANGE_SQ: f64 = 144.0;

/// Vanilla parity: `Creaking.MAX_PLAYER_STUCK_COUNTER`.
const MAX_PLAYER_STUCK_COUNTER: i32 = 4;

/// Vanilla parity: the `0.5` cone of the `isLookingAtMe` call in
/// `Creaking.checkCanMove`.
const GAZE_CONE: f64 = 0.5;

/// Vanilla parity: the `100` blocks of `tearDown`'s pale oak crumble.
const TEAR_DOWN_WOOD_PARTICLES: i32 = 100;
/// Vanilla parity: the `10` blocks of heart crumble in the same call.
const TEAR_DOWN_HEART_PARTICLES: i32 = 10;
/// Vanilla parity: the `0.3` of the bounding-box spread `tearDown` uses.
const TEAR_DOWN_SPREAD: f64 = 0.3;

/// Fields a creaking keeps that are neither synced nor on a base.
struct CreakingState {
    /// Vanilla parity: `Creaking.attackAnimationRemainingTicks`.
    attack_animation_remaining_ticks: i32,
    /// Vanilla parity: `Creaking.invulnerabilityAnimationRemainingTicks`.
    invulnerability_animation_remaining_ticks: i32,
    /// Vanilla parity: `Creaking.playerStuckCounter`.
    player_stuck_counter: i32,
}

/// A creaking.
#[entity_behavior(class = "Creaking")]
pub struct CreakingEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<CreakingEntityData>,
    state: SyncMutex<CreakingState>,
    brain: Brain,
}

// SAFETY: This key is owned by Foton and uniquely identifies `CreakingEntity`.
unsafe impl DowncastType for CreakingEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/creaking");
}

impl CreakingEntity {
    /// Creates a creaking at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a creaking from saved base data.
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
        mob_base.navigation().lock().set_can_float(true);
        let mut entity_data = CreakingEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            state: SyncMutex::new(CreakingState {
                attack_animation_remaining_ticks: 0,
                invulnerability_animation_remaining_ticks: 0,
                player_stuck_counter: 0,
            }),
            brain: creaking_ai::make_brain(),
        }
    }

    /// Ties this creaking to the heart at `pos`.
    ///
    /// Vanilla parity: `Creaking.setTransient`. A heart-bound creaking will walk
    /// through fire and lava to reach a player, because the heart is what keeps
    /// it alive and the hazards cannot.
    pub fn set_transient(&self, pos: BlockPos) {
        self.set_home_pos(pos);
        self.set_pathfinding_malus(PathType::Damaging, 8.0);
        self.set_pathfinding_malus(PathType::PowderSnow, 8.0);
        self.set_pathfinding_malus(PathType::Lava, 8.0);
        self.set_pathfinding_malus(PathType::Fire, 0.0);
        self.set_pathfinding_malus(PathType::FireInNeighbor, 0.0);
    }

    /// Vanilla parity: `Creaking.isHeartBound`.
    #[must_use]
    pub fn is_heart_bound(&self) -> bool {
        self.home_pos().is_some()
    }

    /// Vanilla parity: `Creaking.getHomePos`.
    #[must_use]
    pub fn home_pos(&self) -> Option<BlockPos> {
        *self.entity_data.lock().home_pos.get()
    }

    /// Vanilla parity: `Creaking.setHomePos`.
    pub fn set_home_pos(&self, pos: BlockPos) {
        self.entity_data.lock().home_pos.set(Some(pos));
    }

    /// Vanilla parity: `Creaking.canMove`, the synced half of the freeze.
    #[must_use]
    pub fn can_move(&self) -> bool {
        *self.entity_data.lock().can_move.get()
    }

    /// Vanilla parity: `Creaking.isActive`.
    #[must_use]
    pub fn is_active(&self) -> bool {
        *self.entity_data.lock().is_active.get()
    }

    /// Vanilla parity: `Creaking.setIsActive`.
    pub fn set_is_active(&self, active: bool) {
        self.entity_data.lock().is_active.set(active);
    }

    /// Vanilla parity: `Creaking.isTearingDown`.
    #[must_use]
    pub fn is_tearing_down(&self) -> bool {
        *self.entity_data.lock().is_tearing_down.get()
    }

    /// Vanilla parity: `Creaking.setTearingDown`.
    pub fn set_tearing_down(&self) {
        self.entity_data.lock().is_tearing_down.set(true);
    }

    fn creaking_game_event(&self) {
        let Some(world) = self.level() else {
            return;
        };
        world.game_event(
            &vanilla_game_events::ENTITY_ACTION,
            self.block_position(),
            &GameEventContext::new(Some(self.as_entity_event_source()), None),
        );
    }

    /// Vanilla parity: `Creaking.activate`.
    pub fn activate(&self, player: &SharedEntity) {
        self.brain.set_memory(
            memory_module_types::ATTACK_TARGET,
            EntityMemory::new(player),
        );
        self.creaking_game_event();
        self.make_sound(Some(&sound_events::ENTITY_CREAKING_ACTIVATE));
        self.set_is_active(true);
    }

    /// Vanilla parity: `Creaking.deactivate`.
    pub fn deactivate(&self) {
        self.brain
            .erase_memory(memory_module_types::ATTACK_TARGET.id());
        self.creaking_game_event();
        self.make_sound(Some(&sound_events::ENTITY_CREAKING_DEACTIVATE));
        self.set_is_active(false);
    }

    /// Returns whether a player is standing inside the creaking's hitbox.
    ///
    /// Vanilla parity: `Creaking.playerIsStuckInYou`, which the heart reads to
    /// clear a creaking a player has walked into and pinned. The counter has to
    /// run four ticks in a row, so brushing past one is not enough.
    #[must_use]
    pub fn player_is_stuck_in_you(&self) -> bool {
        let players = self
            .brain
            .get_memory(memory_module_types::NEAREST_PLAYERS)
            .unwrap_or_default();
        if players.is_empty() {
            self.state.lock().player_stuck_counter = 0;
            return false;
        }

        let own_box = self.bounding_box();
        for remembered in players {
            let Some(player) = remembered.get() else {
                continue;
            };
            let eye = DVec3::new(player.position().x, player.get_eye_y(), player.position().z);
            if own_box.contains(eye) {
                let mut state = self.state.lock();
                state.player_stuck_counter += 1;
                return state.player_stuck_counter > MAX_PLAYER_STUCK_COUNTER;
            }
        }

        self.state.lock().player_stuck_counter = 0;
        false
    }

    /// Vanilla parity: `Creaking.checkCanMove`, the whole freeze mechanic.
    ///
    /// A creaking moves while nobody is looking at it. Once it is awake, only a
    /// player whose head is bare stops it -- a carved pumpkin is a disguise the
    /// creaking does not recognize as a gaze.
    #[must_use]
    pub fn check_can_move(&self) -> bool {
        let players = self
            .brain
            .get_memory(memory_module_types::NEAREST_PLAYERS)
            .unwrap_or_default();
        let active = self.is_active();
        if players.is_empty() {
            if active {
                self.deactivate();
            }
            return true;
        }

        let mut has_potential_target = false;
        for remembered in players {
            let Some(player) = remembered.get() else {
                continue;
            };
            let Some(living) = player.as_living_entity() else {
                continue;
            };
            if !Mob::can_attack(self, living) || self.is_allied_to(player.as_ref()) {
                continue;
            }
            has_potential_target = true;

            if active && !player_not_wearing_disguise_item(living) {
                continue;
            }
            let scale = f64::from(LivingEntity::get_scale(self));
            let position_y = self.position().y;
            let eye_y = self.get_eye_y();
            let gaze_heights = [
                eye_y,
                scale.mul_add(0.5, position_y),
                f64::midpoint(eye_y, position_y),
            ];
            if !is_looking_at(self, living, GAZE_CONE, false, true, &gaze_heights) {
                continue;
            }

            if active {
                return false;
            }
            if player.position().distance_squared(self.position()) < ACTIVATION_RANGE_SQ {
                self.activate(&player);
                return false;
            }
        }

        if !has_potential_target && active {
            self.deactivate();
        }
        true
    }

    /// Vanilla parity: `Creaking.tearDown`, the crumble a heart-bound creaking
    /// ends as rather than dropping loot.
    pub fn tear_down(&self) {
        if let Some(world) = self.level() {
            let bounds = self.bounding_box();
            let center = bounds.center();
            let spread = DVec3::new(
                bounds.width() * TEAR_DOWN_SPREAD,
                bounds.height() * TEAR_DOWN_SPREAD,
                bounds.depth() * TEAR_DOWN_SPREAD,
            );

            world.send_particles(
                ParticleData::new(
                    &vanilla_particle_types::BLOCK_CRUMBLE,
                    BlockParticleOption::new(vanilla_blocks::PALE_OAK_WOOD.default_state()),
                ),
                center,
                TEAR_DOWN_WOOD_PARTICLES,
                spread,
                0.0,
            );
            world.send_particles(
                ParticleData::new(
                    &vanilla_particle_types::BLOCK_CRUMBLE,
                    BlockParticleOption::new(
                        vanilla_blocks::CREAKING_HEART
                            .default_state()
                            .set_value(CREAKING_HEART_STATE, CreakingHeartState::Awake),
                    ),
                ),
                center,
                TEAR_DOWN_HEART_PARTICLES,
                spread,
                0.0,
            );
        }

        self.make_sound(self.death_sound());
        self.set_removed(RemovalReason::Discarded);
    }

    /// Vanilla parity: `Creaking.creakingDeathEffects`, which the heart calls
    /// when a player breaks it while its protector is still standing.
    pub fn creaking_death_effects(&self, source: &DamageSource) {
        self.blame_source_for_damage(source);
        self.die(source);
        self.make_sound(Some(&sound_events::ENTITY_CREAKING_TWITCH));
    }

    /// Vanilla parity: `Creaking.blameSourceForDamage`.
    fn blame_source_for_damage(&self, source: &DamageSource) {
        let Some(world) = self.level() else {
            return;
        };
        self.resolve_mob_responsible_for_damage(&world, source);
        self.resolve_player_responsible_for_damage(&world, source);
    }

    /// Runs `visit` against the heart that is keeping this creaking alive, if
    /// it still is.
    ///
    /// Vanilla holds the `CreakingHeartBlockEntity` directly; Foton's block
    /// entities are only downcastable behind a reference, so the heart is
    /// reached inside a closure rather than returned.
    fn with_protecting_heart<T>(
        &self,
        visit: impl FnOnce(&CreakingHeartBlockEntity) -> T,
    ) -> Option<T> {
        let world = self.level()?;
        let home = self.home_pos()?;
        let block_entity = world.get_block_entity(home)?;
        let heart = block_entity.downcast_ref::<CreakingHeartBlockEntity>()?;
        if !heart.is_protector(self.uuid()) {
            return None;
        }
        Some(visit(heart))
    }

    /// Returns whether a heart is still keeping this creaking alive.
    #[must_use]
    fn has_protecting_heart(&self) -> bool {
        self.with_protecting_heart(|_| ()).is_some()
    }

    /// Vanilla parity: the `canMove` flip of `Creaking.aiStep`, which is what
    /// plays the freeze and unfreeze noises.
    fn tick_can_move(&self) {
        let was_able = self.can_move();
        let now_able = self.check_can_move();
        if now_able != was_able {
            self.creaking_game_event();
            if now_able {
                self.make_sound(Some(&sound_events::ENTITY_CREAKING_UNFREEZE));
            } else {
                Mob::stop_in_place(self);
                self.make_sound(Some(&sound_events::ENTITY_CREAKING_FREEZE));
            }
        }
        self.entity_data.lock().can_move.set(now_able);
    }
}

/// Vanilla parity: `LivingEntity.PLAYER_NOT_WEARING_DISGUISE_ITEM`, which is
/// what makes a carved pumpkin a way past a creaking.
fn player_not_wearing_disguise_item(target: &dyn LivingEntity) -> bool {
    if target.entity_type() != &vanilla_entities::PLAYER {
        return true;
    }
    let helmet = target.get_item_by_slot(EquipmentSlot::Head);
    !REGISTRY
        .items
        .is_in_tag(helmet.item(), &ItemTag::GAZE_DISGUISE_EQUIPMENT)
}

impl Entity for CreakingEntity {
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

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    /// Vanilla parity: `Creaking.tick`. A heart-bound creaking whose heart has
    /// stopped protecting it dies on the spot -- that is what makes breaking
    /// the heart the way to be rid of it.
    fn tick(&self) {
        if self.is_heart_bound() && !self.has_protecting_heart() {
            self.set_health(0.0);
        }
        // `Entity::default_tick` is only vanilla's `Entity.baseTick`. The
        // `super.tick()` of `Creaking.tick` is `LivingEntity.tick`, and taking
        // the wrong one costs the creaking its mob effects, its death handling
        // and its whole `ai_step` -- which is the only path to its brain.
        LivingEntity::tick_living_entity(self);
    }

    /// Vanilla parity: `Creaking.fireImmune`. Nothing hurts a heart-bound
    /// creaking, fire included.
    fn fire_immune(&self) -> bool {
        self.is_heart_bound() || self.entity_type.fire_immune
    }

    /// Vanilla parity: `Creaking.canUsePortal`, which refuses to let a creaking
    /// leave the dimension its heart is in.
    fn can_use_portal(&self, ignore_passenger: bool) -> bool {
        !self.is_heart_bound() && self.default_can_use_portal(ignore_passenger)
    }

    /// Vanilla parity: `Creaking.isPushable`.
    fn is_pushable(&self) -> bool {
        self.default_is_pushable() && self.can_move()
    }

    /// Vanilla parity: `Creaking.push`, which a frozen creaking ignores.
    fn push_impulse(&self, impulse: DVec3) {
        if self.can_move() {
            self.default_push_impulse(impulse);
        }
    }

    /// Vanilla parity: `Creaking.playStepSound`.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(&sound_events::ENTITY_CREAKING_STEP, 0.15, 1.0);
    }

    /// Vanilla parity: `Creaking.handleEntityEvent` for the byte 66 the hurt
    /// path broadcasts. The animation is the client's; the sound is not.
    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        if let Some(home) = self.home_pos() {
            let mut pos = NbtCompound::new();
            pos.insert("X", home.x());
            pos.insert("Y", home.y());
            pos.insert("Z", home.z());
            nbt.insert("home_pos", pos);
        }
        self.brain.save(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        if let Some(pos) = nbt.compound("home_pos")
            && let (Some(x), Some(y), Some(z)) = (pos.int("X"), pos.int("Y"), pos.int("Z"))
        {
            self.set_transient(BlockPos::new(x, y, z));
        }
        self.brain.load(nbt);
    }
}

impl LivingEntity for CreakingEntity {
    /// Returns synchronized data declared by vanilla `LivingEntity`.
    fn living_synced_data(&self) -> Option<&dyn LivingEntitySyncedData> {
        Some(&self.entity_data)
    }

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

    /// Vanilla parity: `Creaking.aiStep`, which runs the two animation
    /// countdowns and then re-reads the freeze.
    fn ai_step(&self) -> Option<MoveResult> {
        {
            let mut state = self.state.lock();
            if state.invulnerability_animation_remaining_ticks > 0 {
                state.invulnerability_animation_remaining_ticks -= 1;
            }
            if state.attack_animation_remaining_ticks > 0 {
                state.attack_animation_remaining_ticks -= 1;
            }
        }
        self.tick_can_move();
        self.default_ai_step()
    }

    /// Vanilla parity: `Creaking.tickDeath`. A heart-bound creaking that has
    /// been told to tear down twitches for forty-five ticks and then crumbles;
    /// anything else dies the ordinary way.
    fn tick_death(&self) {
        if !self.is_heart_bound() || !self.is_tearing_down() {
            self.default_tick_death();
            return;
        }
        let death_time = self.living_base().increment_death_time();
        if death_time > TWITCH_DEATH_DURATION && !self.is_removed() {
            self.tear_down();
        }
    }

    /// Vanilla parity: `Creaking.hurtServer`.
    ///
    /// A heart-bound creaking takes no damage at all. What a blow does instead
    /// is start the invulnerability twitch and tell the heart it was hit, which
    /// is where the resin and the heart's own hurt noise come from. The blow
    /// still has to come from something -- a living entity, a projectile, or a
    /// player to blame -- so ambient damage passes straight through.
    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        let Some(_home) = self.home_pos() else {
            return self.living_hurt_server(world, source, amount);
        };
        if source.is(&vanilla_damage_type_tags::DamageTypeTag::BYPASSES_INVULNERABILITY) {
            return self.living_hurt_server(world, source, amount);
        }

        if self.is_invulnerable_to(world, source)
            || self.state.lock().invulnerability_animation_remaining_ticks > 0
            || self.is_dead_or_dying()
        {
            return false;
        }

        self.blame_source_for_damage(source);
        let responsible_player = self.last_hurt_by_player_uuid().is_some();
        let direct_is_living_or_projectile = source
            .direct_entity_id
            .and_then(|id| world.get_entity_by_id(id))
            .is_some_and(|entity| entity.is_living_entity() || entity.as_projectile().is_some());
        if !direct_is_living_or_projectile && !responsible_player {
            return false;
        }

        self.state.lock().invulnerability_animation_remaining_ticks =
            INVULNERABILITY_ANIMATION_DURATION;
        self.broadcast_entity_event(EntityStatus::Shake);
        self.creaking_game_event();

        let protected = self.with_protecting_heart(|heart| {
            if responsible_player {
                heart.creaking_hurt();
            }
        });
        if protected.is_some() {
            self.play_hurt_sound(source);
        }

        true
    }

    /// Vanilla parity: `Creaking.knockback`, which a frozen creaking ignores.
    fn knockback(&self, power: f64, xd: f64, zd: f64) {
        if self.can_move() {
            self.default_knockback(power, xd, zd);
        }
    }

    /// Vanilla parity: `Creaking.getHurtSound`.
    fn hurt_sound(&self, source: &DamageSource) -> Option<SoundEventRef> {
        if self.is_heart_bound() {
            Some(&sound_events::ENTITY_CREAKING_SWAY)
        } else {
            self.default_hurt_sound(source)
        }
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_CREAKING_DEATH)
    }
}

impl Mob for CreakingEntity {
    /// Vanilla parity: `Creaking` derives from `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn brain(&self) -> Option<&Brain> {
        Some(&self.brain)
    }

    /// Vanilla parity: `Creaking.getTarget`.
    fn target(&self) -> Option<SharedEntity> {
        self.target_from_brain()
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    /// Vanilla parity: `Creaking.CreakingPathNavigation.tick`.
    fn tick_path_navigation(&self) {
        if self.can_move() {
            PathfinderMob::tick_pathfinder_path_navigation(self);
        }
    }

    /// Vanilla parity: `Creaking.CreakingMoveControl.tick`.
    fn tick_move_control(&self) {
        if self.can_move() {
            self.default_tick_move_control();
        }
    }

    /// Vanilla parity: `Creaking.CreakingLookControl.tick`.
    fn tick_look_control(&self) {
        if self.can_move() {
            self.default_tick_look_control();
        }
    }

    /// Vanilla parity: `Creaking.CreakingJumpControl.tick`, which does not just
    /// skip the jump but cancels one already asked for.
    fn tick_jump_control(&self) {
        if self.can_move() {
            self.default_tick_jump_control();
        } else {
            self.set_jumping(false);
        }
    }

    /// Vanilla parity: `Creaking.customServerAiStep`.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.brain.tick(&world, self);
        creaking_ai::update_activity(&self.brain, self.can_move());
    }

    /// Vanilla parity: `Creaking.doHurtTarget`.
    fn do_hurt_target(&self, world: &World, target: &SharedEntity) -> bool {
        if target.as_living_entity().is_none() {
            return false;
        }
        self.state.lock().attack_animation_remaining_ticks = ATTACK_ANIMATION_DURATION;
        self.broadcast_entity_event(EntityStatus::StartAttacking);
        self.mob_do_hurt_target(world, target)
    }

    /// Vanilla parity: `Creaking.playAttackSound`.
    fn play_attack_sound(&self) {
        self.make_sound(Some(&sound_events::ENTITY_CREAKING_ATTACK));
    }

    /// Vanilla parity: `Creaking.getAmbientSound`, which is silent while the
    /// creaking is awake -- a hunting creaking gives nothing away.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        if self.is_active() {
            None
        } else {
            Some(&sound_events::ENTITY_CREAKING_AMBIENT)
        }
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_monster_spawn_rules(world, spawn_reason, pos)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for CreakingEntity {
    /// Vanilla parity: `Creaking.getWalkTargetValue`, a flat zero -- a creaking
    /// has no preference for dark or light, unlike every other monster.
    fn get_walk_target_value(&self, _pos: BlockPos) -> f32 {
        0.0
    }
}

impl Enemy for CreakingEntity {}
