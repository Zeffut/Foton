//! Bucket item behavior implementation.
//!
//! Handles water buckets, lava buckets, and empty buckets.
//!
//! Mirrors vanilla's `BucketItem(Fluid fluid)`: `fluid_block = None` = empty bucket,
//! `Some(block)` = filled bucket. Logic is dispatched in `use_item`.
//!
use std::sync::Arc;

use crate::behavior::context::InteractionResult;
use crate::behavior::item_utils::{create_filled_result, player_pov_hit_source_fluid};
use crate::behavior::{
    BLOCK_BEHAVIORS, BlockStateBehaviorExt, BucketHit, DispensibleContainerItem, FLUID_BEHAVIORS,
    ItemBehavior, UseItemContext, pickup_waterlogged_block,
};
use crate::entity::Entity;
use crate::event::Event as _;
use crate::fluid::FluidStateExt;
use crate::player::Player;
use crate::world::{RaytraceAction, World};
use foton_macros::item_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::BlockRef;
use foton_registry::blocks::block_state_ext::BlockStateExt;
use foton_registry::blocks::properties::Direction;
use foton_registry::fluid::FluidState;
use foton_registry::item_stack::ItemStack;
use foton_registry::level_events;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::sound_events;
use foton_registry::vanilla_blocks;
use foton_registry::vanilla_fluids;
use foton_registry::vanilla_game_events;
use foton_registry::vanilla_items;
use foton_utils::types::UpdateFlags;
use foton_utils::{BlockPos, BlockStateId};

use crate::event::{PlayerBucketEmptyEvent, PlayerBucketFillEvent};
use crate::world::game_event::GameEventContext;

/// Handles all bucket variants (empty, water, lava).
#[item_behavior]
pub struct BucketItem {
    #[json_arg(vanilla_blocks, json = "content", optional = "empty")]
    fluid_block: Option<BlockRef>,
}

impl BucketItem {
    /// Creates a new bucket behavior. `None` = empty bucket, `Some(block)` = filled.
    #[must_use]
    pub const fn new(fluid_block: Option<BlockRef>) -> Self {
        Self { fluid_block }
    }
}

impl ItemBehavior for BucketItem {
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        if self.fluid_block.is_none() {
            return use_empty_bucket(context);
        }
        use_filled_bucket(self, self.fluid_block, context)
    }

    fn as_dispensible_container(&self) -> Option<&dyn DispensibleContainerItem> {
        Some(self)
    }
}

impl DispensibleContainerItem for BucketItem {
    fn empty_contents(
        &self,
        user: Option<&Player>,
        world: &Arc<World>,
        pos: BlockPos,
        hit: Option<BucketHit>,
    ) -> bool {
        // Vanilla parity: the `!(this.content instanceof FlowingFluid)` guard,
        // which is what makes an empty bucket empty nothing.
        let Some(fluid_block) = self.fluid_block else {
            return false;
        };
        empty_contents(fluid_block, user, world, pos, hit, EmptySound::Fluid)
    }
}

/// Which sound and side effects emptying a bucket produces.
///
/// Vanilla parity: `BucketItem.playEmptySound`, which `MobBucketItem` replaces
/// with a neutral-category sound and no `FLUID_PLACE` game event.
#[derive(Clone, Copy)]
pub(super) enum EmptySound {
    /// The fluid sounds of a plain water or lava bucket.
    Fluid,
    /// A mob bucket's own emptying sound.
    Mob(SoundEventRef),
}

pub(super) fn filled_bucket_success_stack(context: &UseItemContext) -> ItemStack {
    if context.player.has_infinite_materials() {
        context
            .inv
            .with_item(|item| item.copy_with_count(item.count()))
    } else {
        ItemStack::new(&vanilla_items::BUCKET)
    }
}

fn use_empty_bucket(context: &mut UseItemContext) -> InteractionResult {
    // Vanilla parity: `getPlayerPOVHitResult(.., ClipContext.Fluid.SOURCE_ONLY)`.
    // Vanilla returns PASS when the clip misses (allows other handlers to try).
    let Some(hit_pos) = player_pov_hit_source_fluid(context) else {
        return InteractionResult::Pass;
    };

    let hit_state = context.world.get_block_state(hit_pos);
    let mut event = PlayerBucketFillEvent::new(
        context.player.gameprofile.id,
        context.world.key.to_string(),
        hit_pos,
        "BUCKET",
    );
    context.world.fire_event(&mut event);
    if event.is_cancelled() {
        return InteractionResult::Fail;
    }
    let block_behavior = BLOCK_BEHAVIORS.get_behavior(hit_state.get_block());

    if let Some(result) =
        block_behavior.pickup_block(context.world, hit_pos, hit_state, Some(context.player))
    {
        // Apply sound
        if let Some(sound) = result.sound {
            context
                .world
                .play_block_sound(sound, hit_pos, 1.0, 1.0, None);
        }

        // Give filled bucket
        create_filled_result(context, result.filled_bucket, true);
        context.world.game_event(
            &vanilla_game_events::FLUID_PICKUP,
            hit_pos,
            &GameEventContext::new(Some(context.player), None),
        );

        return InteractionResult::Success;
    }

    // TODO: Remove fallback once all waterloggable blocks implement pickup_block.
    if let Some(result) = pickup_waterlogged_block(
        block_behavior,
        context.world,
        hit_pos,
        hit_state,
        Some(context.player),
    ) {
        if let Some(sound) = result.sound {
            context
                .world
                .play_block_sound(sound, hit_pos, 1.0, 1.0, None);
        }

        create_filled_result(context, result.filled_bucket, true);
        context.world.game_event(
            &vanilla_game_events::FLUID_PICKUP,
            hit_pos,
            &GameEventContext::new(Some(context.player), None),
        );

        return InteractionResult::Success;
    }

    // Nothing was picked up — no fluid source block and no waterlogged block found.
    // Vanilla returns FAIL here so the client knows no item change occurred.
    InteractionResult::Fail
}

/// The block a filled bucket was aimed at.
pub(super) struct FilledBucketTarget {
    pub(super) clicked_pos: BlockPos,
    pub(super) direction: Direction,
    pub(super) clicked_state: BlockStateId,
}

/// Runs the clip that opens `BucketItem.use` for a filled bucket.
///
/// Vanilla parity: `getPlayerPOVHitResult(.., getFluidContext())`, which is
/// `ClipContext.Fluid.NONE` for anything holding contents. Returns the refusal
/// Vanilla would have produced when nothing usable was hit.
pub(super) fn filled_bucket_target(
    context: &UseItemContext<'_>,
) -> Result<FilledBucketTarget, InteractionResult> {
    let (start, end) = context.player.get_ray_endpoints();
    let (ray_block, ray_dir) = context.world.raytrace(start, end, |pos, world| {
        let state = world.get_block_state(pos);
        let block = state.get_block();
        // Filled buckets use ClipContext.Fluid.NONE: ignore fluid shapes, but
        // still test the block shape of waterlogged/container blocks.
        if block == &vanilla_blocks::AIR {
            return RaytraceAction::Pass;
        }
        RaytraceAction::CheckShape
    });

    // Vanilla returns PASS when raytrace misses (allows other handlers to try)
    let (Some(clicked_pos), Some(direction)) = (ray_block, ray_dir) else {
        return Err(InteractionResult::Pass);
    };

    // If the block is out of bounds, return fail
    if !context.world.is_in_valid_bounds(clicked_pos) {
        return Err(InteractionResult::Fail);
    }

    Ok(FilledBucketTarget {
        clicked_pos,
        direction,
        clicked_state: context.world.get_block_state(clicked_pos),
    })
}

/// Uses a bucket that carries something, from the hand.
///
/// Vanilla parity: `BucketItem.use` for anything whose content is not
/// `Fluids.EMPTY` -- the clip, then the item's own `emptyContents`, then
/// `checkExtraContent` and `createFilledResult`.
///
/// `fluid_block` is `None` for a mob bucket that carries no fluid, which still
/// takes this path: vanilla's `MobBucketItem.getFluidContext` is `NONE`
/// whatever it holds, and its `emptyContents` succeeds on the sound alone.
pub(super) fn use_filled_bucket(
    item: &dyn DispensibleContainerItem,
    fluid_block: Option<BlockRef>,
    context: &mut UseItemContext,
) -> InteractionResult {
    let target = match filled_bucket_target(context) {
        Ok(target) => target,
        Err(result) => return result,
    };
    let FilledBucketTarget {
        clicked_pos,
        direction,
        clicked_state,
    } = target;

    let is_water_bucket = fluid_block == Some(&vanilla_blocks::WATER);
    let place_pos =
        filled_bucket_primary_pos(clicked_state, clicked_pos, direction, is_water_bucket);
    let hit = BucketHit {
        block_pos: clicked_pos,
        direction,
    };

    let bucket = if is_water_bucket {
        "WATER_BUCKET"
    } else if fluid_block == Some(&vanilla_blocks::LAVA) {
        "LAVA_BUCKET"
    } else {
        "POWDER_SNOW_BUCKET"
    };
    let mut event = PlayerBucketEmptyEvent::new(
        context.player.gameprofile.id,
        context.world.key.to_string(),
        place_pos,
        bucket,
    );
    context.world.fire_event(&mut event);
    if event.is_cancelled() {
        return InteractionResult::Fail;
    }

    if !item.empty_contents(Some(context.player), context.world, place_pos, Some(hit)) {
        return InteractionResult::Fail;
    }

    // Vanilla hands `checkExtraContent` the position it asked to be emptied,
    // even when the fluid itself ended up one block further along.
    let stack = context.inv.with_item(|item| item.copy_with_count(1));
    item.check_extra_content(Some(context.player), context.world, &stack, place_pos);

    let result_stack = filled_bucket_success_stack(context);
    create_filled_result(context, result_stack, true);
    InteractionResult::Success
}

/// Empties `fluid_block` into the world at `pos`.
///
/// Vanilla parity: `BucketItem.emptyContents`. Both `user` and `hit` are
/// nullable in vanilla, and a dispenser supplies neither -- so nothing below
/// this line may reach for a player.
pub(super) fn empty_contents(
    fluid_block: BlockRef,
    user: Option<&Player>,
    world: &Arc<World>,
    pos: BlockPos,
    hit: Option<BucketHit>,
    empty_sound: EmptySound,
) -> bool {
    // The sneak check only applies while a hit result is in hand; vanilla's
    // `!shiftKeyDown || hitResult == null` is what switches it off on the retry
    // and for a dispenser.
    if try_place_fluid(fluid_block, user, world, pos, hit.is_some(), empty_sound) {
        return true;
    }

    // Vanilla parity: the single `emptyContents(user, level, hitResult
    // .getBlockPos().relative(hitResult.getDirection()), null)` recursion. With
    // no hit result there is nowhere to retry, which is why a dispenser gets
    // exactly one attempt.
    let Some(hit) = hit else {
        return false;
    };
    try_place_fluid(
        fluid_block,
        user,
        world,
        hit.direction.relative(hit.block_pos),
        false,
        empty_sound,
    )
}

/// Tries to put the bucket's fluid into one block.
///
/// `check_sneak` is vanilla's `!shiftKeyDown || hitResult == null`, false once
/// there is no hit result left to redirect with.
fn try_place_fluid(
    fluid_block: BlockRef,
    user: Option<&Player>,
    world: &Arc<World>,
    pos: BlockPos,
    check_sneak: bool,
    empty_sound: EmptySound,
) -> bool {
    if !world.is_in_valid_bounds(pos) {
        return false;
    }

    let state = world.get_block_state(pos);
    let fluid_state = state.get_fluid_state();

    // Vanilla parity (bl4): when sneaking, only air allows placement at this position.
    // Non-air blocks redirect to the neighbor — handled by the secondary call.
    // The secondary call bypasses this check (hitResult == null in vanilla).
    let is_sneaking = user.is_some_and(Player::is_crouching);
    if check_sneak && is_sneaking && !state.get_block().config.is_air {
        return false;
    }

    let is_water_bucket = fluid_block == &vanilla_blocks::WATER;
    let behavior = BLOCK_BEHAVIORS.get_behavior(state.get_block());
    let is_liquid_container = state.is_liquid_container();
    let can_place_liquid = is_water_bucket
        && is_liquid_container
        && behavior.can_place_liquid_with_player(
            state,
            FluidState::source(&vanilla_fluids::WATER).fluid_id,
            user,
        );
    let can_replace = state.can_be_replaced_by_fluid(fluid_block);

    // Vanilla parity: block must be replaceable or liquid-container-admissible for placement.
    if !can_replace && !can_place_liquid {
        return false;
    }

    // Vanilla parity: in worlds where water evaporates (e.g. the Nether),
    // water buckets fizz out without placing any fluid.
    // TODO: Per-position environment attributes (vanilla uses EnvironmentAttributes.WATER_EVAPORATES per-pos)
    if is_water_bucket && world.dimension_type.water_evaporates {
        world.level_event(level_events::PARTICLES_WATER_EVAPORATING, pos, 0, None);
        return true;
    }

    // 1. Try LiquidBlockContainer handling (only if Water bucket).
    if is_water_bucket && is_liquid_container {
        let source_water = FluidState::source(&vanilla_fluids::WATER);
        behavior.place_liquid(world, pos, state, source_water);
        play_empty_sound_and_event(world, user, pos, true, empty_sound);
        return true;
    }

    // 2. Try Standard Placement (Replaceable block)
    if can_replace {
        // If same fluid already exists and is source, just consume bucket (parity)
        let is_same_fluid = if is_water_bucket {
            fluid_state.is_water()
        } else {
            fluid_state.is_lava()
        };

        if is_same_fluid && fluid_state.is_source() {
            play_empty_sound_and_event(world, user, pos, is_water_bucket, empty_sound);
            return true;
        }

        // Vanilla parity: destroy non-liquid replaceable blocks first so they
        // drop their items (e.g. tall grass, flowers, snow layers).
        if !state.get_block().config.liquid && !state.get_block().config.is_air {
            world.destroy_block(pos, true);
        }

        // Place fluid block
        let fluid_state_to_place = fluid_block.default_state();
        if world.set_block(pos, fluid_state_to_place, UpdateFlags::UPDATE_ALL_IMMEDIATE) {
            let fluid_ref = if is_water_bucket {
                &vanilla_fluids::WATER
            } else {
                &vanilla_fluids::LAVA
            };
            let tick_delay = FLUID_BEHAVIORS.get_behavior(fluid_ref).tick_delay(world);
            world.schedule_fluid_tick_default(pos, fluid_ref, tick_delay);

            play_empty_sound_and_event(world, user, pos, is_water_bucket, empty_sound);

            return true;
        }
    }
    false
}

pub(super) fn play_empty_sound_and_event(
    world: &Arc<World>,
    user: Option<&Player>,
    pos: BlockPos,
    is_water_bucket: bool,
    empty_sound: EmptySound,
) {
    match empty_sound {
        EmptySound::Fluid => {
            let sound_event = if is_water_bucket {
                &sound_events::ITEM_BUCKET_EMPTY
            } else {
                &sound_events::ITEM_BUCKET_EMPTY_LAVA
            };
            world.play_block_sound(sound_event, pos, 1.0, 1.0, None);
            world.game_event(
                &vanilla_game_events::FLUID_PLACE,
                pos,
                &GameEventContext::new(user.map(|player| player as &dyn Entity), None),
            );
        }
        // Vanilla's `MobBucketItem.playEmptySound` replaces the whole method:
        // a neutral-category sound, and no fluid-place game event.
        EmptySound::Mob(sound) => {
            world.play_sound(sound, SoundSource::Neutral, pos, 1.0, 1.0, None);
        }
    }
}

fn filled_bucket_primary_pos(
    clicked_state: BlockStateId,
    clicked_pos: BlockPos,
    direction: Direction,
    is_water_bucket: bool,
) -> BlockPos {
    if is_water_bucket && clicked_state.is_liquid_container() {
        clicked_pos
    } else {
        direction.relative(clicked_pos)
    }
}

#[cfg(test)]
mod tests {
    use crate::behavior::init_behaviors;
    use foton_registry::{init_vanilla_registry, vanilla_blocks};

    use super::*;

    #[test]
    fn filled_water_bucket_targets_non_waterlogged_liquid_container_in_place() {
        init_vanilla_registry();
        init_behaviors();

        let kelp = vanilla_blocks::KELP.default_state();

        assert_eq!(
            filled_bucket_primary_pos(kelp, BlockPos::ZERO, Direction::North, true),
            BlockPos::ZERO
        );
        assert_eq!(
            filled_bucket_primary_pos(kelp, BlockPos::ZERO, Direction::North, false),
            Direction::North.relative(BlockPos::ZERO)
        );
    }
}
