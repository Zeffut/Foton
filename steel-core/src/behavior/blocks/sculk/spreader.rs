//! The sculk charge spreader.
//!
//! Vanilla parity: `SculkSpreader`, `SculkSpreader.ChargeCursor`, and the `SculkBehaviour`
//! implementations carried by `SculkBlock` and `SculkVeinBlock`.
//!
//! This is the algorithm that turns experience into sculk. A catalyst drops charge cursors
//! where a mob died; each cursor walks the sculk it stands on, converts neighboring blocks
//! in the spreader's replaceable tag, occasionally grows a sensor or a shrieker, and decays
//! with distance until it is spent. World generation runs the same walk with a harsher
//! configuration to carve the deep dark.
//!
//! Both callers reach it through [`LevelAccessor`], so the only difference between a live
//! world and a generating chunk is which surface answers `get_block_state`, and whether the
//! sounds and particle events land anywhere -- a worldgen region drops them, exactly as
//! vanilla's `WorldGenRegion.playSound` and `WorldGenRegion.levelEvent` do.
//!
//! Deviation: vanilla dispatches through the `SculkBehaviour` interface a block implements.
//! Steel resolves the same three cases -- default, sculk, sculk vein -- from block identity
//! through [`SculkBehaviorKind`], because only two vanilla blocks implement the interface and
//! seven block-behavior methods for two implementors would cost more than they carry.
//!
//! Not implemented: `Block.pushEntitiesUp` when a vein converts its support into sculk.
//! Steel's `push_entities_up` needs a live `World` to query entities, which this shared walk
//! does not have, so a mob standing in the exact block that becomes sculk is not lifted out.

use core::mem;

use rustc_hash::FxHashMap;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_math::fast_floor;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::blocks::shapes;
use steel_registry::fluid::FluidStateExt as _;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::{
    Registry, TaggedRegistryExt as _, level_events, sound_events, vanilla_block_entity_types,
    vanilla_blocks,
};
use steel_utils::axis::Axis;
use steel_utils::random::Random as _;
use steel_utils::random::worldgen_random::WorldgenRandom;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, Direction, Identifier};

use crate::behavior::blocks::multiface_face_property;
use crate::fluid::state::get_fluid_state_from_block;
use crate::world::LevelAccessor;

/// Vanilla `MultifaceSpreader.DEFAULT_SPREAD_ORDER`.
const DEFAULT_SPREAD_TYPES: [SpreadType; 3] = [
    SpreadType::SamePosition,
    SpreadType::SamePlane,
    SpreadType::WrapAround,
];
/// Vanilla `SculkVeinBlock.sameSpaceSpreader`, which only reuses the block it already sits in.
const SAME_SPACE_SPREAD_TYPES: [SpreadType; 1] = [SpreadType::SamePosition];

/// Vanilla `MultifaceSpreader.SpreadType`.
#[derive(Clone, Copy)]
enum SpreadType {
    SamePosition,
    SamePlane,
    WrapAround,
}

/// Vanilla `MultifaceSpreader.SpreadPos`.
struct SpreadPos {
    pos: BlockPos,
    face: Direction,
}

/// Which vanilla `SculkBehaviour` a block state answers with.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SculkBehaviorKind {
    /// Vanilla `SculkBehaviour.DEFAULT`, used by every block that is not sculk.
    Default,
    /// Vanilla `SculkBlock`.
    Sculk,
    /// Vanilla `SculkVeinBlock`.
    SculkVein,
}

/// Vanilla `SculkSpreader`.
pub struct SculkSpreader {
    is_world_generation: bool,
    replaceable_blocks: Identifier,
    growth_spawn_cost: i32,
    no_growth_radius: i32,
    charge_decay_rate: i32,
    additional_decay_rate: i32,
    cursors: Vec<ChargeCursor>,
}

impl SculkSpreader {
    /// Vanilla `SculkSpreader.MAX_CHARGE`.
    pub const MAX_CHARGE: i32 = 1000;
    /// Vanilla `SculkSpreader.MAX_CURSORS`.
    pub const MAX_CURSORS: usize = 32;
    /// Vanilla `SculkSpreader.MAX_GROWTH_RATE_RADIUS`.
    const MAX_GROWTH_RATE_RADIUS: i32 = 24;

    /// Vanilla `SculkSpreader.createLevelSpreader`, the one a catalyst drives.
    #[must_use]
    pub const fn level() -> Self {
        Self {
            is_world_generation: false,
            replaceable_blocks: BlockTag::SCULK_REPLACEABLE,
            growth_spawn_cost: 10,
            no_growth_radius: 4,
            charge_decay_rate: 10,
            additional_decay_rate: 5,
            cursors: Vec::new(),
        }
    }

    /// Vanilla `SculkSpreader.createWorldGenSpreader`, the one that carves the deep dark.
    #[must_use]
    pub const fn worldgen() -> Self {
        Self {
            is_world_generation: true,
            replaceable_blocks: BlockTag::SCULK_REPLACEABLE_WORLD_GEN,
            growth_spawn_cost: 50,
            no_growth_radius: 1,
            charge_decay_rate: 5,
            additional_decay_rate: 10,
            cursors: Vec::new(),
        }
    }

    /// Returns whether this spreader runs during world generation.
    #[must_use]
    pub const fn is_world_generation(&self) -> bool {
        self.is_world_generation
    }

    /// Returns the live charge cursors.
    #[must_use]
    pub fn cursors(&self) -> &[ChargeCursor] {
        &self.cursors
    }

    /// Returns the total charge still held across every cursor.
    #[must_use]
    pub fn total_charge(&self) -> i32 {
        self.cursors.iter().map(|cursor| cursor.charge).sum()
    }

    /// Vanilla `SculkSpreader.addCursors`: splits a reward across capped cursors.
    pub fn add_cursors(&mut self, start_pos: BlockPos, mut charge: i32) {
        while charge > 0 {
            let current_charge = charge.min(Self::MAX_CHARGE);
            self.add_cursor(ChargeCursor::new(start_pos, current_charge));
            charge -= current_charge;
        }
    }

    fn add_cursor(&mut self, cursor: ChargeCursor) {
        if self.cursors.len() < Self::MAX_CURSORS {
            self.cursors.push(cursor);
        }
    }

    /// Vanilla `SculkSpreader.clear`.
    pub fn clear(&mut self) {
        self.cursors.clear();
    }

    /// Vanilla `SculkSpreader.load`.
    pub fn load(&mut self, nbt: &BorrowedNbtCompoundView<'_, '_>) {
        self.cursors.clear();
        let Some(list) = nbt.list("cursors") else {
            return;
        };
        let Some(compounds) = list.compounds() else {
            return;
        };
        for compound in compounds {
            if self.cursors.len() >= Self::MAX_CURSORS {
                break;
            }
            if let Some(cursor) = ChargeCursor::load(&compound) {
                self.cursors.push(cursor);
            }
        }
    }

    /// Vanilla `SculkSpreader.save`.
    pub fn save(&self, nbt: &mut NbtCompound) {
        let cursors = self
            .cursors
            .iter()
            .map(ChargeCursor::save)
            .collect::<Vec<_>>();
        nbt.insert("cursors", NbtList::Compound(cursors));
    }

    /// Vanilla `SculkSpreader.updateCursors`: one pass over every live cursor.
    ///
    /// Cursors that land on the same block merge, which is what stops a large reward from
    /// spending 32 separate walks on the same spot. Vanilla skips the merge during world
    /// generation, and so does this, which is why worldgen output is unchanged by the lift.
    pub fn update_cursors(
        &mut self,
        level: &impl LevelAccessor,
        registry: &Registry,
        origin: BlockPos,
        random: &mut WorldgenRandom,
        spread_veins: bool,
    ) {
        if self.cursors.is_empty() {
            return;
        }

        let mut processed: Vec<ChargeCursor> = Vec::new();
        let mut mergeable: FxHashMap<BlockPos, usize> = FxHashMap::default();
        let mut charge_at: Vec<(BlockPos, i32)> = Vec::new();
        let mut charge_index: FxHashMap<BlockPos, usize> = FxHashMap::default();

        for mut cursor in mem::take(&mut self.cursors) {
            if cursor.is_pos_unreasonable(origin) {
                continue;
            }

            cursor.update(level, registry, origin, random, self, spread_veins);
            if cursor.charge <= 0 {
                level.level_event(level_events::PARTICLES_SCULK_CHARGE, cursor.pos, 0, None);
                continue;
            }

            let pos = cursor.pos;
            match charge_index.get(&pos) {
                Some(&index) => charge_at[index].1 += cursor.charge,
                None => {
                    charge_index.insert(pos, charge_at.len());
                    charge_at.push((pos, cursor.charge));
                }
            }

            let Some(&existing) = mergeable.get(&pos) else {
                mergeable.insert(pos, processed.len());
                processed.push(cursor);
                continue;
            };

            if !self.is_world_generation
                && cursor.charge + processed[existing].charge <= Self::MAX_CHARGE
            {
                processed[existing].merge_with(&mut cursor);
                continue;
            }

            let is_lower = cursor.charge < processed[existing].charge;
            processed.push(cursor);
            if is_lower {
                mergeable.insert(pos, processed.len() - 1);
            }
        }

        Self::emit_charge_particles(level, &processed, &mergeable, &charge_at);
        self.cursors = processed;
    }

    /// The tail of vanilla `SculkSpreader.updateCursors`, which turns the per-block charge
    /// totals into one `PARTICLES_SCULK_CHARGE` level event each.
    fn emit_charge_particles(
        level: &impl LevelAccessor,
        processed: &[ChargeCursor],
        mergeable: &FxHashMap<BlockPos, usize>,
        charge_at: &[(BlockPos, i32)],
    ) {
        for &(pos, charge) in charge_at {
            if charge <= 0 {
                continue;
            }
            let Some(&index) = mergeable.get(&pos) else {
                continue;
            };
            let Some(faces) = processed[index].facings.as_deref() else {
                continue;
            };
            let particle_count = particle_count_for_charge(charge);
            let data = (particle_count << 6) + i32::from(pack_faces(faces));
            level.level_event(level_events::PARTICLES_SCULK_CHARGE, pos, data, None);
        }
    }
}

/// Vanilla `(int)(Math.log1p(charge) / 2.3F) + 1`.
fn particle_count_for_charge(charge: i32) -> i32 {
    (f64::from(charge).ln_1p() / f64::from(2.3_f32)) as i32 + 1
}

/// Vanilla `SculkSpreader.ChargeCursor`.
pub struct ChargeCursor {
    pos: BlockPos,
    charge: i32,
    update_delay: i32,
    decay_delay: i32,
    facings: Option<Vec<Direction>>,
}

impl ChargeCursor {
    /// Vanilla `SculkSpreader.MAX_CURSOR_DISTANCE`.
    const MAX_CURSOR_DISTANCE: i32 = 1024;
    /// Vanilla `ChargeCursor.MAX_CURSOR_DECAY_DELAY`.
    const MAX_CURSOR_DECAY_DELAY: i32 = 1;

    const fn new(pos: BlockPos, charge: i32) -> Self {
        Self {
            pos,
            charge,
            update_delay: 0,
            decay_delay: Self::MAX_CURSOR_DECAY_DELAY,
            facings: None,
        }
    }

    /// Returns where this cursor currently sits.
    #[must_use]
    pub const fn pos(&self) -> BlockPos {
        self.pos
    }

    /// Returns the charge this cursor still carries.
    #[must_use]
    pub const fn charge(&self) -> i32 {
        self.charge
    }

    fn load(nbt: &BorrowedNbtCompoundView<'_, '_>) -> Option<Self> {
        let pos = nbt.int_array("pos")?;
        let [x, y, z] = pos[..] else {
            return None;
        };
        let facings = nbt
            .list("facings")
            .and_then(|list| list.strings())
            .map(|s| {
                s.iter()
                    .filter_map(|name| direction_by_name(&name.to_string_lossy()))
                    .collect::<Vec<_>>()
            });
        Some(Self {
            pos: BlockPos::new(x, y, z),
            charge: nbt
                .int("charge")
                .unwrap_or(0)
                .clamp(0, SculkSpreader::MAX_CHARGE),
            update_delay: nbt.int("update_delay").unwrap_or(0).max(0),
            decay_delay: nbt
                .int("decay_delay")
                .unwrap_or(Self::MAX_CURSOR_DECAY_DELAY)
                .clamp(0, Self::MAX_CURSOR_DECAY_DELAY),
            facings,
        })
    }

    fn save(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.insert(
            "pos",
            NbtTag::IntArray(vec![self.pos.x(), self.pos.y(), self.pos.z()]),
        );
        nbt.insert("charge", self.charge);
        nbt.insert("decay_delay", self.decay_delay);
        nbt.insert("update_delay", self.update_delay);
        if let Some(facings) = &self.facings {
            let names = facings
                .iter()
                .map(|direction| direction_name(*direction).into())
                .collect::<Vec<_>>();
            nbt.insert("facings", NbtList::String(names));
        }
        nbt
    }

    /// Vanilla `ChargeCursor.isPosUnreasonable`.
    fn is_pos_unreasonable(&self, origin: BlockPos) -> bool {
        chessboard_distance(self.pos, origin) > Self::MAX_CURSOR_DISTANCE
    }

    /// Vanilla `ChargeCursor.mergeWith`.
    fn merge_with(&mut self, other: &mut Self) {
        self.charge += other.charge;
        other.charge = 0;
        self.update_delay = self.update_delay.min(other.update_delay);
    }

    /// Vanilla `ChargeCursor.update`.
    fn update(
        &mut self,
        level: &impl LevelAccessor,
        registry: &Registry,
        origin: BlockPos,
        random: &mut WorldgenRandom,
        spreader: &SculkSpreader,
        spread_veins: bool,
    ) {
        if self.charge <= 0 {
            return;
        }
        if self.update_delay > 0 {
            self.update_delay -= 1;
            return;
        }

        let mut current_state = level.get_block_state(self.pos);
        let mut behavior = behavior_of(current_state);
        if spread_veins
            && attempt_spread_vein(
                level,
                self.pos,
                current_state,
                self.facings.as_deref(),
                spreader.is_world_generation,
                behavior,
            )
        {
            if can_change_block_state_on_spread(behavior) {
                current_state = level.get_block_state(self.pos);
                behavior = behavior_of(current_state);
            }
            level.play_block_sound(&sound_events::BLOCK_SCULK_SPREAD, self.pos, 1.0, 1.0, None);
        }

        self.charge = attempt_use_charge(
            level,
            registry,
            random,
            origin,
            spreader,
            spread_veins,
            self,
            behavior,
        );
        if self.charge <= 0 {
            on_discharged(level, current_state, self.pos);
            return;
        }

        if let Some(transfer_pos) = valid_movement_pos(level, self.pos, random) {
            on_discharged(level, current_state, self.pos);
            self.pos = transfer_pos;
            if spreader.is_world_generation && !horizontally_closer_than(self.pos, origin, 15.0) {
                self.charge = 0;
                return;
            }
            current_state = level.get_block_state(transfer_pos);
        }

        if behavior_of(current_state) != SculkBehaviorKind::Default {
            self.facings = Some(available_faces(current_state));
        }

        self.decay_delay = update_decay_delay(behavior, self.decay_delay);
        self.update_delay = sculk_spread_delay(behavior);
    }
}

/// Vanilla `SculkSpreader.ChargeCursor.getBlockBehaviour`.
#[must_use]
pub fn behavior_of(state: BlockStateId) -> SculkBehaviorKind {
    if state.get_block() == &vanilla_blocks::SCULK {
        SculkBehaviorKind::Sculk
    } else if state.get_block() == &vanilla_blocks::SCULK_VEIN {
        SculkBehaviorKind::SculkVein
    } else {
        SculkBehaviorKind::Default
    }
}

/// Vanilla `SculkBehaviour.attemptSpreadVein`.
fn attempt_spread_vein(
    level: &impl LevelAccessor,
    pos: BlockPos,
    state: BlockStateId,
    facings: Option<&[Direction]>,
    post_process: bool,
    behavior: SculkBehaviorKind,
) -> bool {
    match behavior {
        SculkBehaviorKind::Default => match facings {
            None => vein_spread_all(level, state, pos, post_process, true) > 0,
            Some(faces) if !faces.is_empty() => {
                if !state.is_air() && !get_fluid_state_from_block(state).is_water() {
                    return false;
                }
                vein_regrow(level, pos, state, faces)
            }
            Some(_) => vein_spread_all(level, state, pos, post_process, false) > 0,
        },
        SculkBehaviorKind::Sculk | SculkBehaviorKind::SculkVein => {
            vein_spread_all(level, state, pos, post_process, false) > 0
        }
    }
}

/// Vanilla `SculkBehaviour.attemptUseCharge`.
#[expect(
    clippy::too_many_arguments,
    reason = "mirrors vanilla SculkBehaviour.attemptUseCharge plus the level surface"
)]
fn attempt_use_charge(
    level: &impl LevelAccessor,
    registry: &Registry,
    random: &mut WorldgenRandom,
    origin: BlockPos,
    spreader: &SculkSpreader,
    spread_veins: bool,
    cursor: &ChargeCursor,
    behavior: SculkBehaviorKind,
) -> i32 {
    match behavior {
        SculkBehaviorKind::Default => {
            if cursor.decay_delay > 0 {
                cursor.charge
            } else {
                0
            }
        }
        SculkBehaviorKind::Sculk => {
            sculk_block_attempt_use_charge(level, random, origin, spreader, cursor)
        }
        SculkBehaviorKind::SculkVein => {
            vein_attempt_use_charge(level, registry, random, spreader, spread_veins, cursor)
        }
    }
}

/// Vanilla `SculkBlock.attemptUseCharge`.
fn sculk_block_attempt_use_charge(
    level: &impl LevelAccessor,
    random: &mut WorldgenRandom,
    origin: BlockPos,
    spreader: &SculkSpreader,
    cursor: &ChargeCursor,
) -> i32 {
    let charge = cursor.charge;
    if charge == 0 || random.next_i32_bounded(spreader.charge_decay_rate) != 0 {
        return charge;
    }

    let is_close_to_catalyst = closer_than(cursor.pos, origin, spreader.no_growth_radius);
    if !is_close_to_catalyst && can_place_growth(level, cursor.pos) {
        if random.next_i32_bounded(spreader.growth_spawn_cost) < charge {
            let growth_pos = cursor.pos.above();
            let growth_state =
                random_growth_state(level, random, growth_pos, spreader.is_world_generation);
            if level.set_block_state(growth_pos, growth_state, UpdateFlags::UPDATE_ALL) {
                attach_growth_block_entity(level, growth_pos, growth_state);
                let sound_type = &growth_state.get_block().config.sound_type;
                level.play_block_sound(sound_type.place_sound, cursor.pos, 1.0, 1.0, None);
            }
        }

        0.max(charge - spreader.growth_spawn_cost)
    } else if random.next_i32_bounded(spreader.additional_decay_rate) != 0 {
        charge
    } else if is_close_to_catalyst {
        charge - 1
    } else {
        charge - decay_penalty(spreader, cursor.pos, origin, charge)
    }
}

/// Vanilla `SculkVeinBlock.attemptUseCharge`.
fn vein_attempt_use_charge(
    level: &impl LevelAccessor,
    registry: &Registry,
    random: &mut WorldgenRandom,
    spreader: &SculkSpreader,
    spread_veins: bool,
    cursor: &ChargeCursor,
) -> i32 {
    if spread_veins && vein_attempt_place_sculk(level, registry, random, spreader, cursor.pos) {
        cursor.charge - 1
    } else if random.next_i32_bounded(spreader.charge_decay_rate) == 0 {
        fast_floor(f64::from(cursor.charge) * 0.5) as i32
    } else {
        cursor.charge
    }
}

/// Vanilla `SculkVeinBlock.attemptPlaceSculk`.
fn vein_attempt_place_sculk(
    level: &impl LevelAccessor,
    registry: &Registry,
    random: &mut WorldgenRandom,
    spreader: &SculkSpreader,
    pos: BlockPos,
) -> bool {
    let state = level.get_block_state(pos);
    for support in shuffled_directions(random) {
        if !vein_has_face(state, support) {
            continue;
        }

        let support_pos = pos.relative(support);
        let support_state = level.get_block_state(support_pos);
        if !registry
            .blocks
            .is_in_tag(support_state.get_block(), &spreader.replaceable_blocks)
        {
            continue;
        }

        let sculk = vanilla_blocks::SCULK.default_state();
        let _ = level.set_block_state(support_pos, sculk, UpdateFlags::UPDATE_ALL);
        level.play_block_sound(
            &sound_events::BLOCK_SCULK_SPREAD,
            support_pos,
            1.0,
            1.0,
            None,
        );
        let _ = vein_spread_all(
            level,
            sculk,
            support_pos,
            spreader.is_world_generation,
            false,
        );

        let skip = support.opposite();
        for direction in Direction::ALL {
            if direction == skip {
                continue;
            }

            let vein_pos = support_pos.relative(direction);
            let possible_vein = level.get_block_state(vein_pos);
            if possible_vein.get_block() == &vanilla_blocks::SCULK_VEIN {
                on_discharged(level, possible_vein, vein_pos);
            }
        }

        return true;
    }

    false
}

/// Vanilla `MultifaceSpreader.spreadAll` under the sculk vein configuration.
fn vein_spread_all(
    level: &impl LevelAccessor,
    state: BlockStateId,
    pos: BlockPos,
    post_process: bool,
    same_space_only: bool,
) -> i64 {
    let mut count = 0;
    for starting_face in Direction::ALL {
        if !vein_can_spread_from(state, starting_face) {
            continue;
        }

        for spread_direction in Direction::ALL {
            if vein_spread_from_face_toward_direction(
                level,
                state,
                pos,
                starting_face,
                spread_direction,
                post_process,
                same_space_only,
            ) {
                count += 1;
            }
        }
    }
    count
}

/// Vanilla `MultifaceSpreader.spreadFromFaceTowardDirection`.
#[expect(
    clippy::too_many_arguments,
    reason = "mirrors vanilla MultifaceSpreader.spreadFromFaceTowardDirection"
)]
fn vein_spread_from_face_toward_direction(
    level: &impl LevelAccessor,
    state: BlockStateId,
    pos: BlockPos,
    starting_face: Direction,
    spread_direction: Direction,
    post_process: bool,
    same_space_only: bool,
) -> bool {
    let Some(spread_pos) = vein_spread_target(
        level,
        state,
        pos,
        starting_face,
        spread_direction,
        same_space_only,
    ) else {
        return false;
    };
    vein_spread_to_face(level, &spread_pos, post_process)
}

/// Vanilla `MultifaceSpreader.getSpreadFromFaceTowardDirection`.
fn vein_spread_target(
    level: &impl LevelAccessor,
    state: BlockStateId,
    pos: BlockPos,
    starting_face: Direction,
    spread_direction: Direction,
    same_space_only: bool,
) -> Option<SpreadPos> {
    if spread_direction.axis() == starting_face.axis() {
        return None;
    }

    if !vein_is_other_block_valid_as_source(state)
        && (!vein_has_face(state, starting_face) || vein_has_face(state, spread_direction))
    {
        return None;
    }

    let spread_types = if same_space_only {
        SAME_SPACE_SPREAD_TYPES.as_slice()
    } else {
        DEFAULT_SPREAD_TYPES.as_slice()
    };
    for spread_type in spread_types {
        let spread_pos = vein_spread_pos(pos, spread_direction, starting_face, *spread_type);
        if vein_can_spread_into(level, pos, &spread_pos) {
            return Some(spread_pos);
        }
    }

    None
}

/// Vanilla `MultifaceSpreader.spreadToFace`.
fn vein_spread_to_face(
    level: &impl LevelAccessor,
    spread_pos: &SpreadPos,
    post_process: bool,
) -> bool {
    let old_state = level.get_block_state(spread_pos.pos);
    let Some(spread_state) =
        vein_state_for_placement(level, old_state, spread_pos.pos, spread_pos.face)
    else {
        return false;
    };

    if post_process {
        level.mark_pos_for_postprocessing(spread_pos.pos);
    }
    level.set_block_state(spread_pos.pos, spread_state, UpdateFlags::UPDATE_CLIENTS)
}

/// Vanilla `MultifaceSpreader.canSpreadInto`.
fn vein_can_spread_into(
    level: &impl LevelAccessor,
    source_pos: BlockPos,
    spread_pos: &SpreadPos,
) -> bool {
    let existing_state = level.get_block_state(spread_pos.pos);
    vein_state_can_be_replaced(
        level,
        source_pos,
        spread_pos.pos,
        spread_pos.face,
        existing_state,
    ) && vein_is_valid_state_for_placement(level, existing_state, spread_pos.pos, spread_pos.face)
}

/// Vanilla `SculkVeinBlock.SculkVeinSpreaderConfig.stateCanBeReplaced`.
fn vein_state_can_be_replaced(
    level: &impl LevelAccessor,
    source_pos: BlockPos,
    placement_pos: BlockPos,
    placement_direction: Direction,
    existing_state: BlockStateId,
) -> bool {
    let against_state = level.get_block_state(placement_pos.relative(placement_direction));
    if against_state.get_block() == &vanilla_blocks::SCULK
        || against_state.get_block() == &vanilla_blocks::SCULK_CATALYST
        || against_state.get_block() == &vanilla_blocks::MOVING_PISTON
    {
        return false;
    }

    if manhattan_distance(source_pos, placement_pos) == 2 {
        let neighbor_pos = source_pos.relative(placement_direction.opposite());
        if level
            .get_block_state(neighbor_pos)
            .is_face_sturdy_at(neighbor_pos, placement_direction)
        {
            return false;
        }
    }

    let fluid_state = get_fluid_state_from_block(existing_state);
    if !fluid_state.is_empty() && !fluid_state.is_water() {
        return false;
    }

    if existing_state.get_block().has_tag(&BlockTag::FIRE) {
        return false;
    }

    existing_state.is_replaceable() || default_multiface_state_can_be_replaced(existing_state)
}

/// Vanilla `MultifaceSpreader.DefaultSpreaderConfig.stateCanBeReplaced`.
fn default_multiface_state_can_be_replaced(existing_state: BlockStateId) -> bool {
    existing_state.is_air()
        || existing_state.get_block() == &vanilla_blocks::SCULK_VEIN
        || (existing_state.get_block() == &vanilla_blocks::WATER
            && get_fluid_state_from_block(existing_state).is_source())
}

/// Vanilla `MultifaceBlock.getStateForPlacement` for the sculk vein.
fn vein_state_for_placement(
    level: &impl LevelAccessor,
    old_state: BlockStateId,
    placement_pos: BlockPos,
    placement_direction: Direction,
) -> Option<BlockStateId> {
    if !vein_is_valid_state_for_placement(level, old_state, placement_pos, placement_direction) {
        return None;
    }

    let mut new_state = if old_state.get_block() == &vanilla_blocks::SCULK_VEIN {
        old_state
    } else {
        let state = vanilla_blocks::SCULK_VEIN.default_state();
        let fluid_state = get_fluid_state_from_block(old_state);
        if fluid_state.is_water() && fluid_state.is_source() {
            state.set_value(&BlockStateProperties::WATERLOGGED, true)
        } else {
            state
        }
    };
    new_state = new_state.set_value(multiface_face_property(placement_direction), true);
    Some(new_state)
}

/// Vanilla `MultifaceBlock.isValidStateForPlacement`.
fn vein_is_valid_state_for_placement(
    level: &impl LevelAccessor,
    old_state: BlockStateId,
    placement_pos: BlockPos,
    placement_direction: Direction,
) -> bool {
    if old_state.get_block() == &vanilla_blocks::SCULK_VEIN
        && vein_has_face(old_state, placement_direction)
    {
        return false;
    }

    can_attach_to(level, placement_pos, placement_direction)
}

/// Vanilla `MultifaceBlock.canAttachTo`.
fn can_attach_to(
    level: &impl LevelAccessor,
    pos: BlockPos,
    direction_towards_neighbor: Direction,
) -> bool {
    let neighbor_pos = pos.relative(direction_towards_neighbor);
    let neighbor_state = level.get_block_state(neighbor_pos);
    let support_direction = direction_towards_neighbor.opposite();
    shapes::is_offset_face_full(
        neighbor_state.get_support_shape_at(neighbor_pos),
        support_direction,
    ) || shapes::is_offset_face_full(
        neighbor_state.get_collision_shape_at(neighbor_pos),
        support_direction,
    )
}

/// Vanilla `SculkVeinBlock.regrow`.
fn vein_regrow(
    level: &impl LevelAccessor,
    pos: BlockPos,
    existing_state: BlockStateId,
    faces: &[Direction],
) -> bool {
    let mut has_face = false;
    let mut new_state = vanilla_blocks::SCULK_VEIN.default_state();

    for &face in faces {
        if can_attach_to(level, pos, face) {
            new_state = new_state.set_value(multiface_face_property(face), true);
            has_face = true;
        }
    }

    if !has_face {
        return false;
    }

    if !get_fluid_state_from_block(existing_state).is_empty() {
        new_state = new_state.set_value(&BlockStateProperties::WATERLOGGED, true);
    }

    level.set_block_state(pos, new_state, UpdateFlags::UPDATE_ALL)
}

/// Vanilla `SculkVeinBlock.onDischarged`.
fn on_discharged(level: &impl LevelAccessor, mut state: BlockStateId, pos: BlockPos) {
    if state.get_block() != &vanilla_blocks::SCULK_VEIN {
        return;
    }

    for direction in Direction::ALL {
        if vein_has_face(state, direction)
            && level.get_block_state(pos.relative(direction)).get_block() == &vanilla_blocks::SCULK
        {
            state = state.set_value(multiface_face_property(direction), false);
        }
    }

    if !vein_has_any_face(state) {
        state = if get_fluid_state_from_block(state).is_empty() {
            vanilla_blocks::AIR.default_state()
        } else {
            vanilla_blocks::WATER.default_state()
        };
    }

    let _ = level.set_block_state(pos, state, UpdateFlags::UPDATE_ALL);
}

/// Vanilla `ChargeCursor.getValidMovementPos`.
fn valid_movement_pos(
    level: &impl LevelAccessor,
    pos: BlockPos,
    random: &mut WorldgenRandom,
) -> Option<BlockPos> {
    let mut sculk_position = pos;
    for offset in randomized_non_corner_neighbor_offsets(random) {
        let neighbor = pos.offset(offset.x(), offset.y(), offset.z());
        let transferee = level.get_block_state(neighbor);
        if behavior_of(transferee) == SculkBehaviorKind::Default
            || !is_movement_unobstructed(level, pos, neighbor)
        {
            continue;
        }

        sculk_position = neighbor;
        if vein_has_substrate_access(level, transferee, neighbor) {
            break;
        }
    }

    (sculk_position != pos).then_some(sculk_position)
}

/// Vanilla `ChargeCursor.getRandomizedNonCornerNeighbourOffsets`.
fn randomized_non_corner_neighbor_offsets(random: &mut WorldgenRandom) -> Vec<BlockPos> {
    let mut offsets = Vec::with_capacity(18);
    for z in -1..=1 {
        for y in -1..=1 {
            for x in -1..=1 {
                if (x == 0 || y == 0 || z == 0) && (x != 0 || y != 0 || z != 0) {
                    offsets.push(BlockPos::new(x, y, z));
                }
            }
        }
    }

    for i in (1..offsets.len()).rev() {
        let Ok(bound) = i32::try_from(i + 1) else {
            panic!("sculk neighbor offset count exceeds i32 range");
        };
        let j = random.next_i32_bounded(bound) as usize;
        offsets.swap(i, j);
    }
    offsets
}

/// Vanilla `Direction.allShuffled`.
fn shuffled_directions(random: &mut WorldgenRandom) -> [Direction; 6] {
    let mut directions = Direction::ALL;
    for i in (1..directions.len()).rev() {
        let Ok(bound) = i32::try_from(i + 1) else {
            panic!("direction count exceeds i32 range");
        };
        let j = random.next_i32_bounded(bound) as usize;
        directions.swap(i, j);
    }
    directions
}

/// Vanilla `ChargeCursor.isMovementUnobstructed`.
fn is_movement_unobstructed(level: &impl LevelAccessor, from: BlockPos, to: BlockPos) -> bool {
    if manhattan_distance(from, to) == 1 {
        return true;
    }

    let dx = to.x() - from.x();
    let dy = to.y() - from.y();
    let dz = to.z() - from.z();
    let direction_x = direction_from_axis_delta(Axis::X, dx);
    let direction_y = direction_from_axis_delta(Axis::Y, dy);
    let direction_z = direction_from_axis_delta(Axis::Z, dz);
    if dx == 0 {
        is_unobstructed(level, from, direction_y) || is_unobstructed(level, from, direction_z)
    } else if dy == 0 {
        is_unobstructed(level, from, direction_x) || is_unobstructed(level, from, direction_z)
    } else {
        is_unobstructed(level, from, direction_x) || is_unobstructed(level, from, direction_y)
    }
}

/// Vanilla `ChargeCursor.isUnobstructed`.
fn is_unobstructed(level: &impl LevelAccessor, from: BlockPos, direction: Direction) -> bool {
    let test_pos = from.relative(direction);
    !level
        .get_block_state(test_pos)
        .is_face_sturdy_at(test_pos, direction.opposite())
}

/// Vanilla `Direction.fromAxisAndDirection`.
const fn direction_from_axis_delta(axis: Axis, delta: i32) -> Direction {
    match (axis, delta < 0) {
        (Axis::X, true) => Direction::West,
        (Axis::X, false) => Direction::East,
        (Axis::Y, true) => Direction::Down,
        (Axis::Y, false) => Direction::Up,
        (Axis::Z, true) => Direction::North,
        (Axis::Z, false) => Direction::South,
    }
}

/// Vanilla `SculkVeinBlock.hasSubstrateAccess`.
fn vein_has_substrate_access(
    level: &impl LevelAccessor,
    state: BlockStateId,
    pos: BlockPos,
) -> bool {
    if state.get_block() != &vanilla_blocks::SCULK_VEIN {
        return false;
    }

    Direction::ALL.iter().any(|&direction| {
        vein_has_face(state, direction)
            && level
                .get_block_state(pos.relative(direction))
                .get_block()
                .has_tag(&BlockTag::SCULK_REPLACEABLE)
    })
}

/// Vanilla `SculkBlock.canPlaceGrowth`.
fn can_place_growth(level: &impl LevelAccessor, pos: BlockPos) -> bool {
    let above = pos.above();
    let state_above = level.get_block_state(above);
    if !state_above.is_air()
        && (state_above.get_block() != &vanilla_blocks::WATER
            || !get_fluid_state_from_block(state_above).is_water())
    {
        return false;
    }

    let mut growth_count = 0;
    for z in -4..=4 {
        for y in 0..=2 {
            for x in -4..=4 {
                let state = level.get_block_state(pos.offset(x, y, z));
                if state.get_block() == &vanilla_blocks::SCULK_SENSOR
                    || state.get_block() == &vanilla_blocks::SCULK_SHRIEKER
                {
                    growth_count += 1;
                }

                if growth_count > 2 {
                    return false;
                }
            }
        }
    }

    true
}

/// Vanilla `SculkBlock.getRandomGrowthState`.
fn random_growth_state(
    level: &impl LevelAccessor,
    random: &mut WorldgenRandom,
    pos: BlockPos,
    is_world_generation: bool,
) -> BlockStateId {
    let state = if random.next_i32_bounded(11) == 0 {
        vanilla_blocks::SCULK_SHRIEKER
            .default_state()
            .set_value(&BlockStateProperties::CAN_SUMMON, is_world_generation)
    } else {
        vanilla_blocks::SCULK_SENSOR.default_state()
    };

    if state
        .try_get_value(&BlockStateProperties::WATERLOGGED)
        .is_some()
        && !get_fluid_state_from_block(level.get_block_state(pos)).is_empty()
    {
        state.set_value(&BlockStateProperties::WATERLOGGED, true)
    } else {
        state
    }
}

/// Attaches the block entity a freshly grown sensor or shrieker needs.
///
/// A live world builds it inside `set_block`; a worldgen region only records a pending
/// marker, so [`LevelAccessor::attach_block_entity`] fills it in there.
fn attach_growth_block_entity(level: &impl LevelAccessor, pos: BlockPos, state: BlockStateId) {
    let block_entity_type: BlockEntityTypeRef =
        if state.get_block() == &vanilla_blocks::SCULK_SENSOR {
            &vanilla_block_entity_types::SCULK_SENSOR
        } else if state.get_block() == &vanilla_blocks::SCULK_SHRIEKER {
            &vanilla_block_entity_types::SCULK_SHRIEKER
        } else {
            return;
        };
    level.attach_block_entity(pos, block_entity_type, state);
}

/// Vanilla `SculkBlock.getDecayPenalty`.
fn decay_penalty(spreader: &SculkSpreader, pos: BlockPos, origin: BlockPos, charge: i32) -> i32 {
    let no_growth_radius = spreader.no_growth_radius as f32;
    let dx = (pos.x() - origin.x()) as f32;
    let dy = (pos.y() - origin.y()) as f32;
    let dz = (pos.z() - origin.z()) as f32;
    let distance = (dx * dx + dy * dy + dz * dz).sqrt();
    let outer_distance_squared = (distance - no_growth_radius) * (distance - no_growth_radius);
    let max_reach = (SculkSpreader::MAX_GROWTH_RATE_RADIUS - spreader.no_growth_radius) as f32;
    let max_reach_squared = max_reach * max_reach;
    let distance_factor = (outer_distance_squared / max_reach_squared).min(1.0);
    1.max((charge as f32 * distance_factor * 0.5) as i32)
}

/// Vanilla `BlockPos.closerThan`.
fn closer_than(pos: BlockPos, origin: BlockPos, radius: i32) -> bool {
    let radius_squared = i64::from(radius) * i64::from(radius);
    distance_squared(pos, origin) < radius_squared
}

/// Vanilla's worldgen clamp: a cursor that leaves a fifteen-block column dies.
fn horizontally_closer_than(pos: BlockPos, origin: BlockPos, radius: f64) -> bool {
    let dx = f64::from(pos.x() - origin.x());
    let dz = f64::from(pos.z() - origin.z());
    dx * dx + dz * dz < radius * radius
}

fn distance_squared(left: BlockPos, right: BlockPos) -> i64 {
    let dx = i64::from(left.x()) - i64::from(right.x());
    let dy = i64::from(left.y()) - i64::from(right.y());
    let dz = i64::from(left.z()) - i64::from(right.z());
    dx * dx + dy * dy + dz * dz
}

/// Vanilla `Vec3i.distChessboard`.
fn chessboard_distance(left: BlockPos, right: BlockPos) -> i32 {
    (left.x() - right.x())
        .abs()
        .max((left.y() - right.y()).abs())
        .max((left.z() - right.z()).abs())
}

/// Vanilla `Vec3i.distManhattan`.
fn manhattan_distance(left: BlockPos, right: BlockPos) -> i32 {
    (left.x() - right.x()).abs() + (left.y() - right.y()).abs() + (left.z() - right.z()).abs()
}

/// Vanilla `SculkBehaviour.updateDecayDelay`.
const fn update_decay_delay(behavior: SculkBehaviorKind, age: i32) -> i32 {
    match behavior {
        SculkBehaviorKind::Default => {
            if age > 1 {
                age - 1
            } else {
                0
            }
        }
        SculkBehaviorKind::Sculk | SculkBehaviorKind::SculkVein => {
            ChargeCursor::MAX_CURSOR_DECAY_DELAY
        }
    }
}

/// Vanilla `SculkBehaviour.getSculkSpreadDelay`, which no sculk block overrides.
const fn sculk_spread_delay(_behavior: SculkBehaviorKind) -> i32 {
    1
}

/// Vanilla `SculkBehaviour.canChangeBlockStateOnSpread`, which only `SculkBlock` refuses.
const fn can_change_block_state_on_spread(behavior: SculkBehaviorKind) -> bool {
    !matches!(behavior, SculkBehaviorKind::Sculk)
}

/// Vanilla `MultifaceBlock.availableFaces`.
fn available_faces(state: BlockStateId) -> Vec<Direction> {
    let mut faces = Vec::new();
    if state.get_block() != &vanilla_blocks::SCULK_VEIN {
        return faces;
    }

    for direction in Direction::ALL {
        if vein_has_face(state, direction) {
            faces.push(direction);
        }
    }
    faces
}

/// Vanilla `MultifaceSpreader.SpreaderConfig.canSpreadFrom`.
fn vein_can_spread_from(state: BlockStateId, face: Direction) -> bool {
    vein_is_other_block_valid_as_source(state) || vein_has_face(state, face)
}

/// Vanilla `SculkVeinSpreaderConfig.isOtherBlockValidAsSource`.
fn vein_is_other_block_valid_as_source(state: BlockStateId) -> bool {
    state.get_block() != &vanilla_blocks::SCULK_VEIN
}

/// Vanilla `MultifaceSpreader.SpreadType.getSpreadPos`.
fn vein_spread_pos(
    pos: BlockPos,
    spread_direction: Direction,
    from_face: Direction,
    spread_type: SpreadType,
) -> SpreadPos {
    match spread_type {
        SpreadType::SamePosition => SpreadPos {
            pos,
            face: spread_direction,
        },
        SpreadType::SamePlane => SpreadPos {
            pos: pos.relative(spread_direction),
            face: from_face,
        },
        SpreadType::WrapAround => SpreadPos {
            pos: pos.relative(spread_direction).relative(from_face),
            face: spread_direction.opposite(),
        },
    }
}

/// Vanilla `MultifaceBlock.hasAnyFace`.
fn vein_has_any_face(state: BlockStateId) -> bool {
    Direction::ALL
        .iter()
        .any(|&direction| vein_has_face(state, direction))
}

/// Vanilla `MultifaceBlock.hasFace`.
fn vein_has_face(state: BlockStateId, direction: Direction) -> bool {
    state
        .try_get_value(multiface_face_property(direction))
        .unwrap_or(false)
}

/// Vanilla `MultifaceBlock.pack`, which keys the charge particle event off `Direction.ordinal`.
fn pack_faces(faces: &[Direction]) -> u8 {
    faces.iter().fold(0_u8, |code, direction| {
        code | 1 << direction_ordinal(*direction)
    })
}

/// Vanilla `Direction.ordinal`, which is the declaration order of `Direction.values()`.
const fn direction_ordinal(direction: Direction) -> u8 {
    match direction {
        Direction::Down => 0,
        Direction::Up => 1,
        Direction::North => 2,
        Direction::South => 3,
        Direction::West => 4,
        Direction::East => 5,
    }
}

/// Vanilla `Direction.getSerializedName`, which is what the cursor codec writes.
const fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::Down => "down",
        Direction::Up => "up",
        Direction::North => "north",
        Direction::South => "south",
        Direction::West => "west",
        Direction::East => "east",
    }
}

fn direction_by_name(name: &str) -> Option<Direction> {
    Direction::ALL
        .into_iter()
        .find(|direction| direction_name(*direction) == name)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use steel_registry::{REGISTRY, init_vanilla_registry};
    use steel_utils::ChunkPos;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::block_entity::init_block_entities;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
    use crate::world::World;

    fn init() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
    }

    /// A flat slab of sculk with air above it, which is the shape a catalyst spreads over.
    fn sculk_platform(name: &'static str, center: BlockPos, radius: i32) -> Arc<World> {
        let world = fresh_test_world(name);
        for dx in -2..=2 {
            for dz in -2..=2 {
                insert_ready_full_chunk(
                    &world,
                    ChunkPos::from_block_pos(center.offset(dx * 16, 0, dz * 16)),
                );
            }
        }
        let stone = vanilla_blocks::STONE.default_state();
        let sculk = vanilla_blocks::SCULK.default_state();
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                let pos = center.offset(dx, 0, dz);
                world.set_block(pos.below(), stone, UpdateFlags::UPDATE_NONE);
                world.set_block(pos, sculk, UpdateFlags::UPDATE_NONE);
            }
        }
        world
    }

    fn run_until_spent(
        spreader: &mut SculkSpreader,
        world: &Arc<World>,
        origin: BlockPos,
        seed: u64,
        max_passes: usize,
    ) -> usize {
        let mut random = WorldgenRandom::from_seed(seed);
        for pass in 0..max_passes {
            if spreader.cursors().is_empty() {
                return pass;
            }
            spreader.update_cursors(world, &REGISTRY, origin, &mut random, true);
        }
        max_passes
    }

    /// The two vanilla configurations differ in every field, and using the world-generation
    /// one in a live world would make a single mob carve a deep-dark-sized patch.
    #[test]
    fn the_level_spreader_is_gentler_than_the_world_generation_one() {
        let level = SculkSpreader::level();
        let worldgen = SculkSpreader::worldgen();

        assert!(!level.is_world_generation());
        assert!(worldgen.is_world_generation());
        assert_eq!(level.growth_spawn_cost, 10);
        assert_eq!(worldgen.growth_spawn_cost, 50);
        assert_eq!(level.replaceable_blocks, BlockTag::SCULK_REPLACEABLE);
        assert_eq!(
            worldgen.replaceable_blocks,
            BlockTag::SCULK_REPLACEABLE_WORLD_GEN
        );
    }

    /// A reward larger than one cursor can hold is split, and the split stops at the cursor
    /// cap rather than growing without bound -- vanilla's guard against a boss drop
    /// spawning thousands of walks.
    #[test]
    fn a_huge_reward_is_split_into_capped_cursors_and_then_refused() {
        init_vanilla_registry();
        let mut spreader = SculkSpreader::level();

        spreader.add_cursors(BlockPos::ZERO, 2_500);
        assert_eq!(spreader.cursors().len(), 3);
        assert_eq!(spreader.cursors()[0].charge(), SculkSpreader::MAX_CHARGE);
        assert_eq!(spreader.cursors()[1].charge(), SculkSpreader::MAX_CHARGE);
        assert_eq!(spreader.cursors()[2].charge(), 500);

        spreader.add_cursors(BlockPos::ZERO, 1_000_000);
        assert_eq!(spreader.cursors().len(), SculkSpreader::MAX_CURSORS);
    }

    /// The whole point of the walk is that it ends. A charge dropped on a sculk platform has
    /// to decay to nothing in a bounded number of passes; a cursor that never spends its
    /// charge would keep ticking and keep converting blocks forever.
    #[test]
    fn a_charge_decays_to_nothing_in_a_bounded_number_of_passes() {
        init();
        let origin = BlockPos::new(8, 70, 8);
        let world = sculk_platform("sculk_spreader_decay", origin, 8);

        let mut spreader = SculkSpreader::level();
        spreader.add_cursors(origin, 60);
        let before = spreader.total_charge();
        assert_eq!(before, 60);

        let passes = run_until_spent(&mut spreader, &world, origin, 0xDEAD_BEEF, 4_000);

        assert!(
            spreader.cursors().is_empty(),
            "{} cursors still alive after {passes} passes",
            spreader.cursors().len()
        );
        assert!(passes < 4_000, "the walk did not settle");
    }

    /// Charge is never created. Every pass either spends it on a growth, decays it, or
    /// carries it, so the total can only fall -- a spreader that gained charge would grow
    /// without limit.
    #[test]
    fn a_pass_never_leaves_more_charge_than_it_started_with() {
        init();
        let origin = BlockPos::new(8, 70, 8);
        let world = sculk_platform("sculk_spreader_monotonic", origin, 8);

        let mut spreader = SculkSpreader::level();
        spreader.add_cursors(origin, 200);
        let mut random = WorldgenRandom::from_seed(1_234);
        let mut previous = spreader.total_charge();

        for _ in 0..500 {
            spreader.update_cursors(&world, &REGISTRY, origin, &mut random, true);
            let current = spreader.total_charge();
            assert!(
                current <= previous,
                "charge rose from {previous} to {current}"
            );
            previous = current;
        }
    }

    /// A catalyst spreads outward from where the mob died, but never past vanilla's
    /// twenty-four block growth radius. This is the assertion that a lifted spreader has not
    /// turned into a runaway: the sculk it leaves stays inside a bounded box.
    #[test]
    fn a_charge_grows_no_more_sensors_than_its_budget_pays_for() {
        init();
        let origin = BlockPos::new(8, 70, 8);
        // Wider than anything the charge can pay for, so the bound has to come from the
        // algorithm rather than from running out of sculk to stand on.
        let world = sculk_platform("sculk_spreader_budget", origin, 30);

        let charge = 500;
        let mut spreader = SculkSpreader::level();
        let budget = charge / spreader.growth_spawn_cost;
        spreader.add_cursors(origin, charge);
        let passes = run_until_spent(&mut spreader, &world, origin, 42, 40_000);
        assert!(
            spreader.cursors().is_empty(),
            "the walk had not settled after {passes} passes"
        );

        let mut growths = 0;
        for dx in -40..=40 {
            for dz in -40..=40 {
                let block = world.get_block_state(origin.offset(dx, 1, dz)).get_block();
                if block == &vanilla_blocks::SCULK_SENSOR
                    || block == &vanilla_blocks::SCULK_SHRIEKER
                {
                    growths += 1;
                }
            }
        }

        assert!(growths > 0, "a spent charge grew nothing at all");
        assert!(
            growths <= budget,
            "{growths} growths from a charge that only pays for {budget}"
        );
    }

    /// Cursors are the only thing a catalyst keeps across a reload; losing them would strand
    /// the experience a player already spent.
    #[test]
    fn cursors_survive_a_save_and_load() {
        use std::io::Cursor;

        use simdnbt::borrow::read_compound as read_borrowed_compound;

        init_vanilla_registry();
        let mut spreader = SculkSpreader::level();
        spreader.add_cursors(BlockPos::new(-3, 12, 40), 1_200);
        spreader.cursors[0].facings = Some(vec![Direction::Up, Direction::West]);

        let mut written = NbtCompound::new();
        spreader.save(&mut written);
        let mut bytes = Vec::new();
        written.write(&mut bytes);
        let borrowed =
            read_borrowed_compound(&mut Cursor::new(bytes.as_slice())).expect("test NBT reborrows");
        let view: BorrowedNbtCompoundView<'_, '_> = (&borrowed).into();

        let mut loaded = SculkSpreader::level();
        loaded.load(&view);

        assert_eq!(loaded.cursors().len(), 2);
        assert_eq!(loaded.cursors()[0].pos(), BlockPos::new(-3, 12, 40));
        assert_eq!(loaded.cursors()[0].charge(), SculkSpreader::MAX_CHARGE);
        assert_eq!(loaded.cursors()[1].charge(), 200);
        assert_eq!(
            loaded.cursors[0].facings.as_deref(),
            Some([Direction::Up, Direction::West].as_slice())
        );
    }

    /// The charge particle event packs the cursor's faces into the low six bits by vanilla's
    /// `Direction.ordinal`; a different bit order would draw the charge on the wrong sides.
    #[test]
    fn charge_particle_faces_pack_by_vanilla_direction_ordinal() {
        assert_eq!(pack_faces(&[Direction::Down]), 0b0000_0001);
        assert_eq!(pack_faces(&[Direction::East]), 0b0010_0000);
        assert_eq!(
            pack_faces(&[Direction::Up, Direction::North, Direction::West]),
            0b0001_0110
        );
    }
}
