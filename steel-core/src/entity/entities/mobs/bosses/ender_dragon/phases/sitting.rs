//! Sitting on the podium: scanning, roaring, then breathing.

use std::sync::Arc;

use glam::DVec3;
use steel_math::trig;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::vanilla_entities;
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, wrap_degrees};

use super::{DragonPhaseInstance, EnderDragon, EnderDragonPhase};
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::damage::DamageSource;
use crate::entity::entities::AreaEffectCloudEntity;
use crate::entity::{Entity as _, RemovalReason, SharedEntity, next_entity_id};
use crate::world::World;

/// How long the dragon scans before charging off.
///
/// Vanilla parity: `DragonSittingScanningPhase.SITTING_SCANNING_IDLE_TICKS`.
const SCANNING_IDLE_TICKS: i32 = 100;

/// How far above or below itself the dragon will notice someone.
///
/// Vanilla parity: `SITTING_ATTACK_Y_VIEW_RANGE`.
const ATTACK_Y_VIEW_RANGE: f64 = 10.0;

/// How far the dragon will notice someone standing on the podium.
///
/// Vanilla parity: `SITTING_ATTACK_VIEW_RANGE`.
const ATTACK_VIEW_RANGE: f64 = 20.0;

/// How far the dragon will look for someone to charge.
///
/// Vanilla parity: `SITTING_CHARGE_VIEW_RANGE`.
const CHARGE_VIEW_RANGE: f64 = 150.0;

/// Ticks of scanning after which a visible player is attacked rather than faced.
///
/// Vanilla parity: the `this.scanningTime > 25` of `doServerTick`.
const SCAN_TICKS_BEFORE_ATTACK: i32 = 25;

/// How long the roar lasts.
///
/// Vanilla parity: `DragonSittingAttackingPhase.ROAR_DURATION`.
const ROAR_DURATION: i32 = 40;

/// How long one breath lasts.
///
/// Vanilla parity: `DragonSittingFlamingPhase.FLAME_DURATION`.
const FLAME_DURATION: i32 = 200;

/// Breaths before the dragon gives up and takes off.
///
/// Vanilla parity: `SITTING_FLAME_ATTACKS_COUNT`.
const FLAME_ATTACKS_COUNT: i32 = 4;

/// Ticks between entering the phase and the cloud appearing.
///
/// Vanilla parity: `DragonSittingFlamingPhase.WARMUP_TIME`.
const WARMUP_TIME: i32 = 10;

/// Radius of the dragon's breath.
///
/// Vanilla parity: the `this.flame.setRadius(5.0F)` of `doServerTick`.
const FLAME_RADIUS: f32 = 5.0;

/// Looking around from the podium.
///
/// Vanilla parity: `DragonSittingScanningPhase`.
pub struct DragonSittingScanningPhase {
    scanning_time: SyncMutex<i32>,
}

impl Default for DragonSittingScanningPhase {
    fn default() -> Self {
        Self::new()
    }
}

impl DragonSittingScanningPhase {
    /// Creates the phase.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            scanning_time: SyncMutex::new(0),
        }
    }

    /// Turns the dragon's head towards someone it has spotted.
    ///
    /// Vanilla parity: the `else` branch of `doServerTick`, which is the only
    /// place the dragon steers while sitting.
    fn face_target(dragon: &EnderDragon, target: DVec3) {
        let position = dragon.position();
        let y_rot = dragon.rotation().0;
        let aim = DVec3::new(target.x - position.x, 0.0, target.z - position.z).normalize_or_zero();
        let dir = DVec3::new(
            f64::from(trig::sin(f64::from(y_rot).to_radians())),
            0.0,
            -f64::from(trig::cos(f64::from(y_rot).to_radians())),
        )
        .normalize_or_zero();
        let angle = (dir.dot(aim) as f32).acos().to_degrees() + 0.5;
        if (0.0..10.0).contains(&angle) {
            return;
        }

        let head = dragon.head_position();
        let x_attack_dist = target.x - head.x;
        let z_attack_dist = target.z - head.z;
        let y_rot_delta = f64::from(wrap_degrees(
            (180.0 - x_attack_dist.atan2(z_attack_dist).to_degrees() - f64::from(y_rot)) as f32,
        ))
        .clamp(-100.0, 100.0);

        let rot_speed = x_attack_dist.hypot(z_attack_dist) as f32 + 1.0;
        let dist = rot_speed.min(40.0);
        let mut y_rot_a = dragon.y_rot_a() * 0.8;
        y_rot_a += y_rot_delta as f32 * (0.7 / dist / rot_speed);
        dragon.set_y_rot_a(y_rot_a);
        dragon.set_rotation((y_rot + y_rot_a, dragon.rotation().1));
    }
}

impl DragonPhaseInstance for DragonSittingScanningPhase {
    fn phase(&self) -> EnderDragonPhase {
        EnderDragonPhase::SittingScanning
    }

    fn is_sitting(&self) -> bool {
        true
    }

    fn on_hurt(&self, dragon: &EnderDragon, source: &DamageSource, damage: f32) -> f32 {
        super::sitting_on_hurt(dragon, source, damage)
    }

    fn do_server_tick(&self, dragon: &EnderDragon, world: &Arc<World>) {
        let scanning_time = {
            let mut scanning_time = self.scanning_time.lock();
            *scanning_time += 1;
            *scanning_time
        };

        let dragon_y = dragon.position().y;
        let scan_conditions = TargetingConditions::for_combat()
            .range(ATTACK_VIEW_RANGE)
            .selector(move |_, target, _| {
                (target.position().y - dragon_y).abs() <= ATTACK_Y_VIEW_RANGE
            });
        let spotted = super::nearest_player_to(world, dragon, &scan_conditions, dragon.position());

        if let Some(spotted) = spotted {
            if scanning_time > SCAN_TICKS_BEFORE_ATTACK {
                dragon
                    .phase_manager()
                    .set_phase(dragon, EnderDragonPhase::SittingAttacking);
            } else {
                Self::face_target(dragon, spotted.position());
            }
            return;
        }

        if scanning_time < SCANNING_IDLE_TICKS {
            return;
        }

        let charge_conditions = TargetingConditions::for_combat().range(CHARGE_VIEW_RANGE);
        let charge_target =
            super::nearest_player_to(world, dragon, &charge_conditions, dragon.position());
        let manager = dragon.phase_manager();
        manager.set_phase(dragon, EnderDragonPhase::Takeoff);
        if let Some(charge_target) = charge_target {
            manager.set_phase(dragon, EnderDragonPhase::ChargingPlayer);
            if let Some(charging) = manager
                .instance(EnderDragonPhase::ChargingPlayer)
                .as_charging_player()
            {
                charging.set_target(charge_target.position());
            }
        }
    }

    fn begin(&self, _dragon: &EnderDragon) {
        *self.scanning_time.lock() = 0;
    }
}

/// The roar before the breath.
///
/// Vanilla parity: `DragonSittingAttackingPhase`.
pub struct DragonSittingAttackingPhase {
    attacking_ticks: SyncMutex<i32>,
}

impl Default for DragonSittingAttackingPhase {
    fn default() -> Self {
        Self::new()
    }
}

impl DragonSittingAttackingPhase {
    /// Creates the phase.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            attacking_ticks: SyncMutex::new(0),
        }
    }
}

impl DragonPhaseInstance for DragonSittingAttackingPhase {
    fn phase(&self) -> EnderDragonPhase {
        EnderDragonPhase::SittingAttacking
    }

    fn is_sitting(&self) -> bool {
        true
    }

    fn on_hurt(&self, dragon: &EnderDragon, source: &DamageSource, damage: f32) -> f32 {
        super::sitting_on_hurt(dragon, source, damage)
    }

    fn do_server_tick(&self, dragon: &EnderDragon, _world: &Arc<World>) {
        let done = {
            let mut attacking_ticks = self.attacking_ticks.lock();
            let elapsed = *attacking_ticks;
            *attacking_ticks += 1;
            elapsed >= ROAR_DURATION
        };
        if done {
            dragon
                .phase_manager()
                .set_phase(dragon, EnderDragonPhase::SittingFlaming);
        }
    }

    fn begin(&self, _dragon: &EnderDragon) {
        *self.attacking_ticks.lock() = 0;
    }
}

/// The dragon's breath.
///
/// Vanilla parity: `DragonSittingFlamingPhase`.
pub struct DragonSittingFlamingPhase {
    state: SyncMutex<FlamingState>,
}

struct FlamingState {
    flame_ticks: i32,
    flame_count: i32,
    flame: Option<Arc<AreaEffectCloudEntity>>,
}

impl Default for DragonSittingFlamingPhase {
    fn default() -> Self {
        Self::new()
    }
}

impl DragonSittingFlamingPhase {
    /// Creates the phase.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: SyncMutex::new(FlamingState {
                flame_ticks: 0,
                flame_count: 0,
                flame: None,
            }),
        }
    }

    /// Forgets how many breaths this landing has used.
    ///
    /// Vanilla parity: `DragonSittingFlamingPhase.resetFlameCount`, called by
    /// the landing phase so each landing gets a fresh four.
    pub fn reset_flame_count(&self) {
        self.state.lock().flame_count = 0;
    }

    /// Drops the cloud where the dragon's head is pointing.
    ///
    /// Vanilla parity: the `flameTicks == 10` branch of `doServerTick`.
    fn spawn_flame(&self, dragon: &EnderDragon, world: &Arc<World>) {
        let position = dragon.position();
        let head = dragon.head_position();
        let look = DVec3::new(head.x - position.x, 0.0, head.z - position.z).normalize_or_zero();
        let x = head.x + look.x * f64::from(FLAME_RADIUS) / 2.0;
        let z = head.z + look.z * f64::from(FLAME_RADIUS) / 2.0;
        let initial_y = dragon.head().y_at(0.5);

        let mut y = initial_y;
        while world
            .get_block_state(BlockPos::containing(x, y, z))
            .is_air()
        {
            y -= 1.0;
            if y < 0.0 {
                y = initial_y;
                break;
            }
        }
        let y = f64::from(steel_math::fast_floor(y)) + 1.0;

        let cloud = Arc::new(AreaEffectCloudEntity::new(
            &vanilla_entities::AREA_EFFECT_CLOUD,
            next_entity_id(),
            DVec3::new(x, y, z),
            Arc::downgrade(world),
        ));
        cloud.configure_as_dragon_sitting_flame();

        let entity: SharedEntity = cloud.clone();
        if let Err(error) = world.try_add_entity(entity) {
            log::debug!("failed to spawn the dragon's breath: {error}");
            return;
        }
        self.state.lock().flame = Some(cloud);
    }
}

impl DragonPhaseInstance for DragonSittingFlamingPhase {
    fn phase(&self) -> EnderDragonPhase {
        EnderDragonPhase::SittingFlaming
    }

    fn is_sitting(&self) -> bool {
        true
    }

    fn on_hurt(&self, dragon: &EnderDragon, source: &DamageSource, damage: f32) -> f32 {
        super::sitting_on_hurt(dragon, source, damage)
    }

    fn do_server_tick(&self, dragon: &EnderDragon, world: &Arc<World>) {
        let (flame_ticks, flame_count) = {
            let mut state = self.state.lock();
            state.flame_ticks += 1;
            (state.flame_ticks, state.flame_count)
        };

        if flame_ticks >= FLAME_DURATION {
            let next = if flame_count >= FLAME_ATTACKS_COUNT {
                EnderDragonPhase::Takeoff
            } else {
                EnderDragonPhase::SittingScanning
            };
            dragon.phase_manager().set_phase(dragon, next);
            return;
        }

        if flame_ticks == WARMUP_TIME {
            self.spawn_flame(dragon, world);
        }
    }

    fn begin(&self, _dragon: &EnderDragon) {
        let mut state = self.state.lock();
        state.flame_ticks = 0;
        state.flame_count += 1;
    }

    fn end(&self, _dragon: &EnderDragon) {
        // Vanilla parity: `this.flame.discard()`. Leaving the phase early takes
        // the breath with it, so a dragon knocked off the podium does not leave
        // a cloud burning on it.
        if let Some(flame) = self.state.lock().flame.take() {
            flame.set_removed(RemovalReason::Discarded);
        }
    }

    fn as_sitting_flaming(&self) -> Option<&Self> {
        Some(self)
    }
}
