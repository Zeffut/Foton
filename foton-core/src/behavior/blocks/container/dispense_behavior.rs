//! What a dispenser does with each item.
//!
//! Vanilla parity: `DispenseItemBehavior` and the registry
//! `DispenseItemBehavior.bootStrap` fills. A dispenser is only interesting
//! because of this table: without it every item is simply thrown, which is what
//! vanilla itself does for anything with no entry.

use std::sync::{Arc, LazyLock};

use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::{BlockStateProperties, BoolProperty};
use foton_registry::item_stack::ItemStack;
use foton_registry::items::ItemRef;
use foton_registry::vanilla_game_events;
use foton_registry::vanilla_game_rules::TNT_EXPLODES;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{REGISTRY, TaggedRegistryExt as _};
use foton_registry::{level_events, sound_events, vanilla_blocks, vanilla_entities, vanilla_items};
use foton_utils::types::UpdateFlags;
use foton_utils::{BlockPos, BlockStateId, Direction, Downcast as _, Identifier, WorldAabb};
use glam::DVec3;
use rustc_hash::FxHashMap;

use crate::behavior::blocks::FireBlock;
use crate::behavior::blocks::container::dispenser_block::{
    play_dispense_effects, spawn_dispensed_item,
};
use crate::behavior::items::SpawnEggItem;
use crate::behavior::{BLOCK_BEHAVIORS, ITEM_BEHAVIORS, pickup_waterlogged_block};
use crate::block_entity::entities::DispenserBlockEntity;
use crate::entity::entities::{
    ArrowEntity, PrimedTntEntity, SheepEntity, SmallFireballEntity, SulfurCubeEntity,
};
use crate::entity::{Entity, Projectile as _, next_entity_id};
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// Where the dispensing happens.
///
/// Vanilla parity: `BlockSource`. The facing is unpacked from the state, which
/// vanilla's behaviors read out of it themselves.
pub struct DispenseSource<'a> {
    /// The world the dispenser lives in.
    pub world: &'a Arc<World>,
    /// The dispenser's own position.
    pub pos: BlockPos,
    /// The face it points at.
    pub facing: Direction,
    /// The nine slots behind the face.
    ///
    /// Vanilla parity: `BlockSource.blockEntity`, which only
    /// `consumeWithRemainder` reads -- it is how the empty bucket a water
    /// bucket leaves gets back inside the dispenser instead of on the floor.
    pub block_entity: &'a DispenserBlockEntity,
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

/// Whether a block is currently alight.
const LIT: &BoolProperty = &BlockStateProperties::LIT;

/// Whether a block is standing in water.
const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;

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

/// How far in front of the face a fire charge appears.
///
/// Vanilla parity: the `getDispensePosition(source, 1.0, Vec3.ZERO)` of
/// `FireChargeItem.createDispenseConfig`, which is further out and without the
/// lift the other projectiles get.
const FIRE_CHARGE_OFFSET: f64 = 1.0;

/// Speed a dispensed fire charge leaves at.
///
/// Vanilla parity: `FireChargeItem.createDispenseConfig().power`.
const FIRE_CHARGE_POWER: f32 = 1.0;

/// Spread of a dispensed fire charge.
///
/// Vanilla parity: `FireChargeItem.createDispenseConfig().uncertainty`.
const FIRE_CHARGE_UNCERTAINTY: f32 = 6.666_666_5;

/// Shoots a small fireball out of the dispenser.
///
/// Vanilla parity: the `registerProjectileBehavior(Items.FIRE_CHARGE)` of
/// `DispenseItemBehavior.bootStrap`, which builds a `ProjectileDispenseBehavior`
/// around `FireChargeItem`.
///
/// `FireChargeItem.asProjectile` jitters the direction with `random.triangle`
/// before handing the fireball over, but `ProjectileDispenseBehavior.execute`
/// then calls `Projectile.shoot`, which overwrites the velocity outright. The
/// jitter is unobservable, so it is not reproduced here.
struct FireChargeDispenseBehavior;

impl DispenseItemBehavior for FireChargeDispenseBehavior {
    fn execute(&self, source: &DispenseSource<'_>, mut stack: ItemStack) -> DispenseOutcome {
        let position = source.dispense_position(FIRE_CHARGE_OFFSET, DVec3::ZERO);
        let fireball = Arc::new(SmallFireballEntity::new(
            &vanilla_entities::SMALL_FIREBALL,
            next_entity_id(),
            position,
            Arc::downgrade(source.world),
        ));
        // Vanilla parity: the `fireball.setItem(itemStack)` of
        // `FireChargeItem.asProjectile`, which is what the client draws.
        fireball.set_item(stack.copy_with_count(1));
        fireball.shoot(source.normal(), FIRE_CHARGE_POWER, FIRE_CHARGE_UNCERTAINTY);

        if let Err(error) = source
            .world
            .try_add_entity(Arc::clone(&fireball) as Arc<dyn Entity>)
        {
            log::debug!("dispensed fire charge rejected: {error}");
            return DispenseOutcome::Failed(stack);
        }

        stack.shrink(1);
        DispenseOutcome::Acted {
            remainder: stack,
            // Vanilla parity: the `overrideDispenseEvent(1018)` of the same
            // config -- a dispensed fire charge roars, it does not click.
            sound_override: Some(level_events::SOUND_BLAZE_FIREBALL),
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
        if feed_sulfur_cube(source.world, target, &mut stack) {
            return DispenseOutcome::acted(stack);
        }

        let tnt = PrimedTntEntity::prime(source.world, target, None);
        source.world.play_sound_at(
            &sound_events::ENTITY_TNT_PRIMED,
            SoundSource::Blocks,
            tnt.position(),
            1.0,
            1.0,
            None,
        );

        // TODO: vanilla also fires the ENTITY_PLACE game event, which Foton
        // does not have here yet.
        stack.shrink(1);
        DispenseOutcome::acted(stack)
    }
}

/// Grows whatever is in front of the dispenser.
///
/// Vanilla parity: the `Items.BONE_MEAL` entry of
/// `DispenseItemBehavior.bootStrap`.
///
/// TODO: vanilla also falls back to `BoneMealItem.growWaterPlant`, which spreads
/// seagrass and coral across open water; Foton has no equivalent yet, so bone
/// meal aimed at water does nothing rather than doing the wrong thing.
struct BoneMealDispenseBehavior;

impl DispenseItemBehavior for BoneMealDispenseBehavior {
    fn execute(&self, source: &DispenseSource<'_>, mut stack: ItemStack) -> DispenseOutcome {
        let target = source.pos.relative(source.facing);
        let state = source.world.get_block_state(target);
        let behavior = BLOCK_BEHAVIORS.get_behavior(state.get_block());
        let Some(bonemealable) = behavior.as_bonemealable() else {
            return DispenseOutcome::Failed(stack);
        };

        if !bonemealable.is_valid_bonemeal_target(state, source.world.as_ref(), target) {
            return DispenseOutcome::Failed(stack);
        }

        let mut rng = rand::rng();
        if !bonemealable.is_bonemeal_success(state, source.world, &mut rng, target) {
            // Vanilla still counts the attempt as a success and consumes the
            // item; only the growth is left to the next try.
            stack.shrink(1);
            return DispenseOutcome::acted(stack);
        }

        bonemealable.perform_bonemeal(state, source.world, &mut rng, target);
        source.world.level_event(
            level_events::PARTICLES_AND_SOUND_PLANT_GROWTH,
            target,
            15,
            None,
        );
        stack.shrink(1);
        DispenseOutcome::acted(stack)
    }
}

/// Sets fire to whatever is in front of the dispenser.
///
/// Vanilla parity: `FlintAndSteelDispenseItemBehavior`.
///
///
/// TODO: vanilla also lights a sulfur cube standing in front of the
/// dispenser, through `SulfurCube.primeTime`. Foton's flint and foton only
/// reaches blocks.
struct FlintAndSteelDispenseBehavior;

impl DispenseItemBehavior for FlintAndSteelDispenseBehavior {
    fn execute(&self, source: &DispenseSource<'_>, mut stack: ItemStack) -> DispenseOutcome {
        let target = source.pos.relative(source.facing);
        let state = source.world.get_block_state(target);

        let lit = if FireBlock::can_be_placed_at(source.world, target, source.facing) {
            source.world.set_block(
                target,
                FireBlock::get_state(source.world.as_ref(), target),
                UpdateFlags::UPDATE_ALL,
            )
        } else if can_be_lit(state) {
            source
                .world
                .set_block(target, state.set_value(LIT, true), UpdateFlags::UPDATE_ALL)
        } else if state.get_block() == &vanilla_blocks::TNT {
            let _ = PrimedTntEntity::prime(source.world, target, None);
            source.world.remove_block(target, false)
        } else {
            false
        };

        if !lit {
            return DispenseOutcome::Failed(stack);
        }

        // Vanilla parity: the flint and foton wears out one point per use, and
        // the dispenser keeps the broken remainder rather than a ghost item.
        if stack.hurt_and_break(1, false) {
            return DispenseOutcome::acted(ItemStack::empty());
        }
        DispenseOutcome::acted(stack)
    }
}

/// Returns whether this block is something a flame can be set to.
///
/// Vanilla parity: the `CampfireBlock.canLight`, `CandleBlock.canLight` and
/// `CandleCakeBlock.canLight` chain, which all come down to an unlit block that
/// carries the lit property and is not standing in water.
fn can_be_lit(state: BlockStateId) -> bool {
    let unlit = state.try_get_value(LIT) == Some(false);
    let dry = state.try_get_value(WATERLOGGED) != Some(true);
    unlit && dry
}

/// Shears whatever is standing in front of the dispenser.
///
/// Vanilla parity: `ShearsDispenseItemBehavior`.
///
/// TODO: vanilla also shears a full beehive and carves a pumpkin; Foton has the
/// beehive block but no honeycomb-dropping path to reuse here yet.
struct ShearsDispenseBehavior;

impl DispenseItemBehavior for ShearsDispenseBehavior {
    fn execute(&self, source: &DispenseSource<'_>, mut stack: ItemStack) -> DispenseOutcome {
        let target = source.pos.relative(source.facing);
        let box_at_target = WorldAabb::new(
            f64::from(target.x()),
            f64::from(target.y()),
            f64::from(target.z()),
            f64::from(target.x()) + 1.0,
            f64::from(target.y()) + 1.0,
            f64::from(target.z()) + 1.0,
        );

        let sheared = source
            .world
            .get_entities_in_aabb(&box_at_target)
            .into_iter()
            .find_map(|entity| {
                let sheep = entity.as_ref().downcast_ref::<SheepEntity>()?;
                sheep.ready_for_shearing().then(|| {
                    sheep.shear(source.world.as_ref(), &stack);
                })
            })
            .is_some();

        if !sheared {
            return DispenseOutcome::Failed(stack);
        }

        if stack.hurt_and_break(1, false) {
            return DispenseOutcome::acted(ItemStack::empty());
        }
        DispenseOutcome::acted(stack)
    }
}

/// Puts a piece of equipment on whatever is standing in front.
///
/// Vanilla parity: `EquipmentDispenseItemBehavior`. This is how an armour
/// dispenser works, and how a saddle goes onto a pig without a player -- it is
/// not in the registry at all, it is `getDefaultDispenseMethod`'s answer for
/// anything carrying an `equippable` component.
struct EquipmentDispenseBehavior;

impl DispenseItemBehavior for EquipmentDispenseBehavior {
    fn execute(&self, source: &DispenseSource<'_>, mut stack: ItemStack) -> DispenseOutcome {
        if dispense_equipment(source, &mut stack) {
            return DispenseOutcome::acted(stack);
        }
        DefaultDispenseBehavior.execute(source, stack)
    }
}

/// Equips the first thing in front of the dispenser that will take `stack`.
///
/// Vanilla parity: `EquipmentDispenseItemBehavior.dispenseEquipment`, which
/// several registered behaviors fall back to as well. Returns whether anything
/// took it.
pub(super) fn dispense_equipment(source: &DispenseSource<'_>, stack: &mut ItemStack) -> bool {
    let target = source.pos.relative(source.facing);
    let Some(equippable) = stack.get_equippable() else {
        return false;
    };
    let slot = equippable.slot;

    let candidates = source.world.get_entities_in_aabb(&block_aabb(target));
    let Some(wearer) = candidates.iter().find_map(|entity| {
        let living = entity.as_living_entity()?;
        living.can_equip_with_dispenser(stack).then_some(living)
    }) else {
        return false;
    };

    let equipped = stack.split(1);
    wearer.set_item_slot(slot, equipped);
    if let Some(mob) = wearer.as_mob() {
        mob.set_guaranteed_drop(slot);
        mob.set_persistence_required();
    }
    true
}

/// Puts the mob a spawn egg makes in front of the dispenser.
///
/// Vanilla parity: `SpawnEggItemBehavior`, reached through
/// `DispenserBlock.getDefaultDispenseMethod` rather than the registry.
struct SpawnEggDispenseBehavior;

impl DispenseItemBehavior for SpawnEggDispenseBehavior {
    fn execute(&self, source: &DispenseSource<'_>, mut stack: ItemStack) -> DispenseOutcome {
        let Some(entity_type) = SpawnEggItem::entity_type(&stack) else {
            return DispenseOutcome::Failed(stack);
        };
        let target = source.pos.relative(source.facing);

        // Vanilla's `type.spawn(..., tryMoveDown = direction != Direction.UP,
        // movedUp = false)`; Foton's spawn helper has no downward search yet, so
        // an egg fired at a ceiling lands in the block it was aimed at.
        if SpawnEggItem::spawn_at(source.world, entity_type, target).is_none() {
            return DispenseOutcome::Failed(stack);
        }

        source.world.game_event(
            &vanilla_game_events::ENTITY_PLACE,
            source.pos,
            &GameEventContext::new(None, None),
        );
        stack.shrink(1);
        DispenseOutcome::acted(stack)
    }
}

/// Offers a block to a sulfur cube standing in front, and throws it otherwise.
///
/// Vanilla parity: `SulfurCubeBlockDispenseItemBehavior`, the second arm of
/// `DispenserBlock.getDefaultDispenseMethod`.
struct SulfurCubeDispenseBehavior;

impl DispenseItemBehavior for SulfurCubeDispenseBehavior {
    fn execute(&self, source: &DispenseSource<'_>, mut stack: ItemStack) -> DispenseOutcome {
        if feed_sulfur_cube(source.world, source.pos.relative(source.facing), &mut stack) {
            return DispenseOutcome::acted(stack);
        }
        DefaultDispenseBehavior.execute(source, stack)
    }
}

/// Empties a bucket that carries something in front of the dispenser.
///
/// Vanilla parity: the anonymous `filledBucketBehavior` of
/// `DispenseItemBehavior.bootStrap`, shared by every bucket with contents --
/// water, lava, powder snow and all six mob buckets. This is the half of a
/// redstone farm a dispenser could not do at all: `emptyContents` takes a
/// nullable user precisely so that this call site can pass nothing.
struct FilledBucketDispenseBehavior;

impl DispenseItemBehavior for FilledBucketDispenseBehavior {
    fn execute(&self, source: &DispenseSource<'_>, stack: ItemStack) -> DispenseOutcome {
        let behavior = ITEM_BEHAVIORS.get_behavior(stack.item());
        let Some(container) = behavior.as_dispensible_container() else {
            return DefaultDispenseBehavior.execute(source, stack);
        };

        let target = source.pos.relative(source.facing);
        if !container.empty_contents(None, source.world, target, None) {
            return DefaultDispenseBehavior.execute(source, stack);
        }

        // Vanilla reads the stack here, before `consumeWithRemainder` shrinks
        // it, which is how the fish keeps the name the bucket was carrying.
        container.check_extra_content(None, source.world, &stack, target);
        DispenseOutcome::acted(consume_with_remainder(
            source,
            stack,
            ItemStack::new(&vanilla_items::BUCKET),
        ))
    }
}

/// Fills an empty bucket from whatever is in front of the dispenser.
///
/// Vanilla parity: the `Items.BUCKET` entry of
/// `DispenseItemBehavior.bootStrap`. Nothing here plays the pickup sound: the
/// hand-held path does, but the dispenser fires only the game event.
struct EmptyBucketDispenseBehavior;

impl DispenseItemBehavior for EmptyBucketDispenseBehavior {
    fn execute(&self, source: &DispenseSource<'_>, stack: ItemStack) -> DispenseOutcome {
        let target = source.pos.relative(source.facing);
        let state = source.world.get_block_state(target);
        let behavior = BLOCK_BEHAVIORS.get_behavior(state.get_block());

        // The waterlogging fallback is the same one the hand-held empty bucket
        // uses; without it a dispenser could not drain a block a player can.
        let picked = behavior
            .pickup_block(source.world, target, state, None)
            .or_else(|| pickup_waterlogged_block(behavior, source.world, target, state, None));
        let Some(result) = picked.filter(|result| !result.filled_bucket.is_empty()) else {
            return DefaultDispenseBehavior.execute(source, stack);
        };

        source.world.game_event(
            &vanilla_game_events::FLUID_PICKUP,
            target,
            &GameEventContext::new(None, None),
        );
        DispenseOutcome::acted(consume_with_remainder(source, stack, result.filled_bucket))
    }
}

/// Spends one item and finds a home for what it leaves behind.
///
/// Vanilla parity: `DefaultDispenseItemBehavior.consumeWithRemainder`. The
/// remainder only takes the dispensed slot when that slot is now empty;
/// otherwise it goes into the block's own inventory, and out onto the floor
/// when there is no room, with a second clack and puff of smoke.
fn consume_with_remainder(
    source: &DispenseSource<'_>,
    mut dispensed: ItemStack,
    remainder: ItemStack,
) -> ItemStack {
    dispensed.shrink(1);
    if dispensed.is_empty() {
        return remainder;
    }

    // Vanilla parity: `addToInventoryOrDispense`.
    let leftover = source.block_entity.insert_item(remainder);
    if !leftover.is_empty() {
        spawn_dispensed_item(source.world, source.pos, source.facing, leftover);
        play_dispense_effects(source.world, source.pos, source.facing);
    }
    dispensed
}

/// Throws one item out of the dispenser.
///
/// Vanilla parity: `DefaultDispenseItemBehavior`, which is what an item with
/// nothing else to say gets.
pub(super) struct DefaultDispenseBehavior;

impl DispenseItemBehavior for DefaultDispenseBehavior {
    fn execute(&self, source: &DispenseSource<'_>, mut stack: ItemStack) -> DispenseOutcome {
        let thrown = stack.split(1);
        spawn_dispensed_item(source.world, source.pos, source.facing, thrown);
        DispenseOutcome::acted(stack)
    }
}

/// The one-block box in front of the dispenser that its behaviors act in.
fn block_aabb(pos: BlockPos) -> WorldAabb {
    WorldAabb::new(
        f64::from(pos.x()),
        f64::from(pos.y()),
        f64::from(pos.z()),
        f64::from(pos.x()) + 1.0,
        f64::from(pos.y()) + 1.0,
        f64::from(pos.z()) + 1.0,
    )
}

/// Every item the dispenser treats specially.
///
/// Vanilla parity: `DispenserBlock.DISPENSER_REGISTRY`, filled by
/// `DispenseItemBehavior.bootStrap`. Foton covers the entries it has the systems
/// for; the rest fall through to the default throw, which is also what vanilla
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
            vanilla_items::FIRE_CHARGE.key.clone(),
            Box::new(FireChargeDispenseBehavior),
        );
        behaviors.insert(
            vanilla_items::TNT.key.clone(),
            Box::new(TntDispenseBehavior),
        );
        behaviors.insert(
            vanilla_items::BONE_MEAL.key.clone(),
            Box::new(BoneMealDispenseBehavior),
        );
        behaviors.insert(
            vanilla_items::FLINT_AND_STEEL.key.clone(),
            Box::new(FlintAndSteelDispenseBehavior),
        );
        behaviors.insert(
            vanilla_items::SHEARS.key.clone(),
            Box::new(ShearsDispenseBehavior),
        );
        for bucket in filled_buckets() {
            behaviors.insert(bucket.key.clone(), Box::new(FilledBucketDispenseBehavior));
        }
        behaviors.insert(
            vanilla_items::BUCKET.key.clone(),
            Box::new(EmptyBucketDispenseBehavior),
        );
        behaviors
    });

/// Every bucket that carries something.
///
/// Vanilla parity: the ten `registerBehavior(.., filledBucketBehavior)` lines of
/// `DispenseItemBehavior.bootStrap`, which is also the full list of vanilla
/// items implementing `DispensibleContainerItem`.
fn filled_buckets() -> [ItemRef; 10] {
    [
        &vanilla_items::LAVA_BUCKET,
        &vanilla_items::WATER_BUCKET,
        &vanilla_items::POWDER_SNOW_BUCKET,
        &vanilla_items::SALMON_BUCKET,
        &vanilla_items::COD_BUCKET,
        &vanilla_items::PUFFERFISH_BUCKET,
        &vanilla_items::TROPICAL_FISH_BUCKET,
        &vanilla_items::AXOLOTL_BUCKET,
        &vanilla_items::SULFUR_CUBE_BUCKET,
        &vanilla_items::TADPOLE_BUCKET,
    ]
}

/// Feeds a block to a sulfur cube standing in front of the dispenser.
///
/// Vanilla parity: `SulfurCubeBlockDispenseItemBehavior.dispenseBlock`, which
/// is how a dispenser loads a cube -- it is checked before the ordinary
/// behavior, so a dispenser aimed at a cube feeds it rather than throwing.
/// Returns whether a cube took the block.
pub(super) fn feed_sulfur_cube(world: &Arc<World>, pos: BlockPos, stack: &mut ItemStack) -> bool {
    let bounds = WorldAabb::new(
        f64::from(pos.x()),
        f64::from(pos.y()),
        f64::from(pos.z()),
        f64::from(pos.x()) + 1.0,
        f64::from(pos.y()) + 1.0,
        f64::from(pos.z()) + 1.0,
    );
    for entity in world.get_entities_in_aabb(&bounds) {
        let Some(cube) = entity.downcast_ref::<SulfurCubeEntity>() else {
            continue;
        };
        if cube.equip_item(stack) {
            stack.shrink(1);
            return true;
        }
    }
    false
}

/// Returns whether a sulfur cube would swallow this item.
///
/// Vanilla parity: the `itemStack.is(ItemTags.SULFUR_CUBE_SWALLOWABLE)` branch
/// of `DispenserBlock.getDefaultDispenseMethod`.
#[must_use]
pub(super) fn is_sulfur_cube_swallowable(item: ItemRef) -> bool {
    REGISTRY
        .items
        .is_in_tag(item, &ItemTag::SULFUR_CUBE_SWALLOWABLE)
}

/// Returns what the dispenser should do with `stack`.
///
/// Vanilla parity: `DispenserBlock.getDispenseMethod` -- the registry first,
/// then `getDefaultDispenseMethod` for the three things a dispenser knows how
/// to do without an entry of their own.
#[must_use]
pub fn dispense_behavior_for(stack: &ItemStack) -> &'static dyn DispenseItemBehavior {
    if let Some(registered) = DISPENSE_BEHAVIORS.get(&stack.item().key) {
        return registered.as_ref();
    }
    default_dispense_behavior_for(stack)
}

/// Returns the behavior an item with no registry entry gets.
///
/// Vanilla parity: `DispenserBlock.getDefaultDispenseMethod`, in its order:
/// equippable first, then a block a sulfur cube would swallow, then a spawn
/// egg, and a plain throw for everything else.
#[must_use]
fn default_dispense_behavior_for(stack: &ItemStack) -> &'static dyn DispenseItemBehavior {
    if stack.get_equippable().is_some() {
        return &EquipmentDispenseBehavior;
    }
    if is_sulfur_cube_swallowable(stack.item()) {
        return &SulfurCubeDispenseBehavior;
    }
    if SpawnEggItem::entity_type(stack).is_some() {
        return &SpawnEggDispenseBehavior;
    }
    &DefaultDispenseBehavior
}

#[cfg(test)]
mod tests {
    use foton_registry::blocks::BlockRef;
    use foton_registry::init_vanilla_registry;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    /// The nine slots a `DispenseSource` needs behind it.
    fn dispenser_at(world: &Arc<World>, pos: BlockPos) -> DispenserBlockEntity {
        DispenserBlockEntity::new(
            Arc::downgrade(world),
            pos,
            vanilla_blocks::DISPENSER.default_state(),
        )
    }

    #[test]
    fn every_item_foton_handles_specially_is_registered() {
        init_vanilla_registry();
        for item in [
            &vanilla_items::ARROW,
            &vanilla_items::TNT,
            &vanilla_items::BONE_MEAL,
            &vanilla_items::FLINT_AND_STEEL,
            &vanilla_items::SHEARS,
            &vanilla_items::FIRE_CHARGE,
        ] {
            assert!(
                DISPENSE_BEHAVIORS.contains_key(&item.key),
                "{} should have a dispense behavior",
                item.key
            );
        }
    }

    /// Vanilla parity: `CampfireBlock.canLight` and friends only light a block
    /// that is unlit and out of the water.
    #[test]
    fn only_an_unlit_dry_block_can_be_lit() {
        init_vanilla_registry();
        init_behaviors();
        let campfire = vanilla_blocks::CAMPFIRE.default_state();

        assert!(!can_be_lit(campfire), "a campfire starts lit");
        assert!(can_be_lit(campfire.set_value(LIT, false)));
        assert!(!can_be_lit(
            campfire.set_value(LIT, false).set_value(WATERLOGGED, true)
        ));
        assert!(
            !can_be_lit(vanilla_blocks::STONE.default_state()),
            "stone has no lit property to set"
        );
    }

    /// Vanilla parity: `SulfurCubeBlockDispenseItemBehavior.dispenseBlock`,
    /// reached through the sulfur-cube branch of
    /// `DispenserBlock.getDefaultDispenseMethod`. A dispenser aimed at a cube
    /// loads it instead of throwing the block on the floor.
    #[test]
    fn a_dispenser_aimed_at_a_sulfur_cube_feeds_it() {
        use std::sync::Arc;

        use foton_registry::vanilla_entities;
        use glam::DVec3;

        use crate::entity::entities::SulfurCubeEntity;
        use crate::entity::{LivingEntity as _, next_entity_id};
        use crate::inventory::equipment::EquipmentSlot;

        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("dispenser_feeds_sulfur_cube");
        insert_ready_full_chunk(&world, foton_utils::ChunkPos::new(0, 0));
        let target = BlockPos::new(8, 64, 8);

        let cube = Arc::new(SulfurCubeEntity::new(
            &vanilla_entities::SULFUR_CUBE,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Arc::downgrade(&world),
        ));
        cube.set_cube_size(2, true);
        world
            .try_add_entity(cube.clone())
            .expect("the test world accepts a sulfur cube");

        assert!(is_sulfur_cube_swallowable(&vanilla_items::TNT));
        let mut stack = ItemStack::new(&vanilla_items::TNT);
        stack.set_count(3);
        assert!(feed_sulfur_cube(&world, target, &mut stack));
        assert_eq!(stack.count(), 2, "the dispenser gave up exactly one");
        assert!(
            cube.get_item_by_slot(EquipmentSlot::Body)
                .is(&vanilla_items::TNT)
        );

        assert!(
            !feed_sulfur_cube(&world, target, &mut stack),
            "a cube already holding that block takes no more"
        );
    }

    /// A dispensed fire charge is shot, not spat onto the floor.
    ///
    /// Vanilla registers `Items.FIRE_CHARGE` through
    /// `DispenserBlock.registerProjectileBehavior` (`DispenseItemBehavior.bootStrap`).
    /// With no entry it fell through to `DefaultDispenseBehavior`, which is an
    /// item entity on the ground -- exactly what the report described.
    #[test]
    fn a_dispenser_shoots_a_fire_charge_rather_than_dropping_it() {
        use foton_registry::vanilla_entities;

        use crate::entity::entities::ItemEntity;

        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("dispenser_shoots_fire_charge");
        insert_ready_full_chunk(&world, foton_utils::ChunkPos::new(0, 0));
        let pos = BlockPos::new(8, 64, 8);
        let block_entity = dispenser_at(&world, pos);
        let source = DispenseSource {
            world: &world,
            pos,
            facing: Direction::East,
            block_entity: &block_entity,
        };

        let mut stack = ItemStack::new(&vanilla_items::FIRE_CHARGE);
        stack.set_count(2);
        let outcome = dispense_behavior_for(&stack).execute(&source, stack);

        match outcome {
            DispenseOutcome::Acted {
                remainder,
                sound_override,
            } => {
                assert_eq!(remainder.count(), 1, "one charge left the dispenser");
                assert_eq!(
                    sound_override,
                    Some(level_events::SOUND_BLAZE_FIREBALL),
                    "a dispensed fire charge roars rather than clicking"
                );
            }
            DispenseOutcome::Failed(_) => panic!("the fire charge was not dispensed"),
        }

        let search =
            WorldAabb::from_min_max(DVec3::new(4.0, 60.0, 4.0), DVec3::new(13.0, 68.0, 13.0));
        let fireballs = world.get_entities_in_aabb_matching(&search, |entity| {
            entity.entity_type() == &vanilla_entities::SMALL_FIREBALL
        });
        assert_eq!(fireballs.len(), 1, "a small fireball should have been shot");
        assert!(
            fireballs[0].velocity().x > 0.0,
            "it should be travelling the way the dispenser faces"
        );
        assert!(
            world
                .get_entities_in_aabb_matching(&search, |entity| entity
                    .downcast_ref::<ItemEntity>()
                    .is_some())
                .is_empty(),
            "nothing should have been dropped on the floor"
        );
    }

    /// Vanilla parity: an item with no entry takes the default throw, so the
    /// registry must not answer for it.
    #[test]
    fn an_unregistered_item_has_no_behavior() {
        init_vanilla_registry();
        assert!(!DISPENSE_BEHAVIORS.contains_key(&vanilla_items::STONE.key));
    }

    /// A saddle fired at a pig lands on the pig.
    ///
    /// `LivingEntity::can_equip_with_dispenser` had been written and tested,
    /// but nothing in the server ever called it: the dispenser had no
    /// `getDefaultDispenseMethod`, so every piece of armour and every saddle
    /// was simply thrown on the floor.
    #[test]
    fn a_dispenser_aimed_at_a_pig_saddles_it() {
        use std::sync::Arc;

        use foton_registry::vanilla_entities;
        use glam::DVec3;

        use crate::entity::entities::PigEntity;
        use crate::entity::{LivingEntity as _, Mob as _, next_entity_id};
        use crate::inventory::equipment::EquipmentSlot;

        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("dispenser_saddles_a_pig");
        insert_ready_full_chunk(&world, foton_utils::ChunkPos::new(0, 0));

        let pig = Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(Arc::clone(&pig) as Arc<dyn Entity>)
            .expect("the test world accepts a pig");

        let block_entity = dispenser_at(&world, BlockPos::new(8, 64, 7));
        let source = DispenseSource {
            world: &world,
            pos: BlockPos::new(8, 64, 7),
            facing: Direction::South,
            block_entity: &block_entity,
        };
        let mut stack = ItemStack::new(&vanilla_items::SADDLE);
        stack.set_count(2);

        // Through `dispense_behavior_for`, so the routing is under test too --
        // a saddle has no registry entry and only reaches the equipment
        // behavior through `getDefaultDispenseMethod`.
        let DispenseOutcome::Acted { remainder, .. } =
            dispense_behavior_for(&stack).execute(&source, stack)
        else {
            panic!("a saddle aimed at a bare pig should go on the pig");
        };
        assert_eq!(
            remainder.count(),
            1,
            "the dispenser gave up exactly one saddle"
        );
        assert!(
            pig.get_item_by_slot(EquipmentSlot::Saddle)
                .is(&vanilla_items::SADDLE)
        );
        assert!(
            pig.is_persistence_required(),
            "a mob a dispenser dressed stops despawning, saddle and all"
        );

        let mut second = remainder;
        assert!(
            !dispense_equipment(&source, &mut second),
            "a pig that already has its saddle takes no second one"
        );
    }

    /// A spawn egg fired out of a dispenser makes its mob rather than landing
    /// on the floor as an item.
    #[test]
    fn a_dispensed_spawn_egg_makes_its_mob() {
        use crate::entity::init_entities;

        init_vanilla_registry();
        init_behaviors();
        init_entities();
        let world = fresh_test_world("dispenser_spawn_egg");
        insert_ready_full_chunk(&world, foton_utils::ChunkPos::new(0, 0));

        let block_entity = dispenser_at(&world, BlockPos::new(8, 64, 7));
        let source = DispenseSource {
            world: &world,
            pos: BlockPos::new(8, 64, 7),
            facing: Direction::South,
            block_entity: &block_entity,
        };
        let mut stack = ItemStack::new(&vanilla_items::COW_SPAWN_EGG);
        stack.set_count(2);

        let DispenseOutcome::Acted { remainder, .. } =
            dispense_behavior_for(&stack).execute(&source, stack)
        else {
            panic!("a cow egg aimed at open air should make a cow");
        };
        assert_eq!(remainder.count(), 1, "one egg is spent");

        let cows = world.get_entities_in_aabb_matching(&block_aabb(BlockPos::new(8, 64, 8)), |e| {
            e.entity_type() == &vanilla_entities::COW
        });
        assert_eq!(cows.len(), 1, "exactly one cow, in front of the dispenser");
    }

    /// Vanilla parity: `getDispensePosition`, which places things in front of the
    /// face rather than inside the block.
    #[test]
    fn the_dispense_position_sits_in_front_of_the_face() {
        let world = fresh_test_world("dispense_position");
        let block_entity = dispenser_at(&world, BlockPos::new(10, 64, 10));
        let source = DispenseSource {
            world: &world,
            pos: BlockPos::new(10, 64, 10),
            facing: Direction::East,
            block_entity: &block_entity,
        };

        let position = source.dispense_position(DISPENSE_OFFSET, DVec3::ZERO);

        assert!((position.x - 11.2).abs() < 1e-9, "x was {}", position.x);
        assert!((position.y - 64.5).abs() < 1e-9);
        assert!((position.z - 10.5).abs() < 1e-9);
    }

    /// The dispenser and the block in front of it, in a chunk that is loaded.
    struct BucketBench {
        world: Arc<World>,
        block_entity: DispenserBlockEntity,
        pos: BlockPos,
        target: BlockPos,
    }

    impl BucketBench {
        fn new(key: &'static str) -> Self {
            init_vanilla_registry();
            init_behaviors();
            let world = fresh_test_world(key);
            insert_ready_full_chunk(&world, foton_utils::ChunkPos::new(0, 0));
            let pos = BlockPos::new(8, 64, 7);
            let block_entity = dispenser_at(&world, pos);
            Self {
                world,
                block_entity,
                pos,
                target: BlockPos::new(8, 64, 8),
            }
        }

        fn source(&self) -> DispenseSource<'_> {
            DispenseSource {
                world: &self.world,
                pos: self.pos,
                facing: Direction::South,
                block_entity: &self.block_entity,
            }
        }

        /// Runs the whole routing, not just the behavior, so a bucket that
        /// stopped being registered fails here too.
        fn dispense(&self, stack: ItemStack) -> ItemStack {
            let source = self.source();
            match dispense_behavior_for(&stack).execute(&source, stack) {
                DispenseOutcome::Acted { remainder, .. } => remainder,
                DispenseOutcome::Failed(_) => panic!("the dispenser refused the bucket"),
            }
        }

        fn block_in_front(&self) -> BlockRef {
            self.world.get_block_state(self.target).get_block()
        }
    }

    /// Vanilla parity: the `filledBucketBehavior` of
    /// `DispenseItemBehavior.bootStrap`. A dispenser that cannot place water is
    /// half the redstone farms in the game; Foton threw the bucket on the floor.
    #[test]
    fn a_dispensed_water_bucket_places_water_and_keeps_the_empty_bucket() {
        let bench = BucketBench::new("dispenser_water_bucket");
        assert!(
            bench.block_in_front() == &vanilla_blocks::AIR,
            "the bench starts with nothing in front of the dispenser"
        );

        let remainder = bench.dispense(ItemStack::new(&vanilla_items::WATER_BUCKET));

        assert_eq!(bench.block_in_front(), &vanilla_blocks::WATER);
        assert!(
            remainder.is(&vanilla_items::BUCKET),
            "the slot keeps what the bucket became"
        );
        assert_eq!(remainder.count(), 1);
    }

    /// Vanilla parity: `DefaultDispenseItemBehavior.consumeWithRemainder`, which
    /// only hands the remainder back through the dispensed slot once that slot
    /// has emptied; anything left over goes into the block's own inventory.
    ///
    /// No bucket ever reaches the second arm -- every filled bucket stacks to
    /// one, so shrinking it always empties the slot -- but the next behavior
    /// that hands back a remainder will, and this is the only thing that calls
    /// `DispenserBlockEntity::insert_item` at all.
    #[test]
    fn a_remainder_goes_into_the_block_when_the_slot_is_still_full() {
        init_vanilla_registry();
        let world = fresh_test_world("dispenser_consume_with_remainder");
        let pos = BlockPos::new(8, 64, 7);
        let block_entity = dispenser_at(&world, pos);
        block_entity.set_item(0, ItemStack::with_count(&vanilla_items::STONE, 3));
        let source = DispenseSource {
            world: &world,
            pos,
            facing: Direction::South,
            block_entity: &block_entity,
        };

        let kept = consume_with_remainder(
            &source,
            block_entity.get_item(0),
            ItemStack::new(&vanilla_items::DIRT),
        );

        assert!(kept.is(&vanilla_items::STONE) && kept.count() == 2);
        assert!(
            block_entity.get_item(1).is(&vanilla_items::DIRT),
            "the remainder went back into the block, not onto the floor"
        );
    }

    /// Vanilla parity: the `Items.BUCKET` entry of
    /// `DispenseItemBehavior.bootStrap`, the other half of a water farm.
    #[test]
    fn a_dispenser_aimed_at_water_fills_its_empty_bucket() {
        let bench = BucketBench::new("dispenser_empty_bucket");
        assert!(
            bench.world.set_block(
                bench.target,
                vanilla_blocks::WATER.default_state(),
                UpdateFlags::UPDATE_ALL,
            ),
            "the water the dispenser is meant to drink has to be there"
        );

        let remainder = bench.dispense(ItemStack::new(&vanilla_items::BUCKET));

        assert!(
            bench.block_in_front() == &vanilla_blocks::AIR,
            "the source block is gone"
        );
        assert!(remainder.is(&vanilla_items::WATER_BUCKET));
    }

    /// Vanilla parity: `SolidBucketItem.emptyContents`. Powder snow is a block,
    /// not a fluid, and reaches the same registered behavior.
    #[test]
    fn a_dispensed_powder_snow_bucket_lays_powder_snow() {
        let bench = BucketBench::new("dispenser_powder_snow_bucket");

        let remainder = bench.dispense(ItemStack::new(&vanilla_items::POWDER_SNOW_BUCKET));

        assert_eq!(bench.block_in_front(), &vanilla_blocks::POWDER_SNOW);
        assert!(remainder.is(&vanilla_items::BUCKET));
    }

    /// Vanilla parity: `MobBucketItem.checkExtraContent`, which the dispense
    /// behavior calls right after `emptyContents`. Without it the water lands
    /// and the fish stays in a bucket that no longer exists.
    #[test]
    fn a_dispensed_axolotl_bucket_lets_the_axolotl_out() {
        use crate::entity::init_entities;

        let bench = BucketBench::new("dispenser_axolotl_bucket");
        init_entities();

        let remainder = bench.dispense(ItemStack::new(&vanilla_items::AXOLOTL_BUCKET));

        assert_eq!(bench.block_in_front(), &vanilla_blocks::WATER);
        assert!(remainder.is(&vanilla_items::BUCKET));
        let axolotls = bench
            .world
            .get_entities_in_aabb_matching(&block_aabb(bench.target), |entity| {
                entity.entity_type() == &vanilla_entities::AXOLOTL
            });
        assert_eq!(
            axolotls.len(),
            1,
            "exactly one axolotl, where the water went"
        );
    }

    /// Vanilla parity: the `defaultDispenseItemBehavior.dispense` arm of the
    /// filled-bucket behavior. A bucket with nowhere to empty is thrown, and the
    /// dispenser does not quietly swallow it.
    #[test]
    fn a_water_bucket_with_nowhere_to_go_is_thrown_instead() {
        let bench = BucketBench::new("dispenser_blocked_water_bucket");
        assert!(bench.world.set_block(
            bench.target,
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_ALL,
        ));

        let remainder = bench.dispense(ItemStack::new(&vanilla_items::WATER_BUCKET));

        assert!(
            bench.block_in_front() == &vanilla_blocks::STONE,
            "the stone is untouched"
        );
        assert!(remainder.is_empty(), "the only bucket left the block");
        let around_the_face = WorldAabb::new(6.0, 62.0, 6.0, 11.0, 67.0, 11.0);
        let thrown = bench
            .world
            .get_entities_in_aabb_matching(&around_the_face, |entity| {
                entity.entity_type() == &vanilla_entities::ITEM
            });
        assert_eq!(thrown.len(), 1, "the bucket is on the floor, not destroyed");
    }
}
