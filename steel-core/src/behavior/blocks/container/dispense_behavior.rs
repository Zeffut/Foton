//! What a dispenser does with each item.
//!
//! Vanilla parity: `DispenseItemBehavior` and the registry
//! `DispenseItemBehavior.bootStrap` fills. A dispenser is only interesting
//! because of this table: without it every item is simply thrown, which is what
//! vanilla itself does for anything with no entry.

use std::sync::{Arc, LazyLock};

use glam::DVec3;
use rustc_hash::FxHashMap;
use steel_protocol::packets::game::SoundSource;
use steel_registry::item_stack::ItemStack;
use steel_registry::items::ItemRef;
use steel_registry::vanilla_game_rules::TNT_EXPLODES;
use steel_registry::{level_events, sound_events, vanilla_entities, vanilla_items};
use steel_utils::{BlockPos, Direction, Identifier};

use crate::entity::entities::{ArrowEntity, PrimedTntEntity};
use crate::entity::{Entity, Projectile as _, next_entity_id};
use crate::world::World;

/// Where the dispensing happens.
///
/// Vanilla parity: `BlockSource`, minus the block entity, which Steel's
/// behaviors do not need yet.
pub struct DispenseSource<'a> {
    /// The world the dispenser lives in.
    pub world: &'a Arc<World>,
    /// The dispenser's own position.
    pub pos: BlockPos,
    /// The face it points at.
    pub facing: Direction,
}

impl DispenseSource<'_> {
    /// Returns the point in front of the block where things appear.
    ///
    /// Vanilla parity: `DispenserBlock.getDispensePosition`, whose `offset` is
    /// what lifts a fired projectile slightly above the middle of the face.
    #[must_use]
    pub fn dispense_position(&self, scale: f64, offset: DVec3) -> DVec3 {
        let center = DVec3::new(
            f64::from(self.pos.x()) + 0.5,
            f64::from(self.pos.y()) + 0.5,
            f64::from(self.pos.z()) + 0.5,
        );
        center + self.normal() * scale + offset
    }

    /// Returns the unit vector the dispenser points along.
    #[must_use]
    pub fn normal(&self) -> DVec3 {
        let (x, y, z) = self.facing.offset();
        DVec3::new(f64::from(x), f64::from(y), f64::from(z))
    }
}

/// What became of the item, and whether the block should celebrate.
///
/// Vanilla parity: the success flag of `OptionalDispenseItemBehavior`, which is
/// why a dispenser that could not act makes the flat failure click instead of
/// its usual clack and puff of smoke.
pub enum DispenseOutcome {
    /// The behavior acted; play the dispense sound and the smoke.
    Acted {
        /// What stays in the slot.
        remainder: ItemStack,
        /// A level event to play instead of the usual dispense sound.
        sound_override: Option<i32>,
    },
    /// Nothing happened; play the failure click and leave the slot alone.
    Failed(ItemStack),
}

impl DispenseOutcome {
    /// Acts with the usual dispenser sound.
    #[must_use]
    pub const fn acted(remainder: ItemStack) -> Self {
        Self::Acted {
            remainder,
            sound_override: None,
        }
    }
}

/// What a dispenser does with one item.
///
/// Vanilla parity: `DispenseItemBehavior`.
pub trait DispenseItemBehavior: Send + Sync {
    /// Acts on one item taken from the dispenser.
    fn execute(&self, source: &DispenseSource<'_>, stack: ItemStack) -> DispenseOutcome;
}

/// Distance in front of the block that projectiles and items appear at.
///
/// Vanilla parity: the `0.7` shared by `getDispensePosition` and the default
/// `DispenseConfig`.
pub const DISPENSE_OFFSET: f64 = 0.7;

/// Extra lift a fired projectile gets.
///
/// Vanilla parity: the `new Vec3(0.0, 0.1, 0.0)` of
/// `ProjectileItem.DispenseConfig.Builder`.
const PROJECTILE_LIFT: f64 = 0.1;

/// Speed a dispensed projectile leaves at.
///
/// Vanilla parity: `DispenseConfig.Builder.power`.
const PROJECTILE_POWER: f32 = 1.1;

/// Spread of a dispensed projectile.
///
/// Vanilla parity: `DispenseConfig.Builder.uncertainty`.
const PROJECTILE_UNCERTAINTY: f32 = 6.0;

/// Shoots an arrow out of the dispenser.
///
/// Vanilla parity: the `ProjectileDispenseBehavior` registered for
/// `Items.ARROW`.
struct ArrowDispenseBehavior;

impl DispenseItemBehavior for ArrowDispenseBehavior {
    fn execute(&self, source: &DispenseSource<'_>, mut stack: ItemStack) -> DispenseOutcome {
        let position =
            source.dispense_position(DISPENSE_OFFSET, DVec3::new(0.0, PROJECTILE_LIFT, 0.0));
        let arrow = Arc::new(ArrowEntity::new(
            &vanilla_entities::ARROW,
            next_entity_id(),
            position,
            Arc::downgrade(source.world),
        ));
        arrow.shoot(source.normal(), PROJECTILE_POWER, PROJECTILE_UNCERTAINTY);

        if let Err(error) = source
            .world
            .try_add_entity(Arc::clone(&arrow) as Arc<dyn Entity>)
        {
            log::debug!("dispensed arrow rejected: {error}");
            return DispenseOutcome::Failed(stack);
        }

        stack.shrink(1);
        DispenseOutcome::Acted {
            remainder: stack,
            sound_override: Some(level_events::SOUND_DISPENSER_PROJECTILE_LAUNCH),
        }
    }
}

/// Places primed TNT in front of the dispenser.
///
/// Vanilla parity: the `OptionalDispenseItemBehavior` registered for
/// `Blocks.TNT`.
struct TntDispenseBehavior;

impl DispenseItemBehavior for TntDispenseBehavior {
    fn execute(&self, source: &DispenseSource<'_>, mut stack: ItemStack) -> DispenseOutcome {
        if !source.world.get_game_rule(&TNT_EXPLODES) {
            return DispenseOutcome::Failed(stack);
        }

        let target = source.pos.relative(source.facing);
        let tnt = PrimedTntEntity::prime(source.world, target, None);
        source.world.play_sound_at(
            &sound_events::ENTITY_TNT_PRIMED,
            SoundSource::Blocks,
            tnt.position(),
            1.0,
            1.0,
            None,
        );

        // TODO: vanilla also fires the ENTITY_PLACE game event, and checks
        // `SulfurCubeBlockDispenseItemBehavior.dispenseBlock` first so a sulfur
        // cube swallows the charge; Steel has neither yet.
        stack.shrink(1);
        DispenseOutcome::acted(stack)
    }
}

/// Every item the dispenser treats specially.
///
/// Vanilla parity: `DispenserBlock.DISPENSER_REGISTRY`, filled by
/// `DispenseItemBehavior.bootStrap`. Steel covers the two entries whose entities
/// exist; the rest fall through to the default throw, which is also what vanilla
/// does for an unregistered item.
static DISPENSE_BEHAVIORS: LazyLock<FxHashMap<Identifier, Box<dyn DispenseItemBehavior>>> =
    LazyLock::new(|| {
        let mut behaviors: FxHashMap<Identifier, Box<dyn DispenseItemBehavior>> =
            FxHashMap::default();
        behaviors.insert(
            vanilla_items::ARROW.key.clone(),
            Box::new(ArrowDispenseBehavior),
        );
        behaviors.insert(
            vanilla_items::TNT.key.clone(),
            Box::new(TntDispenseBehavior),
        );
        behaviors
    });

/// Returns the behavior registered for `item`, if any.
///
/// Vanilla parity: the `DISPENSER_REGISTRY.get` of
/// `DispenserBlock.getDispenseMethod`.
#[must_use]
pub fn dispense_behavior_for(item: ItemRef) -> Option<&'static dyn DispenseItemBehavior> {
    DISPENSE_BEHAVIORS.get(&item.key).map(AsRef::as_ref)
}

#[cfg(test)]
mod tests {
    use steel_registry::init_vanilla_registry;

    use super::*;
    use crate::test_support::fresh_test_world;

    #[test]
    fn arrows_and_tnt_are_the_registered_behaviors() {
        init_vanilla_registry();
        assert!(dispense_behavior_for(&vanilla_items::ARROW).is_some());
        assert!(dispense_behavior_for(&vanilla_items::TNT).is_some());
    }

    /// Vanilla parity: an item with no entry takes the default throw, so the
    /// registry must not answer for it.
    #[test]
    fn an_unregistered_item_has_no_behavior() {
        init_vanilla_registry();
        assert!(dispense_behavior_for(&vanilla_items::STONE).is_none());
    }

    /// Vanilla parity: `getDispensePosition`, which places things in front of the
    /// face rather than inside the block.
    #[test]
    fn the_dispense_position_sits_in_front_of_the_face() {
        let world = fresh_test_world("dispense_position");
        let source = DispenseSource {
            world: &world,
            pos: BlockPos::new(10, 64, 10),
            facing: Direction::East,
        };

        let position = source.dispense_position(DISPENSE_OFFSET, DVec3::ZERO);

        assert!((position.x - 11.2).abs() < 1e-9, "x was {}", position.x);
        assert!((position.y - 64.5).abs() < 1e-9);
        assert!((position.z - 10.5).abs() < 1e-9);
    }
}
