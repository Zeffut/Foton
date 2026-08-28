//! Shared vanilla `TamableAnimal` state and hooks.
//!
//! Vanilla parity: `TamableAnimal` and `OwnableEntity`. A tameable animal is an
//! [`Animal`] that remembers one player, obeys a sit order, and teleports to its
//! owner when it falls too far behind. Wolf, cat and parrot all sit on this, and
//! none of them behaves right without it: the sit order, the collar owner check
//! and the "who may I attack" rule are all decided here rather than per mob.

use glam::DVec3;
use rustc_hash::FxHashSet;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_protocol::packets::game::CSystemChat;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::data_components::vanilla_components::{FOOD, FoodProperties};
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_game_rules::SHOW_DEATH_MESSAGES;
use steel_utils::entity_events::EntityStatus;
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, UuidExt};
use text_components::translation::TranslatedMessage;
use uuid::Uuid;

use crate::advancement::triggers;
use crate::entity::ai::path::{PathType, PathfindingContext};
use crate::entity::ai::walk::WalkPathEvaluator;
use crate::entity::damage::DamageSource;
use crate::entity::{Animal, Entity, LivingEntity, Mob, SharedEntity};
use crate::physics::{WorldCollisionProvider, collision};
use crate::player::Player;

/// Bit of the synced flags byte that marks a sitting pose.
///
/// Vanilla parity: the `& 1` of `TamableAnimal.isInSittingPose`.
const FLAG_IN_SITTING_POSE: i8 = 1;

/// Bit of the synced flags byte that marks a tamed animal.
///
/// Vanilla parity: the `& 4` of `TamableAnimal.isTame`.
const FLAG_TAME: i8 = 4;

/// Squared distance past which a pet teleports rather than walks to its owner.
///
/// Vanilla parity: `TamableAnimal.TELEPORT_WHEN_DISTANCE_IS_SQ`.
pub const TELEPORT_WHEN_DISTANCE_IS_SQ: f64 = 144.0;

/// How many landing spots a teleporting pet tries before giving up.
///
/// Vanilla parity: the ten attempts of `TamableAnimal.teleportToAroundBlockPos`.
const TELEPORT_ATTEMPTS: i32 = 10;

/// Smallest horizontal offset a teleport may land at.
///
/// Vanilla parity: `MIN_HORIZONTAL_DISTANCE_FROM_TARGET_AFTER_TELEPORTING`, which
/// is why a pet never lands on top of the player it was following.
const MIN_HORIZONTAL_TELEPORT_DISTANCE: i32 = 2;

/// Largest horizontal offset a teleport may land at.
///
/// Vanilla parity: `MAX_HORIZONTAL_DISTANCE_FROM_TARGET_AFTER_TELEPORTING`.
const MAX_HORIZONTAL_TELEPORT_DISTANCE: i32 = 3;

/// Largest vertical offset a teleport may land at.
///
/// Vanilla parity: `MAX_VERTICAL_DISTANCE_FROM_TARGET_AFTER_TELEPORTING`.
const MAX_VERTICAL_TELEPORT_DISTANCE: i32 = 1;

/// How deep the owner chain is walked before it is treated as a cycle.
///
/// Vanilla walks it with a set of everything it has seen; Steel keeps the set
/// and adds this cap so a corrupted save cannot spin the server.
const MAX_OWNER_CHAIN_DEPTH: usize = 16;

/// Runtime fields shared by vanilla tameable animals.
///
/// Vanilla parity: the single `orderedToSit` field of `TamableAnimal`. The tame
/// flag, the sitting pose and the owner all live in synchronized entity data
/// instead, so the entity owns them and this only holds what is server-side.
#[derive(Debug)]
pub struct TamableAnimalBase {
    ordered_to_sit: SyncMutex<bool>,
}

impl TamableAnimalBase {
    /// Creates default tameable runtime state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ordered_to_sit: SyncMutex::new(false),
        }
    }

    /// Returns vanilla `TamableAnimal.orderedToSit`.
    #[must_use]
    pub fn is_ordered_to_sit(&self) -> bool {
        *self.ordered_to_sit.lock()
    }

    /// Sets vanilla `TamableAnimal.orderedToSit`.
    pub fn set_ordered_to_sit(&self, ordered_to_sit: bool) {
        *self.ordered_to_sit.lock() = ordered_to_sit;
    }
}

impl Default for TamableAnimalBase {
    fn default() -> Self {
        Self::new()
    }
}

/// Vanilla-shaped behavior shared by entities that extend `TamableAnimal`.
pub trait TamableAnimal: Animal {
    /// Returns shared tameable runtime state.
    fn tamable_base(&self) -> &TamableAnimalBase;

    /// Returns the synchronized `TamableAnimal.DATA_FLAGS_ID` byte.
    fn tamable_flags(&self) -> i8;

    /// Sets the synchronized `TamableAnimal.DATA_FLAGS_ID` byte.
    fn set_tamable_flags(&self, flags: i8);

    /// Returns the synchronized owner reference.
    ///
    /// Vanilla parity: `TamableAnimal.DATA_OWNERUUID_ID`. Vanilla stores an
    /// `EntityReference`; Steel stores the UUID it resolves from.
    fn owner_uuid(&self) -> Option<Uuid>;

    /// Sets the synchronized owner reference.
    fn set_owner_uuid(&self, owner: Option<Uuid>);

    /// Returns vanilla `TamableAnimal.isTame`.
    fn is_tame(&self) -> bool {
        self.tamable_flags() & FLAG_TAME != 0
    }

    /// Applies vanilla `TamableAnimal.setTame`.
    fn set_tame(&self, is_tame: bool, include_side_effects: bool) {
        let current = self.tamable_flags();
        let updated = if is_tame {
            current | FLAG_TAME
        } else {
            current & !FLAG_TAME
        };
        self.set_tamable_flags(updated);

        if include_side_effects {
            self.apply_taming_side_effects();
        }
    }

    /// Hook for vanilla `TamableAnimal.applyTamingSideEffects`.
    fn apply_taming_side_effects(&self) {}

    /// Returns vanilla `TamableAnimal.isInSittingPose`.
    fn is_in_sitting_pose(&self) -> bool {
        self.tamable_flags() & FLAG_IN_SITTING_POSE != 0
    }

    /// Applies vanilla `TamableAnimal.setInSittingPose`.
    fn set_in_sitting_pose(&self, value: bool) {
        let current = self.tamable_flags();
        let updated = if value {
            current | FLAG_IN_SITTING_POSE
        } else {
            current & !FLAG_IN_SITTING_POSE
        };
        self.set_tamable_flags(updated);
    }

    /// Returns vanilla `TamableAnimal.isOrderedToSit`.
    fn is_ordered_to_sit(&self) -> bool {
        self.tamable_base().is_ordered_to_sit()
    }

    /// Sets vanilla `TamableAnimal.setOrderedToSit`.
    fn set_ordered_to_sit(&self, ordered_to_sit: bool) {
        self.tamable_base().set_ordered_to_sit(ordered_to_sit);
    }

    /// Returns vanilla `OwnableEntity.getOwner`.
    fn owner(&self) -> Option<SharedEntity> {
        let uuid = self.owner_uuid()?;
        let world = self.level()?;
        let owner = world.get_entity_by_uuid(&uuid)?;
        owner.as_living_entity()?;
        Some(owner)
    }

    /// Returns vanilla `OwnableEntity.getRootOwner`.
    ///
    /// A pet owned by a pet answers to whoever is at the top of the chain. A
    /// cycle resolves to nothing, exactly as vanilla's `seen` set makes it.
    fn root_owner(&self) -> Option<SharedEntity> {
        let mut seen = FxHashSet::default();
        seen.insert(self.uuid());

        let mut owner = self.owner()?;
        for _ in 0..MAX_OWNER_CHAIN_DEPTH {
            if !seen.insert(owner.uuid()) {
                return None;
            }
            let Some(tamable) = owner.as_tamable_animal() else {
                return Some(owner);
            };
            let Some(next) = tamable.owner() else {
                return Some(owner);
            };
            owner = next;
        }

        None
    }

    /// Returns vanilla `TamableAnimal.isOwnedBy`.
    fn is_owned_by(&self, entity: &dyn Entity) -> bool {
        self.owner_uuid() == Some(entity.uuid())
    }

    /// Applies vanilla `TamableAnimal.tame`.
    fn tame(&self, player: &Player) {
        self.set_tame(true, true);
        self.set_owner_uuid(Some(player.gameprofile.id));
        triggers::entity::tame_animal(player, self.as_entity_event_source());
    }

    /// Returns vanilla `TamableAnimal.canAttack`.
    fn can_attack_tamable(&self, target: &dyn LivingEntity) -> bool {
        if self.is_owned_by(target.as_entity_event_source()) {
            return false;
        }
        Mob::can_attack(self, target)
    }

    /// Returns vanilla `TamableAnimal.wantsToAttack`.
    fn wants_to_attack(&self, _target: &dyn LivingEntity, _owner: &dyn Entity) -> bool {
        true
    }

    /// Returns vanilla `TamableAnimal.considersEntityAsAlly`.
    fn considers_entity_as_ally_tamable(&self, other: &dyn Entity) -> bool {
        if !self.is_tame() {
            return false;
        }
        let Some(owner) = self.root_owner() else {
            return false;
        };
        if owner.uuid() == other.uuid() {
            return true;
        }
        owner.is_allied_to(other)
    }

    /// Applies vanilla `TamableAnimal.feed`.
    fn feed(
        &self,
        player: &Player,
        hand: InteractionHand,
        item_stack: &ItemStack,
        healing_factor: f32,
        default_heal: f32,
    ) {
        let nutrition = item_stack.get(FOOD).map(FoodProperties::nutrition);
        Mob::use_player_item(self, player, hand);
        self.heal(nutrition.map_or(default_heal, |nutrition| healing_factor * nutrition as f32));
        self.play_eating_sound();
    }

    /// Shows the vanilla taming outcome.
    ///
    /// Vanilla parity: `TamableAnimal.spawnTamingParticles` runs on the client
    /// off entity events 6 and 7, so the server's whole job is broadcasting one
    /// of them.
    fn spawn_taming_particles(&self, success: bool) {
        self.broadcast_entity_event(if success {
            EntityStatus::TamingSucceeded
        } else {
            EntityStatus::TamingFailed
        });
    }

    /// Returns vanilla `TamableAnimal.unableToMoveToOwner`.
    fn unable_to_move_to_owner(&self) -> bool {
        if self.is_ordered_to_sit() || self.is_passenger() || self.may_be_leashed() {
            return true;
        }

        self.owner()
            .is_some_and(|owner| owner.as_player().is_some_and(Entity::is_spectator))
    }

    /// Returns vanilla `TamableAnimal.canFlyToOwner`.
    fn can_fly_to_owner(&self) -> bool {
        false
    }

    /// Returns vanilla `TamableAnimal.shouldTryTeleportToOwner`.
    fn should_try_teleport_to_owner(&self) -> bool {
        self.owner().is_some_and(|owner| {
            self.position().distance_squared(owner.position()) >= TELEPORT_WHEN_DISTANCE_IS_SQ
        })
    }

    /// Applies vanilla `TamableAnimal.tryToTeleportToOwner`.
    fn try_to_teleport_to_owner(&self) {
        let Some(owner) = self.owner() else {
            return;
        };
        self.teleport_to_around_block_pos(owner.block_position());
    }

    /// Applies vanilla `TamableAnimal.teleportToAroundBlockPos`.
    fn teleport_to_around_block_pos(&self, target_pos: BlockPos) {
        for _ in 0..TELEPORT_ATTEMPTS {
            let xd = rand::random_range(
                -MAX_HORIZONTAL_TELEPORT_DISTANCE..=MAX_HORIZONTAL_TELEPORT_DISTANCE,
            );
            let zd = rand::random_range(
                -MAX_HORIZONTAL_TELEPORT_DISTANCE..=MAX_HORIZONTAL_TELEPORT_DISTANCE,
            );
            if xd.abs() < MIN_HORIZONTAL_TELEPORT_DISTANCE
                && zd.abs() < MIN_HORIZONTAL_TELEPORT_DISTANCE
            {
                continue;
            }

            let yd = rand::random_range(
                -MAX_VERTICAL_TELEPORT_DISTANCE..=MAX_VERTICAL_TELEPORT_DISTANCE,
            );
            if self.maybe_teleport_to(BlockPos::new(
                target_pos.x() + xd,
                target_pos.y() + yd,
                target_pos.z() + zd,
            )) {
                return;
            }
        }
    }

    /// Applies vanilla `TamableAnimal.maybeTeleportTo`.
    fn maybe_teleport_to(&self, pos: BlockPos) -> bool {
        if !self.can_teleport_to(pos) {
            return false;
        }

        let (x, y, z) = pos.get_bottom_center();
        if self.try_set_position(DVec3::new(x, y, z)).is_err() {
            return false;
        }
        self.mob_base().navigation().lock().stop();
        true
    }

    /// Applies vanilla `TamableAnimal.canTeleportTo`.
    fn can_teleport_to(&self, pos: BlockPos) -> bool {
        let Some(world) = self.level() else {
            return false;
        };

        let mut context = PathfindingContext::new(world.as_ref(), self.block_position());
        if WalkPathEvaluator::path_type_static(&mut context, pos) != PathType::Walkable {
            return false;
        }

        if !self.can_fly_to_owner()
            && world
                .get_block_state(pos.below())
                .get_block()
                .has_tag(&BlockTag::LEAVES)
        {
            return false;
        }

        let current = self.block_position();
        let delta = DVec3::new(
            f64::from(pos.x() - current.x()),
            f64::from(pos.y() - current.y()),
            f64::from(pos.z() - current.z()),
        );
        let moved_box = self.bounding_box().translate(delta);
        !collision::has_collision(
            &WorldCollisionProvider::for_entity(&world, self.as_entity_event_source()),
            moved_box,
        )
    }

    /// Sends the vanilla death message a pet's owner gets.
    ///
    /// Vanilla parity: the `TamableAnimal.die` override, which sends the owner
    /// `getCombatTracker().getDeathMessage()`. Steel has no combat tracker yet,
    /// so the message is built the same way [`Player`] already builds its own:
    /// the `death.attack.<message_id>` key with the victim's display name.
    fn notify_owner_of_death(&self, source: &DamageSource) {
        let Some(world) = self.level() else {
            return;
        };
        if !world.get_game_rule(&SHOW_DEATH_MESSAGES) {
            return;
        }
        let Some(owner) = self.owner() else {
            return;
        };
        let Some(player) = owner.as_player() else {
            return;
        };

        let content = TranslatedMessage {
            key: format!("death.attack.{}", source.damage_type.message_id).into(),
            fallback: None,
            args: Some(Box::new([self.display_name()])),
        }
        .component();
        player.send_packet(CSystemChat {
            content,
            overlay: false,
        });
    }

    /// Saves vanilla tameable animal fields.
    fn save_tamable_animal(&self, nbt: &mut NbtCompound) {
        if let Some(owner) = self.owner_uuid() {
            nbt.insert("Owner", NbtTag::IntArray(owner.to_int_array().to_vec()));
        }
        nbt.insert("Sitting", i8::from(self.is_ordered_to_sit()));
    }

    /// Loads vanilla tameable animal fields.
    fn load_tamable_animal(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        let owner = nbt
            .int_array("Owner")
            .and_then(|values| Uuid::from_int_array(&values));
        if let Some(owner) = owner {
            self.set_owner_uuid(Some(owner));
            self.set_tame(true, false);
        } else {
            self.set_owner_uuid(None);
            self.set_tame(false, true);
        }

        let ordered_to_sit = nbt.byte("Sitting").is_some_and(|value| value != 0);
        self.set_ordered_to_sit(ordered_to_sit);
        self.set_in_sitting_pose(ordered_to_sit);
    }
}

/// Returns whether `entity` is a tamed animal.
///
/// Vanilla parity: the `entity instanceof TamableAnimal animal && animal.isTame()`
/// test that `Wolf.wantsToAttack` and `Fox.FoxAlertableEntitiesSelector` share.
#[must_use]
pub fn is_tamed(entity: &dyn Entity) -> bool {
    entity
        .as_tamable_animal()
        .is_some_and(TamableAnimal::is_tame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tame_and_sitting_flags_occupy_different_bits() {
        // The two live in one synced byte; overlapping them would make a sitting
        // pet read as tamed, which is what the client draws the collar from.
        assert_eq!(FLAG_TAME & FLAG_IN_SITTING_POSE, 0);
    }

    #[test]
    fn a_fresh_tamable_animal_is_not_ordered_to_sit() {
        let base = TamableAnimalBase::new();
        assert!(!base.is_ordered_to_sit());

        base.set_ordered_to_sit(true);
        assert!(base.is_ordered_to_sit());
    }
}
