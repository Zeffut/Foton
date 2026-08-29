//! Trident item behavior.
//!
//! Vanilla parity: `TridentItem`. Holding right-click winds the trident up;
//! releasing it either throws the trident as a projectile or, with riptide and
//! standing in water or rain, hurls the player instead.

use std::sync::Arc;

use foton_macros::item_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_events;
use glam::DVec3;

use crate::behavior::context::{InteractionResult, UseItemContext};
use crate::behavior::item::ItemBehavior;
use crate::enchantment_helper;
use crate::entity::entities::{ThrownTridentEntity, TridentPickup};
use crate::entity::{Entity, LivingEntity};
use crate::physics::MoverType;
use crate::world::World;
use foton_registry::stat::Stat;
use foton_registry::vanilla_stat_types;

/// Ticks the trident must be held before the throw is allowed.
///
/// Vanilla parity: `TridentItem.THROW_THRESHOLD_TIME`.
const THROW_THRESHOLD_TIME: i32 = 10;

/// Speed the thrown trident leaves the hand at.
///
/// Vanilla parity: `TridentItem.PROJECTILE_SHOOT_POWER`.
const PROJECTILE_SHOOT_POWER: f32 = 2.5;

/// Spread applied to the throw.
///
/// Vanilla parity: the `1.0F` inaccuracy passed to `spawnProjectileFromRotation`.
const THROW_UNCERTAINTY: f32 = 1.0;

/// Ticks the trident stays usable while held, effectively forever.
///
/// Vanilla parity: `TridentItem.getUseDuration`.
const TRIDENT_USE_DURATION: i32 = 72_000;

/// Height a riptide launch adds when it starts from the ground.
///
/// Vanilla parity: the `1.1999999F` step in `TridentItem.releaseUsing`, which
/// lifts the player clear of the block they were standing on.
const RIPTIDE_GROUND_LIFT: f64 = 1.199_999_9;

/// Behavior for the trident.
#[item_behavior]
pub struct TridentItem;

impl TridentItem {
    /// Creates a new trident behavior.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Returns the riptide launch impulse for a look direction and strength.
    ///
    /// Vanilla parity: the riptide branch of `TridentItem.releaseUsing`. Vanilla
    /// builds the look vector by hand and then renormalizes it to `strength`,
    /// which is what this reproduces.
    #[must_use]
    pub fn riptide_impulse(yaw: f32, pitch: f32, strength: f32) -> DVec3 {
        let yaw = yaw.to_radians();
        let pitch = pitch.to_radians();
        let direction = DVec3::new(
            f64::from(-yaw.sin() * pitch.cos()),
            f64::from(-pitch.sin()),
            f64::from(yaw.cos() * pitch.cos()),
        );
        let length = direction.length();
        if length == 0.0 {
            return DVec3::ZERO;
        }
        direction * (f64::from(strength) / length)
    }
}

impl Default for TridentItem {
    fn default() -> Self {
        Self::new()
    }
}

impl ItemBehavior for TridentItem {
    /// Vanilla parity: `TridentItem.use`.
    ///
    /// A trident one hit from breaking refuses to wind up, and a riptide trident
    /// refuses unless the player is wet -- there is nothing for it to launch off.
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let (about_to_break, riptide_strength) = context.inv.with_item(|item| {
            (
                item.next_damage_will_break(),
                enchantment_helper::get_trident_spin_attack_strength(item),
            )
        });
        if about_to_break {
            return InteractionResult::Fail;
        }
        if riptide_strength > 0.0 && !context.player.is_in_water_or_rain() {
            return InteractionResult::Fail;
        }

        context.player.start_using_item(context.hand);
        InteractionResult::Consume
    }

    fn get_use_duration(&self, _stack: &ItemStack, _user: &dyn LivingEntity) -> i32 {
        TRIDENT_USE_DURATION
    }

    /// Throws the trident, or rides it, when the wind-up is released.
    ///
    /// Vanilla parity: `TridentItem.releaseUsing`.
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

        let time_held = TRIDENT_USE_DURATION - time_left;
        if time_held < THROW_THRESHOLD_TIME {
            return false;
        }

        let riptide_strength = enchantment_helper::get_trident_spin_attack_strength(stack);
        // A riptide trident only does anything at all with something to swim
        // through, and never while riding.
        let riptide_can_launch = player.is_in_water_or_rain() && !player.is_passenger();
        if riptide_strength > 0.0 && !riptide_can_launch {
            return false;
        }
        if stack.next_damage_will_break() {
            return false;
        }

        let sound = enchantment_helper::pick_trident_sound(stack)
            .unwrap_or(&sound_events::ITEM_TRIDENT_THROW);
        player.award_stat(Stat::new(&vanilla_stat_types::USED, stack.item));

        let infinite = player.has_infinite_materials();
        // Vanilla `hurtWithoutBreaking`, which is why the caller already refused
        // a trident whose next point of damage would destroy it.
        let _ = stack.hurt_and_break(1, infinite);

        if riptide_strength == 0.0 {
            let thrown = stack.copy_with_count(1);
            if !infinite {
                stack.shrink(1);
            }

            let trident = ThrownTridentEntity::throw_from(
                world,
                player,
                &thrown,
                PROJECTILE_SHOOT_POWER,
                THROW_UNCERTAINTY,
            );
            if infinite {
                trident.set_pickup(TridentPickup::CreativeOnly);
            }

            world.play_sound_at(
                sound,
                SoundSource::Players,
                trident.position(),
                1.0,
                1.0,
                None,
            );
            return true;
        }

        let (yaw, pitch) = player.rotation();
        player.push_impulse(Self::riptide_impulse(yaw, pitch, riptide_strength));
        // Not ported: `LivingEntity.startAutoSpinAttack(20, 8.0F, stack)`, which
        // drives the spin-attack pose, the 20-tick `autoSpinAttackTicks` travel
        // override and the damage the spinning player deals on contact. Foton has
        // no auto-spin-attack state, so the launch happens without the spin.
        if player.on_ground() {
            let _ = player.move_entity(
                MoverType::SelfMovement,
                DVec3::new(0.0, RIPTIDE_GROUND_LIFT, 0.0),
            );
        }

        world.play_sound_at(
            sound,
            SoundSource::Players,
            player.position(),
            1.0,
            1.0,
            None,
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::{
        init_vanilla_registry, vanilla_blocks, vanilla_enchantments, vanilla_entities,
        vanilla_items,
    };
    use foton_utils::types::{InteractionHand, UpdateFlags};
    use foton_utils::{BlockPos, ChunkPos, WorldAabb};

    use crate::behavior::init_behaviors;
    use crate::player::Player;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

    use super::*;

    /// Mid-chunk on purpose: the fluid scan reads the block ring around the
    /// entity, and every chunk it touches has to be loaded.
    const STANDING_ON: DVec3 = DVec3::new(8.5, 64.0, 8.5);

    fn trident_stack(riptide: u32) -> ItemStack {
        let mut stack = ItemStack::new(&vanilla_items::TRIDENT);
        if riptide > 0 {
            stack.set_enchantments(
                &[(vanilla_enchantments::RIPTIDE.key.clone(), riptide)],
                true,
            );
        }
        stack
    }

    fn world_and_thrower(key: &'static str) -> (Arc<World>, Arc<Player>) {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world(key);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let player = TestPlayerBuilder::new(Arc::clone(&world), "TridentThrower", 1).build();
        player.base().set_position_local(STANDING_ON);
        (world, player)
    }

    fn flood_the_thrower(world: &Arc<World>, player: &Arc<Player>) {
        assert!(world.set_block(
            BlockPos::new(8, 64, 8),
            vanilla_blocks::WATER.default_state(),
            UpdateFlags::UPDATE_ALL,
        ));
        player.refresh_fluid_contact();
        assert!(
            player.is_in_water_or_rain(),
            "the fixture must actually put the thrower in water"
        );
    }

    fn use_result(world: &Arc<World>, player: &Arc<Player>, stack: ItemStack) -> InteractionResult {
        player
            .inventory
            .lock()
            .set_item_in_hand(InteractionHand::MainHand, stack);
        let mut context = UseItemContext::new(
            player,
            InteractionHand::MainHand,
            world,
            Arc::clone(&player.inventory),
        );
        TridentItem::new().use_item(&mut context)
    }

    fn tridents_in_flight(world: &Arc<World>) -> usize {
        world
            .get_entities_in_aabb(&WorldAabb::new(-16.0, 0.0, -16.0, 16.0, 128.0, 16.0))
            .iter()
            .filter(|entity| entity.entity_type() == &vanilla_entities::TRIDENT)
            .count()
    }

    #[test]
    fn a_riptide_launch_follows_the_look_direction_at_the_enchantment_strength() {
        // Riptide I is 1.5, and each level adds 0.75 (`RIPTIDE` level-based value).
        let straight_ahead = TridentItem::riptide_impulse(0.0, 0.0, 1.5);
        assert!((straight_ahead.z - 1.5).abs() < 1.0e-6);
        assert!(straight_ahead.x.abs() < 1.0e-6 && straight_ahead.y.abs() < 1.0e-6);

        // Looking straight up throws the player straight up.
        let upward = TridentItem::riptide_impulse(0.0, -90.0, 3.0);
        assert!((upward.y - 3.0).abs() < 1.0e-6);

        // The impulse is always exactly the enchantment's strength long.
        let diagonal = TridentItem::riptide_impulse(35.0, -20.0, 2.25);
        assert!((diagonal.length() - 2.25).abs() < 1.0e-6);
    }

    #[test]
    fn a_trident_one_hit_from_breaking_will_not_wind_up() {
        let (world, player) = world_and_thrower("trident_use_near_broken");
        let mut stack = trident_stack(0);
        stack.set_damage_value(stack.get_max_damage() - 1);

        assert_eq!(use_result(&world, &player, stack), InteractionResult::Fail);
    }

    #[test]
    fn a_riptide_trident_only_winds_up_when_the_thrower_is_wet() {
        let (world, player) = world_and_thrower("trident_use_riptide");

        assert_eq!(
            use_result(&world, &player, trident_stack(2)),
            InteractionResult::Fail,
            "riptide has nothing to launch off on dry land"
        );

        flood_the_thrower(&world, &player);
        assert_eq!(
            use_result(&world, &player, trident_stack(2)),
            InteractionResult::Consume
        );
    }

    #[test]
    fn a_plain_trident_winds_up_anywhere() {
        let (world, player) = world_and_thrower("trident_use_plain");

        assert_eq!(
            use_result(&world, &player, trident_stack(0)),
            InteractionResult::Consume
        );
    }

    #[test]
    fn releasing_a_wound_up_trident_throws_it_and_empties_the_hand() {
        let (world, player) = world_and_thrower("trident_release_throw");
        let mut stack = trident_stack(0);

        let thrown = TridentItem::new().release_using(
            &mut stack,
            &world,
            player.as_ref(),
            TRIDENT_USE_DURATION - 20,
        );

        assert!(thrown);
        assert!(stack.is_empty(), "the thrown trident leaves the hand");
        assert_eq!(tridents_in_flight(&world), 1);
    }

    #[test]
    fn a_tap_is_too_short_to_throw() {
        let (world, player) = world_and_thrower("trident_release_tap");
        let mut stack = trident_stack(0);

        let thrown = TridentItem::new().release_using(
            &mut stack,
            &world,
            player.as_ref(),
            TRIDENT_USE_DURATION - (THROW_THRESHOLD_TIME - 1),
        );

        assert!(!thrown);
        assert_eq!(stack.count(), 1);
        assert_eq!(
            stack.get_damage_value(),
            0,
            "a cancelled throw costs nothing"
        );
        assert_eq!(tridents_in_flight(&world), 0);
    }

    #[test]
    fn releasing_a_riptide_trident_launches_the_thrower_instead_of_the_trident() {
        let (world, player) = world_and_thrower("trident_release_riptide");
        flood_the_thrower(&world, &player);
        let mut stack = trident_stack(1);
        let before = player.velocity();

        let launched = TridentItem::new().release_using(
            &mut stack,
            &world,
            player.as_ref(),
            TRIDENT_USE_DURATION - 20,
        );

        assert!(launched);
        assert_eq!(
            tridents_in_flight(&world),
            0,
            "riptide keeps the trident in hand"
        );
        assert_eq!(stack.count(), 1);
        assert!(
            player.velocity() != before,
            "riptide hurls the thrower along their look"
        );
        assert_eq!(
            stack.get_damage_value(),
            1,
            "vanilla still spends a point of durability on the ride"
        );
    }
}
