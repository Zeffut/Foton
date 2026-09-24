//! Damage with no entity behind it: falling, burning, freezing, drowning.

use foton_registry::vanilla_damage_types;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use uuid::Uuid;

use super::Event;
use crate::entity::damage::DamageSource;

/// Something that is not an entity is about to hurt a living entity.
///
/// Damage an entity deals goes through
/// [`EntityDamageByEntityEvent`](super::EntityDamageByEntityEvent) instead,
/// which is why this is only fired for sources with no causing or direct
/// entity. A listener may cancel the hurt or change how much it does.
pub struct EntityDamageEvent {
    entity: Uuid,
    cause: &'static str,
    damage: f64,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for EntityDamageEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_damage");
}

impl Event for EntityDamageEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl EntityDamageEvent {
    /// Creates the event for `damage` of the Bukkit `cause` about to hit
    /// `entity`.
    #[must_use]
    pub const fn new(entity: Uuid, cause: &'static str, damage: f64) -> Self {
        Self {
            entity,
            cause,
            damage,
            cancelled: false,
        }
    }

    /// Who is being hurt.
    #[must_use]
    pub const fn entity(&self) -> Uuid {
        self.entity
    }

    /// The Bukkit `DamageCause` name.
    #[must_use]
    pub const fn cause(&self) -> &'static str {
        self.cause
    }

    /// How much damage will be dealt.
    #[must_use]
    pub const fn damage(&self) -> f64 {
        self.damage
    }

    /// Changes how much damage will be dealt.
    pub const fn set_damage(&mut self, damage: f64) {
        self.damage = damage;
    }

    /// Stops the hurt, or lets it happen again.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }

    /// The Bukkit `DamageCause` of a source no entity is behind, or `None`
    /// for one an entity is.
    ///
    /// The table is Bukkit's own description of each cause, read against the
    /// vanilla damage type that produces it: `FREEZE` is "damage caused from
    /// freezing", `HOT_FLOOR` "when an entity steps on a magma block", and so
    /// on. A type no cause describes is `CUSTOM`, which is what Paper reports
    /// for one it does not know either.
    #[must_use]
    pub fn environmental_cause(source: &DamageSource) -> Option<&'static str> {
        if source.causing_entity_id.is_some() || source.direct_entity_id.is_some() {
            return None;
        }
        let key = &source.damage_type.key;
        let is = |damage_type: &foton_registry::damage_type::DamageType| {
            &damage_type.key == key
        };
        Some(if is(&vanilla_damage_types::GENERIC_KILL) {
            "KILL"
        } else if is(&vanilla_damage_types::OUTSIDE_BORDER) {
            "WORLD_BORDER"
        } else if is(&vanilla_damage_types::CACTUS)
            || is(&vanilla_damage_types::SWEET_BERRY_BUSH)
            || is(&vanilla_damage_types::STALAGMITE)
        {
            "CONTACT"
        } else if is(&vanilla_damage_types::IN_WALL) {
            "SUFFOCATION"
        } else if is(&vanilla_damage_types::FALL) {
            "FALL"
        } else if is(&vanilla_damage_types::IN_FIRE) {
            "FIRE"
        } else if is(&vanilla_damage_types::ON_FIRE) {
            "FIRE_TICK"
        } else if is(&vanilla_damage_types::LAVA) {
            "LAVA"
        } else if is(&vanilla_damage_types::DROWN) {
            "DROWNING"
        } else if is(&vanilla_damage_types::EXPLOSION)
            || is(&vanilla_damage_types::BAD_RESPAWN_POINT)
        {
            "BLOCK_EXPLOSION"
        } else if is(&vanilla_damage_types::OUT_OF_WORLD) {
            "VOID"
        } else if is(&vanilla_damage_types::LIGHTNING_BOLT) {
            "LIGHTNING"
        } else if is(&vanilla_damage_types::STARVE) {
            "STARVATION"
        } else if is(&vanilla_damage_types::MAGIC) || is(&vanilla_damage_types::INDIRECT_MAGIC) {
            "MAGIC"
        } else if is(&vanilla_damage_types::WITHER) {
            "WITHER"
        } else if is(&vanilla_damage_types::DRAGON_BREATH) {
            "DRAGON_BREATH"
        } else if is(&vanilla_damage_types::FLY_INTO_WALL) {
            "FLY_INTO_WALL"
        } else if is(&vanilla_damage_types::HOT_FLOOR) {
            "HOT_FLOOR"
        } else if is(&vanilla_damage_types::CAMPFIRE) {
            "CAMPFIRE"
        } else if is(&vanilla_damage_types::CRAMMING) {
            "CRAMMING"
        } else if is(&vanilla_damage_types::DRY_OUT) {
            "DRYOUT"
        } else if is(&vanilla_damage_types::FREEZE) {
            "FREEZE"
        } else {
            "CUSTOM"
        })
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::{init_vanilla_registry, vanilla_damage_types};

    use super::*;

    #[test]
    fn an_entity_behind_the_damage_leaves_it_to_the_by_entity_event() {
        init_vanilla_registry();
        let freezing = DamageSource::environment(&vanilla_damage_types::FREEZE);
        assert_eq!(EntityDamageEvent::environmental_cause(&freezing), Some("FREEZE"));
        let pushed = DamageSource::environment(&vanilla_damage_types::FALL).with_causing_entity(7);
        assert_eq!(EntityDamageEvent::environmental_cause(&pushed), None);
    }
}
