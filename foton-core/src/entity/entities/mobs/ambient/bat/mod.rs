//! Bat entity.
//!
//! Vanilla parity: `Bat` and `AmbientCreature`. The first mob in Foton that
//! flies, and it does so without any pathfinding at all: a bat hangs from a
//! ceiling until something disturbs it, then drifts toward a point it picks at
//! random and picks another when it arrives.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::BatEntityData;
use foton_registry::{level_events, sound_events};
use foton_utils::locks::SyncMutex;
use foton_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::entity::EntitySpawnReason;
use crate::entity::damage::DamageSource;
use crate::entity::spawn_rules::check_bat_spawn_rules;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntityMovementEmission, EntitySyncedData, LivingEntity,
    LivingEntityBase, LivingTravelInput, Mob, MobBase, PathfinderMob,
};
use crate::world::World;

/// Bit of the synced flags byte that marks a hanging bat.
///
/// Vanilla parity: `Bat.FLAG_RESTING`.
const RESTING_FLAG: i8 = 1;

/// How close a player has to come to wake a hanging bat.
///
/// Vanilla parity: the `range(4.0)` of `Bat.BAT_RESTING_TARGETING`.
const WAKE_RANGE: f64 = 4.0;

/// Fraction of upward speed a flying bat keeps each tick.
///
/// Vanilla parity: the `multiply(1.0, 0.6, 1.0)` of `Bat.tick`, which is what
/// makes a bat bob rather than climb.
const VERTICAL_DRAG: f64 = 0.6;

/// One chance in this many ticks that a hanging bat turns its head.
///
/// Vanilla parity: the `nextInt(200)` of `Bat.customServerAiStep`.
const HEAD_TURN_CHANCE: i32 = 200;

/// One chance in this many ticks that a flying bat picks a new destination.
///
/// Vanilla parity: the `nextInt(30)` of the same method.
const RETARGET_CHANCE: i32 = 30;

/// One chance in this many ticks that a flying bat settles down again.
///
/// Vanilla parity: the `nextInt(100)` of the same method.
const REST_CHANCE: i32 = 100;

/// How close a bat has to get before its destination counts as reached.
///
/// Vanilla parity: the `closerToCenterThan(position, 2.0)` check.
const ARRIVAL_DISTANCE: f64 = 2.0;

/// Horizontal spread of a new destination, in blocks.
///
/// Vanilla parity: the `nextInt(7) - nextInt(7)` of the same method.
const WANDER_HORIZONTAL_RANGE: i32 = 7;

/// Vertical spread of a new destination, in blocks.
///
/// Vanilla parity: the `nextInt(6) - 2` of the same method.
const WANDER_VERTICAL_RANGE: i32 = 6;

/// How far below its own height a bat may aim.
///
/// Vanilla parity: the `- 2.0` of the same expression.
const WANDER_VERTICAL_DROP: i32 = 2;

/// How hard a bat steers horizontally toward its destination.
///
/// Vanilla parity: the `signum(dx) * 0.5` of the steering term.
const STEER_HORIZONTAL: f64 = 0.5;

/// How hard a bat steers vertically toward its destination.
///
/// Vanilla parity: the `signum(dy) * 0.7` of the same term, stronger than the
/// horizontal pull so a bat climbs to its ceiling quickly.
const STEER_VERTICAL: f64 = 0.7;

/// Share of the gap a bat closes each tick.
///
/// Vanilla parity: the `0.1F` factor applied to the whole steering term.
const STEER_RESPONSIVENESS: f64 = 0.1;

/// Forward travel input a flying bat holds.
///
/// Vanilla parity: the `this.zza = 0.5F` of `Bat.customServerAiStep`.
const FORWARD_INPUT: f32 = 0.5;

/// Volume a bat squeaks at.
///
/// Vanilla parity: `Bat.getSoundVolume`.
const BAT_SOUND_VOLUME: f32 = 0.1;

/// A bat.
#[entity_behavior(class = "Bat")]
pub struct BatEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<BatEntityData>,
    /// Where the bat is drifting toward, if it is flying.
    ///
    /// Vanilla parity: `Bat.targetPosition`.
    target_position: SyncMutex<Option<BlockPos>>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `BatEntity`.
unsafe impl DowncastType for BatEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/bat");
}

impl BatEntity {
    /// Creates a bat at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a bat from saved base data.
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
        let mut entity_data = BatEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        // Vanilla parity: a bat starts hanging, and only the server decides
        // otherwise. It registers no goals at all; everything it does happens in
        // its own AI step.
        let bat = Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            target_position: SyncMutex::new(None),
        };
        bat.set_resting(true);
        bat
    }

    /// Returns whether the bat is hanging from a ceiling.
    ///
    /// Vanilla parity: `Bat.isResting`.
    #[must_use]
    pub fn is_resting(&self) -> bool {
        *self.entity_data.lock().id_flags.get() & RESTING_FLAG != 0
    }

    /// Hangs the bat up, or lets it go.
    ///
    /// Vanilla parity: `Bat.setResting`.
    pub fn set_resting(&self, resting: bool) {
        let mut data = self.entity_data.lock();
        let flags = *data.id_flags.get();
        let updated = if resting {
            flags | RESTING_FLAG
        } else {
            flags & !RESTING_FLAG
        };
        data.id_flags.set(updated);
    }

    /// Wakes the bat and plays the liftoff.
    fn take_off(&self, world: &Arc<World>, pos: BlockPos) {
        self.set_resting(false);
        if !self.is_silent() {
            world.level_event(level_events::SOUND_BAT_LIFTOFF, pos, 0, None);
        }
    }

    /// Decides whether a hanging bat stays put.
    ///
    /// Vanilla parity: the resting branch of `Bat.customServerAiStep`. A bat
    /// needs something solid overhead, and drops off it as soon as a player
    /// comes near.
    fn tick_resting(&self, world: &Arc<World>, pos: BlockPos) {
        let above = pos.above();
        if !world.get_block_state(above).is_static_redstone_conductor() {
            self.take_off(world, pos);
            return;
        }

        if rand::random_range(0..HEAD_TURN_CHANCE) == 0 {
            self.set_y_head_rot(rand::random_range(0..360) as f32);
        }

        let disturbed = world
            .nearest_player(self.position(), WAKE_RANGE, |player| {
                !player.is_spectator() && !player.has_infinite_materials()
            })
            .is_some();
        if disturbed {
            self.take_off(world, pos);
        }
    }

    /// Drifts a flying bat toward its destination.
    ///
    /// Vanilla parity: the flying branch of `Bat.customServerAiStep`.
    fn tick_flying(&self, world: &Arc<World>, pos: BlockPos) {
        self.retarget_if_needed(world);

        let Some(target) = *self.target_position.lock() else {
            return;
        };

        let position = self.position();
        let dx = f64::from(target.x()) + 0.5 - position.x;
        let dy = f64::from(target.y()) + 0.1 - position.y;
        let dz = f64::from(target.z()) + 0.5 - position.z;

        let velocity = self.velocity();
        let steered = velocity
            + DVec3::new(
                dx.signum().mul_add(STEER_HORIZONTAL, -velocity.x) * STEER_RESPONSIVENESS,
                dy.signum().mul_add(STEER_VERTICAL, -velocity.y) * STEER_RESPONSIVENESS,
                dz.signum().mul_add(STEER_HORIZONTAL, -velocity.z) * STEER_RESPONSIVENESS,
            );
        self.set_velocity(steered);

        // Vanilla turns the whole bat to face where it is going rather than
        // steering a body separately.
        let wanted_yaw = (steered.z.atan2(steered.x).to_degrees() as f32) - 90.0;
        let (yaw, pitch) = self.rotation();
        self.set_rotation((yaw + wrap_degrees(wanted_yaw - yaw), pitch));

        let input = self.travel_input();
        self.set_travel_input(LivingTravelInput::new(
            input.sideways(),
            input.vertical(),
            FORWARD_INPUT,
        ));

        if rand::random_range(0..REST_CHANCE) == 0
            && world
                .get_block_state(pos.above())
                .is_static_redstone_conductor()
        {
            self.set_resting(true);
        }
    }

    /// Picks a new destination when the old one is gone or reached.
    ///
    /// Vanilla parity: the `targetPosition` bookkeeping of the same method.
    fn retarget_if_needed(&self, world: &Arc<World>) {
        let position = self.position();
        let mut target = self.target_position.lock();

        if let Some(current) = *target
            && (!world.get_block_state(current).is_air() || current.y() <= world.get_min_y())
        {
            *target = None;
        }

        let reached = target.is_some_and(|current| {
            let center = DVec3::new(
                f64::from(current.x()) + 0.5,
                f64::from(current.y()) + 0.5,
                f64::from(current.z()) + 0.5,
            );
            center.distance(position) < ARRIVAL_DISTANCE
        });

        if target.is_none() || reached || rand::random_range(0..RETARGET_CHANCE) == 0 {
            let spread = || {
                rand::random_range(0..WANDER_HORIZONTAL_RANGE)
                    - rand::random_range(0..WANDER_HORIZONTAL_RANGE)
            };
            *target = Some(BlockPos::new(
                position.x.floor() as i32 + spread(),
                position.y.floor() as i32 + rand::random_range(0..WANDER_VERTICAL_RANGE)
                    - WANDER_VERTICAL_DROP,
                position.z.floor() as i32 + spread(),
            ));
        }
    }
}

/// Wraps an angle into the range a turn should take.
///
/// Vanilla parity: `Mth.wrapDegrees`.
fn wrap_degrees(degrees: f32) -> f32 {
    let wrapped = degrees % 360.0;
    if wrapped >= 180.0 {
        wrapped - 360.0
    } else if wrapped < -180.0 {
        wrapped + 360.0
    } else {
        wrapped
    }
}

impl Entity for BatEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `Bat.tick`. A hanging bat is pinned to the underside of
    /// its block; a flying one loses most of its vertical speed each tick.
    fn base_tick(&self) {
        if self.is_resting() {
            self.set_velocity(DVec3::ZERO);
            let position = self.position();
            let hanging_y = position.y.floor() + 1.0 - self.bounding_box().height();
            if let Err(error) = self.try_set_position(DVec3::new(position.x, hanging_y, position.z))
            {
                log::debug!("bat could not hang at {position:?}: {error}");
            }
        } else {
            let velocity = self.velocity();
            self.set_velocity(DVec3::new(
                velocity.x,
                velocity.y * VERTICAL_DRAG,
                velocity.z,
            ));
        }

        Mob::base_tick_mob(self);
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    /// Vanilla parity: `Bat.getMovementEmission`.
    fn movement_emission(&self) -> EntityMovementEmission {
        EntityMovementEmission::Events
    }

    /// Vanilla parity: `Bat.isPushable`.
    fn is_pushable(&self) -> bool {
        false
    }

    /// Vanilla parity: `Bat.checkFallDamage` is empty; a bat never lands hard.
    fn check_fall_damage(
        &self,
        _vertical_movement: f64,
        _on_ground: bool,
        _on_state: BlockStateId,
        _pos: BlockPos,
        _world: &Arc<World>,
    ) {
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        nbt.insert("BatFlags", *self.entity_data.lock().id_flags.get());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        if let Some(flags) = nbt.byte("BatFlags") {
            self.entity_data.lock().id_flags.set(flags);
        }
    }
}

impl LivingEntity for BatEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
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
        BAT_SOUND_VOLUME
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_BAT_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_BAT_DEATH)
    }

    /// Vanilla parity: `Bat.hurtServer`, which drops a hanging bat before it
    /// takes the hit.
    fn before_actually_hurt(&self, _source: &DamageSource, _amount: f32) {
        if self.is_resting() {
            self.set_resting(false);
        }
    }

    /// Vanilla parity: `Bat.pushEntities` is empty.
    fn push_entities(&self) {}

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }
}

impl Mob for BatEntity {
    /// Returns whether this mob accepts where the spawner put it.
    ///
    /// Vanilla parity: `Bat::checkBatSpawnRules`. Underground, dark, on a block
    /// bats spawn on, and even then only half the time.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_bat_spawn_rules(world, spawn_reason, pos)
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

    /// Vanilla parity: `Bat.getAmbientSound`. A hanging bat squeaks a quarter as
    /// often as a flying one.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        if self.is_resting() && rand::random_range(0..4) != 0 {
            return None;
        }
        Some(&sound_events::ENTITY_BAT_AMBIENT)
    }

    /// Vanilla parity: `Bat.customServerAiStep`.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        let pos = self.block_position();

        if self.is_resting() {
            self.tick_resting(&world, pos);
        } else {
            self.tick_flying(&world, pos);
        }
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for BatEntity {}
