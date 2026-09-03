//! Enderman entity.
//!
//! Vanilla parity: `EnderMan`. Present in more biomes than any other mob, and
//! the only one whose whole character comes from a single rule: it ignores you
//! until you look it in the eye, and then it will not let go. The grudge is
//! [`NeutralMob`]'s; what is here is the stare that starts it, the teleport
//! that carries it, the water that ends it, and the block it walks off with.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::vanilla_enchantments::SILK_TOUCH;
use foton_registry::vanilla_entity_data::EndermanEntityData;
use foton_registry::vanilla_game_rules::MOB_GRIEFING;
use foton_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_attributes, vanilla_blocks,
    vanilla_damage_types, vanilla_entities, vanilla_game_events, vanilla_items,
};
use foton_utils::locks::SyncMutex;
use foton_utils::types::UpdateFlags;
use foton_utils::{
    BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey, WorldAabb,
};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use uuid::Uuid;

use crate::behavior::{BLOCK_BEHAVIORS, BlockLootContext, update_from_neighbour_shapes};
use crate::block_entity::block_state_nbt;
use crate::entity::Enemy;
use crate::entity::SharedEntity;
use crate::entity::ai::goal::{
    FloatGoal, Goal, GoalControls, HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal,
    NearestAttackableTargetGoal, RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
    reduced_tick_delay,
};
use crate::entity::damage::DamageSource;
use crate::entity::living_entity::is_looking_at;
use crate::entity::neutral_mob::{
    NeutralMob, PersistentAnger, read_persistent_anger, resolve_anger_target,
    write_persistent_anger,
};
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase, Mob,
    MobBase, PathfinderMob,
};
use crate::world::game_event::GameEventContext;
use crate::world::{ClipBlockShape, ClipFluid, LevelReader as _, World};
use foton_registry::fluid::is_water_fluid;

/// Experience this mob drops.
///
/// Vanilla parity: the `this.xpReward = 5` of the `Monster` constructor, which
/// every monster inherits and this one does not override.
const XP_REWARD: i32 = 5;

/// How narrow the stare cone is.
///
/// Vanilla parity: the `0.025` of `isBeingStaredBy`. It is divided by the
/// distance, so a player across a field has to be far more precisely on target
/// than one standing in front of the enderman.
const STARE_CONE: f64 = 0.025;

/// Speed multiplier while chasing.
///
/// Vanilla parity: the attacking movement-speed modifier, `0.15` additive.
const ATTACKING_SPEED_BONUS: f64 = 0.15;

/// Ticks a chase runs before daylight may drive the enderman off.
///
/// Vanilla parity: the `targetChangeTime + 600` of `customServerAiStep`. It is
/// why an enderman that has just been provoked does not blink away at once.
const DAYLIGHT_GRACE_TICKS: i32 = 600;

/// Brightness above which daylight starts to bother an enderman.
///
/// Vanilla parity: the `br > 0.5F` of `customServerAiStep`.
const DAYLIGHT_BRIGHTNESS: f32 = 0.5;

/// Damage water does per tick.
///
/// Vanilla parity: the `1.0F` of `LivingEntity.baseTick` for water-sensitive
/// mobs. Rain and water both hurt, which is why an enderman caught in a shower
/// teleports repeatedly.
const WATER_DAMAGE: f32 = 1.0;

/// How often an empty-handed enderman looks for a block to take.
///
/// Vanilla parity: the `reducedTickDelay(20)` of `EndermanTakeBlockGoal.canUse`.
const TAKE_BLOCK_INTERVAL_TICKS: i32 = 20;

/// How often a carrying enderman looks for somewhere to put its block.
///
/// Vanilla parity: the `reducedTickDelay(2000)` of
/// `EndermanLeaveBlockGoal.canUse`. It is why a block an enderman took can end
/// up a hundred blocks from where it started.
const LEAVE_BLOCK_INTERVAL_TICKS: i32 = 2000;

/// NBT key the carried block is stored under.
///
/// Vanilla parity: the `carriedBlockState` of `EnderMan.addAdditionalSaveData`.
const TAG_CARRIED_BLOCK_STATE: &str = "carriedBlockState";

/// Shortest grudge, in ticks.
///
/// Vanilla parity: `PERSISTENT_ANGER_TIME`, twenty to thirty-nine seconds.
const ANGER_MIN_TICKS: i64 = 20 * 20;
/// Longest grudge, in ticks.
const ANGER_MAX_TICKS: i64 = 39 * 20;

/// An enderman.
#[entity_behavior(class = "EnderMan")]
pub struct EndermanEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<EndermanEntityData>,
    anger: PersistentAnger,
    /// Tick the current target was taken on, for the daylight grace period.
    target_change_time: SyncMutex<i32>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `EndermanEntity`.
unsafe impl DowncastType for EndermanEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/enderman");
}

impl EndermanEntity {
    /// Creates an enderman at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates an enderman from saved base data.
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
        let mut entity_data = EndermanEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla parity: the goal order of `EnderMan.registerGoals`.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(0, FloatGoal::new(&mob_base));
            goals.add_goal(1, EndermanFreezeWhenLookedAt);
            goals.add_goal(2, MeleeAttackGoal::new(1.0, false));
            goals.add_goal(7, WaterAvoidingRandomStrollGoal::new(1.0));
            goals.add_goal(8, LookAtPlayerGoal::new(8.0));
            goals.add_goal(8, RandomLookAroundGoal::new());
            goals.add_goal(10, EndermanLeaveBlockGoal);
            goals.add_goal(11, EndermanTakeBlockGoal);
        }

        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(1, EndermanLookForPlayerGoal);
            targets.add_goal(2, HurtByTargetGoal::new());
            targets.add_goal(
                3,
                NearestAttackableTargetGoal::new(true, |_, target, _| {
                    target.entity_type() == &vanilla_entities::ENDERMITE
                }),
            );
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            anger: PersistentAnger::new(),
            target_change_time: SyncMutex::new(0),
        }
    }

    /// Returns the block the enderman is walking around with, if any.
    ///
    /// Vanilla parity: `getCarriedBlock`.
    #[must_use]
    pub fn carried_block(&self) -> Option<BlockStateId> {
        *self.entity_data.lock().ender_man().carry_state.get()
    }

    /// Puts a block in the enderman's hands, or takes the one it has away.
    ///
    /// Vanilla parity: `setCarriedBlock`.
    pub fn set_carried_block(&self, state: Option<BlockStateId>) {
        self.entity_data
            .lock()
            .ender_man_mut()
            .carry_state
            .set(state);
    }

    /// Returns whether the enderman has its arms up and its mouth open.
    ///
    /// Vanilla parity: `isCreepy`, which is set whenever it has a target.
    #[must_use]
    pub fn is_creepy(&self) -> bool {
        *self.entity_data.lock().ender_man().creepy.get()
    }

    fn set_creepy(&self, creepy: bool) {
        self.entity_data.lock().ender_man_mut().creepy.set(creepy);
    }

    /// Returns whether a player has met the enderman's eyes.
    ///
    /// Vanilla parity: `hasBeenStaredAt`, which the client uses for the shriek.
    #[must_use]
    pub fn has_been_stared_at(&self) -> bool {
        *self.entity_data.lock().ender_man().stared_at.get()
    }

    fn set_been_stared_at(&self) {
        self.entity_data.lock().ender_man_mut().stared_at.set(true);
    }

    /// Returns whether this player is looking the enderman in the eye.
    ///
    /// Vanilla parity: `isBeingStaredBy`. The gaze has to land on the
    /// enderman's own eye height, not anywhere on its body, which is why
    /// looking at its feet is safe.
    fn is_being_stared_by(&self, player: &dyn LivingEntity) -> bool {
        // TODO: vanilla exempts a player wearing a carved pumpkin via
        // PLAYER_NOT_WEARING_DISGUISE_ITEM; equipment predicates are not wired.
        is_looking_at(self, player, STARE_CONE, true, false, &[self.get_eye_y()])
    }

    /// Blinks to a random spot within sixty-four blocks.
    ///
    /// Vanilla parity: the no-argument `teleport`.
    fn teleport_randomly(&self) -> bool {
        if !Entity::is_alive(self) {
            return false;
        }
        let position = self.position();
        let target = DVec3::new(
            (rand::random::<f64>() - 0.5).mul_add(64.0, position.x),
            position.y + f64::from(rand::random_range(0..64) - 32),
            (rand::random::<f64>() - 0.5).mul_add(64.0, position.z),
        );
        self.teleport_to_spot(target)
    }

    /// Blinks away from something, roughly sixteen blocks back.
    ///
    /// Vanilla parity: `teleportTowards`, which despite the name moves the
    /// enderman away: it is how one escapes an arrow or a splash of water.
    fn teleport_away_from(&self, from: DVec3, from_eye_y: f64) -> bool {
        let position = self.position();
        let away = DVec3::new(
            position.x - from.x,
            (position.y + 0.5) - from_eye_y,
            position.z - from.z,
        );
        if away.length_squared() <= 0.0 {
            return false;
        }
        let away = away.normalize();

        let target = DVec3::new(
            (rand::random::<f64>() - 0.5).mul_add(8.0, position.x) - away.x * 16.0,
            position.y + f64::from(rand::random_range(0..16) - 8) - away.y * 16.0,
            (rand::random::<f64>() - 0.5).mul_add(8.0, position.z) - away.z * 16.0,
        );
        self.teleport_to_spot(target)
    }

    /// Tries one teleport, refusing water and open air.
    ///
    /// Vanilla parity: the three-argument `teleport`.
    fn teleport_to_spot(&self, target: DVec3) -> bool {
        let Some(world) = self.level() else {
            return false;
        };

        // Vanilla walks down to the first block that stops movement and refuses
        // the spot if it is wet, so an enderman never lands in a lake.
        let mut pos = foton_utils::BlockPos::containing(target.x, target.y, target.z);
        while pos.y() > world.get_min_y() && !world.get_block_state(pos).blocks_motion() {
            pos = pos.below();
        }

        let landing = world.get_block_state(pos);
        if !landing.blocks_motion() || is_water_fluid(landing.get_fluid_state().fluid_id) {
            return false;
        }

        if !self.random_teleport(target) {
            return false;
        }

        self.play_sound(&sound_events::ENTITY_ENDERMAN_TELEPORT, 1.0, 1.0);
        true
    }
}

/// Stands stock still while a player is staring.
///
/// Vanilla parity: `EnderMan.EndermanFreezeWhenLookedAt`. This is the behaviour
/// players describe as the enderman "noticing" them.
struct EndermanFreezeWhenLookedAt;

impl Goal for EndermanFreezeWhenLookedAt {
    fn controls(&self) -> GoalControls {
        GoalControls::JUMP | GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(enderman) = mob.downcast_ref::<EndermanEntity>() else {
            return false;
        };
        let Some(target) = mob.target() else {
            return false;
        };
        let Some(living) = target.as_living_entity() else {
            return false;
        };
        target.as_player().is_some() && enderman.is_being_stared_by(living)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        mob.mob_base().navigation().lock().stop();
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(target) = mob.target() else {
            return;
        };
        let position = target.position();
        mob.mob_base().controls().lock().look_control.set_look_at(
            DVec3::new(position.x, target.get_eye_y(), position.z),
            // Vanilla parity: `getMaxHeadYRot`/`getMaxHeadXRot` for a mob,
            // which is a full-speed turn of the head toward the starer.
            10.0,
            40.0,
        );
    }
}

/// Takes as a target whoever is staring, and whoever it is already angry at.
///
/// Vanilla parity: `EnderMan.EndermanLookForPlayerGoal`.
struct EndermanLookForPlayerGoal;

impl Goal for EndermanLookForPlayerGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::TARGET
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(enderman) = mob.downcast_ref::<EndermanEntity>() else {
            return false;
        };
        let Some(world) = mob.level() else {
            return false;
        };

        let staring = world.nearest_player(mob.position(), 64.0, |player| {
            enderman.is_being_stared_by(player) || enderman.is_angry_at(player, &world)
        });

        let Some(player) = staring else {
            return false;
        };
        let target: SharedEntity = player;
        if let Some(living) = target.as_living_entity()
            && enderman.is_being_stared_by(living)
        {
            enderman.set_been_stared_at();
        }
        mob.set_target(Some(&target))
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.target().is_some()
    }
}

impl Entity for EndermanEntity {
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

        // Vanilla parity: `isSensitiveToWater`. Water and rain both burn, and
        // the enderman blinks away rather than standing in it.
        let Some(world) = self.level() else {
            return;
        };
        let pos = self.block_position();
        let wet = self.is_in_water() || world.is_raining_at(pos);
        if wet && Entity::is_alive(self) {
            let source = DamageSource::environment(&vanilla_damage_types::DROWN);
            self.hurt_server(&world, &source, WATER_DAMAGE);
            self.teleport_randomly();
        }
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    /// Vanilla parity: `EnderMan.addAdditionalSaveData`, whose own contribution
    /// is the carried block and the grudge.
    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        if let Some(state) = self.carried_block() {
            nbt.insert(
                TAG_CARRIED_BLOCK_STATE,
                NbtTag::Compound(block_state_nbt::save(state)),
            );
        }
        write_persistent_anger(self, nbt);
    }

    /// Vanilla parity: `EnderMan.readAdditionalSaveData`. The air filter is
    /// vanilla's: a saved air state means "carrying nothing", not "carrying a
    /// pocket of air".
    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        let carried = nbt
            .compound(TAG_CARRIED_BLOCK_STATE)
            .and_then(block_state_nbt::load)
            .filter(|state| !state.is_air());
        self.set_carried_block(carried);

        let angry_at = nbt
            .int_array("angry_at")
            .and_then(|values| <Uuid as foton_utils::UuidExt>::from_int_array(&values));
        read_persistent_anger(
            self,
            nbt.long("anger_end_time"),
            nbt.int("AngerTime"),
            angry_at,
        );
        if let Some(world) = self.level()
            && let Some(target) = resolve_anger_target(&world, angry_at)
        {
            self.set_target(Some(&target));
        }
    }
}

impl LivingEntity for EndermanEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: `Mob.serverAiStep`, which is where a mob's goals run.
    /// Without this the goal selector is never ticked and every goal this mob
    /// registers is dead code.
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

    /// Vanilla parity: `EnderMan.isSensitiveToWater`.
    fn is_sensitive_to_water(&self) -> bool {
        true
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ENDERMAN_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ENDERMAN_DEATH)
    }

    /// Drops whatever the block it was carrying would have dropped.
    ///
    /// Vanilla parity: `EnderMan.dropCustomDeathLoot`, which runs the block's
    /// own loot table against a fake diamond axe enchanted from the
    /// `minecraft:enderman_loot_drop` provider. Foton has no
    /// `EnchantmentProvider` registry, so the one enchantment that provider
    /// grants is applied here instead; the provider is shipped at
    /// `foton-utils/build_assets/builtin_datapacks/minecraft/enchantment_provider/enderman_loot_drop.json`
    /// and this should read it once a build step for that registry exists.
    /// Without the enchantment an enderman holding a grass block would drop
    /// dirt, which is not what happens in game.
    fn drop_custom_death_loot(&self, _source: &DamageSource, _killed_by_player: bool) {
        let (Some(world), Some(carried)) = (self.level(), self.carried_block()) else {
            return;
        };

        let mut fake_tool = ItemStack::new(&vanilla_items::DIAMOND_AXE);
        fake_tool.set_enchantments(&[(SILK_TOUCH.key.clone(), 1)], false);

        let position = self.position();
        let drops = BlockLootContext::new(&world, self.block_position())
            .with_entity(Some(self))
            .with_tool(&fake_tool)
            .get_drops(carried);
        for drop in drops {
            if !drop.is_empty() {
                world.spawn_item(position, drop);
            }
        }
    }

    /// Blinks away from whatever hit it.
    ///
    /// Vanilla parity: the `hurtServer` override, which is why an enderman
    /// cannot be shot: the arrow lands and it is already somewhere else.
    fn before_actually_hurt(&self, source: &DamageSource, _amount: f32) {
        let Some(world) = self.level() else {
            return;
        };
        let Some(attacker) = source
            .causing_entity_id
            .and_then(|id| world.get_entity_by_id(id))
        else {
            self.teleport_randomly();
            return;
        };
        self.teleport_away_from(attacker.position(), attacker.get_eye_y());
    }
}

impl Mob for EndermanEntity {
    /// Vanilla parity: `EnderMan` derives from `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    /// Vanilla parity: `EnderMan.requiresCustomPersistence`. An enderman with a
    /// block in its hands never despawns, which is what lets the block travel.
    /// The two disjuncts in front are the shared `Mob` body, which Rust has no
    /// `super` to reach.
    fn requires_custom_persistence(&self) -> bool {
        self.is_passenger() || self.is_leashed() || self.carried_block().is_some()
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
        Some(if self.is_creepy() {
            &sound_events::ENTITY_ENDERMAN_SCREAM
        } else {
            &sound_events::ENTITY_ENDERMAN_AMBIENT
        })
    }

    /// Marks the enderman as roused and speeds it up.
    ///
    /// Vanilla parity: the `setTarget` override. The speed modifier is
    /// transient: it is added when a target is taken and removed when it is
    /// dropped, so a calm enderman moves at its ordinary pace.
    fn set_target(&self, target: Option<&SharedEntity>) -> bool {
        let previous = self.target();
        let changed = self.mob_base().set_target(target, |_| true);
        if !changed {
            return false;
        }
        let accepted = self.finish_target_change(previous, target);
        if !accepted {
            return false;
        }

        if target.is_none() {
            *self.target_change_time.lock() = 0;
            self.set_creepy(false);
            self.entity_data.lock().ender_man_mut().stared_at.set(false);
            self.attributes().lock().set_base_value(
                vanilla_attributes::MOVEMENT_SPEED,
                self.entity_type
                    .default_attributes
                    .iter()
                    .find(|(key, _)| *key == "minecraft:movement_speed")
                    .map_or(0.3, |(_, value)| *value),
            );
        } else {
            *self.target_change_time.lock() = self.tick_count();
            self.set_creepy(true);
            let base = self
                .attributes()
                .lock()
                .required_value(vanilla_attributes::MOVEMENT_SPEED);
            self.attributes().lock().set_base_value(
                vanilla_attributes::MOVEMENT_SPEED,
                base + ATTACKING_SPEED_BONUS,
            );
        }

        true
    }

    /// Runs the anger clock, and flees the sun.
    ///
    /// Vanilla parity: `aiStep` plus `customServerAiStep`. Daylight only drives
    /// an enderman off once its grudge has had thirty seconds to cool, which is
    /// why one that has just been provoked stays and fights in the open.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.update_persistent_anger(&world, true);

        let grace_over =
            self.tick_count() >= *self.target_change_time.lock() + DAYLIGHT_GRACE_TICKS;
        if !world.is_bright_outside() || !grace_over {
            return;
        }

        let pos = self.block_position();
        let brightness = world.light_level_dependent_magic_value(pos);
        if brightness > DAYLIGHT_BRIGHTNESS
            && world.can_see_sky(pos)
            && rand::random::<f32>() * 30.0 < (brightness - 0.4) * 2.0
        {
            self.set_target(None);
            self.teleport_randomly();
        }
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

/// Puts the carried block down somewhere it will stand.
///
/// Vanilla parity: `EnderMan.EndermanLeaveBlockGoal`. Together with the take
/// goal this is what quietly rearranges a landscape overnight.
struct EndermanLeaveBlockGoal;

impl Goal for EndermanLeaveBlockGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(enderman) = mob.downcast_ref::<EndermanEntity>() else {
            return false;
        };
        if enderman.carried_block().is_none() {
            return false;
        }
        mob.level().is_some_and(|world| {
            world.get_game_rule(&MOB_GRIEFING)
                && rand::random_range(0..reduced_tick_delay(LEAVE_BLOCK_INTERVAL_TICKS)) == 0
        })
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let (Some(enderman), Some(world)) = (mob.downcast_ref::<EndermanEntity>(), mob.level())
        else {
            return;
        };
        let Some(carried) = enderman.carried_block() else {
            return;
        };

        let position = mob.position();
        let target = BlockPos::new(
            (position.x - 1.0 + rand::random::<f64>() * 2.0).floor() as i32,
            (position.y + rand::random::<f64>() * 2.0).floor() as i32,
            (position.z - 1.0 + rand::random::<f64>() * 2.0).floor() as i32,
        );
        let below = target.below();
        let carried = update_from_neighbour_shapes(&world, carried, target);
        if !can_place_block(&world, mob, target, carried, below) {
            return;
        }

        world.set_block(target, carried, UpdateFlags::UPDATE_ALL);
        world.game_event_at(
            &vanilla_game_events::BLOCK_PLACE,
            block_center(target),
            &GameEventContext::new(Some(mob), Some(carried)),
        );
        enderman.set_carried_block(None);
    }
}

/// Returns the middle of a block, the point vanilla's `Vec3.atCenterOf` gives.
fn block_center(pos: BlockPos) -> DVec3 {
    let (x, y, z) = pos.get_center();
    DVec3::new(x, y, z)
}

/// Vanilla parity: `EndermanLeaveBlockGoal.canPlaceBlock`.
fn can_place_block(
    world: &Arc<World>,
    enderman: &dyn PathfinderMob,
    pos: BlockPos,
    carried: BlockStateId,
    below: BlockPos,
) -> bool {
    let target_state = world.get_block_state(pos);
    let below_state = world.get_block_state(below);
    if !target_state.is_air()
        || below_state.is_air()
        || below_state.get_block() == &vanilla_blocks::BEDROCK
        || !world.is_collision_shape_full_block_at(below, below_state)
    {
        return false;
    }
    if !BLOCK_BEHAVIORS
        .get_behavior(carried.get_block())
        .can_survive(carried, world.as_ref(), pos)
    {
        return false;
    }

    // Vanilla parity: `level.getEntities(this.enderman, unitCube)`, which is
    // what stops an enderman entombing you in the block it is holding.
    let cube = WorldAabb::new(
        f64::from(pos.x()),
        f64::from(pos.y()),
        f64::from(pos.z()),
        f64::from(pos.x()) + 1.0,
        f64::from(pos.y()) + 1.0,
        f64::from(pos.z()) + 1.0,
    );
    world
        .get_entities_in_aabb_matching(&cube, |entity| entity.id() != enderman.id())
        .is_empty()
}

/// Takes a block out of the ground and walks off with it.
///
/// Vanilla parity: `EnderMan.EndermanTakeBlockGoal`.
struct EndermanTakeBlockGoal;

impl Goal for EndermanTakeBlockGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(enderman) = mob.downcast_ref::<EndermanEntity>() else {
            return false;
        };
        if enderman.carried_block().is_some() {
            return false;
        }
        mob.level().is_some_and(|world| {
            world.get_game_rule(&MOB_GRIEFING)
                && rand::random_range(0..reduced_tick_delay(TAKE_BLOCK_INTERVAL_TICKS)) == 0
        })
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let (Some(enderman), Some(world)) = (mob.downcast_ref::<EndermanEntity>(), mob.level())
        else {
            return;
        };

        let position = mob.position();
        let target = BlockPos::new(
            (position.x - 2.0 + rand::random::<f64>() * 4.0).floor() as i32,
            (position.y + rand::random::<f64>() * 3.0).floor() as i32,
            (position.z - 2.0 + rand::random::<f64>() * 4.0).floor() as i32,
        );
        let state = world.get_block_state(target);
        if !REGISTRY
            .blocks
            .is_in_tag(state.get_block(), &BlockTag::ENDERMAN_HOLDABLE)
        {
            return;
        }

        // Vanilla parity: the clip that stops an enderman reaching through a
        // wall for the block behind it.
        let block_pos = mob.block_position();
        let from = DVec3::new(
            f64::from(block_pos.x()) + 0.5,
            f64::from(target.y()) + 0.5,
            f64::from(block_pos.z()) + 0.5,
        );
        let to = block_center(target);
        let hit = world.clip(from, to, ClipBlockShape::Outline, ClipFluid::None);
        if hit.block_pos != target {
            return;
        }

        world.remove_block(target, false);
        world.game_event_at(
            &vanilla_game_events::BLOCK_DESTROY,
            to,
            &GameEventContext::new(Some(mob), Some(state)),
        );
        enderman.set_carried_block(Some(state.get_block().default_state()));
    }
}

impl PathfinderMob for EndermanEntity {}

impl NeutralMob for EndermanEntity {
    fn persistent_anger(&self) -> &PersistentAnger {
        &self.anger
    }

    /// Vanilla parity: `startPersistentAngerTimer`, twenty to thirty-nine
    /// seconds.
    fn start_persistent_anger_timer(&self) {
        self.set_time_to_remain_angry(rand::random_range(ANGER_MIN_TICKS..=ANGER_MAX_TICKS));
    }
}

impl Enemy for EndermanEntity {}

#[cfg(test)]
mod tests;
