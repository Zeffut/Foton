//! Wind charge item.
//!
//! Vanilla parity: `WindChargeItem`. Throwing one spawns a [`WindChargeEntity`]
//! from the thrower's eye along their look. The half-second cooldown that keeps
//! a stack from being emptied in one burst comes from the item's own
//! `minecraft:use_cooldown` component, not from anything written here.

use std::sync::Arc;

use foton_macros::item_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::{sound_events, vanilla_entities};
use glam::DVec3;

use crate::behavior::context::{InteractionResult, UseItemContext};
use crate::behavior::item::ItemBehavior;
use crate::entity::entities::WindChargeEntity;
use crate::entity::{Entity, Projectile, SharedEntity, next_entity_id};

/// How hard a wind charge is thrown.
///
/// Vanilla parity: `WindChargeItem.PROJECTILE_SHOOT_POWER`.
const SHOOT_POWER: f32 = 1.5;

/// How wide the throw scatters.
///
/// Vanilla parity: the `1.0F` uncertainty of the
/// `Projectile.spawnProjectileFromRotation` call in `WindChargeItem.use`.
const SHOOT_UNCERTAINTY: f32 = 1.0;

/// Behavior for the wind charge item.
#[item_behavior]
pub struct WindChargeItem;

impl ItemBehavior for WindChargeItem {
    /// Vanilla parity: `WindChargeItem.use`.
    ///
    /// Vanilla returns `InteractionResult.SUCCESS` and the caller then applies
    /// the `Item.Properties.useCooldown(0.5F)` the item was registered with.
    /// Foton does the same: `use_item` applies the stack's
    /// `minecraft:use_cooldown` for any result that
    /// `should_apply_item_use_side_effects`, so the half second is data, not
    /// code, and the only thing owed here is the `Success`.
    ///
    /// Not implemented: the dispenser path. Vanilla's `WindChargeItem` is a
    /// `ProjectileItem` whose `asProjectile` and `createDispenseConfig` let a
    /// dispenser fire one, with its own triangular spread and the 1051 dispense
    /// event; Foton has no `ProjectileItem` dispense hook to hang that on.
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let player = context.player;
        let world = context.world;

        // Vanilla parity: the deliberately jittery pitch of `WindChargeItem.use`.
        // Vanilla plays this after spawning but unconditionally, so it is played
        // first here rather than being lost to a spawn Foton could not complete.
        let sound_pitch = 0.4 / rand::random::<f32>().mul_add(0.4, 0.8);
        world.play_sound_at(
            &sound_events::ENTITY_WIND_CHARGE_THROW,
            SoundSource::Neutral,
            player.position(),
            0.5,
            sound_pitch,
            None,
        );

        // Vanilla parity: `WindChargeItem.use` spawns at the thrower's eye
        // height over their feet's horizontal position, and unlike a snowball
        // it does not drop that spawn by 0.1.
        let player_pos = player.position();
        let charge = Arc::new(WindChargeEntity::new(
            &vanilla_entities::WIND_CHARGE,
            next_entity_id(),
            DVec3::new(player_pos.x, player.get_eye_y(), player_pos.z),
            Arc::downgrade(world),
        ));

        if let Some(owner) = world.players.get_by_uuid(&player.gameprofile.id) {
            let owner: SharedEntity = owner;
            charge.set_owner_entity(Some(&owner));
        } else {
            charge.set_owner_uuid(Some(player.gameprofile.id));
        }

        let (yaw, pitch) = player.rotation();
        charge.shoot_from_rotation(player, pitch, yaw, 0.0, SHOOT_POWER, SHOOT_UNCERTAINTY);

        let charge: SharedEntity = charge;
        if let Err(error) = world.try_add_entity(charge) {
            log::debug!("failed to spawn wind charge: {error}");
            return InteractionResult::Fail;
        }

        // TODO: award the ITEM_USED stat once a stats system exists.
        if !player.has_infinite_materials() {
            context.inv.with_item(|item| item.shrink(1));
        }

        InteractionResult::Success
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::item_stack::ItemStack;
    use foton_registry::{init_vanilla_registry, vanilla_items};
    use foton_utils::types::InteractionHand;
    use foton_utils::{ChunkPos, WorldAabb};

    use crate::behavior::init_behaviors;
    use crate::player::game_mode::use_item;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

    use super::*;

    #[test]
    fn throwing_a_wind_charge_sends_one_out_at_eye_height_and_spends_it() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("wind_charge_item_throw");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let player =
            TestPlayerBuilder::new(Arc::clone(&world), "Thrower", next_entity_id()).build();
        player
            .inventory
            .lock()
            .set_selected_item(ItemStack::with_count(&vanilla_items::WIND_CHARGE, 3));

        let result = use_item(&player, &world, InteractionHand::MainHand);

        assert_eq!(result, InteractionResult::Success);
        let thrown = world
            .get_entities_in_aabb(&WorldAabb::new(-8.0, -8.0, -8.0, 8.0, 24.0, 8.0))
            .into_iter()
            .find(|entity| entity.entity_type() == &vanilla_entities::WIND_CHARGE)
            .expect("using a wind charge should put one in the world");

        assert!((thrown.position().y - player.get_eye_y()).abs() < 1.0e-9);
        assert_eq!(thrown.projectile_owner_uuid(), Some(player.gameprofile.id));
        assert!(
            thrown.velocity().length() > 0.0,
            "the charge should leave the hand already moving"
        );
        assert_eq!(player.inventory.lock().get_selected_item_mut().count, 2);
    }

    #[test]
    fn throwing_a_wind_charge_puts_the_stack_on_its_half_second_cooldown() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("wind_charge_item_cooldown");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let player =
            TestPlayerBuilder::new(Arc::clone(&world), "Thrower", next_entity_id()).build();
        let stack = ItemStack::with_count(&vanilla_items::WIND_CHARGE, 2);
        player.inventory.lock().set_selected_item(stack.clone());

        assert!(!player.is_item_on_cooldown(&stack));
        assert_eq!(
            use_item(&player, &world, InteractionHand::MainHand),
            InteractionResult::Success
        );

        // Vanilla parity: `Item.Properties.useCooldown(0.5F)` on the registered
        // item, which the use path applies for a successful result.
        assert!(player.is_item_on_cooldown(&stack));
        assert_eq!(
            use_item(&player, &world, InteractionHand::MainHand),
            InteractionResult::Pass,
            "a second throw is refused while the cooldown runs"
        );
    }
}
