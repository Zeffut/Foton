//! The wither.
//!
//! Vanilla parity: `WitherBoss`. The fight has three parts. It arrives
//! invulnerable and healing, counts down for eleven seconds and then blows a
//! hole in whatever it was built in. It fights with three heads that aim
//! independently -- the middle one follows the mob's own target, the two side
//! ones each pick their own from anything alive nearby -- and it chews through
//! the blocks around it every time it is hurt. Below half health it is
//! *powered*: arrows and wind charges stop working on it, and it stops trying
//! to climb above its target.
//!
//! **Gaps**: vanilla also overrides `getBlockExplosionResistance` on the
//! charged skull, which is what lets the blue skull eat obsidian; that lives on
//! [`WitherSkullEntity`](crate::entity::entities::WitherSkullEntity) and is
//! already documented there as a missing `World::explode` hook. The head
//! rotation arrays (`xRotHeads`/`yRotHeads`) are not kept: their only readers
//! are `getHeadYRots`/`getHeadXRots`, which the client renderer calls, and the
//! client recomputes them from the synced targets in its own `aiStep`.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::{BossBarColor, BossBarOverlay, SoundSource};
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::vanilla_damage_type_tags::DamageTypeTag;
use foton_registry::vanilla_entity_data::WitherBossEntityData;
use foton_registry::vanilla_entity_type_tags::EntityTypeTag;
use foton_registry::vanilla_game_rules::{MOB_EXPLOSION_DROP_DECAY, MOB_GRIEFING};
use foton_registry::{
    REGISTRY, TaggedRegistryExt as _, blocks::block_state_ext::BlockStateExt as _, level_events,
    sound_events, vanilla_entities, vanilla_items,
};
use foton_utils::locks::SyncMutex;
use foton_utils::types::Difficulty;
use foton_utils::{BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use text_components::TextComponent;

use crate::boss_event::ServerBossEvent;
use crate::entity::Enemy;
use crate::entity::LivingEntitySyncedData;
use crate::entity::ai::goal::{
    Goal, GoalControls, HurtByTargetGoal, LookAtPlayerGoal, NearestAttackableTargetGoal,
    RandomLookAroundGoal, RangedAttackGoal, WaterAvoidingRandomFlyingGoal,
};
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::damage::DamageSource;
use crate::entity::entities::{
    ArrowEntity, ThrownTridentEntity, WindChargeEntity, WitherSkullEntity,
};
use crate::entity::entity_type_name;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, HurtingProjectile as _, LivingEntity,
    LivingEntityBase, Mob, MobBase, MobEffectInstance, MoveControlKind, NavigationKind,
    PathfinderMob, RemovalReason, SharedEntity, next_entity_id,
};
use crate::physics::MoveResult;
use crate::player::Player;
use crate::world::World;
use crate::world::explosion::{ExplosionBlockInteraction, ExplosionSpec};

#[cfg(test)]
mod tests;

/// Experience a wither drops.
///
/// Vanilla parity: the `this.xpReward = 50` of the constructor.
const XP_REWARD: i32 = 50;

/// Ticks the summon spends invulnerable.
///
/// Vanilla parity: `WitherBoss.INVULNERABLE_TICKS`. The bar fills over exactly
/// this many ticks, so it is both the countdown and the progress denominator.
pub const INVULNERABLE_TICKS: i32 = 220;

/// Health the wither gets back every ten ticks while it is arriving.
///
/// Vanilla parity: the `this.heal(10.0F)` of the invulnerable branch.
const SPAWN_HEAL: f32 = 10.0;

/// Health the wither gets back every second once the fight has started.
const COMBAT_HEAL: f32 = 1.0;

/// Radius of the blast the summon ends with.
///
/// Vanilla parity: the `7.0F` of `customServerAiStep`.
const SPAWN_EXPLOSION_RADIUS: f32 = 7.0;

/// How far a side head looks for something to shoot.
///
/// Vanilla parity: the `range(20.0)` of `TARGETING_CONDITIONS`.
const HEAD_TARGET_RANGE: f64 = 20.0;

/// Squared distance past which a side head gives up on its target.
///
/// Vanilla parity: the `distanceToSqr(current) > 900.0` of the head loop.
const HEAD_ATTACK_RANGE_SQR: f64 = 900.0;

/// How far out the head scan reaches, as a bounding-box inflation.
///
/// Vanilla parity: the `inflate(20.0, 8.0, 20.0)` of the same loop.
const HEAD_SCAN_INFLATION: DVec3 = DVec3::new(20.0, 8.0, 20.0);

/// Head-idle count past which a head fires at nothing in particular.
///
/// Vanilla parity: the `this.idleHeadUpdates[i - 1]++ > 15` of the head loop,
/// which is what makes a wither that cannot see anyone still spit at the walls.
const IDLE_UPDATES_BEFORE_BLIND_SHOT: i32 = 15;

/// How far a blind shot may land from the wither, horizontally.
const BLIND_SHOT_HORIZONTAL_RANGE: f64 = 10.0;

/// How far a blind shot may land from the wither, vertically.
const BLIND_SHOT_VERTICAL_RANGE: f64 = 5.0;

/// Ticks between being hurt and the blocks around the wither going.
///
/// Vanilla parity: the `this.destroyBlocksTick = 20` of `hurtServer`.
const DESTROY_BLOCKS_DELAY: i32 = 20;

/// How much each hit adds to every head's idle counter.
///
/// Vanilla parity: the `idleHeadUpdates[i] + 3` of `hurtServer`, which is why
/// hitting a wither makes it fire back sooner.
const IDLE_UPDATES_PER_HIT: i32 = 3;

/// Age an item entity is given so it lasts five minutes longer than usual.
///
/// Vanilla parity: `ItemEntity.setExtendedLifetime`.
const EXTENDED_LIFETIME_AGE: i32 = -6000;

/// Height of the middle head above the wither's feet, before scaling.
const CENTER_HEAD_HEIGHT: f32 = 3.0;

/// Height of a side head above the wither's feet, before scaling.
const SIDE_HEAD_HEIGHT: f32 = 2.2;

/// How far a side head sits out from the body, before scaling.
const SIDE_HEAD_OFFSET: f64 = 1.3;

/// Chance the middle head charges its shot into a blue skull.
///
/// Vanilla parity: the `this.random.nextFloat() < 0.001F` of the two-argument
/// `performRangedAttack`, which only the middle head can roll.
const DANGEROUS_SKULL_CHANCE: f32 = 0.001;

/// Squared horizontal speed past which the wither faces where it is going.
const FACING_SPEED_SQR: f64 = 0.05;

/// Squared horizontal distance to its target past which the wither closes in.
///
/// Vanilla parity: the `delta.horizontalDistanceSqr() > 9.0` of `aiStep`.
const CHASE_DISTANCE_SQR: f64 = 9.0;

/// Returns whether a wither would shoot at this entity.
///
/// Vanilla parity: `WitherBoss.LIVING_ENTITY_SELECTOR`. `attackable` is what
/// keeps an armor stand out of it: the stand overrides it to `false`, so the
/// wither ignores stands without naming the class.
fn is_wither_prey(target: &dyn LivingEntity) -> bool {
    !REGISTRY
        .entity_types
        .is_in_tag(target.entity_type(), &EntityTypeTag::WITHER_FRIENDS)
        && target.attackable()
}

/// Vanilla parity: `Level.ExplosionInteraction.MOB`, which the level resolves
/// through `mobGriefing` and the drop-decay rule.
fn mob_explosion_interaction(world: &Arc<World>) -> ExplosionBlockInteraction {
    if world.get_game_rule(&MOB_GRIEFING) {
        world.explosion_destroy_type(&MOB_EXPLOSION_DROP_DECAY)
    } else {
        ExplosionBlockInteraction::Keep
    }
}

/// Returns whether the wither's blast and skulls may take this block.
///
/// Vanilla parity: `WitherBoss.canDestroy`.
#[must_use]
pub fn can_destroy(state: BlockStateId) -> bool {
    !state.is_air() && !state.get_block().has_tag(&BlockTag::WITHER_IMMUNE)
}

/// A wither.
#[entity_behavior(class = "WitherBoss")]
pub struct WitherBoss {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<WitherBossEntityData>,
    /// The purple bar every nearby player sees.
    ///
    /// Vanilla parity: `WitherBoss.bossEvent`.
    boss_event: ServerBossEvent,
    /// Tick each side head next reconsiders what it is doing.
    ///
    /// Vanilla parity: `WitherBoss.nextHeadUpdate`.
    next_head_update: SyncMutex<[i32; 2]>,
    /// How many times each side head has come up empty.
    ///
    /// Vanilla parity: `WitherBoss.idleHeadUpdates`.
    idle_head_updates: SyncMutex<[i32; 2]>,
    /// Ticks left before the blocks around a hurt wither go.
    ///
    /// Vanilla parity: `WitherBoss.destroyBlocksTick`.
    destroy_blocks_tick: SyncMutex<i32>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `WitherBoss`.
unsafe impl DowncastType for WitherBoss {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/wither");
}

impl WitherBoss {
    /// Creates a wither at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a wither from saved base data.
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
        let mut entity_data = WitherBossEntityData::new();
        // Vanilla parity: the `setHealth(getMaxHealth())` of the constructor,
        // which is what `initialize_synced_data` does for every mob here.
        living_base.initialize_synced_data(&mut entity_data);
        mob_base.set_xp_reward(XP_REWARD);

        {
            // Keep vanilla WitherBoss goal priorities in the same order.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(0, WitherDoNothingGoal);
            goals.add_goal(2, RangedAttackGoal::new(1.0, 40, 20.0, fire_wither_skull));
            goals.add_goal(5, WaterAvoidingRandomFlyingGoal::new(1.0));
            goals.add_goal(6, LookAtPlayerGoal::new(8.0));
            goals.add_goal(7, RandomLookAroundGoal::new());
        }

        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(1, HurtByTargetGoal::new());
            // Vanilla parity: `new NearestAttackableTargetGoal<>(this,
            // LivingEntity.class, 0, false, false, LIVING_ENTITY_SELECTOR)` --
            // it rescans every tick, needs neither sight nor a path, and takes
            // anything alive that is not a wither friend.
            targets.add_goal(
                2,
                NearestAttackableTargetGoal::new_with_interval(0, false, false, |_, target, _| {
                    is_wither_prey(target)
                }),
            );
        }

        let boss_event = ServerBossEvent::with_random_id(
            display_name_of(&base, entity_type),
            BossBarColor::Purple,
            BossBarOverlay::Progress,
        );
        boss_event.set_darken_screen(true);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            boss_event,
            next_head_update: SyncMutex::new([0; 2]),
            idle_head_updates: SyncMutex::new([0; 2]),
            destroy_blocks_tick: SyncMutex::new(0),
        }
    }

    /// Returns the bar this wither shows the players fighting it.
    #[must_use]
    pub const fn boss_event(&self) -> &ServerBossEvent {
        &self.boss_event
    }

    /// Returns how many ticks of the arrival are left.
    ///
    /// Vanilla parity: `WitherBoss.getInvulnerableTicks`.
    #[must_use]
    pub fn invulnerable_ticks(&self) -> i32 {
        *self.entity_data.lock().id_inv.get()
    }

    /// Vanilla parity: `WitherBoss.setInvulnerableTicks`.
    pub fn set_invulnerable_ticks(&self, ticks: i32) {
        self.entity_data.lock().id_inv.set(ticks);
    }

    /// Returns what one head is aiming at, or `0` for nothing.
    ///
    /// Vanilla parity: `WitherBoss.getAlternativeTarget`. Slot `0` mirrors the
    /// mob's own target and drives the middle head; slots `1` and `2` are the
    /// side heads, which choose for themselves.
    #[must_use]
    pub fn alternative_target(&self, slot: usize) -> i32 {
        let data = self.entity_data.lock();
        match slot {
            0 => *data.target_a.get(),
            1 => *data.target_b.get(),
            _ => *data.target_c.get(),
        }
    }

    /// Vanilla parity: `WitherBoss.setAlternativeTarget`.
    pub fn set_alternative_target(&self, slot: usize, entity_id: i32) {
        let mut data = self.entity_data.lock();
        match slot {
            0 => data.target_a.set(entity_id),
            1 => data.target_b.set(entity_id),
            _ => data.target_c.set(entity_id),
        }
    }

    /// Returns whether the wither is below half health.
    ///
    /// Vanilla parity: `WitherBoss.isPowered`, the armored second half of the
    /// fight.
    #[must_use]
    pub fn is_powered(&self) -> bool {
        self.get_health() <= self.get_max_health() / 2.0
    }

    /// Starts the arrival: eleven seconds of invulnerability, an empty bar and
    /// a third of the health bar.
    ///
    /// Vanilla parity: `WitherBoss.makeInvulnerable`, which the summoning skull
    /// calls before the wither is added to the world.
    pub fn make_invulnerable(&self) {
        self.set_invulnerable_ticks(INVULNERABLE_TICKS);
        self.boss_event.set_progress(0.0);
        self.set_health(self.get_max_health() / 3.0);
    }

    /// Returns where one head sits.
    ///
    /// Vanilla parity: `WitherBoss.getHeadX` / `getHeadY` / `getHeadZ`. Head
    /// `0` is the body position itself. Vanilla offsets the side heads by
    /// `180 * (index - 1)` degrees, so heads `1` and `3` share an angle; that
    /// is vanilla's own arithmetic and the skull spawn positions follow it.
    #[must_use]
    pub fn head_position(&self, head: usize) -> DVec3 {
        let position = self.position();
        let scale = f64::from(self.get_scale());
        let height = if head == 0 {
            CENTER_HEAD_HEIGHT
        } else {
            SIDE_HEAD_HEIGHT
        };
        let y = position.y + f64::from(height) * scale;
        if head == 0 {
            return DVec3::new(position.x, y, position.z);
        }

        #[expect(
            clippy::cast_precision_loss,
            reason = "the head index is 1 or 2, exactly representable"
        )]
        let head_angle = (self.y_body_rot() + 180.0 * (head as f32 - 1.0)).to_radians();
        DVec3::new(
            position.x + f64::from(head_angle.cos()) * SIDE_HEAD_OFFSET * scale,
            y,
            position.z + f64::from(head_angle.sin()) * SIDE_HEAD_OFFSET * scale,
        )
    }

    /// Spits a skull from one head at a living target.
    ///
    /// Vanilla parity: the two-argument `WitherBoss.performRangedAttack`. Only
    /// the middle head can roll the charged blue skull.
    fn perform_ranged_attack(&self, head: usize, target: &SharedEntity) {
        let position = target.position();
        let dangerous = head == 0 && rand::random::<f32>() < DANGEROUS_SKULL_CHANCE;
        self.perform_ranged_attack_at(
            head,
            DVec3::new(
                position.x,
                position.y + target.get_eye_height() * 0.5,
                position.z,
            ),
            dangerous,
        );
    }

    /// Spits a skull from one head at a point.
    ///
    /// Vanilla parity: the five-argument `WitherBoss.performRangedAttack`.
    fn perform_ranged_attack_at(&self, head: usize, target: DVec3, dangerous: bool) {
        let Some(world) = self.level() else {
            return;
        };

        if !self.is_silent() {
            world.level_event(
                level_events::SOUND_WITHER_BOSS_SHOOT,
                self.block_position(),
                0,
                None,
            );
        }

        let muzzle = self.head_position(head);
        let skull = Arc::new(WitherSkullEntity::new(
            &vanilla_entities::WITHER_SKULL,
            next_entity_id(),
            muzzle,
            Arc::downgrade(&world),
        ));
        if dangerous {
            skull.set_dangerous(true);
        }

        let direction = target - muzzle;
        if let Some(owner) = world.get_entity_by_id(self.id()) {
            skull.shoot_from_owner(&owner, direction);
        } else {
            // Only reachable before the wither itself is in the world, which
            // no goal can do; the skull still needs a heading.
            skull.set_rotation(self.rotation());
            skull.assign_directional_movement(direction);
        }

        let entity: SharedEntity = skull;
        if let Err(error) = world.try_add_entity(entity) {
            log::debug!("wither failed to spit a skull: {error}");
        }
    }

    /// Runs the eleven seconds between the summon and the fight.
    ///
    /// Vanilla parity: the `getInvulnerableTicks() > 0` branch of
    /// `customServerAiStep`.
    fn tick_arrival(&self, world: &Arc<World>) {
        let remaining = self.invulnerable_ticks() - 1;
        #[expect(
            clippy::cast_precision_loss,
            reason = "the countdown is 220 ticks, exactly representable"
        )]
        let progress = 1.0 - remaining as f32 / INVULNERABLE_TICKS as f32;
        self.boss_event.set_progress(progress);

        if remaining <= 0 {
            let position = self.position();
            world.explode(
                ExplosionSpec::new(
                    Some(self.id()),
                    Some(self.id()),
                    None,
                    SPAWN_EXPLOSION_RADIUS,
                    false,
                    mob_explosion_interaction(world),
                ),
                DVec3::new(position.x, self.get_eye_y(), position.z),
            );
            if !self.is_silent() {
                world.global_level_event(
                    level_events::SOUND_WITHER_BOSS_SPAWN,
                    self.block_position(),
                    0,
                );
            }
        }

        self.set_invulnerable_ticks(remaining);
        if self.tick_count() % 10 == 0 {
            self.heal(SPAWN_HEAL);
        }
    }

    /// Lets one side head reconsider what it is shooting at.
    ///
    /// Vanilla parity: one iteration of the `for (int i = 1; i < 3; i++)` loop
    /// of `customServerAiStep`. `slot` is vanilla's `i`, so the head it drives
    /// is `slot + 1` and the counters it keeps are at `slot - 1`.
    fn tick_side_head(&self, world: &Arc<World>, slot: usize) {
        let tick = self.tick_count();
        if tick < self.next_head_update.lock()[slot - 1] {
            return;
        }
        self.next_head_update.lock()[slot - 1] = tick + 10 + rand::random_range(0..10);

        if matches!(world.difficulty(), Difficulty::Normal | Difficulty::Hard) {
            let idle = self.idle_head_updates.lock()[slot - 1];
            self.idle_head_updates.lock()[slot - 1] = idle + 1;
            if idle > IDLE_UPDATES_BEFORE_BLIND_SHOT {
                let position = self.position();
                let blind_target = DVec3::new(
                    rand::random_range(
                        position.x - BLIND_SHOT_HORIZONTAL_RANGE
                            ..position.x + BLIND_SHOT_HORIZONTAL_RANGE,
                    ),
                    rand::random_range(
                        position.y - BLIND_SHOT_VERTICAL_RANGE
                            ..position.y + BLIND_SHOT_VERTICAL_RANGE,
                    ),
                    rand::random_range(
                        position.z - BLIND_SHOT_HORIZONTAL_RANGE
                            ..position.z + BLIND_SHOT_HORIZONTAL_RANGE,
                    ),
                );
                self.perform_ranged_attack_at(slot + 1, blind_target, true);
                self.idle_head_updates.lock()[slot - 1] = 0;
            }
        }

        let head_target = self.alternative_target(slot);
        if head_target > 0 {
            self.tick_committed_head(world, slot, head_target);
            return;
        }

        let search = self.bounding_box().inflate_xyz(
            HEAD_SCAN_INFLATION.x,
            HEAD_SCAN_INFLATION.y,
            HEAD_SCAN_INFLATION.z,
        );
        let conditions = TargetingConditions::for_combat()
            .range(HEAD_TARGET_RANGE)
            .selector(|_, target, _| is_wither_prey(target));
        let candidates = world.get_entities_in_aabb_matching(&search, |entity| {
            entity
                .as_living_entity()
                .is_some_and(|living| conditions.test(world, Some(self), living))
        });
        if candidates.is_empty() {
            return;
        }
        let chosen = &candidates[rand::random_range(0..candidates.len())];
        self.set_alternative_target(slot, chosen.id());
    }

    /// Fires at the target a side head already has, or drops it.
    ///
    /// Vanilla parity: the `headTarget > 0` branch of the head loop.
    fn tick_committed_head(&self, world: &Arc<World>, slot: usize, head_target: i32) {
        let current = world.get_entity_by_id(head_target).filter(|entity| {
            entity.as_living_entity().is_some_and(|living| {
                Mob::can_attack(self, living)
                    && entity.position().distance_squared(self.position()) <= HEAD_ATTACK_RANGE_SQR
                    && self.has_line_of_sight(entity.as_ref())
            })
        });

        let Some(current) = current else {
            self.set_alternative_target(slot, 0);
            return;
        };

        self.perform_ranged_attack(slot + 1, &current);
        self.next_head_update.lock()[slot - 1] = self.tick_count() + 40 + rand::random_range(0..20);
        self.idle_head_updates.lock()[slot - 1] = 0;
    }

    /// Eats the blocks a hurt wither is standing in.
    ///
    /// Vanilla parity: the `destroyBlocksTick` branch of `customServerAiStep`.
    fn tick_block_destruction(&self, world: &Arc<World>) {
        let remaining = {
            let mut destroy_blocks_tick = self.destroy_blocks_tick.lock();
            if *destroy_blocks_tick <= 0 {
                return;
            }
            *destroy_blocks_tick -= 1;
            *destroy_blocks_tick
        };
        if remaining != 0 || !world.get_game_rule(&MOB_GRIEFING) {
            return;
        }

        let dimensions = self.base().dimensions();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "vanilla floors the same two dimensions into ints"
        )]
        let half_width = (dimensions.width / 2.0 + 1.0).floor() as i32;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "vanilla floors the same two dimensions into ints"
        )]
        let height = dimensions.height.floor() as i32;
        let origin = self.block_position();

        let mut destroyed = false;
        for pos in BlockPos::between_closed(
            BlockPos::new(origin.x() - half_width, origin.y(), origin.z() - half_width),
            BlockPos::new(
                origin.x() + half_width,
                origin.y() + height,
                origin.z() + half_width,
            ),
        ) {
            if can_destroy(world.get_block_state(pos)) {
                destroyed = world.destroy_block_by_entity(pos, true, self) || destroyed;
            }
        }

        if destroyed {
            world.level_event(
                level_events::SOUND_WITHER_BLOCK_BREAK,
                self.block_position(),
                0,
                None,
            );
        }
    }

    /// Returns whether a hit lands at all.
    ///
    /// Vanilla parity: the guard chain at the top of `WitherBoss.hurtServer`,
    /// split out so the counters it would otherwise bump stay in one place.
    fn rejects_damage(&self, world: &World, source: &DamageSource) -> bool {
        if self.is_invulnerable_to(world, source) {
            return true;
        }
        if source.is(&DamageTypeTag::WITHER_IMMUNE_TO) {
            return true;
        }
        if self.invulnerable_ticks() > 0 && !source.is(&DamageTypeTag::BYPASSES_INVULNERABILITY) {
            return true;
        }
        // Vanilla parity: `source.getEntity() instanceof WitherBoss`, then the
        // separate `WITHER_FRIENDS` test on the same entity.
        let causing = source
            .causing_entity_id
            .and_then(|id| world.get_entity_by_id(id));
        if let Some(causing) = &causing {
            if causing.downcast_ref::<Self>().is_some() {
                return true;
            }
            if REGISTRY
                .entity_types
                .is_in_tag(causing.entity_type(), &EntityTypeTag::WITHER_FRIENDS)
            {
                return true;
            }
        }

        // Vanilla parity: a powered wither shrugs off arrows and wind charges.
        // Foton has no `AbstractArrow` layer, so its two concrete subclasses
        // are named; a third would have to be added here.
        if self.is_powered()
            && let Some(direct) = source
                .direct_entity_id
                .and_then(|id| world.get_entity_by_id(id))
            && (direct.downcast_ref::<ArrowEntity>().is_some()
                || direct.downcast_ref::<ThrownTridentEntity>().is_some()
                || direct.downcast_ref::<WindChargeEntity>().is_some())
        {
            return true;
        }

        false
    }
}

/// Returns the name the bar carries.
///
/// Vanilla parity: `Entity.getDisplayName`, read before the wither exists as a
/// trait object. A summoned wither has no custom name, so this is the
/// translated type name; `set_custom_name` retitles the bar afterwards.
fn display_name_of(base: &EntityBase, entity_type: EntityTypeRef) -> TextComponent {
    base.custom_name()
        .unwrap_or_else(|| entity_type_name(entity_type))
}

/// Stands perfectly still while the wither is arriving.
///
/// Vanilla parity: `WitherBoss.WitherDoNothingGoal`. It holds the move, jump
/// and look controls without doing anything with them, which is what keeps the
/// wither hanging in the air through the countdown.
struct WitherDoNothingGoal;

impl Goal for WitherDoNothingGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE | GoalControls::JUMP | GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.downcast_ref::<WitherBoss>()
            .is_some_and(|wither| wither.invulnerable_ticks() > 0)
    }
}

/// Vanilla parity: `WitherBoss.performRangedAttack(LivingEntity, float)`,
/// which sends everything through the middle head and ignores the power.
fn fire_wither_skull(mob: &dyn PathfinderMob, target: &SharedEntity, _power: f32) {
    let Some(wither) = mob.downcast_ref::<WitherBoss>() else {
        return;
    };
    wither.perform_ranged_attack(0, target);
}

impl Entity for WitherBoss {
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

    /// Vanilla parity: `WitherBoss.startSeenByPlayer`.
    fn start_seen_by_player(&self, player: &Arc<Player>) {
        self.boss_event.add_player(player);
    }

    /// Vanilla parity: `WitherBoss.stopSeenByPlayer`.
    fn stop_seen_by_player(&self, player: &Player) {
        self.boss_event.remove_player(player);
    }

    /// Vanilla parity: `WitherBoss.setCustomName`, which retitles the bar.
    fn set_custom_name(&self, custom_name: Option<TextComponent>) {
        self.base().set_custom_name(custom_name);
        self.boss_event.set_name(self.display_name());
    }

    /// Vanilla parity: `WitherBoss.makeStuckInBlock` is empty, so cobwebs and
    /// sweet berries do not slow a wither down.
    fn make_stuck_in_block(&self, _state: BlockStateId, _speed_multiplier: DVec3) {}

    /// Vanilla parity: `WitherBoss.canRide` returns false.
    fn can_ride(&self, _vehicle: &dyn Entity) -> bool {
        false
    }

    /// Vanilla parity: `WitherBoss.canUsePortal` returns false.
    fn can_use_portal(&self, _ignore_passenger: bool) -> bool {
        false
    }

    /// Vanilla parity: `WitherBoss.checkDespawn`. A wither never despawns for
    /// distance; peaceful is the only thing that removes one.
    fn check_despawn(&self) {
        let Some(world) = self.level() else {
            return;
        };
        if world.difficulty() == Difficulty::Peaceful && !self.entity_type.allowed_in_peaceful {
            self.set_removed(RemovalReason::Discarded);
        } else {
            self.set_no_action_time(0);
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        nbt.insert("Invul", self.invulnerable_ticks());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.set_invulnerable_ticks(nbt.int("Invul").unwrap_or(0));
        // Vanilla parity: `readAdditionalSaveData` retitles the bar for a
        // wither that was named before it was saved.
        if self.custom_name().is_some() {
            self.boss_event.set_name(self.display_name());
        }
    }
}

impl LivingEntity for WitherBoss {
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

    /// Vanilla parity: `WitherBoss.aiStep`, the half that moves the mob. The
    /// rest of vanilla's override is the head rotation arrays and the smoke,
    /// both of which the client owns.
    fn ai_step(&self) -> Option<MoveResult> {
        let mut movement = self.velocity() * DVec3::new(1.0, 0.6, 1.0);

        let chased = self.level().and_then(|world| {
            let target_id = self.alternative_target(0);
            (target_id > 0)
                .then(|| world.get_entity_by_id(target_id))
                .flatten()
        });
        if let Some(chased) = chased {
            let position = self.position();
            let target_position = chased.position();
            let mut vertical = movement.y;
            if position.y < target_position.y
                || !self.is_powered() && position.y < target_position.y + 5.0
            {
                vertical = vertical.max(0.0);
                vertical += 0.3 - vertical * 0.6;
            }
            movement = DVec3::new(movement.x, vertical, movement.z);

            let to_target = DVec3::new(
                target_position.x - position.x,
                0.0,
                target_position.z - position.z,
            );
            if to_target.length_squared() > CHASE_DISTANCE_SQR {
                let heading = to_target.normalize();
                movement += DVec3::new(
                    heading.x * 0.3 - movement.x * 0.6,
                    0.0,
                    heading.z * 0.3 - movement.z * 0.6,
                );
            }
        }

        self.set_velocity(movement);
        if movement.x * movement.x + movement.z * movement.z > FACING_SPEED_SQR {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "vanilla casts the same angle to a float"
            )]
            let yaw = movement.z.atan2(movement.x).to_degrees() as f32 - 90.0;
            self.set_rotation((yaw, self.rotation().1));
        }

        self.default_ai_step()
    }

    /// Vanilla parity: `WitherBoss.hurtServer`.
    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        if self.rejects_damage(world, source) {
            return false;
        }

        {
            let mut destroy_blocks_tick = self.destroy_blocks_tick.lock();
            if *destroy_blocks_tick <= 0 {
                *destroy_blocks_tick = DESTROY_BLOCKS_DELAY;
            }
        }
        for idle in self.idle_head_updates.lock().iter_mut() {
            *idle += IDLE_UPDATES_PER_HIT;
        }

        self.living_hurt_server(world, source, amount)
    }

    /// Vanilla parity: `WitherBoss.addEffect` returns false for everything.
    /// Foton routes every effect through `can_be_affected`, so the blanket
    /// refusal and vanilla's narrower `canBeAffected` collapse into one.
    fn can_be_affected(&self, _effect: &MobEffectInstance) -> bool {
        false
    }

    /// Vanilla parity: `WitherBoss.dropCustomDeathLoot`.
    fn drop_custom_death_loot(&self, source: &DamageSource, killed_by_player: bool) {
        self.drop_custom_death_loot_mob(source, killed_by_player);
        if let Some(nether_star) =
            self.spawn_at_location(ItemStack::new(&vanilla_items::NETHER_STAR), 0.0)
        {
            nether_star.set_age(EXTENDED_LIFETIME_AGE);
        }
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
        Some(&sound_events::ENTITY_WITHER_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_WITHER_DEATH)
    }
}

impl Mob for WitherBoss {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    /// Vanilla parity: the `FlyingMoveControl(this, 10, false)` the constructor
    /// installs.
    fn move_control_kind(&self) -> MoveControlKind {
        MoveControlKind::Flying {
            max_turn: 10.0,
            hovers_in_place: false,
        }
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_WITHER_AMBIENT)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    /// Vanilla parity: `WitherBoss.customServerAiStep`.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };

        if self.invulnerable_ticks() > 0 {
            self.tick_arrival(&world);
            return;
        }

        for slot in 1..3 {
            self.tick_side_head(&world, slot);
        }

        match self.target() {
            Some(target) => self.set_alternative_target(0, target.id()),
            None => self.set_alternative_target(0, 0),
        }

        self.tick_block_destruction(&world);

        if self.tick_count() % 20 == 0 {
            self.heal(COMBAT_HEAL);
        }

        self.boss_event
            .set_progress(self.get_health() / self.get_max_health());
    }
}

impl PathfinderMob for WitherBoss {
    /// Vanilla parity: `WitherBoss.createNavigation`, a `FlyingPathNavigation`
    /// that may float and may not open doors.
    fn navigation_kind(&self) -> NavigationKind {
        NavigationKind::Flying
    }
}

impl Enemy for WitherBoss {}
