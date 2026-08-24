//! Creaking heart block entity.
//!
//! Vanilla parity: `CreakingHeartBlockEntity`.
//!
//! What survives without a `Creaking` entity is the periodic state check: every twenty-odd
//! ticks the heart re-reads its logs and the `creaking_active` environment attribute, and
//! writes `uprooted`, `dormant` or `awake` back into its block state. That is the whole of
//! the heart a player can see with no creaking in the world.
//!
//! Not implemented, all of it downstream of the missing `Creaking` entity: spawning the
//! protector, the 34-block tether that despawns it, `creakingHurt` and the resin it spreads,
//! the trail particles between heart and creaking, and the comparator output that scales
//! with the creaking's distance. Vanilla stores only the creaking's UUID here, so a world
//! saved by vanilla keeps that field through Steel untouched -- see `preserved_creaking`.

use std::sync::{Arc, Weak};

use rand::RngExt as _;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::CreakingHeartState;
use steel_registry::vanilla_block_entity_types;
use steel_utils::locks::SyncMutex;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use crate::behavior::blocks::{
    CREAKING_HEART_STATE, CreakingHeartBlock, creaking_heart_awake_or_dormant,
};
use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;

/// Vanilla `CreakingHeartBlockEntity.UPDATE_TICKS`.
const UPDATE_TICKS: i32 = 20;
/// Vanilla `CreakingHeartBlockEntity.UPDATE_TICKS_VARIANCE`.
const UPDATE_TICKS_VARIANCE: i32 = 5;

struct CreakingHeartTickerState {
    ticker: i32,
    /// The `creaking` UUID vanilla stores here.
    ///
    /// Steel has no `Creaking` to resolve it against, so the tag is carried through a save
    /// and load untouched rather than dropped -- a world that already has a creaking tied
    /// to this heart keeps that link when it goes back to vanilla.
    preserved_creaking: Option<NbtTag>,
}

/// Vanilla `CreakingHeartBlockEntity`.
pub struct CreakingHeartBlockEntity {
    base: BlockEntityBase,
    state: SyncMutex<CreakingHeartTickerState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `CreakingHeartBlockEntity`.
unsafe impl DowncastType for CreakingHeartBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/creaking_heart");
}

impl CreakingHeartBlockEntity {
    /// Creates creaking heart storage.
    #[must_use]
    pub fn new(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(
                &vanilla_block_entity_types::CREAKING_HEART,
                world,
                pos,
                state,
            ),
            state: SyncMutex::new(CreakingHeartTickerState {
                ticker: 0,
                preserved_creaking: None,
            }),
        }
    }

    /// Vanilla `CreakingHeartBlockEntity.updateCreakingState`.
    ///
    /// Vanilla only falls back to `uprooted` when there is no creaking left to protect; with
    /// no creaking there ever is, so losing the logs always uproots the heart.
    fn updated_state(state: BlockStateId, world: &World, pos: BlockPos) -> BlockStateId {
        if !CreakingHeartBlock::has_required_logs(state, world, pos) {
            return state.set_value(CREAKING_HEART_STATE, CreakingHeartState::Uprooted);
        }

        state.set_value(CREAKING_HEART_STATE, creaking_heart_awake_or_dormant(world))
    }
}

impl BlockEntity for CreakingHeartBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla `CreakingHeartBlockEntity.serverTick`, minus everything that needs a creaking.
    fn tick(&self, world: &Arc<World>) {
        {
            let mut state = self.state.lock();
            state.ticker -= 1;
            if state.ticker >= 0 {
                return;
            }
            state.ticker = rand::rng().random_range(0..UPDATE_TICKS_VARIANCE) + UPDATE_TICKS;
        }

        let pos = self.get_block_pos();
        let state = self.get_block_state();
        let updated = Self::updated_state(state, world.as_ref(), pos);
        if updated != state {
            world.set_block(pos, updated, UpdateFlags::UPDATE_ALL);
        }
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let view: NbtCompoundView<'_, '_> = nbt.into();
        self.state.lock().preserved_creaking =
            view.get("creaking").map(|creaking| creaking.to_owned());
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        if let Some(creaking) = self.state.lock().preserved_creaking.clone() {
            nbt.insert("creaking", creaking);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use steel_registry::{init_vanilla_registry, vanilla_blocks};
    use steel_utils::UuidExt as _;
    use uuid::Uuid;

    use super::*;

    fn heart() -> CreakingHeartBlockEntity {
        init_vanilla_registry();
        CreakingHeartBlockEntity::new(
            Weak::new(),
            BlockPos::new(1, 2, 3),
            vanilla_blocks::CREAKING_HEART.default_state(),
        )
    }

    /// Steel cannot resolve the creaking, but dropping its UUID would orphan a creaking a
    /// vanilla world had already spawned. The tag has to come back out the way it went in.
    #[test]
    fn a_heart_carries_a_vanilla_creaking_link_through_a_save_and_load() {
        let uuid = Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef);
        let mut disk = NbtCompound::new();
        disk.insert("creaking", NbtTag::IntArray(uuid.to_int_array().to_vec()));
        let mut bytes = Vec::new();
        disk.write(&mut bytes);
        let borrowed =
            read_borrowed_compound(&mut Cursor::new(bytes.as_slice())).expect("test NBT reborrows");

        let loaded = heart();
        loaded.load_additional(&borrowed);

        let mut written = NbtCompound::new();
        loaded.save_additional(&mut written);
        assert_eq!(
            written.int_array("creaking").map(<[i32]>::to_vec),
            Some(uuid.to_int_array().to_vec())
        );
    }

    /// A heart placed by a player has no creaking, and must not invent an empty tag that a
    /// vanilla client or server would then try to resolve.
    #[test]
    fn a_heart_with_no_creaking_writes_no_creaking_tag() {
        let nbt = NbtCompound::new();
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let borrowed =
            read_borrowed_compound(&mut Cursor::new(bytes.as_slice())).expect("test NBT reborrows");

        let loaded = heart();
        loaded.load_additional(&borrowed);

        let mut written = NbtCompound::new();
        loaded.save_additional(&mut written);
        assert!(written.get("creaking").is_none());
    }
}
