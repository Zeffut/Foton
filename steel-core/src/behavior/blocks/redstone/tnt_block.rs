//! TNT block behavior.
//!
//! Vanilla parity: `TntBlock`. A redstone signal, flint and steel or a fire
//! charge turns the block into a [`PrimedTntEntity`] that detonates once its
//! fuse burns out.
//!
//! TODO: also light TNT from a projectile on fire and a neighboring explosion,
//! matching `TntBlock.onCaughtFire`, `onProjectileHit` and `wasExploded`.

use std::sync::Arc;

use steel_macros::block_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::vanilla_game_rules::TNT_EXPLODES;
use steel_registry::{sound_events, vanilla_blocks, vanilla_items};
use steel_utils::{
    BlockPos, BlockStateId,
    types::{InteractionHand, UpdateFlags},
};

use crate::behavior::InventoryAccess;
use crate::behavior::block::BlockBehavior;
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::entity::Entity as _;
use crate::entity::entities::PrimedTntEntity;
use crate::player::Player;
use crate::world::{SignalGetter as _, World};

/// What the player used to light the TNT.
enum Igniter {
    /// Wears out by one point of durability.
    FlintAndSteel,
    /// Is consumed.
    FireCharge,
}

/// Behavior for the TNT block.
#[block_behavior]
pub struct TntBlock {
    block: BlockRef,
}

impl TntBlock {
    /// Creates a new TNT block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Turns the block at `pos` into primed TNT.
    ///
    /// Vanilla parity: `TntBlock.prime`. Returns false when the `tntExplodes`
    /// game rule is off, which leaves the block untouched.
    pub fn prime(world: &Arc<World>, pos: BlockPos, source_id: Option<i32>) -> bool {
        if !world.get_game_rule(&TNT_EXPLODES) {
            return false;
        }

        let entity = PrimedTntEntity::prime(world, pos, source_id);
        world.play_sound(
            &sound_events::ENTITY_TNT_PRIMED,
            SoundSource::Blocks,
            pos,
            1.0,
            1.0,
            None,
        );
        drop(entity);
        true
    }

    /// Primes and clears the block when a redstone signal reaches it.
    fn prime_if_powered(world: &Arc<World>, pos: BlockPos) {
        if world.has_neighbor_signal(pos) && Self::prime(world, pos, None) {
            world.set_block(
                pos,
                vanilla_blocks::AIR.default_state(),
                UpdateFlags::UPDATE_ALL,
            );
        }
    }
}

impl BlockBehavior for TntBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn on_place(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        if old_state.get_block() == self.block {
            return;
        }
        Self::prime_if_powered(world, pos);
    }

    /// Lights the TNT with flint and steel or a fire charge.
    ///
    /// Vanilla parity: `TntBlock.useItemOn`.
    fn use_item_on(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        hand: InteractionHand,
        _hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let igniter = inv.with_item(|stack| {
            if stack.is(&vanilla_items::FLINT_AND_STEEL) {
                Some(Igniter::FlintAndSteel)
            } else if stack.is(&vanilla_items::FIRE_CHARGE) {
                Some(Igniter::FireCharge)
            } else {
                None
            }
        });
        let Some(igniter) = igniter else {
            return InteractionResult::TryEmptyHandInteraction;
        };

        if !Self::prime(world, pos, Some(player.id())) {
            return InteractionResult::Pass;
        }
        world.set_block(
            pos,
            vanilla_blocks::AIR.default_state(),
            UpdateFlags::UPDATE_ALL,
        );

        let infinite = player.has_infinite_materials();
        match igniter {
            // Flint and steel wears out; a fire charge is spent.
            Igniter::FlintAndSteel => {
                inv.with_inventory(|inventory| {
                    inventory.hurt_item_in_hand(hand, 1, infinite);
                });
            }
            Igniter::FireCharge => {
                if !infinite {
                    inv.with_item(|stack| {
                        let remaining = stack.count() - 1;
                        stack.set_count(remaining);
                    });
                }
            }
        }

        InteractionResult::Success
    }

    fn handle_neighbor_changed(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source_block: BlockRef,
        _moved_by_piston: bool,
    ) {
        Self::prime_if_powered(world, pos);
    }
}
