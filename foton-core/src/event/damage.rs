//! Damage about to be dealt to an entity, as plugins are told about it.

use foton_registry::damage_type::DamageType;
use foton_registry::{vanilla_damage_types, vanilla_entities};
use foton_utils::BlockPos;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use uuid::Uuid;

use super::Event;
use crate::entity::Entity;
use crate::entity::damage::DamageSource;
use crate::world::World;

/// What dealt the damage, as Bukkit splits its damage events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Damager {
    /// Nothing did: `EntityDamageEvent` itself.
    Nothing,
    /// An entity did: `EntityDamageByEntityEvent`.
    Entity(Uuid),
    /// A block did, possibly one nobody recorded: `EntityDamageByBlockEvent`.
    Block(Option<BlockPos>),
}

/// One of the reductions between a hit's raw amount and what lands.
///
/// Bukkit's `EntityDamageEvent.DamageModifier`, less `BASE`, which is the raw
/// amount itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamageModifier {
    /// What the invulnerability frames take off a hit landing inside them.
    InvulnerabilityReduction,
    /// The extra a freezing hit does to those it hurts more.
    Freezing,
    /// What a helmet takes off a blow aimed at it.
    HardHat,
    /// What a raised shield takes.
    Blocking,
    /// What armor takes.
    Armor,
    /// What the resistance effect takes.
    Resistance,
    /// What protection enchantments take.
    Magic,
    /// What absorption hearts soak up.
    Absorption,
}

impl DamageModifier {
    /// The Bukkit constant's name.
    #[must_use]
    pub const fn bukkit_name(self) -> &'static str {
        match self {
            Self::InvulnerabilityReduction => "INVULNERABILITY_REDUCTION",
            Self::Freezing => "FREEZING",
            Self::HardHat => "HARD_HAT",
            Self::Blocking => "BLOCKING",
            Self::Armor => "ARMOR",
            Self::Resistance => "RESISTANCE",
            Self::Magic => "MAGIC",
            Self::Absorption => "ABSORPTION",
        }
    }
}

/// An entity is about to take damage.
///
/// Paper parity: the one event `CraftEventFactory.handleEntityDamageEvent`
/// fires for every hit, whose Bukkit class depends on what dealt it. A
/// listener may cancel the hit or change its raw amount; the hurt that follows
/// is worked out again from the new amount, as Paper's modifier functions do.
/// The modifiers are what each reduction would take from the raw amount the
/// event was fired with; they are for plugins to read.
pub struct EntityDamageEvent {
    entity: Uuid,
    damager: Damager,
    cause: &'static str,
    damage: f64,
    modifiers: Vec<(DamageModifier, f64)>,
    critical: bool,
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
    /// Creates the event for `source` about to deal `damage` to `entity`,
    /// which the `modifiers` would reduce, each by a negative amount.
    #[must_use]
    pub fn new(
        world: &World,
        entity: Uuid,
        source: &DamageSource,
        damage: f64,
        modifiers: Vec<(DamageModifier, f64)>,
    ) -> Self {
        let (damager, cause) = Self::classify(world, source);
        Self {
            entity,
            damager,
            cause,
            damage,
            modifiers,
            critical: source.critical,
            cancelled: false,
        }
    }

    /// Who is being hurt.
    #[must_use]
    pub const fn entity(&self) -> Uuid {
        self.entity
    }

    /// What dealt the damage.
    #[must_use]
    pub const fn damager(&self) -> Damager {
        self.damager
    }

    /// The Bukkit `DamageCause` name.
    #[must_use]
    pub const fn cause(&self) -> &'static str {
        self.cause
    }

    /// The raw amount, before blocking, armor and the rest.
    #[must_use]
    pub const fn damage(&self) -> f64 {
        self.damage
    }

    /// What each reduction takes from the raw amount the event was fired with.
    #[must_use]
    pub fn modifiers(&self) -> &[(DamageModifier, f64)] {
        &self.modifiers
    }

    /// Whether the attacker landed a critical hit.
    #[must_use]
    pub const fn critical(&self) -> bool {
        self.critical
    }

    /// Changes the raw amount.
    pub const fn set_damage(&mut self, damage: f64) {
        self.damage = damage;
    }

    /// Stops the hurt, or lets it happen again.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }

    /// Fires the event for `source` hurting `entity`, which is not a living
    /// entity, and answers whether the hurt may go ahead.
    ///
    /// Paper parity: `CraftEventFactory.handleNonLivingEntityDamageEvent`.
    /// Nothing reduces the damage, so the final amount is the raw one. Where
    /// `cancel_on_zero` holds, a listener that brings it to zero stops the
    /// hurt as cancelling would.
    pub fn allows_non_living_hurt(
        entity: &dyn Entity,
        world: &World,
        source: &DamageSource,
        damage: f32,
        cancel_on_zero: bool,
    ) -> bool {
        let mut event = Self::new(world, entity.uuid(), source, f64::from(damage), Vec::new());
        world.fire_event(&mut event);
        !event.cancelled && (!cancel_on_zero || event.damage != 0.0)
    }

    /// Splits a source into the damager and cause Bukkit reports.
    ///
    /// Paper parity: `CraftEventFactory.handleEntityDamageEvent`, branch for
    /// branch. A direct entity that no longer resolves is treated as absent.
    fn classify(world: &World, source: &DamageSource) -> (Damager, &'static str) {
        let damager = source
            .event_entity_damager
            .or(source.direct_entity_id)
            .and_then(|id| world.get_entity_by_id(id));
        if let Some(damager) = damager {
            return (
                Damager::Entity(damager.uuid()),
                Self::entity_cause(source, damager.as_ref()),
            );
        }
        let is = |damage_type: &DamageType| source.damage_type.key == damage_type.key;
        if is(&vanilla_damage_types::OUT_OF_WORLD) {
            return (Damager::Block(source.block_damager), "VOID");
        }
        if is(&vanilla_damage_types::LAVA) {
            return (Damager::Block(source.block_damager), "LAVA");
        }
        // Vanilla keeps no block behind a bad respawn point's blast; Paper
        // carries one through its block snapshot, which is what reaches this
        // branch.
        if source.block_damager.is_some() || is(&vanilla_damage_types::BAD_RESPAWN_POINT) {
            return (
                Damager::Block(source.block_damager),
                Self::block_cause(source),
            );
        }
        (Damager::Nothing, Self::environmental_cause(source))
    }

    fn entity_cause(source: &DamageSource, damager: &dyn Entity) -> &'static str {
        if let Some(cause) = source.known_cause {
            return cause;
        }
        let is = |damage_type: &DamageType| source.damage_type.key == damage_type.key;
        if is(&vanilla_damage_types::FIREWORKS)
            || is(&vanilla_damage_types::EXPLOSION)
            || is(&vanilla_damage_types::PLAYER_EXPLOSION)
        {
            return if damager.entity_type() == &vanilla_entities::TNT {
                "BLOCK_EXPLOSION"
            } else {
                "ENTITY_EXPLOSION"
            };
        }
        if damager.as_projectile().is_some() {
            let potion = damager.entity_type() == &vanilla_entities::SPLASH_POTION
                || damager.entity_type() == &vanilla_entities::LINGERING_POTION;
            return if potion { "MAGIC" } else { "PROJECTILE" };
        }
        if is(&vanilla_damage_types::THORNS) {
            "THORNS"
        } else if is(&vanilla_damage_types::SONIC_BOOM) {
            "SONIC_BOOM"
        } else if is(&vanilla_damage_types::FALLING_STALACTITE)
            || is(&vanilla_damage_types::FALLING_BLOCK)
            || is(&vanilla_damage_types::FALLING_ANVIL)
        {
            "FALLING_BLOCK"
        } else if is(&vanilla_damage_types::LIGHTNING_BOLT) {
            "LIGHTNING"
        } else if is(&vanilla_damage_types::DRAGON_BREATH) {
            "DRAGON_BREATH"
        } else if is(&vanilla_damage_types::MAGIC) {
            "MAGIC"
        } else {
            "ENTITY_ATTACK"
        }
    }

    fn block_cause(source: &DamageSource) -> &'static str {
        if let Some(cause) = source.known_cause {
            return cause;
        }
        let is = |damage_type: &DamageType| source.damage_type.key == damage_type.key;
        if is(&vanilla_damage_types::CACTUS)
            || is(&vanilla_damage_types::SWEET_BERRY_BUSH)
            || is(&vanilla_damage_types::STALAGMITE)
            || is(&vanilla_damage_types::HOT_FLOOR)
            || is(&vanilla_damage_types::CAMPFIRE)
        {
            "CONTACT"
        } else if is(&vanilla_damage_types::MAGIC) {
            "MAGIC"
        } else if is(&vanilla_damage_types::IN_FIRE) {
            "FIRE"
        } else if is(&vanilla_damage_types::BAD_RESPAWN_POINT) {
            "BLOCK_EXPLOSION"
        } else {
            "CUSTOM"
        }
    }

    /// The cause of damage that neither an entity nor a block dealt.
    fn environmental_cause(source: &DamageSource) -> &'static str {
        if let Some(cause) = source.known_cause {
            return cause;
        }
        let is = |damage_type: &DamageType| source.damage_type.key == damage_type.key;
        if is(&vanilla_damage_types::IN_FIRE) {
            "FIRE"
        } else if is(&vanilla_damage_types::STARVE) {
            "STARVATION"
        } else if is(&vanilla_damage_types::WITHER) {
            "WITHER"
        } else if is(&vanilla_damage_types::IN_WALL) {
            "SUFFOCATION"
        } else if is(&vanilla_damage_types::DROWN) {
            "DROWNING"
        } else if is(&vanilla_damage_types::ON_FIRE) {
            "FIRE_TICK"
        } else if is(&vanilla_damage_types::MAGIC) {
            "MAGIC"
        } else if is(&vanilla_damage_types::FALL) {
            "FALL"
        } else if is(&vanilla_damage_types::FLY_INTO_WALL) {
            "FLY_INTO_WALL"
        } else if is(&vanilla_damage_types::CRAMMING) {
            "CRAMMING"
        } else if is(&vanilla_damage_types::DRY_OUT) {
            "DRYOUT"
        } else if is(&vanilla_damage_types::FREEZE) {
            "FREEZE"
        } else if is(&vanilla_damage_types::GENERIC_KILL) {
            "KILL"
        } else if is(&vanilla_damage_types::OUTSIDE_BORDER) {
            "WORLD_BORDER"
        } else {
            "CUSTOM"
        }
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::{init_vanilla_registry, vanilla_damage_types};

    use super::*;
    use crate::test_support::fresh_test_world;

    /// A block-dealt hit is classified by its damage type the way Paper
    /// does, which is not the way the Bukkit cause names read: a magma block
    /// and a campfire are `CONTACT`, never `HOT_FLOOR` or `CAMPFIRE`.
    #[test]
    fn block_damage_is_contact_and_unnamed_damage_is_environmental() {
        init_vanilla_registry();
        let world = fresh_test_world("damage_event_classify");

        let magma = DamageSource::environment(&vanilla_damage_types::HOT_FLOOR)
            .with_block_damager(BlockPos::new(1, 2, 3));
        assert_eq!(
            EntityDamageEvent::classify(&world, &magma),
            (Damager::Block(Some(BlockPos::new(1, 2, 3))), "CONTACT")
        );

        let lava = DamageSource::environment(&vanilla_damage_types::LAVA);
        assert_eq!(
            EntityDamageEvent::classify(&world, &lava),
            (Damager::Block(None), "LAVA")
        );

        let freezing = DamageSource::environment(&vanilla_damage_types::FREEZE);
        assert_eq!(
            EntityDamageEvent::classify(&world, &freezing),
            (Damager::Nothing, "FREEZE")
        );

        // A melting snow golem burns, but Paper knows better than the type.
        let melting =
            DamageSource::environment(&vanilla_damage_types::ON_FIRE).with_known_cause("MELTING");
        assert_eq!(
            EntityDamageEvent::classify(&world, &melting),
            (Damager::Nothing, "MELTING")
        );
    }
}
