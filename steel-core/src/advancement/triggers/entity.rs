//! Triggers that fire from what a player did to, or with, another entity.
//!
//! Every one of these evaluates its entity predicate against a context whose
//! `THIS_ENTITY` is the *other* entity, which is what
//! `EntityPredicate.createContext(player, entity)` builds. The `player`
//! predicate is checked separately by [`super::fire`], against a context whose
//! subject is the player -- the same split vanilla's
//! `SimpleCriterionTrigger.trigger` makes.

use steel_registry::advancement::TriggerInstance;
use steel_registry::item_stack::ItemStack;

use super::fire;
use crate::advancement::predicate::{PredicateContext, Subject, item_matches};
use crate::entity::Entity;
use crate::entity::damage::DamageSource;
use crate::player::Player;

/// Vanilla parity: `EntityPredicate.createContext(player, entity)`.
fn context_for<'a>(player: &'a Player, entity: &'a dyn Entity) -> PredicateContext<'a> {
    PredicateContext {
        player,
        origin: player.position(),
        subject: Subject::Entity(entity),
        block_state: None,
        tool: None,
    }
}

/// Credits `killer` with killing `victim`, firing whichever of the two kill
/// triggers applies.
///
/// Vanilla parity: `Entity.awardKillScore` together with the `ServerPlayer`
/// override. The override wraps the base in `victim != this`, so a player who
/// kills themselves fires neither half -- which is why the guard is here and
/// not in either trigger.
pub fn award_kill_score(killer: &dyn Entity, victim: &dyn Entity, killing_blow: &DamageSource) {
    if killer.as_player().is_some() && killer.id() == victim.id() {
        return;
    }
    if let Some(victim_player) = victim.as_player() {
        entity_killed_player(victim_player, killer, killing_blow);
    }
    if let Some(killer_player) = killer.as_player() {
        player_killed_entity(killer_player, victim, killing_blow);
    }
}

/// Vanilla parity: `CriteriaTriggers.PLAYER_KILLED_ENTITY`.
pub fn player_killed_entity(player: &Player, victim: &dyn Entity, killing_blow: &DamageSource) {
    killed(
        player,
        "minecraft:player_killed_entity",
        victim,
        killing_blow,
    );
}

/// Vanilla parity: `CriteriaTriggers.ENTITY_KILLED_PLAYER`.
pub fn entity_killed_player(player: &Player, killer: &dyn Entity, killing_blow: &DamageSource) {
    killed(
        player,
        "minecraft:entity_killed_player",
        killer,
        killing_blow,
    );
}

/// Vanilla parity: `CriteriaTriggers.KILL_MOB_NEAR_SCULK_CATALYST`.
pub fn kill_mob_near_sculk_catalyst(
    player: &Player,
    victim: &dyn Entity,
    killing_blow: &DamageSource,
) {
    killed(
        player,
        "minecraft:kill_mob_near_sculk_catalyst",
        victim,
        killing_blow,
    );
}

/// Vanilla parity: `KilledTrigger.TriggerInstance.matches`, which tests the
/// killing blow first and then the entity, and accepts anyone when the entity
/// predicate is absent.
fn killed(
    player: &Player,
    trigger_id: &'static str,
    entity: &dyn Entity,
    killing_blow: &DamageSource,
) {
    let context = context_for(player, entity);
    fire(player, trigger_id, |instance| {
        let (predicate, blow) = match instance {
            TriggerInstance::PlayerKilledEntity {
                entity,
                killing_blow,
                ..
            }
            | TriggerInstance::EntityKilledPlayer {
                entity,
                killing_blow,
                ..
            }
            | TriggerInstance::KillMobNearSculkCatalyst {
                entity,
                killing_blow,
                ..
            } => (*entity, killing_blow),
            _ => return false,
        };
        if let Some(blow) = blow
            && !context.matches_damage_source(blow, killing_blow)
        {
            return false;
        }
        context.matches_conditions(predicate)
    });
}

/// Vanilla parity: `CriteriaTriggers.TAME_ANIMAL`.
pub fn tame_animal(player: &Player, animal: &dyn Entity) {
    let context = context_for(player, animal);
    fire(player, "minecraft:tame_animal", |instance| {
        let TriggerInstance::TameAnimal { entity, .. } = instance else {
            return false;
        };
        context.matches_conditions(entity)
    });
}

/// Vanilla parity: `CriteriaTriggers.BRED_ANIMALS`.
///
/// The two parents are interchangeable, and a criterion that names a child
/// fails outright when none was born -- which is what turtles do, since they
/// lay an egg instead.
pub fn bred_animals(
    player: &Player,
    parent: &dyn Entity,
    partner: &dyn Entity,
    child: Option<&dyn Entity>,
) {
    let parent_context = context_for(player, parent);
    let partner_context = context_for(player, partner);
    let child_context = child.map(|child| context_for(player, child));

    fire(player, "minecraft:bred_animals", |instance| {
        let TriggerInstance::BredAnimals {
            parent: wanted_parent,
            partner: wanted_partner,
            child: wanted_child,
            ..
        } = instance
        else {
            return false;
        };

        // An absent `child` predicate lowers to an empty slice, and every
        // vanilla criterion that uses this trigger fills one in, so the two
        // cases vanilla's `Optional` separates cannot be told apart here and
        // do not need to be.
        if !wanted_child.is_empty() {
            let Some(context) = child_context.as_ref() else {
                return false;
            };
            if !context.matches_conditions(wanted_child) {
                return false;
            }
        }

        (parent_context.matches_conditions(wanted_parent)
            && partner_context.matches_conditions(wanted_partner))
            || (partner_context.matches_conditions(wanted_parent)
                && parent_context.matches_conditions(wanted_partner))
    });
}

/// Vanilla parity: `CriteriaTriggers.START_RIDING_TRIGGER`.
///
/// `PlayerTrigger` carries nothing but its `player` predicate, so mounting is
/// the whole condition.
pub fn started_riding(player: &Player) {
    fire(player, "minecraft:started_riding", |_| true);
}

/// Vanilla parity: `CriteriaTriggers.PLAYER_INTERACTED_WITH_ENTITY`.
pub fn player_interacted_with_entity(player: &Player, item: &ItemStack, entity: &dyn Entity) {
    interacted(
        player,
        "minecraft:player_interacted_with_entity",
        item,
        entity,
    );
}

/// Vanilla parity: `CriteriaTriggers.PLAYER_SHEARED_EQUIPMENT`.
pub fn player_sheared_equipment(player: &Player, item: &ItemStack, entity: &dyn Entity) {
    interacted(player, "minecraft:player_sheared_equipment", item, entity);
}

/// Vanilla parity: `PlayerInteractTrigger.TriggerInstance.matches`, which both
/// of the triggers above share.
fn interacted(player: &Player, trigger_id: &'static str, item: &ItemStack, entity: &dyn Entity) {
    let context = context_for(player, entity);
    fire(player, trigger_id, |instance| {
        let (wanted_item, wanted_entity) = match instance {
            TriggerInstance::PlayerInteractedWithEntity { item, entity, .. }
            | TriggerInstance::PlayerShearedEquipment { item, entity, .. } => (item, *entity),
            _ => return false,
        };
        if let Some(wanted_item) = wanted_item
            && !item_matches(wanted_item, item)
        {
            return false;
        }
        context.matches_conditions(wanted_entity)
    });
}

/// Vanilla parity: `CriteriaTriggers.ENTITY_HURT_PLAYER`.
pub fn entity_hurt_player(
    player: &Player,
    source: &DamageSource,
    original_damage: f32,
    damage: f32,
    blocked: bool,
) {
    hurt(
        player,
        "minecraft:entity_hurt_player",
        player,
        source,
        original_damage,
        damage,
        blocked,
    );
}

/// Vanilla parity: `CriteriaTriggers.PLAYER_HURT_ENTITY`.
pub fn player_hurt_entity(
    player: &Player,
    victim: &dyn Entity,
    source: &DamageSource,
    original_damage: f32,
    damage: f32,
    blocked: bool,
) {
    hurt(
        player,
        "minecraft:player_hurt_entity",
        victim,
        source,
        original_damage,
        damage,
        blocked,
    );
}

/// Vanilla parity: `EntityHurtPlayerTrigger` and `DamageTrigger`, which share a
/// `DamagePredicate` and differ only in whether they also check the entity that
/// was hurt.
///
/// The two damage figures are not interchangeable: `dealt` is what the blow was
/// worth before a shield ate any of it, `taken` is what survived that. A
/// criterion asking for `dealt` would be satisfied by a blocked hit and one
/// asking for `taken` would not.
fn hurt(
    player: &Player,
    trigger_id: &'static str,
    entity: &dyn Entity,
    source: &DamageSource,
    original_damage: f32,
    damage: f32,
    blocked: bool,
) {
    let context = context_for(player, entity);
    fire(player, trigger_id, |instance| {
        let (wanted_damage, wanted_entity) = match instance {
            TriggerInstance::EntityHurtPlayer { damage, .. } => (damage, &[][..]),
            TriggerInstance::PlayerHurtEntity { damage, entity, .. } => (damage, *entity),
            _ => return false,
        };
        if let Some(wanted_damage) = wanted_damage
            && !context.matches_damage(wanted_damage, source, original_damage, damage, blocked)
        {
            return false;
        }
        context.matches_conditions(wanted_entity)
    });
}
