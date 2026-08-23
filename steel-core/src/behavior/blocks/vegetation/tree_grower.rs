//! What a sapling grows into.
//!
//! Vanilla parity: `TreeGrower`, the table that turns a sapling into a
//! configured tree feature. Every entry here is transcribed from the static
//! constants of `TreeGrower.java`; the sapling that points at each one comes
//! from the extracted `tree_grower_name`.
//!
//! The interesting part is not the table but that it can run at all: a tree is
//! a worldgen feature, and worldgen features write through a `WorldGenRegion`
//! that only exists while a chunk is being generated. The tree feature is
//! written against [`LevelAccessor`] instead, so the same code that grows a
//! forest during generation grows one from a sapling in a live world -- through
//! `World::set_block`, so the client sees it.

use std::sync::Arc;

use rand::{Rng, RngExt};
use steel_registry::REGISTRY;
use steel_registry::RegistryExt as _;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::feature::ConfiguredFeatureKind;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_blocks;
use steel_utils::random::worldgen_random::WorldgenRandom;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, Identifier};

use crate::fluid::fluid_state_to_block;
use crate::world::{LevelReader as _, World};
use crate::worldgen::feature::FeatureDecorationRunner;

/// How far around a sapling vanilla looks for flowers.
///
/// Vanilla parity: the `below().north(2).west(2)` to `above().south(2).east(2)`
/// box of `TreeGrower.hasFlowers`. Flowers near an oak are what turn it into a
/// bee-nest oak, which is the whole reason the check exists.
const FLOWER_SEARCH_RADIUS: i32 = 2;

/// One sapling's worth of tree choices.
///
/// Vanilla parity: `TreeGrower`. The keys are vanilla configured-feature paths,
/// resolved through the registry when a tree is actually grown.
pub struct TreeGrower {
    /// Chance of preferring the secondary variant of each pair.
    secondary_chance: f32,
    /// The 2x2 tree, if this sapling can make one.
    mega_tree: Option<&'static str>,
    /// The other 2x2 tree.
    secondary_mega_tree: Option<&'static str>,
    /// The ordinary tree.
    tree: Option<&'static str>,
    /// The other ordinary tree.
    secondary_tree: Option<&'static str>,
    /// The tree grown when there are flowers nearby.
    flowers: Option<&'static str>,
    /// The other flowering tree.
    secondary_flowers: Option<&'static str>,
}

impl TreeGrower {
    const fn simple(
        mega_tree: Option<&'static str>,
        tree: Option<&'static str>,
        flowers: Option<&'static str>,
    ) -> Self {
        Self {
            secondary_chance: 0.0,
            mega_tree,
            secondary_mega_tree: None,
            tree,
            secondary_tree: None,
            flowers,
            secondary_flowers: None,
        }
    }

    /// Returns the grower a sapling names, if it is one Steel knows.
    ///
    /// Vanilla parity: the `GROWERS` map `TreeGrower`'s constructor fills.
    #[must_use]
    pub fn by_name(name: &str) -> Option<&'static Self> {
        Some(match name {
            "oak" => &OAK,
            "spruce" => &SPRUCE,
            "birch" => &BIRCH,
            "jungle" => &JUNGLE,
            "acacia" => &ACACIA,
            "cherry" => &CHERRY,
            "dark_oak" => &DARK_OAK,
            "pale_oak" => &PALE_OAK,
            "mangrove" => &MANGROVE,
            "azalea" => &AZALEA,
            _ => return None,
        })
    }

    /// Vanilla parity: `TreeGrower.getConfiguredFeature`.
    fn configured_feature(&self, rng: &mut dyn Rng, has_flowers: bool) -> Option<&'static str> {
        if rng.random::<f32>() < self.secondary_chance {
            if has_flowers && self.secondary_flowers.is_some() {
                return self.secondary_flowers;
            }
            if self.secondary_tree.is_some() {
                return self.secondary_tree;
            }
        }

        if has_flowers && self.flowers.is_some() {
            self.flowers
        } else {
            self.tree
        }
    }

    /// Vanilla parity: `TreeGrower.getConfiguredMegaFeature`.
    fn configured_mega_feature(&self, rng: &mut dyn Rng) -> Option<&'static str> {
        if self.secondary_mega_tree.is_some() && rng.random::<f32>() < self.secondary_chance {
            self.secondary_mega_tree
        } else {
            self.mega_tree
        }
    }

    /// Grows the tree, replacing the sapling.
    ///
    /// Vanilla parity: `TreeGrower.growTree`. Returns whether a tree was placed;
    /// on failure the sapling is put back exactly as it was, which is what lets
    /// a player try again rather than losing it.
    pub fn grow_tree(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        state: BlockStateId,
        rng: &mut dyn Rng,
    ) -> bool {
        if let Some(mega) = self.configured_mega_feature(rng)
            && Self::grow_mega_tree(world, pos, state, rng, mega)
        {
            return true;
        }

        let has_flowers = has_flowers_near(world, pos);
        let Some(key) = self.configured_feature(rng, has_flowers) else {
            return false;
        };

        // Vanilla clears the sapling first so the trunk placer sees an empty
        // column, and restores it when the tree does not fit.
        let cleared = fluid_state_to_block(world.get_block_state(pos).get_fluid_state());
        world.set_block(pos, cleared, UpdateFlags::UPDATE_CLIENTS);

        if place_tree(world, pos, rng, key) {
            true
        } else {
            world.set_block(pos, state, UpdateFlags::UPDATE_CLIENTS);
            false
        }
    }

    /// Grows a 2x2 tree if this sapling is one corner of a 2x2 patch.
    ///
    /// Vanilla parity: the mega-tree half of `TreeGrower.growTree`, which tries
    /// each of the four corners the sapling could be.
    fn grow_mega_tree(
        world: &Arc<World>,
        pos: BlockPos,
        state: BlockStateId,
        rng: &mut dyn Rng,
        key: &'static str,
    ) -> bool {
        for dx in [0, -1] {
            for dz in [0, -1] {
                if !is_two_by_two_sapling(world, state, pos, dx, dz) {
                    continue;
                }

                let corners = [
                    pos.offset(dx, 0, dz),
                    pos.offset(dx + 1, 0, dz),
                    pos.offset(dx, 0, dz + 1),
                    pos.offset(dx + 1, 0, dz + 1),
                ];
                let air = vanilla_blocks::AIR.default_state();
                for corner in corners {
                    world.set_block(corner, air, UpdateFlags::UPDATE_CLIENTS);
                }

                if place_tree(world, corners[0], rng, key) {
                    return true;
                }

                for corner in corners {
                    world.set_block(corner, state, UpdateFlags::UPDATE_CLIENTS);
                }
                return false;
            }
        }

        false
    }
}

/// Places one configured tree feature in a live world.
fn place_tree(world: &Arc<World>, pos: BlockPos, rng: &mut dyn Rng, key: &'static str) -> bool {
    let identifier = Identifier::vanilla_static(key);
    let Some(feature) = REGISTRY.configured_features.by_key(&identifier) else {
        log::warn!("a tree grower names an unknown configured feature {identifier}");
        return false;
    };
    let ConfiguredFeatureKind::Tree(config) = &feature.kind else {
        log::warn!("a tree grower names {identifier}, which is not a tree feature");
        return false;
    };

    let mut random = WorldgenRandom::from_seed(rng.random());
    // A tree grown from a sapling has no nested ground feature to place: the
    // only one is the pale moss patch, and placing it needs the worldgen
    // dispatcher. A pale oak grown by hand comes up without its moss.
    let mut ground_features = Vec::new();
    FeatureDecorationRunner::place_tree_feature(
        world,
        &REGISTRY,
        &mut random,
        config,
        pos,
        &mut ground_features,
    )
}

/// Vanilla parity: `TreeGrower.isTwoByTwoSapling`.
fn is_two_by_two_sapling(
    world: &Arc<World>,
    state: BlockStateId,
    pos: BlockPos,
    dx: i32,
    dz: i32,
) -> bool {
    let block = state.get_block();
    [(0, 0), (1, 0), (0, 1), (1, 1)]
        .into_iter()
        .all(|(ox, oz)| {
            world
                .get_block_state(pos.offset(dx + ox, 0, dz + oz))
                .get_block()
                == block
        })
}

/// Vanilla parity: `TreeGrower.hasFlowers`.
fn has_flowers_near(world: &Arc<World>, pos: BlockPos) -> bool {
    let from = pos.offset(-FLOWER_SEARCH_RADIUS, -1, -FLOWER_SEARCH_RADIUS);
    let to = pos.offset(FLOWER_SEARCH_RADIUS, 1, FLOWER_SEARCH_RADIUS);
    BlockPos::between_closed(from, to).any(|candidate| {
        world
            .get_block_state(candidate)
            .get_block()
            .has_tag(&BlockTag::FLOWERS)
    })
}

// The table, transcribed from the static constants of `TreeGrower.java`.

static OAK: TreeGrower = TreeGrower {
    secondary_chance: 0.1,
    mega_tree: None,
    secondary_mega_tree: None,
    tree: Some("oak"),
    secondary_tree: Some("fancy_oak"),
    flowers: Some("oak_bees_005"),
    secondary_flowers: Some("fancy_oak_bees_005"),
};

static SPRUCE: TreeGrower = TreeGrower {
    secondary_chance: 0.5,
    mega_tree: Some("mega_spruce"),
    secondary_mega_tree: Some("mega_pine"),
    tree: Some("spruce"),
    secondary_tree: None,
    flowers: None,
    secondary_flowers: None,
};

static MANGROVE: TreeGrower = TreeGrower {
    secondary_chance: 0.85,
    mega_tree: None,
    secondary_mega_tree: None,
    tree: Some("mangrove"),
    secondary_tree: Some("tall_mangrove"),
    flowers: None,
    secondary_flowers: None,
};

static AZALEA: TreeGrower = TreeGrower::simple(None, Some("azalea_tree"), None);
static BIRCH: TreeGrower = TreeGrower::simple(None, Some("birch"), Some("birch_bees_005"));
static JUNGLE: TreeGrower =
    TreeGrower::simple(Some("mega_jungle_tree"), Some("jungle_tree_no_vine"), None);
static ACACIA: TreeGrower = TreeGrower::simple(None, Some("acacia"), None);
static CHERRY: TreeGrower = TreeGrower::simple(None, Some("cherry"), Some("cherry_bees_005"));
static DARK_OAK: TreeGrower = TreeGrower::simple(Some("dark_oak"), None, None);
static PALE_OAK: TreeGrower = TreeGrower::simple(Some("pale_oak_bonemeal"), None, None);

#[cfg(test)]
mod tests {
    use steel_registry::init_vanilla_registry;

    use super::*;

    /// Every sapling in the extracted data names a grower this table knows.
    ///
    /// Without this, a sapling whose grower is missing would simply never grow
    /// and nothing would say so.
    #[test]
    fn every_sapling_grower_name_resolves() {
        for name in [
            "oak", "spruce", "birch", "jungle", "acacia", "cherry", "dark_oak", "pale_oak",
            "mangrove", "azalea",
        ] {
            assert!(
                TreeGrower::by_name(name).is_some(),
                "no grower named {name}"
            );
        }
        assert!(TreeGrower::by_name("nonexistent").is_none());
    }

    /// Every feature the table names exists and really is a tree.
    ///
    /// A typo here would be a sapling that consumes itself and grows nothing.
    #[test]
    fn every_named_feature_is_a_tree_that_exists() {
        init_vanilla_registry();

        for grower in [
            &OAK, &SPRUCE, &MANGROVE, &AZALEA, &BIRCH, &JUNGLE, &ACACIA, &CHERRY, &DARK_OAK,
            &PALE_OAK,
        ] {
            for key in [
                grower.mega_tree,
                grower.secondary_mega_tree,
                grower.tree,
                grower.secondary_tree,
                grower.flowers,
                grower.secondary_flowers,
            ]
            .into_iter()
            .flatten()
            {
                let identifier = Identifier::vanilla_static(key);
                let feature = REGISTRY
                    .configured_features
                    .by_key(&identifier)
                    .unwrap_or_else(|| panic!("unknown configured feature {identifier}"));
                assert!(
                    matches!(feature.kind, ConfiguredFeatureKind::Tree(_)),
                    "{identifier} is not a tree feature"
                );
            }
        }
    }

    /// A grower with no ordinary tree only ever makes a 2x2 one.
    #[test]
    fn dark_oak_has_no_single_sapling_tree() {
        let mut rng = rand::rng();
        assert!(DARK_OAK.configured_feature(&mut rng, false).is_none());
        assert_eq!(DARK_OAK.configured_mega_feature(&mut rng), Some("dark_oak"));
    }

    /// Flowers turn an oak into a bee-nest oak.
    #[test]
    fn flowers_change_what_an_oak_grows_into() {
        let mut rng = rand::rng();
        let plain = OAK.configured_feature(&mut rng, false);
        let flowering = OAK.configured_feature(&mut rng, true);

        assert!(matches!(plain, Some("oak" | "fancy_oak")));
        assert!(matches!(
            flowering,
            Some("oak_bees_005" | "fancy_oak_bees_005")
        ));
    }
}
