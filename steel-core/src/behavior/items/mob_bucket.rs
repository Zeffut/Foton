//! Fish, axolotl, tadpole and sulfur-cube buckets.

use std::sync::Arc;

use glam::DVec3;
use steel_macros::item_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_game_events;
use steel_utils::BlockPos;

use crate::behavior::{
    BucketHit, DispensibleContainerItem, InteractionResult, ItemBehavior, UseItemContext,
};
use crate::entity::bucketable::read_bucket_entity_data;
use crate::entity::{ENTITIES, Entity, EntitySpawnReason, next_entity_id};
use crate::player::Player;
use crate::world::World;
use crate::world::game_event::GameEventContext;
use steel_registry::item_stack::ItemStack;

use super::bucket::{EmptySound, empty_contents, play_empty_sound_and_event, use_filled_bucket};

/// A bucket that carries one mob.
///
/// Vanilla parity: `MobBucketItem`, which extends `BucketItem` and adds the
/// spawn in `checkExtraContent`.
///
/// Steel gap: the mob only appears once Steel implements its entity, which is
/// the same shape as Vanilla's `EntityType.create` returning null and which
/// `MobBucketItem.spawn` already guards against.
///
/// Steel gap: only the axolotl and the sulfur cube implement [`Bucketable`] so
/// far, so only those two come back out of a bucket as the animal that went in.
/// Every other mob bucket still spawns a fresh mob -- the loop below is ready
/// for them, and each one closes by implementing the trait.
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

    /// Vanilla parity: `MobBucketItem.spawn`.
    fn spawn(&self, world: &Arc<World>, stack: &ItemStack, pos: BlockPos) {
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
            read_bucket_entity_data(stack, |tag| bucketable.load_from_bucket_tag(tag));
            bucketable.set_from_bucket(true);
        }

        if let Err(error) = world.try_add_entity(entity) {
            log::warn!("Failed to spawn bucketed mob: {error}");
        }
    }
}

impl ItemBehavior for MobBucketItem {
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        use_filled_bucket(self, self.content, context)
    }

    fn as_dispensible_container(&self) -> Option<&dyn DispensibleContainerItem> {
        Some(self)
    }
}

impl DispensibleContainerItem for MobBucketItem {
    /// Vanilla parity: `MobBucketItem.emptyContents`, whose `content ==
    /// Fluids.EMPTY` arm succeeds on the sound alone.
    fn empty_contents(
        &self,
        user: Option<&Player>,
        world: &Arc<World>,
        pos: BlockPos,
        hit: Option<BucketHit>,
    ) -> bool {
        let empty_sound = EmptySound::Mob(self.empty_sound);
        let Some(fluid_block) = self.content else {
            play_empty_sound_and_event(world, user, pos, false, empty_sound);
            return true;
        };
        empty_contents(fluid_block, user, world, pos, hit, empty_sound)
    }

    /// Vanilla parity: `MobBucketItem.checkExtraContent`, the reason a fish
    /// bucket is worth anything at all.
    fn check_extra_content(
        &self,
        user: Option<&Player>,
        world: &Arc<World>,
        stack: &ItemStack,
        pos: BlockPos,
    ) {
        self.spawn(world, stack, pos);
        world.game_event(
            &vanilla_game_events::ENTITY_PLACE,
            pos,
            &GameEventContext::new(user.map(|player| player as &dyn Entity), None),
        );
    }
}
