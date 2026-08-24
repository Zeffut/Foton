//! The monster spawner's block entity.
//!
//! Vanilla parity: `net.minecraft.world.level.block.entity.SpawnerBlockEntity`.

use std::sync::{Arc, Weak};

use simdnbt::borrow::BaseNbtCompound as BorrowedNbtCompound;
use simdnbt::owned::NbtCompound;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::{vanilla_block_entity_types, vanilla_blocks};
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;
use crate::world::base_spawner::{BaseSpawner, Spawner, SpawnerOwner};

/// Vanilla `SpawnerBlockEntity`.
pub struct SpawnerBlockEntity {
    base: BlockEntityBase,
    spawner: BaseSpawner,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SpawnerBlockEntity`.
unsafe impl DowncastType for SpawnerBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/mob_spawner");
}

impl SpawnerBlockEntity {
    /// Creates the storage behind one spawner block.
    #[must_use]
    pub fn new(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(&vanilla_block_entity_types::MOB_SPAWNER, world, pos, state),
            spawner: BaseSpawner::new(),
        }
    }

    /// Returns the spawner behind this block.
    ///
    /// Vanilla parity: `SpawnerBlockEntity.getSpawner`.
    #[must_use]
    pub const fn spawner(&self) -> &BaseSpawner {
        &self.spawner
    }
}

impl SpawnerOwner for SpawnerBlockEntity {
    /// Vanilla parity: the `broadcastEvent` of `SpawnerBlockEntity`'s spawner.
    fn broadcast_spawner_event(&self, world: &Arc<World>, pos: BlockPos, id: i32) {
        world.block_event(pos, &vanilla_blocks::SPAWNER, id, 0);
    }

    /// Vanilla parity: the `setNextSpawnData` override, whose whole job is to
    /// re-send the block so the client's spinning mob follows the change.
    fn on_next_spawn_data_set(&self, world: &Arc<World>, pos: BlockPos) {
        world.send_block_updated(pos);
    }
}

impl Spawner for SpawnerBlockEntity {
    /// Vanilla parity: `SpawnerBlockEntity.setEntityId`.
    fn set_spawner_entity_id(&self, entity_type: EntityTypeRef) {
        let world = self.get_level();
        self.spawner
            .set_entity_id(self, &entity_type.key, world.as_ref(), self.get_block_pos());
        self.set_changed();
    }
}

impl BlockEntity for SpawnerBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        self.spawner.load(&nbt.into());
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.spawner.save(nbt);
    }

    /// Vanilla parity: `SpawnerBlockEntity.getUpdateTag`, which sends everything
    /// but the spawn potentials -- the client only needs to know what is coming
    /// next, not the whole weighted list.
    fn get_update_tag(&self) -> Option<NbtCompound> {
        let mut nbt = self.save_custom_only();
        while nbt.remove("SpawnPotentials").is_some() {}
        Some(nbt)
    }

    /// Vanilla parity: `SpawnerBlockEntity.triggerEvent`.
    fn trigger_event(&self, param_a: i32, _param_b: i32) -> bool {
        BaseSpawner::on_event_triggered(param_a)
    }

    /// Vanilla parity: `SpawnerBlockEntity.serverTick`.
    fn tick(&self, world: &Arc<World>) {
        self.spawner.server_tick(self, world, self.get_block_pos());
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound;

    use steel_registry::init_vanilla_registry;
    use steel_utils::Identifier;

    use super::*;

    /// A spawner's whole identity is the mob it is pointed at, and the update
    /// tag is what the client renders from. Vanilla strips the potentials out
    /// of it; keeping them would ship the whole weighted list to every player
    /// who walks past a dungeon.
    #[test]
    fn the_update_tag_names_the_mob_without_the_whole_potentials_list() {
        init_vanilla_registry();
        let entity = SpawnerBlockEntity::new(
            Weak::new(),
            BlockPos::new(8, 64, 8),
            vanilla_blocks::SPAWNER.default_state(),
        );

        let mut saved = NbtCompound::new();
        let mut spawn_entity = NbtCompound::new();
        spawn_entity.insert("id", "minecraft:zombie");
        let mut spawn_data = NbtCompound::new();
        spawn_data.insert("entity", spawn_entity);
        saved.insert("SpawnData", spawn_data);

        let mut bytes = Vec::new();
        saved.write(&mut bytes);
        let borrowed =
            read_compound(&mut Cursor::new(&bytes)).expect("hand-built spawner nbt must parse");
        entity.load_additional(&borrowed);

        let update_tag = entity
            .get_update_tag()
            .expect("spawner sends an update tag");
        assert!(!update_tag.contains("SpawnPotentials"));
        assert!(update_tag.contains("SpawnData"));
        assert_eq!(
            entity.spawner().next_entity_type_key(),
            Some(Identifier::vanilla_static("zombie"))
        );
    }
}
