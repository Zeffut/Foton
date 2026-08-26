//! Fish, axolotl, tadpole and sulfur-cube buckets.

use std::sync::Arc;

use glam::DVec3;
use steel_macros::item_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_game_events;
use steel_utils::BlockPos;

use crate::behavior::item_utils::create_filled_result;
use crate::behavior::{InteractionResult, ItemBehavior, UseItemContext};
use crate::entity::bucketable::read_bucket_entity_data;
use crate::entity::{ENTITIES, EntitySpawnReason, next_entity_id};
use crate::world::game_event::GameEventContext;

use super::bucket::{
    EmptySound, filled_bucket_success_stack, filled_bucket_target, play_empty_sound_and_event,
    use_filled_bucket,
};

/// A bucket that carries one mob.
///
/// Vanilla parity: `MobBucketItem`, which extends `BucketItem` and adds the
/// spawn in `checkExtraContent`.
///
/// Steel gap: the mob only appears once Steel implements its entity. Cod,
/// salmon and the axolotl do; the sulfur cube does not yet, so its bucket
/// places its water and empties without producing anything. That is the same
/// shape as Vanilla's `EntityType.create` returning null, which
/// `MobBucketItem.spawn` already guards against.
///
/// Steel gap: only the axolotl implements [`Bucketable`] so far, so only an
/// axolotl comes back out of its bucket as the animal that went in. Every other
/// mob bucket still spawns a fresh mob -- the loop below is ready for them, and
/// each one closes by implementing the trait.
#[item_behavior]
pub struct MobBucketItem {
    #[json_arg(vanilla_entities, json = "type")]
    mob_type: EntityTypeRef,
    #[json_arg(vanilla_blocks, json = "content", optional = "empty")]
    content: Option<BlockRef>,
    #[json_arg(sound_events, json = "empty_sound")]
    empty_sound: SoundEventRef,
}

impl MobBucketItem {
    /// Creates a mob bucket behavior.
    #[must_use]
    pub const fn new(
        mob_type: EntityTypeRef,
        content: Option<BlockRef>,
        empty_sound: SoundEventRef,
    ) -> Self {
        Self {
            mob_type,
            content,
            empty_sound,
        }
    }

    /// Vanilla parity: `MobBucketItem.spawn` plus the `ENTITY_PLACE` game event
    /// its `checkExtraContent` fires alongside.
    fn spawn(&self, context: &UseItemContext<'_>, pos: BlockPos) {
        let world = context.world;
        let (x, y, z) = pos.get_bottom_center();
        let Some(entity) = ENTITIES.create(
            self.mob_type,
            next_entity_id(),
            DVec3::new(x, y, z),
            Arc::downgrade(world),
        ) else {
            return;
        };

        if let Some(mob) = entity.as_mob() {
            let _ = mob.finalize_spawn(world, EntitySpawnReason::Bucket, None);
        }

        // Vanilla parity: the `instanceof Bucketable` half of
        // `MobBucketItem.spawn`, which replays what the bucket saved before the
        // mob joins the world.
        if let Some(bucketable) = entity.as_bucketable() {
            context.inv.with_item(|item| {
                read_bucket_entity_data(item, |tag| bucketable.load_from_bucket_tag(tag));
            });
            bucketable.set_from_bucket(true);
        }

        if let Err(error) = world.try_add_entity(entity) {
            log::warn!("Failed to spawn bucketed mob: {error}");
            return;
        }

        world.game_event(
            &vanilla_game_events::ENTITY_PLACE,
            pos,
            &GameEventContext::new(Some(context.player), None),
        );
    }

    /// Vanilla parity: the `content == Fluids.EMPTY` short-circuit of
    /// `MobBucketItem.emptyContents`, which succeeds on the sound alone.
    fn empty_without_fluid(
        &self,
        context: &mut UseItemContext,
        empty_sound: EmptySound,
    ) -> InteractionResult {
        let target = match filled_bucket_target(context) {
            Ok(target) => target,
            Err(result) => return result,
        };
        // Vanilla's `placePos` is the offset block unless the content is water,
        // which this branch never is.
        let pos = target.direction.relative(target.clicked_pos);

        play_empty_sound_and_event(context, pos, false, empty_sound);
        self.spawn(context, pos);
        let result_stack = filled_bucket_success_stack(context);
        create_filled_result(context, result_stack, true);
        InteractionResult::Success
    }
}

impl ItemBehavior for MobBucketItem {
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let empty_sound = EmptySound::Mob(self.empty_sound);
        let Some(fluid_block) = self.content else {
            return self.empty_without_fluid(context, empty_sound);
        };

        use_filled_bucket(fluid_block, context, empty_sound, |used_on, pos| {
            self.spawn(used_on, pos);
        })
    }
}
