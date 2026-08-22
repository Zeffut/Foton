//! Infested block behavior.
//!
//! Vanilla parity: `InfestedBlock` and `InfestedRotatedPillarBlock`. Seven
//! blocks that pass for ordinary stone until something breaks them, at which
//! point the silverfish hiding inside comes out. They are also what a silverfish
//! burrows into, and what it calls on for help when it is hurt.

use std::sync::{Arc, LazyLock};

use glam::DVec3;
use rustc_hash::FxHashMap;
use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, EnumProperty};
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_enchantment_tags::EnchantmentTag;
use steel_registry::vanilla_game_rules::BLOCK_DROPS;
use steel_registry::{REGISTRY, RegistryExt as _, TaggedRegistryExt as _, vanilla_entities};
use steel_utils::axis::Axis;
use steel_utils::{BlockPos, BlockStateId, Identifier};

use super::rotated_pillar_block::RotatedPillarBlock;
use crate::behavior::BLOCK_BEHAVIORS;
use crate::behavior::block::BlockBehavior;
use crate::behavior::context::BlockPlaceContext;
use crate::entity::{ENTITIES, EntitySpawnReason, next_entity_id};
use crate::world::World;
use steel_utils::entity_events::EntityStatus;

/// The shared capability of the blocks a silverfish can hide in.
///
/// Vanilla keeps the host pairing on the `InfestedBlock` class itself; Steel
/// splits it into a capability so the deepslate variant, which is a rotated
/// pillar rather than a plain block, can carry it too.
pub trait Infested {
    /// Returns the ordinary block this one is pretending to be.
    ///
    /// Vanilla parity: `InfestedBlock.getHostBlock`.
    fn host_block(&self) -> BlockRef;

    /// Carries the properties the host and infested blocks share across a
    /// conversion.
    ///
    /// Vanilla parity: the property loop of
    /// `InfestedBlock.getNewStateWithProperties`, which copies every property
    /// the two states have in common. Of the seven vanilla pairs only deepslate
    /// has one at all, so the default keeps the destination untouched.
    fn copy_shared_properties(&self, _from: BlockStateId, to: BlockStateId) -> BlockStateId {
        to
    }
}

/// Behavior for the six infested stone variants.
#[block_behavior(class = "InfestedBlock")]
pub struct InfestedBlock {
    block: BlockRef,
    #[json_arg(vanilla_blocks, json = "host_block")]
    host_block: BlockRef,
}

/// Behavior for infested deepslate, which is a pillar and keeps an axis.
#[block_behavior(class = "InfestedRotatedPillarBlock")]
pub struct InfestedRotatedPillarBlock {
    block: BlockRef,
    #[json_arg(vanilla_blocks, json = "host_block")]
    host_block: BlockRef,
}

impl InfestedBlock {
    /// Creates the behavior for one infested block.
    #[must_use]
    pub const fn new(block: BlockRef, host_block: BlockRef) -> Self {
        Self { block, host_block }
    }

    /// Returns this behavior's registered block.
    #[must_use]
    pub const fn block(&self) -> BlockRef {
        self.block
    }
}

impl InfestedRotatedPillarBlock {
    /// Creates the behavior for infested deepslate.
    #[must_use]
    pub const fn new(block: BlockRef, host_block: BlockRef) -> Self {
        Self { block, host_block }
    }
}

impl Infested for InfestedBlock {
    fn host_block(&self) -> BlockRef {
        self.host_block
    }
}

impl Infested for InfestedRotatedPillarBlock {
    fn host_block(&self) -> BlockRef {
        self.host_block
    }

    /// Keeps the pillar lying the way it was laid.
    ///
    /// Vanilla parity: the same property loop as the plain variant; deepslate is
    /// the only host that brings a property with it.
    fn copy_shared_properties(&self, from: BlockStateId, to: BlockStateId) -> BlockStateId {
        to.copy_value(AXIS, &from)
    }
}

/// The axis both deepslate and infested deepslate carry.
const AXIS: &EnumProperty<Axis> = &BlockStateProperties::AXIS;

/// Every host block, mapped to the infested block that mimics it.
///
/// Vanilla parity: `InfestedBlock.BLOCK_BY_HOST_BLOCK`, which each constructor
/// populates as the blocks register. Steel builds it once on first use by
/// asking the behavior registry, so it reads the same `host_block` pairing
/// `classes.json` already carries. This must not be touched before
/// [`crate::behavior::init_behaviors`] has run.
static INFESTED_BY_HOST: LazyLock<FxHashMap<Identifier, BlockRef>> = LazyLock::new(|| {
    REGISTRY
        .blocks
        .iter()
        .filter_map(|(_, block)| {
            let behavior = BLOCK_BEHAVIORS.get_behavior(block);
            let infested = behavior.as_infested()?;
            Some((infested.host_block().key.clone(), block))
        })
        .collect()
});

/// Returns whether a silverfish could burrow into this block.
///
/// Vanilla parity: `InfestedBlock.isCompatibleHostBlock`.
#[must_use]
pub fn is_compatible_host_block(state: BlockStateId) -> bool {
    INFESTED_BY_HOST.contains_key(&state.get_block().key)
}

/// Returns the infested state that replaces `host_state`.
///
/// Vanilla parity: `InfestedBlock.infestedStateByHost`. Returns `None` for a
/// block no silverfish can hide in.
#[must_use]
pub fn infested_state_by_host(host_state: BlockStateId) -> Option<BlockStateId> {
    let infested_block = *INFESTED_BY_HOST.get(&host_state.get_block().key)?;
    let behavior = BLOCK_BEHAVIORS.get_behavior(infested_block);
    let infested = behavior.as_infested()?;
    Some(infested.copy_shared_properties(host_state, infested_block.default_state()))
}

/// Returns the ordinary state hiding under `infested_state`.
///
/// Vanilla parity: `InfestedBlock.hostStateByInfested`. Returns `None` when the
/// state is not an infested block at all.
#[must_use]
pub fn host_state_by_infested(infested_state: BlockStateId) -> Option<BlockStateId> {
    let behavior = BLOCK_BEHAVIORS.get_behavior(infested_state.get_block());
    let infested = behavior.as_infested()?;
    Some(infested.copy_shared_properties(infested_state, infested.host_block().default_state()))
}

/// Releases the silverfish that was hiding in the block.
///
/// Vanilla parity: `InfestedBlock.spawnInfestation`.
pub fn spawn_infestation(world: &Arc<World>, pos: BlockPos) {
    let position = DVec3::new(
        f64::from(pos.x()) + 0.5,
        f64::from(pos.y()),
        f64::from(pos.z()) + 0.5,
    );
    let Some(silverfish) = ENTITIES.create(
        &vanilla_entities::SILVERFISH,
        next_entity_id(),
        position,
        Arc::downgrade(world),
    ) else {
        return;
    };

    if let Some(mob) = silverfish.as_mob() {
        let _ = mob.finalize_spawn(world, EntitySpawnReason::Triggered, None);
    }

    if let Err(error) = world.try_add_entity(Arc::clone(&silverfish)) {
        log::debug!("infestation rejected at {pos:?}: {error}");
        return;
    }

    // Vanilla parity: `spawnAnim`, the puff of particles that sells the
    // silverfish bursting out of the stone.
    silverfish.broadcast_entity_event(EntityStatus::Poof);
}

/// Returns whether `tool` keeps the silverfish inside.
///
/// Vanilla parity: the `EnchantmentTags.PREVENTS_INFESTED_SPAWNS` check of
/// `InfestedBlock.spawnAfterBreak`, which is how Silk Touch mines infested
/// stone without waking anything.
#[must_use]
pub fn prevents_infested_spawns(tool: &ItemStack) -> bool {
    let Some(enchantments) = tool.get_enchantments() else {
        return false;
    };
    enchantments.iter().any(|(key, _)| {
        REGISTRY
            .enchantments
            .by_key(key)
            .is_some_and(|enchantment| {
                REGISTRY
                    .enchantments
                    .is_in_tag(enchantment, &EnchantmentTag::PREVENTS_INFESTED_SPAWNS)
            })
    })
}

/// The shared body of `InfestedBlock.spawnAfterBreak`.
fn spawn_after_break_infested(world: &Arc<World>, pos: BlockPos, tool: &ItemStack) {
    if !world.get_game_rule(&BLOCK_DROPS) || prevents_infested_spawns(tool) {
        return;
    }
    spawn_infestation(world, pos);
}

impl BlockBehavior for InfestedBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn as_infested(&self) -> Option<&dyn Infested> {
        Some(self)
    }

    /// Vanilla parity: `InfestedBlock.spawnAfterBreak`.
    fn spawn_after_break(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        tool: &ItemStack,
        _drop_experience: bool,
    ) {
        spawn_after_break_infested(world, pos, tool);
    }
}

impl BlockBehavior for InfestedRotatedPillarBlock {
    fn as_infested(&self) -> Option<&dyn Infested> {
        Some(self)
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(RotatedPillarBlock::placement_state(self.block, context))
    }

    /// Vanilla parity: `InfestedBlock.spawnAfterBreak`, inherited unchanged.
    fn spawn_after_break(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        tool: &ItemStack,
        _drop_experience: bool,
    ) {
        spawn_after_break_infested(world, pos, tool);
    }
}
