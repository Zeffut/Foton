//! The loot table a container carries until something first looks inside it.
//!
//! Vanilla parity: `net.minecraft.world.RandomizableContainer`, whose
//! `lootTable`/`lootTableSeed` pair lives on `RandomizableContainerBlockEntity`
//! for blocks and is duplicated by `ContainerEntity` for the chest minecart,
//! the hopper minecart and the chest boats. Rust has no inheritance, so every
//! randomizable container owns a [`ContainerLoot`] and forwards to it.
//!
//! Worldgen writes the table; nothing rolls it until the container is read or
//! written. That is deliberate in vanilla: a chest generated a thousand blocks
//! away costs nothing until a player -- or a hopper -- reaches it, and the
//! stored seed makes the eventual roll independent of when that happens.

use std::str::FromStr as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use glam::DVec3;
use rand::{SeedableRng as _, rngs::StdRng};
use simdnbt::borrow::NbtCompound as NbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_registry::item_stack::ItemStack;
use steel_registry::loot_table::{LootContext, LootFillContainer, LootTableRef};
use steel_registry::{REGISTRY, RegistryExt as _};
use steel_utils::{Identifier, locks::SyncMutex};

use crate::block_entity::BlockEntityBase;
use crate::entity::{LivingEntity as _, entity_loot_ref};
use crate::inventory::container::Container;
use crate::inventory::lock::SharedContainer;
use crate::player::Player;
use crate::world::World;

/// Vanilla parity: `RandomizableContainer.LOOT_TABLE_TAG`.
const LOOT_TABLE_TAG: &str = "LootTable";

/// Vanilla parity: `RandomizableContainer.LOOT_TABLE_SEED_TAG`.
const LOOT_TABLE_SEED_TAG: &str = "LootTableSeed";

/// Lets [`steel_registry::loot_table::LootTable::fill`] place rolled loot into
/// any Steel container.
///
/// `LootTable` lives in `steel-registry`, which cannot see [`Container`], so
/// the adapter has to be written on this side of the crate boundary.
impl LootFillContainer for dyn Container {
    fn get_container_size(&self) -> usize {
        Container::get_container_size(self)
    }

    fn get_item(&self, slot: usize) -> &ItemStack {
        Container::get_item(self, slot)
    }

    fn set_item(&mut self, slot: usize, stack: ItemStack) {
        Container::set_item(self, slot, stack);
    }
}

/// A loot table that has not been rolled into its container yet.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PackedLootTable {
    key: Identifier,
    seed: i64,
}

/// The `lootTable`/`lootTableSeed` pair of a randomizable container.
///
/// Vanilla parity: the fields and default methods of `RandomizableContainer`.
///
/// `packed` mirrors "a table is still waiting" without the mutex, because every
/// read of and write to the container has to ask, and almost every container in
/// the world answers no.
#[derive(Debug, Default)]
pub struct ContainerLoot {
    packed: AtomicBool,
    table: SyncMutex<Option<PackedLootTable>>,
}

impl ContainerLoot {
    /// Creates a container loot slot with nothing packed in it.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Arms this container with a table to roll on first access.
    ///
    /// Vanilla parity: `RandomizableContainer.setLootTable(lootTable, seed)`.
    pub fn set_loot_table(&self, key: Identifier, seed: i64) {
        *self.table.lock() = Some(PackedLootTable { key, seed });
        self.packed.store(true, Ordering::Relaxed);
    }

    /// Returns the table still waiting to be rolled.
    ///
    /// Vanilla parity: `RandomizableContainer.getLootTable`.
    #[must_use]
    pub fn loot_table(&self) -> Option<Identifier> {
        self.table.lock().as_ref().map(|packed| packed.key.clone())
    }

    /// Returns the seed the packed table will be rolled with.
    ///
    /// Vanilla parity: `RandomizableContainer.getLootTableSeed`.
    #[must_use]
    pub fn loot_table_seed(&self) -> i64 {
        self.table.lock().as_ref().map_or(0, |packed| packed.seed)
    }

    /// Returns whether a table is still waiting to be rolled.
    #[must_use]
    pub fn is_packed(&self) -> bool {
        self.packed.load(Ordering::Relaxed)
    }

    /// Reads the pair off disk, answering whether a table was found.
    ///
    /// Vanilla parity: `RandomizableContainer.tryLoadLootTable`. A `false`
    /// answer is the caller's signal to load the `Items` list instead: a
    /// container never stores both.
    pub fn try_load_loot_table(&self, nbt: &NbtCompoundView<'_, '_>) -> bool {
        let key = nbt
            .string(LOOT_TABLE_TAG)
            .and_then(|value| Identifier::from_str(&value.to_string()).ok());
        let seed = nbt.long(LOOT_TABLE_SEED_TAG).unwrap_or(0);

        let found = key.is_some();
        *self.table.lock() = key.map(|key| PackedLootTable { key, seed });
        self.packed.store(found, Ordering::Relaxed);
        found
    }

    /// Writes the pair to disk, answering whether there was one to write.
    ///
    /// Vanilla parity: `RandomizableContainer.trySaveLootTable`, which omits a
    /// zero seed because zero means "roll me freshly" rather than "roll me with
    /// seed zero".
    pub fn try_save_loot_table(&self, nbt: &mut NbtCompound) -> bool {
        let Some(packed) = self.table.lock().clone() else {
            return false;
        };
        nbt.insert(LOOT_TABLE_TAG, packed.key.to_string());
        if packed.seed != 0 {
            nbt.insert(LOOT_TABLE_SEED_TAG, packed.seed);
        }
        true
    }

    /// Rolls a still-packed table into the container owned by `base`.
    ///
    /// Vanilla parity: `RandomizableContainer.unpackLootTable`, whose `ORIGIN`
    /// is the center of the block.
    pub(crate) fn unpack_for_block_entity(
        &self,
        base: &BlockEntityBase,
        container: &SharedContainer,
        player: Option<&Player>,
    ) {
        if !self.is_packed() {
            return;
        }
        let Some(world) = base.level() else {
            return;
        };
        let pos = base.pos();
        let origin = DVec3::new(
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()) + 0.5,
            f64::from(pos.z()) + 0.5,
        );
        if self.unpack_at(&world, origin, container, player) {
            // Vanilla parity: `fill` reaches the container through `setItem`,
            // which is what marks a block entity changed.
            base.set_changed();
        }
    }

    /// Rolls a still-packed table into `container`, answering whether it did.
    ///
    /// Vanilla parity: the body shared by `RandomizableContainer.unpackLootTable`
    /// and `ContainerEntity.unpackChestVehicleLootTable`; they differ only in
    /// where `ORIGIN` comes from, so a moving container passes its own position.
    pub fn unpack_at(
        &self,
        world: &Arc<World>,
        origin: DVec3,
        container: &SharedContainer,
        player: Option<&Player>,
    ) -> bool {
        // Vanilla clears the field before filling, so the `setItem` calls that
        // follow cannot roll the same table a second time.
        let Some(packed) = self.take() else {
            return false;
        };
        let Some(table) = REGISTRY.loot_tables.by_key(&packed.key) else {
            log::warn!(
                "Container at {origin} referenced unknown loot table {}",
                packed.key
            );
            return false;
        };

        // TODO: Trigger the vanilla `GENERATE_LOOT` criterion for `player` once
        // Steel has advancements.
        if packed.seed == 0 {
            // Vanilla parity: `LootContext.Builder.withOptionalRandomSeed`
            // leaves the level's own random source in place for seed zero.
            Self::fill(table, container, &mut rand::rng(), world, origin, player);
        } else {
            Self::fill(
                table,
                container,
                &mut StdRng::seed_from_u64(packed.seed as u64),
                world,
                origin,
                player,
            );
        }
        true
    }

    /// Clears the packed table and returns it, if there was one.
    fn take(&self) -> Option<PackedLootTable> {
        if !self.is_packed() {
            return None;
        }
        let packed = self.table.lock().take();
        self.packed.store(false, Ordering::Relaxed);
        packed
    }

    fn fill<R: rand::Rng>(
        table: LootTableRef,
        container: &SharedContainer,
        rng: &mut R,
        world: &Arc<World>,
        origin: DVec3,
        player: Option<&Player>,
    ) {
        let mut ctx = LootContext::new(rng)
            .with_origin(origin.x, origin.y, origin.z)
            .with_game_time(world.game_time());
        if let Some(player) = player {
            ctx = ctx
                .with_luck(player.get_luck())
                .with_this_entity(entity_loot_ref(player));
        }

        let mut locked = container.lock();
        table.fill(&mut *locked, &mut ctx);
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use simdnbt::owned::NbtList;

    use super::{ContainerLoot, NbtCompound, NbtCompoundView};
    use steel_utils::Identifier;

    fn view_of(nbt: &NbtCompound) -> Vec<u8> {
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        bytes
    }

    fn load(loot: &ContainerLoot, nbt: &NbtCompound) -> bool {
        let bytes = view_of(nbt);
        let base = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
            .expect("test nbt should reborrow");
        let view: NbtCompoundView<'_, '_> = (&base).into();
        loot.try_load_loot_table(&view)
    }

    #[test]
    fn a_saved_pair_survives_a_round_trip() {
        let loot = ContainerLoot::new();
        loot.set_loot_table(
            Identifier::new_static("minecraft", "chests/simple_dungeon"),
            42,
        );

        let mut saved = NbtCompound::new();
        assert!(loot.try_save_loot_table(&mut saved));

        let reloaded = ContainerLoot::new();
        assert!(load(&reloaded, &saved));
        assert_eq!(
            reloaded.loot_table().map(|key| key.to_string()),
            Some("minecraft:chests/simple_dungeon".to_owned())
        );
        assert_eq!(reloaded.loot_table_seed(), 42);
    }

    /// Zero is vanilla's "pick a seed when you roll me", so it is not written.
    #[test]
    fn a_zero_seed_is_left_out_of_the_save() {
        let loot = ContainerLoot::new();
        loot.set_loot_table(Identifier::new_static("minecraft", "chests/igloo_chest"), 0);

        let mut saved = NbtCompound::new();
        assert!(loot.try_save_loot_table(&mut saved));

        assert!(saved.string("LootTable").is_some());
        assert!(saved.long("LootTableSeed").is_none());
    }

    #[test]
    fn a_container_with_no_table_saves_nothing_and_tells_the_caller() {
        let loot = ContainerLoot::new();
        let mut saved = NbtCompound::new();

        assert!(!loot.try_save_loot_table(&mut saved));
        assert!(saved.string("LootTable").is_none());
        assert!(!loot.is_packed());
    }

    /// Loading a chest that has already been opened must clear an older table,
    /// otherwise a block entity reused for a second position would roll again.
    #[test]
    fn loading_without_a_table_clears_a_previous_one() {
        let loot = ContainerLoot::new();
        loot.set_loot_table(
            Identifier::new_static("minecraft", "chests/simple_dungeon"),
            7,
        );

        let mut empty = NbtCompound::new();
        empty.insert("Items", NbtList::Compound(Vec::new()));
        assert!(!load(&loot, &empty));

        assert!(!loot.is_packed());
        assert_eq!(loot.loot_table(), None);
    }
}
