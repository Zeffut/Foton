//! Mobs that remember who wronged them.
//!
//! Vanilla parity: `NeutralMob`. A neutral mob is not passive and not hostile:
//! it is peaceful until provoked, and then it stays angry at that specific
//! attacker for a while after losing sight of them. Six vanilla mobs share this
//! -- enderman, zombified piglin, wolf, bee, iron golem, polar bear -- and none
//! of them behave right without it, because the memory is the behaviour: a wolf
//! that forgot the moment you broke line of sight would simply be passive.
//!
//! Foton had no implementation of any of it.

use std::sync::Arc;

use foton_registry::vanilla_game_rules::{FORGIVE_DEAD_PLAYERS, UNIVERSAL_ANGER};
use foton_utils::UuidExt as _;
use foton_utils::locks::SyncMutex;
use foton_utils::types::Difficulty;
use simdnbt::owned::{NbtCompound, NbtTag};
use uuid::Uuid;

use crate::entity::{Entity, LivingEntity, Mob, SharedEntity};
use crate::player::Player;
use crate::world::World;

/// NBT key vanilla stores the anger deadline under.
///
/// Vanilla parity: `NeutralMob.TAG_ANGER_END_TIME`.
pub const TAG_ANGER_END_TIME: &str = "anger_end_time";

/// NBT key vanilla stores the grudge's target under.
///
/// Vanilla parity: `NeutralMob.TAG_ANGRY_AT`.
pub const TAG_ANGRY_AT: &str = "angry_at";

/// The anger a neutral mob is carrying.
///
/// Vanilla keeps these two on the mob itself; Foton groups them so an entity
/// holds one field rather than two, the way it holds a [`crate::entity::MobBase`].
#[derive(Debug, Default)]
pub struct PersistentAnger {
    /// Game time the anger runs out at, or a negative value for calm.
    end_time: SyncMutex<i64>,
    /// Who the mob is angry at, if it is angry at someone in particular.
    target: SyncMutex<Option<Uuid>>,
}

impl PersistentAnger {
    /// Creates a calm mob's anger state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            end_time: SyncMutex::new(-1),
            target: SyncMutex::new(None),
        }
    }
}

/// A mob that stays angry at whoever provoked it.
///
/// Vanilla parity: the `NeutralMob` interface.
pub trait NeutralMob: Mob {
    /// Returns this mob's anger state.
    fn persistent_anger(&self) -> &PersistentAnger;

    /// Starts the anger timer for however long this kind of mob sulks.
    ///
    /// Vanilla parity: `startPersistentAngerTimer`, which each mob implements
    /// with its own random range -- twenty to thirty-nine seconds for most.
    fn start_persistent_anger_timer(&self);

    /// Returns the game time this mob calms down at.
    fn persistent_anger_end_time(&self) -> i64 {
        *self.persistent_anger().end_time.lock()
    }

    /// Sets the game time this mob calms down at.
    fn set_persistent_anger_end_time(&self, end_time: i64) {
        *self.persistent_anger().end_time.lock() = end_time;
    }

    /// Stays angry for `ticks` from now.
    ///
    /// Vanilla parity: `setTimeToRemainAngry`.
    fn set_time_to_remain_angry(&self, ticks: i64) {
        let now = self.level().map_or(0, |world| world.game_time());
        self.set_persistent_anger_end_time(now + ticks);
    }

    /// Returns who this mob is angry at.
    fn persistent_anger_target(&self) -> Option<Uuid> {
        *self.persistent_anger().target.lock()
    }

    /// Sets who this mob is angry at.
    fn set_persistent_anger_target(&self, target: Option<Uuid>) {
        *self.persistent_anger().target.lock() = target;
    }

    /// Returns whether the anger timer is still running.
    ///
    /// Vanilla parity: `isAngry`.
    fn is_angry(&self) -> bool {
        let end_time = self.persistent_anger_end_time();
        if end_time <= 0 {
            return false;
        }
        let now = self.level().map_or(0, |world| world.game_time());
        end_time - now > 0
    }

    /// Returns whether this mob would attack `entity` on sight.
    ///
    /// Vanilla parity: `isAngryAt`.
    fn is_angry_at(&self, entity: &dyn LivingEntity, world: &Arc<World>) -> bool {
        if is_valid_player_target(entity, world) && self.is_angry_at_all_players(world) {
            return true;
        }
        self.persistent_anger_target()
            .is_some_and(|target| target == entity.uuid())
    }

    /// Returns whether universal anger has turned this mob on everyone.
    ///
    /// Vanilla parity: `isAngryAtAllPlayers`. The game rule exists so a server
    /// can make one player's mistake everyone's problem.
    fn is_angry_at_all_players(&self, world: &Arc<World>) -> bool {
        world.get_game_rule(&UNIVERSAL_ANGER)
            && self.is_angry()
            && self.persistent_anger_target().is_none()
    }

    /// Forgets the grudge entirely.
    ///
    /// Vanilla parity: `stopBeingAngry`.
    fn stop_being_angry(&self) {
        self.set_last_hurt_by_mob(None);
        self.set_persistent_anger_target(None);
        self.set_target(None);
        self.set_persistent_anger_end_time(-1);
    }

    /// Drops the grudge but stays angry at everyone.
    ///
    /// Vanilla parity: `forgetCurrentTargetAndRefreshUniversalAnger`.
    fn forget_current_target_and_refresh_universal_anger(&self) {
        self.stop_being_angry();
        self.start_persistent_anger_timer();
    }

    /// Forgives a player who died, if the server forgives the dead.
    ///
    /// Vanilla parity: `playerDied`.
    fn player_died(&self, world: &Arc<World>, player: Uuid) {
        if !world.get_game_rule(&FORGIVE_DEAD_PLAYERS) {
            return;
        }
        if self.persistent_anger_target() == Some(player) {
            self.stop_being_angry();
        }
    }

    /// Reconciles the anger with the current target, once per tick.
    ///
    /// Vanilla parity: `updatePersistentAnger`. `stay_angry_if_target_present`
    /// is what separates a wolf, which keeps refreshing its grudge while it can
    /// see you, from an enderman, which does not.
    fn update_persistent_anger(&self, world: &Arc<World>, stay_angry_if_target_present: bool) {
        let anger_target = self.persistent_anger_target();
        let target = self.target();

        // A mob that killed what it was angry at has nothing left to be angry
        // about. Players are excluded: vanilla forgives them only by the game
        // rule, through `player_died`.
        if let Some(previous) = target.as_ref()
            && let Some(living) = previous.as_living_entity()
            && !Entity::is_alive(living)
            && anger_target == Some(previous.uuid())
            && previous.is_mob()
        {
            self.stop_being_angry();
            return;
        }

        if let Some(target) = target.as_ref() {
            let is_new_target = anger_target != Some(target.uuid());
            if is_new_target {
                self.set_persistent_anger_target(Some(target.uuid()));
            }
            if is_new_target || stay_angry_if_target_present {
                self.start_persistent_anger_timer();
            }
        }

        if anger_target.is_some() && !self.is_angry() {
            let target_still_valid = target.as_ref().is_some_and(|target| {
                target
                    .as_living_entity()
                    .is_some_and(|living| is_valid_player_target(living, world))
            });
            if !target_still_valid || !stay_angry_if_target_present {
                self.stop_being_angry();
                return;
            }
        }

        // Anger at someone who has stepped out of reach -- into creative, into
        // spectator, or into a peaceful world -- is dropped rather than kept
        // pending, so the mob does not lunge the moment they step back.
        if let Some(uuid) = self.persistent_anger_target()
            && let Some(entity) = world.get_entity_by_uuid(&uuid)
            && let Some(player) = entity.as_player()
            && !player_can_be_angered_at(player, world)
        {
            self.stop_being_angry();
        }
    }
}

/// Returns whether this entity is a player a mob may hold a grudge against.
///
/// Vanilla parity: the private `isValidPlayerTarget`.
fn is_valid_player_target(target: &dyn LivingEntity, world: &Arc<World>) -> bool {
    let Some(player) = target.as_player() else {
        return false;
    };
    player_can_be_angered_at(player, world)
}

/// Returns whether a mob may stay angry at this player.
fn player_can_be_angered_at(player: &Player, world: &Arc<World>) -> bool {
    use foton_utils::types::GameType;
    !matches!(player.game_mode(), GameType::Creative | GameType::Spectator)
        && world.difficulty() != Difficulty::Peaceful
}

/// Writes the anger a mob is carrying.
///
/// Vanilla parity: `addPersistentAngerSaveData`. The grudge is stored as the
/// same four-int UUID array vanilla's `EntityReference` codec writes.
pub fn write_persistent_anger(mob: &dyn NeutralMob, nbt: &mut NbtCompound) {
    nbt.insert(TAG_ANGER_END_TIME, mob.persistent_anger_end_time());
    if let Some(target) = mob.persistent_anger_target() {
        nbt.insert(
            TAG_ANGRY_AT,
            NbtTag::IntArray(target.to_int_array().to_vec()),
        );
    }
}

/// Reads the anger a mob was saved with.
///
/// Vanilla parity: `readPersistentAngerSaveData`, including the fall back to
/// the older `AngerTime` key, which stored a duration rather than an end time.
pub fn read_persistent_anger(
    mob: &dyn NeutralMob,
    end_time: Option<i64>,
    legacy_anger_ticks: Option<i32>,
    angry_at: Option<Uuid>,
) {
    if let Some(end_time) = end_time {
        mob.set_persistent_anger_end_time(end_time);
    } else if let Some(ticks) = legacy_anger_ticks {
        mob.set_time_to_remain_angry(i64::from(ticks));
    } else {
        mob.set_persistent_anger_end_time(-1);
    }

    mob.set_persistent_anger_target(angry_at);
}

/// Returns the target a saved grudge points at, if it is still in the world.
///
/// Vanilla parity: the `EntityReference.getLivingEntity` of
/// `readPersistentAngerSaveData`. A grudge against someone who has logged out
/// or died resolves to nothing, and the mob simply calms down.
#[must_use]
pub fn resolve_anger_target(world: &Arc<World>, target: Option<Uuid>) -> Option<SharedEntity> {
    let uuid = target?;
    world.get_entity_by_uuid(&uuid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_calm_mob_has_no_end_time() {
        let anger = PersistentAnger::new();
        assert_eq!(*anger.end_time.lock(), -1);
        assert!(anger.target.lock().is_none());
    }

    #[test]
    fn the_default_is_the_same_as_calm_for_the_target() {
        // `Default` exists for entities that build their state field by field;
        // it must not accidentally start a mob out angry.
        let anger = PersistentAnger::default();
        assert!(anger.target.lock().is_none());
    }
}
