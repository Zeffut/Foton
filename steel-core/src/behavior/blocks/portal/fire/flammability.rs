//! How readily each block catches fire and burns away.
//!
//! Vanilla parity: `FireBlock.bootStrap`, which fills two `Object2IntMap<Block>`
//! on the fire block itself, once, at startup. Steel keeps the same pair of
//! numbers, indexed by block id so the spread scan around a fire -- fifty-odd
//! positions every fire tick -- stays a plain lookup.

use std::sync::LazyLock;

use steel_registry::blocks::BlockRef;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::{
    REGISTRY, RegistryEntry as _, RegistryExt as _, TaggedRegistryExt as _, vanilla_blocks,
};

/// How readily a block lights, and how readily it is eaten once alight.
///
/// Vanilla parity: the `igniteOdds` and `burnOdds` arguments of
/// `FireBlock.setFlammable`.
#[derive(Clone, Copy)]
pub(super) struct Flammability {
    pub(super) ignite_odds: i32,
    pub(super) burn_odds: i32,
}

impl Flammability {
    /// What every block starts as: fire neither spreads to it nor eats it.
    const NONE: Self = Self {
        ignite_odds: 0,
        burn_odds: 0,
    };
}

/// Returns what fire makes of this block.
pub(super) fn flammability(block: BlockRef) -> Flammability {
    TABLE.get(block.id()).copied().unwrap_or(Flammability::NONE)
}

static TABLE: LazyLock<Box<[Flammability]>> = LazyLock::new(build_table);

fn build_table() -> Box<[Flammability]> {
    let mut table = vec![Flammability::NONE; REGISTRY.blocks.len()];
    for &(block, ignite_odds, burn_odds) in SET_FLAMMABLE {
        table[block.id()] = Flammability {
            ignite_odds,
            burn_odds,
        };
    }
    // `Blocks.WOOL.forEach` and `Blocks.CARPET.forEach` in `bootStrap`. Both
    // collections are the sixteen dyed blocks, which is exactly what these tags
    // hold.
    for (tag, ignite_odds, burn_odds) in
        [(&BlockTag::WOOL, 30, 60), (&BlockTag::WOOL_CARPETS, 60, 20)]
    {
        for block in REGISTRY.blocks.iter_tag(tag) {
            table[block.id()] = Flammability {
                ignite_odds,
                burn_odds,
            };
        }
    }
    table.into_boxed_slice()
}

/// Every `setFlammable` call in `FireBlock.bootStrap`, in vanilla's order.
///
/// The wool and carpet collections are missing on purpose; they are applied from
/// their tags in [`build_table`], at the point vanilla reaches them.
static SET_FLAMMABLE: &[(BlockRef, i32, i32)] = &[
    (&vanilla_blocks::OAK_PLANKS, 5, 20),
    (&vanilla_blocks::SPRUCE_PLANKS, 5, 20),
    (&vanilla_blocks::BIRCH_PLANKS, 5, 20),
    (&vanilla_blocks::JUNGLE_PLANKS, 5, 20),
    (&vanilla_blocks::ACACIA_PLANKS, 5, 20),
    (&vanilla_blocks::CHERRY_PLANKS, 5, 20),
    (&vanilla_blocks::DARK_OAK_PLANKS, 5, 20),
    (&vanilla_blocks::PALE_OAK_PLANKS, 5, 20),
    (&vanilla_blocks::MANGROVE_PLANKS, 5, 20),
    (&vanilla_blocks::BAMBOO_PLANKS, 5, 20),
    (&vanilla_blocks::BAMBOO_MOSAIC, 5, 20),
    (&vanilla_blocks::OAK_SLAB, 5, 20),
    (&vanilla_blocks::SPRUCE_SLAB, 5, 20),
    (&vanilla_blocks::BIRCH_SLAB, 5, 20),
    (&vanilla_blocks::JUNGLE_SLAB, 5, 20),
    (&vanilla_blocks::ACACIA_SLAB, 5, 20),
    (&vanilla_blocks::CHERRY_SLAB, 5, 20),
    (&vanilla_blocks::DARK_OAK_SLAB, 5, 20),
    (&vanilla_blocks::PALE_OAK_SLAB, 5, 20),
    (&vanilla_blocks::MANGROVE_SLAB, 5, 20),
    (&vanilla_blocks::BAMBOO_SLAB, 5, 20),
    (&vanilla_blocks::BAMBOO_MOSAIC_SLAB, 5, 20),
    (&vanilla_blocks::OAK_FENCE_GATE, 5, 20),
    (&vanilla_blocks::SPRUCE_FENCE_GATE, 5, 20),
    (&vanilla_blocks::BIRCH_FENCE_GATE, 5, 20),
    (&vanilla_blocks::JUNGLE_FENCE_GATE, 5, 20),
    (&vanilla_blocks::ACACIA_FENCE_GATE, 5, 20),
    (&vanilla_blocks::CHERRY_FENCE_GATE, 5, 20),
    (&vanilla_blocks::DARK_OAK_FENCE_GATE, 5, 20),
    (&vanilla_blocks::PALE_OAK_FENCE_GATE, 5, 20),
    (&vanilla_blocks::MANGROVE_FENCE_GATE, 5, 20),
    (&vanilla_blocks::BAMBOO_FENCE_GATE, 5, 20),
    (&vanilla_blocks::OAK_FENCE, 5, 20),
    (&vanilla_blocks::SPRUCE_FENCE, 5, 20),
    (&vanilla_blocks::BIRCH_FENCE, 5, 20),
    (&vanilla_blocks::JUNGLE_FENCE, 5, 20),
    (&vanilla_blocks::ACACIA_FENCE, 5, 20),
    (&vanilla_blocks::CHERRY_FENCE, 5, 20),
    (&vanilla_blocks::DARK_OAK_FENCE, 5, 20),
    (&vanilla_blocks::PALE_OAK_FENCE, 5, 20),
    (&vanilla_blocks::MANGROVE_FENCE, 5, 20),
    (&vanilla_blocks::BAMBOO_FENCE, 5, 20),
    (&vanilla_blocks::OAK_STAIRS, 5, 20),
    (&vanilla_blocks::BIRCH_STAIRS, 5, 20),
    (&vanilla_blocks::SPRUCE_STAIRS, 5, 20),
    (&vanilla_blocks::JUNGLE_STAIRS, 5, 20),
    (&vanilla_blocks::ACACIA_STAIRS, 5, 20),
    (&vanilla_blocks::CHERRY_STAIRS, 5, 20),
    (&vanilla_blocks::DARK_OAK_STAIRS, 5, 20),
    (&vanilla_blocks::PALE_OAK_STAIRS, 5, 20),
    (&vanilla_blocks::MANGROVE_STAIRS, 5, 20),
    (&vanilla_blocks::BAMBOO_STAIRS, 5, 20),
    (&vanilla_blocks::BAMBOO_MOSAIC_STAIRS, 5, 20),
    (&vanilla_blocks::OAK_LOG, 5, 5),
    (&vanilla_blocks::SPRUCE_LOG, 5, 5),
    (&vanilla_blocks::BIRCH_LOG, 5, 5),
    (&vanilla_blocks::JUNGLE_LOG, 5, 5),
    (&vanilla_blocks::ACACIA_LOG, 5, 5),
    (&vanilla_blocks::CHERRY_LOG, 5, 5),
    (&vanilla_blocks::PALE_OAK_LOG, 5, 5),
    (&vanilla_blocks::DARK_OAK_LOG, 5, 5),
    (&vanilla_blocks::MANGROVE_LOG, 5, 5),
    (&vanilla_blocks::BAMBOO_BLOCK, 5, 5),
    (&vanilla_blocks::STRIPPED_OAK_LOG, 5, 5),
    (&vanilla_blocks::STRIPPED_SPRUCE_LOG, 5, 5),
    (&vanilla_blocks::STRIPPED_BIRCH_LOG, 5, 5),
    (&vanilla_blocks::STRIPPED_JUNGLE_LOG, 5, 5),
    (&vanilla_blocks::STRIPPED_ACACIA_LOG, 5, 5),
    (&vanilla_blocks::STRIPPED_CHERRY_LOG, 5, 5),
    (&vanilla_blocks::STRIPPED_DARK_OAK_LOG, 5, 5),
    (&vanilla_blocks::STRIPPED_PALE_OAK_LOG, 5, 5),
    (&vanilla_blocks::STRIPPED_MANGROVE_LOG, 5, 5),
    (&vanilla_blocks::STRIPPED_BAMBOO_BLOCK, 5, 5),
    (&vanilla_blocks::STRIPPED_OAK_WOOD, 5, 5),
    (&vanilla_blocks::STRIPPED_SPRUCE_WOOD, 5, 5),
    (&vanilla_blocks::STRIPPED_BIRCH_WOOD, 5, 5),
    (&vanilla_blocks::STRIPPED_JUNGLE_WOOD, 5, 5),
    (&vanilla_blocks::STRIPPED_ACACIA_WOOD, 5, 5),
    (&vanilla_blocks::STRIPPED_CHERRY_WOOD, 5, 5),
    (&vanilla_blocks::STRIPPED_DARK_OAK_WOOD, 5, 5),
    (&vanilla_blocks::STRIPPED_PALE_OAK_WOOD, 5, 5),
    (&vanilla_blocks::STRIPPED_MANGROVE_WOOD, 5, 5),
    (&vanilla_blocks::OAK_WOOD, 5, 5),
    (&vanilla_blocks::SPRUCE_WOOD, 5, 5),
    (&vanilla_blocks::BIRCH_WOOD, 5, 5),
    (&vanilla_blocks::JUNGLE_WOOD, 5, 5),
    (&vanilla_blocks::ACACIA_WOOD, 5, 5),
    (&vanilla_blocks::CHERRY_WOOD, 5, 5),
    (&vanilla_blocks::PALE_OAK_WOOD, 5, 5),
    (&vanilla_blocks::DARK_OAK_WOOD, 5, 5),
    (&vanilla_blocks::MANGROVE_WOOD, 5, 5),
    (&vanilla_blocks::MANGROVE_ROOTS, 5, 20),
    (&vanilla_blocks::OAK_LEAVES, 30, 60),
    (&vanilla_blocks::SPRUCE_LEAVES, 30, 60),
    (&vanilla_blocks::BIRCH_LEAVES, 30, 60),
    (&vanilla_blocks::JUNGLE_LEAVES, 30, 60),
    (&vanilla_blocks::ACACIA_LEAVES, 30, 60),
    (&vanilla_blocks::CHERRY_LEAVES, 30, 60),
    (&vanilla_blocks::DARK_OAK_LEAVES, 30, 60),
    (&vanilla_blocks::PALE_OAK_LEAVES, 30, 60),
    (&vanilla_blocks::MANGROVE_LEAVES, 30, 60),
    (&vanilla_blocks::BOOKSHELF, 30, 20),
    (&vanilla_blocks::TNT, 15, 100),
    (&vanilla_blocks::SHORT_GRASS, 60, 100),
    (&vanilla_blocks::FERN, 60, 100),
    (&vanilla_blocks::DEAD_BUSH, 60, 100),
    (&vanilla_blocks::SHORT_DRY_GRASS, 60, 100),
    (&vanilla_blocks::TALL_DRY_GRASS, 60, 100),
    (&vanilla_blocks::SUNFLOWER, 60, 100),
    (&vanilla_blocks::LILAC, 60, 100),
    (&vanilla_blocks::ROSE_BUSH, 60, 100),
    (&vanilla_blocks::PEONY, 60, 100),
    (&vanilla_blocks::TALL_GRASS, 60, 100),
    (&vanilla_blocks::LARGE_FERN, 60, 100),
    (&vanilla_blocks::DANDELION, 60, 100),
    (&vanilla_blocks::GOLDEN_DANDELION, 60, 100),
    (&vanilla_blocks::POPPY, 60, 100),
    (&vanilla_blocks::OPEN_EYEBLOSSOM, 60, 100),
    (&vanilla_blocks::CLOSED_EYEBLOSSOM, 60, 100),
    (&vanilla_blocks::BLUE_ORCHID, 60, 100),
    (&vanilla_blocks::ALLIUM, 60, 100),
    (&vanilla_blocks::AZURE_BLUET, 60, 100),
    (&vanilla_blocks::RED_TULIP, 60, 100),
    (&vanilla_blocks::ORANGE_TULIP, 60, 100),
    (&vanilla_blocks::WHITE_TULIP, 60, 100),
    (&vanilla_blocks::PINK_TULIP, 60, 100),
    (&vanilla_blocks::OXEYE_DAISY, 60, 100),
    (&vanilla_blocks::CORNFLOWER, 60, 100),
    (&vanilla_blocks::LILY_OF_THE_VALLEY, 60, 100),
    (&vanilla_blocks::TORCHFLOWER, 60, 100),
    (&vanilla_blocks::PITCHER_PLANT, 60, 100),
    (&vanilla_blocks::WITHER_ROSE, 60, 100),
    (&vanilla_blocks::PINK_PETALS, 60, 100),
    (&vanilla_blocks::WILDFLOWERS, 60, 100),
    (&vanilla_blocks::LEAF_LITTER, 60, 100),
    (&vanilla_blocks::CACTUS_FLOWER, 60, 100),
    // `Blocks.WOOL.forEach(block -> setFlammable(block, 30, 60))` lands here.
    (&vanilla_blocks::VINE, 15, 100),
    (&vanilla_blocks::COAL_BLOCK, 5, 5),
    (&vanilla_blocks::HAY_BLOCK, 60, 20),
    (&vanilla_blocks::TARGET, 15, 20),
    // `Blocks.CARPET.forEach(block -> setFlammable(block, 60, 20))` lands here.
    (&vanilla_blocks::PALE_MOSS_BLOCK, 5, 100),
    (&vanilla_blocks::PALE_MOSS_CARPET, 5, 100),
    (&vanilla_blocks::PALE_HANGING_MOSS, 5, 100),
    (&vanilla_blocks::DRIED_KELP_BLOCK, 30, 60),
    (&vanilla_blocks::BAMBOO, 60, 60),
    (&vanilla_blocks::SCAFFOLDING, 60, 60),
    (&vanilla_blocks::LECTERN, 30, 20),
    (&vanilla_blocks::COMPOSTER, 5, 20),
    (&vanilla_blocks::SWEET_BERRY_BUSH, 60, 100),
    (&vanilla_blocks::BEEHIVE, 5, 20),
    (&vanilla_blocks::BEE_NEST, 30, 20),
    (&vanilla_blocks::AZALEA_LEAVES, 30, 60),
    (&vanilla_blocks::FLOWERING_AZALEA_LEAVES, 30, 60),
    (&vanilla_blocks::CAVE_VINES, 15, 60),
    (&vanilla_blocks::CAVE_VINES_PLANT, 15, 60),
    (&vanilla_blocks::SPORE_BLOSSOM, 60, 100),
    (&vanilla_blocks::AZALEA, 30, 60),
    (&vanilla_blocks::FLOWERING_AZALEA, 30, 60),
    (&vanilla_blocks::BIG_DRIPLEAF, 60, 100),
    (&vanilla_blocks::BIG_DRIPLEAF_STEM, 60, 100),
    (&vanilla_blocks::SMALL_DRIPLEAF, 60, 100),
    (&vanilla_blocks::HANGING_ROOTS, 30, 60),
    (&vanilla_blocks::GLOW_LICHEN, 15, 100),
    (&vanilla_blocks::FIREFLY_BUSH, 60, 100),
    (&vanilla_blocks::BUSH, 60, 100),
    (&vanilla_blocks::ACACIA_SHELF, 30, 20),
    (&vanilla_blocks::BAMBOO_SHELF, 30, 20),
    (&vanilla_blocks::BIRCH_SHELF, 30, 20),
    (&vanilla_blocks::CHERRY_SHELF, 30, 20),
    (&vanilla_blocks::DARK_OAK_SHELF, 30, 20),
    (&vanilla_blocks::JUNGLE_SHELF, 30, 20),
    (&vanilla_blocks::MANGROVE_SHELF, 30, 20),
    (&vanilla_blocks::OAK_SHELF, 30, 20),
    (&vanilla_blocks::PALE_OAK_SHELF, 30, 20),
    (&vanilla_blocks::SPRUCE_SHELF, 30, 20),
];

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_blocks};

    use super::{Flammability, flammability};

    /// The table is only useful if it is exactly vanilla's, so spot-check the
    /// three shapes it has: a plain entry, one that comes from a tag, and the
    /// silence that keeps stone from burning.
    #[test]
    fn the_table_matches_the_bootstrap_it_mirrors() {
        init_vanilla_registry();

        let oak = flammability(&vanilla_blocks::OAK_PLANKS);
        assert_eq!((oak.ignite_odds, oak.burn_odds), (5, 20));

        let wool = flammability(&vanilla_blocks::LIME_WOOL);
        assert_eq!((wool.ignite_odds, wool.burn_odds), (30, 60));

        let carpet = flammability(&vanilla_blocks::LIME_CARPET);
        assert_eq!((carpet.ignite_odds, carpet.burn_odds), (60, 20));

        let stone = flammability(&vanilla_blocks::STONE);
        assert_eq!(
            (stone.ignite_odds, stone.burn_odds),
            (Flammability::NONE.ignite_odds, Flammability::NONE.burn_odds)
        );
    }
}
