//! What the dragon is currently doing.
//!
//! Vanilla parity: the `phases` package. The dragon has no goals. Its whole
//! behaviour is one of eleven phase objects, each of which answers where to fly,
//! how fast, how sharply to turn, and what a hit does -- and each of which can
//! hand over to another.
//!
//! Vanilla builds the phase list reflectively from a `Class` per phase and keys
//! it by an auto-incrementing id. Foton names them in an enum instead: the ids
//! are protocol-visible (`DATA_PHASE` is synced, and the client switches its own
//! phase on it), so they are pinned to the vanilla order rather than derived
//! from declaration order at runtime.
//!
//! Vanilla's phase instance holds `this.dragon`. A Rust phase cannot hold a
//! back-reference to the entity that owns it, so every method takes the dragon
//! instead. Each instance keeps its own mutable state behind its own lock,
//! which is what lets a phase hand over to another from inside its own tick --
//! vanilla's `doServerTick` does exactly that, and the manager must not be
//! holding a lock across the call.

use std::sync::{Arc, OnceLock};

use foton_utils::locks::SyncMutex;
use foton_utils::{BlockPos, Downcast as _};
use glam::DVec3;

use super::EnderDragon;
use crate::entity::Entity;
use crate::entity::ai::path::Path;
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::damage::DamageSource;
use crate::entity::entities::{
    ArrowEntity, EndCrystalEntity, SpectralArrowEntity, ThrownTridentEntity, WindChargeEntity,
};
use crate::player::Player;
use crate::world::World;

mod charging_player;
mod dying;
mod holding_pattern;
mod hovering;
mod landing;
mod sitting;
mod strafe_player;

pub use charging_player::DragonChargePlayerPhase;
pub use dying::DragonDeathPhase;
pub use holding_pattern::DragonHoldingPatternPhase;
pub use hovering::DragonHoverPhase;
pub use landing::{DragonLandingApproachPhase, DragonLandingPhase, DragonTakeoffPhase};
pub use sitting::{
    DragonSittingAttackingPhase, DragonSittingFlamingPhase, DragonSittingScanningPhase,
};
pub use strafe_player::DragonStrafePlayerPhase;

/// How many phases there are.
///
/// Vanilla parity: `EnderDragonPhase.getCount()`.
pub const PHASE_COUNT: usize = 11;

/// Which phase the dragon is in.
///
/// Vanilla parity: the `EnderDragonPhase` constants. The discriminants are the
/// synced `DATA_PHASE` values and must stay in vanilla's declaration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum EnderDragonPhase {
    /// Circling the outer ring. The dragon's resting state.
    HoldingPattern = 0,
    /// Lining up a fireball on one player.
    StrafePlayer = 1,
    /// Flying in towards the podium.
    LandingApproach = 2,
    /// Dropping onto the podium.
    Landing = 3,
    /// Leaving the podium again.
    Takeoff = 4,
    /// Sitting on the podium breathing dragon's breath.
    SittingFlaming = 5,
    /// Sitting on the podium looking for someone.
    SittingScanning = 6,
    /// Sitting on the podium roaring.
    SittingAttacking = 7,
    /// Diving at a point.
    ChargingPlayer = 8,
    /// The death spiral.
    Dying = 9,
    /// Holding still. The phase a freshly built dragon starts in.
    Hovering = 10,
}

impl EnderDragonPhase {
    /// Every phase, in id order.
    pub const ALL: [Self; PHASE_COUNT] = [
        Self::HoldingPattern,
        Self::StrafePlayer,
        Self::LandingApproach,
        Self::Landing,
        Self::Takeoff,
        Self::SittingFlaming,
        Self::SittingScanning,
        Self::SittingAttacking,
        Self::ChargingPlayer,
        Self::Dying,
        Self::Hovering,
    ];

    /// Returns the synced phase id.
    ///
    /// Vanilla parity: `EnderDragonPhase.getId`.
    #[must_use]
    pub const fn id(self) -> i32 {
        self as i32
    }

    /// Returns the phase with this id.
    ///
    /// Vanilla parity: `EnderDragonPhase.getById`, which falls back to the
    /// holding pattern for anything out of range -- so a corrupt `DragonPhase`
    /// in a save puts the dragon back in the air rather than refusing to load.
    #[must_use]
    pub const fn from_id(id: i32) -> Self {
        match id {
            1 => Self::StrafePlayer,
            2 => Self::LandingApproach,
            3 => Self::Landing,
            4 => Self::Takeoff,
            5 => Self::SittingFlaming,
            6 => Self::SittingScanning,
            7 => Self::SittingAttacking,
            8 => Self::ChargingPlayer,
            9 => Self::Dying,
            10 => Self::Hovering,
            _ => Self::HoldingPattern,
        }
    }

    /// Builds this phase's instance.
    ///
    /// Vanilla parity: `EnderDragonPhase.createInstance`, which reflects on the
    /// stored `Class`.
    fn create_instance(self) -> Arc<dyn DragonPhaseInstance> {
        match self {
            Self::HoldingPattern => Arc::new(DragonHoldingPatternPhase::new()),
            Self::StrafePlayer => Arc::new(DragonStrafePlayerPhase::new()),
            Self::LandingApproach => Arc::new(DragonLandingApproachPhase::new()),
            Self::Landing => Arc::new(DragonLandingPhase::new()),
            Self::Takeoff => Arc::new(DragonTakeoffPhase::new()),
            Self::SittingFlaming => Arc::new(DragonSittingFlamingPhase::new()),
            Self::SittingScanning => Arc::new(DragonSittingScanningPhase::new()),
            Self::SittingAttacking => Arc::new(DragonSittingAttackingPhase::new()),
            Self::ChargingPlayer => Arc::new(DragonChargePlayerPhase::new()),
            Self::Dying => Arc::new(DragonDeathPhase::new()),
            Self::Hovering => Arc::new(DragonHoverPhase::new()),
        }
    }
}

/// One phase's behaviour.
///
/// Vanilla parity: `DragonPhaseInstance`, with `AbstractDragonPhaseInstance`'s
/// bodies as the trait defaults. `doClientTick` is not carried: every override
/// of it in vanilla only spawns particles, which is client-local work.
pub trait DragonPhaseInstance: Send + Sync {
    /// Which phase this is.
    fn phase(&self) -> EnderDragonPhase;

    /// Whether the dragon is on the ground in this phase.
    ///
    /// Vanilla parity: `DragonPhaseInstance.isSitting`. It gates the head
    /// offset, the wing knockback, and whether a hit accumulates towards a
    /// takeoff.
    fn is_sitting(&self) -> bool {
        false
    }

    /// Runs one server tick of this phase.
    ///
    /// Vanilla parity: `doServerTick`. A phase may hand over to another from
    /// inside this call.
    fn do_server_tick(&self, _dragon: &EnderDragon, _world: &Arc<World>) {}

    /// Reacts to one of the pillar crystals being destroyed.
    ///
    /// Vanilla parity: `onCrystalDestroyed`.
    fn on_crystal_destroyed(
        &self,
        _dragon: &EnderDragon,
        _world: &Arc<World>,
        _crystal: &EndCrystalEntity,
        _pos: BlockPos,
        _source: &DamageSource,
        _player: Option<&Arc<Player>>,
    ) {
    }

    /// Resets this phase's state as it is entered.
    ///
    /// Vanilla parity: `begin`.
    fn begin(&self, _dragon: &EnderDragon) {}

    /// Cleans up as this phase is left.
    ///
    /// Vanilla parity: `end`.
    fn end(&self, _dragon: &EnderDragon) {}

    /// How fast the dragon flies in this phase.
    ///
    /// Vanilla parity: `AbstractDragonPhaseInstance.getFlySpeed`.
    fn fly_speed(&self) -> f32 {
        0.6
    }

    /// How sharply the dragon turns in this phase.
    ///
    /// Vanilla parity: `AbstractDragonPhaseInstance.getTurnSpeed` -- the faster
    /// it is already going, the wider it turns.
    fn turn_speed(&self, dragon: &EnderDragon) -> f32 {
        let rot_speed = horizontal_distance(dragon.velocity()) as f32 + 1.0;
        let dist = rot_speed.min(40.0);
        0.7 / dist / rot_speed
    }

    /// Where the dragon is flying, if anywhere.
    ///
    /// Vanilla parity: `getFlyTargetLocation`. `None` freezes the dragon in
    /// place -- `aiStep` skips the whole movement block without a target.
    fn fly_target_location(&self) -> Option<DVec3> {
        None
    }

    /// Adjusts incoming damage.
    ///
    /// Vanilla parity: `onHurt`.
    fn on_hurt(&self, _dragon: &EnderDragon, _source: &DamageSource, damage: f32) -> f32 {
        damage
    }

    /// Returns this as the breathing phase, when it is that phase.
    ///
    /// Vanilla reaches a specific phase through the generic
    /// `getPhase(EnderDragonPhase<T>)`, whose type parameter carries the
    /// concrete class. A Rust trait object has no such parameter, so the three
    /// phases another phase needs to talk to say so themselves.
    fn as_sitting_flaming(&self) -> Option<&DragonSittingFlamingPhase> {
        None
    }

    /// Returns this as the strafe phase, when it is that phase.
    fn as_strafe_player(&self) -> Option<&DragonStrafePlayerPhase> {
        None
    }

    /// Returns this as the charge phase, when it is that phase.
    fn as_charging_player(&self) -> Option<&DragonChargePlayerPhase> {
        None
    }
}

/// What a hit does to a dragon sitting on the podium.
///
/// Vanilla parity: `AbstractDragonSittingPhase.onHurt`. An arrow or a wind
/// charge is set alight and does nothing; everything else lands normally. That
/// is what stops a sitting dragon being shot off the podium from range.
///
/// Foton has no `AbstractArrow` layer, so its three concrete subclasses are
/// named. `BreezeWindChargeEntity` is deliberately absent: vanilla tests
/// `instanceof WindCharge`, and a breeze's charge is an `AbstractWindCharge`
/// sibling rather than a `WindCharge`.
pub(super) fn sitting_on_hurt(dragon: &EnderDragon, source: &DamageSource, damage: f32) -> f32 {
    /// Vanilla `igniteForSeconds(1.0F)`.
    const IGNITE_TICKS: i32 = 20;

    let Some(world) = dragon.level() else {
        return damage;
    };
    let Some(direct) = source
        .direct_entity_id
        .and_then(|id| world.get_entity_by_id(id))
    else {
        return damage;
    };

    let is_deflected = direct.downcast_ref::<ArrowEntity>().is_some()
        || direct.downcast_ref::<SpectralArrowEntity>().is_some()
        || direct.downcast_ref::<ThrownTridentEntity>().is_some()
        || direct.downcast_ref::<WindChargeEntity>().is_some();
    if !is_deflected {
        return damage;
    }

    direct.ignite_for_ticks(IGNITE_TICKS);
    0.0
}

/// Folds a ring index back into the ring the dragon is allowed to use.
///
/// Vanilla parity: the `targetNodeIndex %= 12` / `-= 12; &= 7; += 12` snippet
/// that the holding pattern, strafe and takeoff phases each carry. With the
/// outer ring available the index wraps within `0..12`; without it, the index
/// is folded into the eight middle-ring nodes at `12..20`.
pub(super) const fn wrap_ring_target(target_node: i32, outer_ring_available: bool) -> usize {
    if outer_ring_available {
        let wrapped = target_node % 12;
        let wrapped = if wrapped < 0 { wrapped + 12 } else { wrapped };
        return wrapped as usize;
    }

    (((target_node - 12) & 7) + 12) as usize
}

/// Returns the nearest player passing `conditions`.
///
/// Vanilla parity: `Level.getNearestPlayer(TargetingConditions, LivingEntity,
/// double, double, double)`. The range lives in the conditions, so the world
/// scan itself is unbounded.
pub(super) fn nearest_player_to(
    world: &Arc<World>,
    dragon: &EnderDragon,
    conditions: &TargetingConditions,
    position: DVec3,
) -> Option<Arc<Player>> {
    world.nearest_player(position, -1.0, |player| {
        conditions.test(world, Some(dragon), player)
    })
}

/// Returns a block position as vanilla's `getX()/getY()/getZ()` triple.
///
/// Vanilla passes the integer corner, not the block center, to
/// `getNearestPlayer`.
pub(super) fn corner_of(pos: BlockPos) -> DVec3 {
    DVec3::new(f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()))
}

/// Vanilla `Vec3i.distToCenterSqr`.
pub(super) fn dist_to_center_sqr(pos: BlockPos, point: DVec3) -> f64 {
    let center = DVec3::new(
        f64::from(pos.x()) + 0.5,
        f64::from(pos.y()) + 0.5,
        f64::from(pos.z()) + 0.5,
    );
    center.distance_squared(point)
}

/// Vanilla `Entity.getY(double)`.
pub(super) fn y_at(entity: &dyn Entity, progress: f64) -> f64 {
    let aabb = entity.bounding_box();
    aabb.min_y() + (aabb.max_y() - aabb.min_y()) * progress
}

/// Vanilla `Vec3.horizontalDistance`.
pub(super) fn horizontal_distance(vector: DVec3) -> f64 {
    vector.x.hypot(vector.z)
}

/// The dragon's current phase, and the instances it has built so far.
///
/// Vanilla parity: `EnderDragonPhaseManager`.
pub struct EnderDragonPhaseManager {
    instances: [OnceLock<Arc<dyn DragonPhaseInstance>>; PHASE_COUNT],
    current: SyncMutex<EnderDragonPhase>,
}

impl Default for EnderDragonPhaseManager {
    fn default() -> Self {
        Self::new()
    }
}

impl EnderDragonPhaseManager {
    /// Creates a manager parked in [`EnderDragonPhase::Hovering`].
    ///
    /// Vanilla parity: the constructor's `setPhase(HOVERING)`. Vanilla runs a
    /// real transition there so that `DATA_PHASE` is written and the hover
    /// phase's `begin` runs; here the synced default is already `HOVERING`
    /// (see `EnderDragonEntityData`) and a fresh hover instance is already in
    /// its begun state, so the transition has nothing left to do.
    #[must_use]
    pub fn new() -> Self {
        Self {
            instances: [const { OnceLock::new() }; PHASE_COUNT],
            current: SyncMutex::new(EnderDragonPhase::Hovering),
        }
    }

    /// Returns which phase the dragon is in.
    #[must_use]
    pub fn current_phase(&self) -> EnderDragonPhase {
        *self.current.lock()
    }

    /// Returns the instance for the dragon's current phase.
    ///
    /// Vanilla parity: `getCurrentPhase`.
    #[must_use]
    pub fn current_instance(&self) -> Arc<dyn DragonPhaseInstance> {
        self.instance(self.current_phase())
    }

    /// Returns a phase's instance, building it on first use.
    ///
    /// Vanilla parity: `getPhase`, which lazily fills its `phases` array.
    #[must_use]
    pub fn instance(&self, phase: EnderDragonPhase) -> Arc<dyn DragonPhaseInstance> {
        self.instances[phase.id() as usize]
            .get_or_init(|| phase.create_instance())
            .clone()
    }

    /// Moves the dragon to another phase.
    ///
    /// Vanilla parity: `setPhase`. Re-entering the phase already running is a
    /// no-op, so a phase that keeps asking for itself does not restart.
    pub fn set_phase(&self, dragon: &EnderDragon, target: EnderDragonPhase) {
        let previous = {
            let mut current = self.current.lock();
            if *current == target {
                return;
            }
            let previous = *current;
            *current = target;
            previous
        };

        self.instance(previous).end(dragon);
        dragon.set_synced_phase(target);
        self.instance(target).begin(dragon);
    }

    /// Restores the phase a saved dragon was in.
    ///
    /// Vanilla parity: the `readAdditionalSaveData` call to `setPhase`, which
    /// goes through the same transition.
    pub fn load_phase(&self, dragon: &EnderDragon, phase: EnderDragonPhase) {
        self.set_phase(dragon, phase);
    }
}

/// Picks the next flight target from a path and steps past it.
///
/// Vanilla parity: the `navigateToNextPathNode` that the holding pattern,
/// strafe, landing-approach and takeoff phases each carry a private copy of.
/// The height is randomized upwards from the node, so the ring is flown at a
/// wobble rather than at a fixed altitude.
pub(super) fn navigate_to_next_path_node(path: &mut Path) -> Option<DVec3> {
    if path.is_done() {
        return None;
    }

    let current = path.next_node_pos()?;
    path.advance();

    let mut y_target;
    loop {
        y_target = f64::from(current.y()) + f64::from(rand::random::<f32>()) * 20.0;
        if y_target >= f64::from(current.y()) {
            break;
        }
    }

    Some(DVec3::new(
        f64::from(current.x()),
        y_target,
        f64::from(current.z()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_ids_are_the_synced_values_the_client_switches_on() {
        assert_eq!(EnderDragonPhase::HoldingPattern.id(), 0);
        assert_eq!(EnderDragonPhase::Dying.id(), 9);
        assert_eq!(EnderDragonPhase::Hovering.id(), 10);
    }

    #[test]
    fn every_phase_round_trips_through_its_id() {
        for phase in EnderDragonPhase::ALL {
            assert_eq!(EnderDragonPhase::from_id(phase.id()), phase);
        }
    }

    #[test]
    fn an_out_of_range_saved_phase_puts_the_dragon_back_in_the_holding_pattern() {
        assert_eq!(
            EnderDragonPhase::from_id(-1),
            EnderDragonPhase::HoldingPattern
        );
        assert_eq!(
            EnderDragonPhase::from_id(PHASE_COUNT as i32),
            EnderDragonPhase::HoldingPattern
        );
    }

    #[test]
    fn only_the_sitting_phases_and_the_hover_report_themselves_as_sitting() {
        let manager = EnderDragonPhaseManager::new();
        let sitting: Vec<_> = EnderDragonPhase::ALL
            .into_iter()
            .filter(|phase| manager.instance(*phase).is_sitting())
            .collect();

        assert_eq!(
            sitting,
            vec![
                EnderDragonPhase::SittingFlaming,
                EnderDragonPhase::SittingScanning,
                EnderDragonPhase::SittingAttacking,
                EnderDragonPhase::Hovering,
            ]
        );
    }
}
