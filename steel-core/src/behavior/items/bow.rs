//! Bow item behavior.
//!
//! Vanilla parity: `BowItem`. Holding right-click draws the bow; releasing it
//! fires an arrow whose speed depends on how long the string was held.

use std::sync::Arc;

use steel_macros::item_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_events;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::vanilla_items;
use steel_registry::{REGISTRY, TaggedRegistryExt as _};

use crate::behavior::context::{InteractionResult, UseItemContext};
use crate::behavior::item::ItemBehavior;
use crate::behavior::items::arrow_entity_type_for;
use crate::enchantment_helper;
use crate::entity::LivingEntity;
use crate::entity::entities::{ArrowEntity, ArrowPickup};
use crate::inventory::container::Container as _;
use crate::inventory::equipment::EquipmentSlot;
use crate::player::Player;
use crate::player::player_inventory::PlayerInventory;
use crate::world::World;

/// Ticks needed for a fully drawn bow.
///
/// Vanilla parity: `BowItem.MAX_DRAW_DURATION`.
const MAX_DRAW_DURATION: f32 = 20.0;

/// Ticks the bow stays usable while held, effectively forever.
const BOW_USE_DURATION: i32 = 72_000;

/// Draw strength below which the shot is cancelled.
const MINIMUM_POWER: f32 = 0.1;

/// Speed multiplier applied to a fully drawn shot.
const SHOT_POWER_SCALE: f32 = 3.0;

/// Spread applied to the shot direction.
const SHOT_UNCERTAINTY: f32 = 1.0;

/// Draw strength at which the shot becomes critical.
///
/// Vanilla parity: the `pow == 1.0F` argument `BowItem.releaseUsing` hands to
/// `shoot`.
const FULL_DRAW_POWER: f32 = 1.0;

/// Returns the first inventory slot holding ammunition a bow accepts.
///
/// Vanilla parity: `Player.getProjectile` against
/// `BowItem.getAllSupportedProjectiles`, which is the `#minecraft:arrows` tag.
fn find_arrow_slot(player: &Player) -> Option<usize> {
    let inventory = player.inventory.lock();
    (0..PlayerInventory::INVENTORY_SIZE).find(|slot| {
        let item = inventory.get_item(*slot);
        !item.is_empty() && REGISTRY.items.is_in_tag(item.item(), &ItemTag::ARROWS)
    })
}

/// Returns whether the player carries at least one arrow.
fn has_arrow(player: &Player) -> bool {
    find_arrow_slot(player).is_some()
}

/// Returns a single-item copy of the ammunition the bow would draw.
fn held_arrow(player: &Player) -> Option<ItemStack> {
    let slot = find_arrow_slot(player)?;
    let inventory = player.inventory.lock();
    Some(inventory.get_item(slot).copy_with_count(1))
}

/// Removes one arrow from the player's inventory and returns what was taken.
fn take_one_arrow(player: &Player) -> Option<ItemStack> {
    let slot = find_arrow_slot(player)?;
    let mut inventory = player.inventory.lock();

    let taken = {
        let stack = inventory.get_item(slot);
        stack.copy_with_count(1)
    };
    let remaining = inventory.get_item(slot).count() - 1;
    if remaining <= 0 {
        inventory.set_item(slot, ItemStack::empty());
    } else {
        let mut stack = inventory.get_item(slot).clone();
        stack.set_count(remaining);
        inventory.set_item(slot, stack);
    }
    Some(taken)
}

/// How far a mob will stand off and still fire a bow.
///
/// Vanilla parity: `BowItem.getDefaultProjectileRange`.
const BOW_PROJECTILE_RANGE: i32 = 15;

/// Behavior for the bow.
#[item_behavior]
pub struct BowItem;

impl BowItem {
    /// Creates a new bow behavior.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Converts a draw time into a shot strength between 0 and 1.
    ///
    /// Vanilla parity: `BowItem.getPowerForTime`. The curve is deliberately not
    /// linear: the last few ticks of the draw add far more speed than the first.
    #[must_use]
    pub fn power_for_time(time_held: i32) -> f32 {
        let drawn = time_held as f32 / MAX_DRAW_DURATION;
        drawn.mul_add(drawn, drawn * 2.0).min(3.0) / 3.0
    }
}

impl Default for BowItem {
    fn default() -> Self {
        Self::new()
    }
}

impl ItemBehavior for BowItem {
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        // Vanilla parity: `BowItem.use` refuses to draw without ammunition.
        if !context.player.has_infinite_materials() && !has_arrow(context.player) {
            return InteractionResult::Fail;
        }
        context.player.start_using_item(context.hand);
        InteractionResult::Consume
    }

    /// Vanilla parity: `BowItem.getDefaultProjectileRange`, nearly twice a
    /// crossbow's -- which is why a skeleton opens fire from much further off
    /// than a piglin does.
    fn default_projectile_range(&self) -> Option<i32> {
        Some(BOW_PROJECTILE_RANGE)
    }

    fn get_use_duration(&self, _stack: &ItemStack, _user: &dyn LivingEntity) -> i32 {
        BOW_USE_DURATION
    }

    /// Fires the arrow when the string is released.
    ///
    /// Vanilla parity: `BowItem.releaseUsing`, and the `ProjectileWeaponItem`
    /// halves it leans on -- `draw`, `useAmmo`, `createProjectile` and `shoot`.
    fn release_using(
        &self,
        stack: &mut ItemStack,
        world: &Arc<World>,
        user: &dyn LivingEntity,
        time_left: i32,
    ) -> bool {
        let Some(player) = user.as_player() else {
            return false;
        };

        let time_held = BOW_USE_DURATION - time_left;
        let power = Self::power_for_time(time_held);
        if power < MINIMUM_POWER {
            return false;
        }

        // Vanilla parity: `Player.getProjectile` falls back to a conjured arrow
        // for a player who does not pay for their ammunition.
        let infinite = player.has_infinite_materials();
        let Some(ammo) =
            held_arrow(player).or_else(|| infinite.then(|| ItemStack::new(&vanilla_items::ARROW)))
        else {
            return false;
        };

        // Vanilla parity: `ProjectileWeaponItem.useAmmo`. Infinity sets the
        // `ammo_use` count to zero for a plain arrow, and a free draw leaves an
        // arrow only its shooter's own kind of player can collect.
        let ammo_to_use = if infinite {
            0
        } else {
            enchantment_helper::process_ammo_use(stack, &ammo, 1)
        };
        if ammo_to_use > ammo.count() {
            return false;
        }
        let free_shot = ammo_to_use == 0;
        if !free_shot && take_one_arrow(player).is_none() {
            return false;
        }

        let arrow = ArrowEntity::shoot_from(
            world,
            user,
            arrow_entity_type_for(&ammo),
            power * SHOT_POWER_SCALE,
            SHOT_UNCERTAINTY,
        );
        // Vanilla parity: `ProjectileWeaponItem.createProjectile` marks a fully
        // drawn shot critical and hands the arrow the weapon it came off, which
        // is what Power and Punch are read from when it lands.
        arrow.set_crit_arrow(power >= FULL_DRAW_POWER);
        arrow.set_fired_from_weapon(Some(stack.copy_with_count(stack.count())));
        if free_shot {
            arrow.set_pickup(ArrowPickup::CreativeOnly);
        }

        // Vanilla parity: `Projectile.applyOnProjectileSpawned` runs the
        // ammunition's enchantments and then the weapon's -- which is where
        // Flame sets the arrow alight.
        let mut spent_ammo = ammo.copy_with_count(1);
        enchantment_helper::on_projectile_spawned(
            world,
            &mut spent_ammo,
            arrow.as_ref(),
            Some(user.as_entity_event_source()),
        );
        enchantment_helper::on_projectile_spawned(
            world,
            stack,
            arrow.as_ref(),
            Some(user.as_entity_event_source()),
        );

        world.play_sound_at(
            &sound_events::ENTITY_ARROW_SHOOT,
            SoundSource::Players,
            user.position(),
            1.0,
            power.mul_add(0.5, 1.0 / 0.4f32.mul_add(rand::random::<f32>(), 1.2)),
            None,
        );

        // Vanilla parity: the `weapon.hurtAndBreak` of `ProjectileWeaponItem.shoot`.
        // `LivingEntity.releaseUsingItem` has already taken the bow out of the
        // hand, so damaging the -- now empty -- hand slot instead would be a
        // no-op, and the bow would come back undamaged.
        if stack.hurt_and_break(1, infinite)
            && let Some(hand) = player.active_item_use_hand()
        {
            user.on_equipped_item_broken(EquipmentSlot::for_hand(hand));
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_power_matches_the_vanilla_curve() {
        // Vanilla: pow = t/20; pow = (pow*pow + pow*2)/3, capped at 1.
        assert!((BowItem::power_for_time(0) - 0.0).abs() < f32::EPSILON);
        assert!((BowItem::power_for_time(20) - 1.0).abs() < f32::EPSILON);

        // A half-drawn bow is far weaker than half strength: 0.5 gives 0.4166...
        let half = BowItem::power_for_time(10);
        assert!((half - 0.416_666_66).abs() < 1e-5, "got {half}");
        assert!(half < 0.5, "the draw curve must not be linear");
    }

    #[test]
    fn a_full_draw_never_exceeds_full_power() {
        assert!((BowItem::power_for_time(40) - 1.0).abs() < f32::EPSILON);
        assert!((BowItem::power_for_time(72_000) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_tap_is_below_the_firing_threshold() {
        assert!(BowItem::power_for_time(1) < MINIMUM_POWER);
        assert!(BowItem::power_for_time(3) >= MINIMUM_POWER);
    }
}

/// Firing a bow, entered where the client enters it: a `ReleaseUseItem` player
/// action after the string has been held.
#[cfg(test)]
mod firing_tests {
    use std::sync::Arc;

    use glam::DVec3;
    use steel_protocol::packets::game::{PlayerAction, SPlayerAction};
    use steel_registry::data_components::vanilla_components::{ENCHANTMENTS, ItemEnchantments};
    use steel_registry::item_stack::ItemStack;
    use steel_registry::items::ItemRef;
    use steel_registry::{
        init_vanilla_registry, vanilla_enchantments, vanilla_entities, vanilla_items,
    };
    use steel_utils::types::InteractionHand;
    use steel_utils::{BlockPos, ChunkPos, Direction, Downcast as _, Identifier, WorldAabb};

    use crate::behavior::init_behaviors;
    use crate::entity::entities::{ArrowEntity, ArrowPickup, PigEntity};
    use crate::entity::init_entities;
    use crate::entity::{Entity, LivingEntity as _, SharedEntity, next_entity_id};
    use crate::inventory::container::Container as _;
    use crate::player::Player;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};
    use crate::world::World;

    use super::{BOW_USE_DURATION, MAX_DRAW_DURATION};

    fn bow_world(key: &'static str) -> Arc<World> {
        init_vanilla_registry();
        init_behaviors();
        init_entities();
        let world = fresh_test_world(key);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        world
    }

    fn enchanted(item: ItemRef, enchantment: &Identifier, level: u32) -> ItemStack {
        let mut levels = ItemEnchantments::empty();
        levels.set(enchantment.clone(), level);
        let mut stack = ItemStack::new(item);
        stack.set(ENCHANTMENTS, levels);
        stack
    }

    /// Arms an archer with `bow` and one arrow, and holds the string for
    /// `draw_ticks`.
    fn archer(world: &Arc<World>, bow: ItemStack, arrows: i32) -> Arc<Player> {
        let player = TestPlayerBuilder::new(Arc::clone(world), "Archer", next_entity_id()).build();
        {
            let mut inventory = player.inventory.lock();
            inventory.set_item_in_hand(InteractionHand::MainHand, bow);
            if arrows > 0 {
                let mut quiver = ItemStack::new(&vanilla_items::ARROW);
                quiver.set_count(arrows);
                inventory.set_item(9, quiver);
            }
        }
        player
    }

    /// Draws and looses, coming in through the packet the client sends.
    fn loose(player: &Arc<Player>, draw_ticks: i32) {
        player.start_using_item(InteractionHand::MainHand);
        for _ in 0..draw_ticks {
            player.updating_using_item();
        }
        player.handle_player_action(SPlayerAction {
            action: PlayerAction::ReleaseUseItem,
            pos: BlockPos::new(0, 0, 0),
            direction: Direction::Down,
            sequence: 0,
        });
    }

    /// Runs `visit` against the one arrow the bow put in the world.
    fn with_loosed_arrow<R>(world: &Arc<World>, visit: impl FnOnce(&ArrowEntity) -> R) -> R {
        let everywhere = WorldAabb::new(-256.0, -64.0, -256.0, 256.0, 320.0, 256.0);
        let arrows = world.get_entities_in_aabb_matching(&everywhere, |entity| {
            entity.downcast_ref::<ArrowEntity>().is_some()
        });
        assert_eq!(
            arrows.len(),
            1,
            "the bow should have put exactly one arrow in the world"
        );
        let arrow = arrows[0]
            .as_ref()
            .downcast_ref::<ArrowEntity>()
            .expect("filtered above");
        visit(arrow)
    }

    #[test]
    fn a_fully_drawn_bow_looses_a_critical_arrow() {
        let world = bow_world("bow_full_draw_is_critical");
        let player = archer(&world, ItemStack::new(&vanilla_items::BOW), 1);

        loose(&player, MAX_DRAW_DURATION as i32);

        assert!(
            with_loosed_arrow(&world, ArrowEntity::is_crit_arrow),
            "a bow held to full draw fires a critical arrow"
        );
    }

    #[test]
    fn a_half_drawn_bow_does_not() {
        let world = bow_world("bow_half_draw_is_not_critical");
        let player = archer(&world, ItemStack::new(&vanilla_items::BOW), 1);

        loose(&player, MAX_DRAW_DURATION as i32 / 2);

        assert!(
            !with_loosed_arrow(&world, ArrowEntity::is_crit_arrow),
            "only a full draw is critical"
        );
    }

    #[test]
    fn the_arrow_remembers_the_bow_that_fired_it() {
        let world = bow_world("bow_arrow_remembers_weapon");
        let bow = enchanted(&vanilla_items::BOW, &vanilla_enchantments::POWER.key, 3);
        let player = archer(&world, bow, 1);

        loose(&player, MAX_DRAW_DURATION as i32);

        let weapon = with_loosed_arrow(&world, ArrowEntity::weapon_item)
            .expect("the arrow should carry the bow it came off");
        assert!(
            weapon.is(&vanilla_items::BOW),
            "Power and Punch are read off this stack when the arrow lands"
        );
    }

    #[test]
    fn a_flame_bow_looses_a_burning_arrow() {
        let world = bow_world("bow_flame_ignites_the_arrow");
        let bow = enchanted(&vanilla_items::BOW, &vanilla_enchantments::FLAME.key, 1);
        let player = archer(&world, bow, 1);

        loose(&player, MAX_DRAW_DURATION as i32);

        assert!(
            with_loosed_arrow(&world, Entity::remaining_fire_ticks) > 0,
            "Flame sets the arrow alight as it spawns"
        );
    }

    #[test]
    fn a_plain_bow_looses_a_cold_arrow() {
        let world = bow_world("bow_without_flame_is_cold");
        let player = archer(&world, ItemStack::new(&vanilla_items::BOW), 1);

        loose(&player, MAX_DRAW_DURATION as i32);

        assert_eq!(with_loosed_arrow(&world, Entity::remaining_fire_ticks), 0);
    }

    #[test]
    fn infinity_keeps_the_arrow_in_the_quiver() {
        let world = bow_world("bow_infinity_keeps_the_arrow");
        let bow = enchanted(&vanilla_items::BOW, &vanilla_enchantments::INFINITY.key, 1);
        let player = archer(&world, bow, 1);

        loose(&player, MAX_DRAW_DURATION as i32);

        assert_eq!(
            player.inventory.lock().get_item(9).count(),
            1,
            "an Infinity bow spends no ammunition"
        );
        assert_eq!(
            with_loosed_arrow(&world, ArrowEntity::pickup),
            ArrowPickup::CreativeOnly,
            "and the arrow it leaves cannot be picked back up for free"
        );
    }

    #[test]
    fn a_plain_bow_spends_its_arrow() {
        let world = bow_world("bow_spends_its_arrow");
        let player = archer(&world, ItemStack::new(&vanilla_items::BOW), 1);

        loose(&player, MAX_DRAW_DURATION as i32);

        assert!(
            player.inventory.lock().get_item(9).is_empty(),
            "a plain bow takes the arrow out of the quiver"
        );
        assert_eq!(
            with_loosed_arrow(&world, ArrowEntity::pickup),
            ArrowPickup::Allowed
        );
    }

    #[test]
    fn firing_wears_the_bow_down() {
        let world = bow_world("bow_loses_durability");
        let player = archer(&world, ItemStack::new(&vanilla_items::BOW), 4);

        loose(&player, MAX_DRAW_DURATION as i32);

        let damage = player
            .inventory
            .lock()
            .get_item_in_hand(InteractionHand::MainHand)
            .get_damage_value();
        assert_eq!(damage, 1, "every shot costs the bow a point of durability");
    }

    #[test]
    fn a_tap_fires_nothing() {
        let world = bow_world("bow_tap_fires_nothing");
        let player = archer(&world, ItemStack::new(&vanilla_items::BOW), 1);

        loose(&player, 1);

        assert_eq!(
            player.inventory.lock().get_item(9).count(),
            1,
            "a shot below the minimum draw keeps its arrow"
        );
    }

    /// Power is read off the weapon the arrow carries, at the moment it lands.
    #[test]
    fn power_makes_the_arrow_hit_harder() {
        let world = bow_world("bow_power_hits_harder");

        let plain = shoot_at_a_pig(&world, 8.5, None);
        let powered = shoot_at_a_pig(
            &world,
            12.5,
            Some(enchanted(
                &vanilla_items::BOW,
                &vanilla_enchantments::POWER.key,
                5,
            )),
        );

        assert!(
            powered > plain,
            "Power V should beat a bare arrow's {plain}, but it dealt {powered}"
        );
    }

    /// Punch is read off the same weapon, and shows up as a shove.
    #[test]
    fn punch_shoves_the_target_further() {
        let world = bow_world("bow_punch_shoves");

        let plain = shove_a_pig(&world, 8.5, None);
        let punched = shove_a_pig(
            &world,
            12.5,
            Some(enchanted(
                &vanilla_items::BOW,
                &vanilla_enchantments::PUNCH.key,
                2,
            )),
        );

        assert!(
            punched > plain,
            "Punch II should shove harder than a bare arrow's {plain},              but it managed {punched}"
        );
    }

    /// Fires one arrow down a lane at a pig and reports how far it was shoved.
    fn shove_a_pig(world: &Arc<World>, z: f64, weapon: Option<ItemStack>) -> f64 {
        let (pig, arrow) = lane(world, z, weapon);
        for _ in 0..40 {
            Entity::tick(arrow.as_ref());
            if arrow.is_removed() || arrow.is_in_ground() {
                break;
            }
        }
        let pushed = pig.velocity();
        pushed.x.hypot(pushed.z)
    }

    /// Fires one arrow down a lane at a pig and reports the damage it did.
    fn shoot_at_a_pig(world: &Arc<World>, z: f64, weapon: Option<ItemStack>) -> f32 {
        let (pig, arrow) = lane(world, z, weapon);
        let before = pig.get_health();
        for _ in 0..40 {
            Entity::tick(arrow.as_ref());
            if arrow.is_removed() || arrow.is_in_ground() {
                break;
            }
        }
        before - pig.get_health()
    }

    /// Sets a pig two blocks down a lane with an arrow already pointed at it.
    fn lane(
        world: &Arc<World>,
        z: f64,
        weapon: Option<ItemStack>,
    ) -> (Arc<PigEntity>, Arc<ArrowEntity>) {
        let pig = Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            DVec3::new(10.5, 64.0, z),
            Arc::downgrade(world),
        ));
        world
            .try_add_entity(Arc::clone(&pig) as SharedEntity)
            .expect("the test chunk is loaded");

        let arrow = Arc::new(ArrowEntity::new(
            &vanilla_entities::ARROW,
            next_entity_id(),
            DVec3::new(8.5, 64.9, z),
            Arc::downgrade(world),
        ));
        arrow.set_velocity(DVec3::new(1.0, 0.0, 0.0));
        arrow.set_fired_from_weapon(weapon);
        world
            .try_add_entity(Arc::clone(&arrow) as SharedEntity)
            .expect("the test chunk is loaded");
        (pig, arrow)
    }

    #[test]
    fn the_use_duration_is_effectively_endless() {
        assert!(BOW_USE_DURATION > MAX_DRAW_DURATION as i32);
    }
}
