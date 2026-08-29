//! Matching multi-block shapes against the world.
//!
//! Vanilla parity: `net.minecraft.world.level.block.state.pattern.BlockPattern`,
//! `BlockPatternBuilder` and `BlockInWorld`. Vanilla builds patterns for the
//! carved pumpkin's three golems, for the wither, and for the end crystal
//! respawn ritual, so this lives next to the world rather than inside any one
//! block behavior.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::Arc;

use foton_utils::{BlockPos, BlockStateId, Direction};
use rustc_hash::FxHashMap;

use crate::block_entity::SharedBlockEntity;
use crate::world::LevelReader;

#[cfg(test)]
mod tests;

/// Vanilla `Direction.values()`, in declaration order.
///
/// `BlockPattern.find` returns the first orientation that matches, so the
/// iteration order is observable.
const ALL_DIRECTIONS: [Direction; 6] = [
    Direction::Down,
    Direction::Up,
    Direction::North,
    Direction::South,
    Direction::West,
    Direction::East,
];

/// One position of a pattern, resolved against the world.
///
/// Vanilla parity: `BlockInWorld`. Vanilla resolves the state lazily; Foton
/// resolves it through the per-search cache instead, which serves the same
/// purpose of reading each position once.
pub struct BlockInWorld<'a> {
    level: &'a dyn LevelReader,
    pos: BlockPos,
    state: BlockStateId,
}

impl<'a> BlockInWorld<'a> {
    /// Reads the block at `pos`.
    #[must_use]
    pub fn new(level: &'a dyn LevelReader, pos: BlockPos) -> Self {
        Self {
            level,
            pos,
            state: level.get_block_state(pos),
        }
    }

    /// Returns the state at this position.
    #[must_use]
    pub const fn state(&self) -> BlockStateId {
        self.state
    }

    /// Returns this position.
    #[must_use]
    pub const fn pos(&self) -> BlockPos {
        self.pos
    }

    /// Returns the block entity at this position, when the level has one.
    #[must_use]
    pub fn block_entity(&self) -> Option<SharedBlockEntity> {
        self.level.get_block_entity(self.pos)
    }
}

/// A test one pattern position has to pass.
pub type BlockPatternPredicate = Arc<dyn Fn(&BlockInWorld<'_>) -> bool + Send + Sync>;

/// Lifts a block-state test into a pattern predicate.
///
/// Vanilla parity: `BlockInWorld.hasState`.
#[must_use]
pub fn has_state(
    predicate: impl Fn(BlockStateId) -> bool + Send + Sync + 'static,
) -> BlockPatternPredicate {
    Arc::new(move |block: &BlockInWorld<'_>| predicate(block.state()))
}

/// Reads each position of a search at most once.
///
/// Vanilla parity: the Guava `LoadingCache` of `BlockPattern.createLevelCache`.
/// A single `find` walks the same positions under 24 orientations, so without
/// this every candidate origin would re-read the whole neighborhood.
struct LevelCache<'a> {
    level: &'a dyn LevelReader,
    states: RefCell<FxHashMap<BlockPos, BlockStateId>>,
}

impl<'a> LevelCache<'a> {
    fn new(level: &'a dyn LevelReader) -> Self {
        Self {
            level,
            states: RefCell::new(FxHashMap::default()),
        }
    }

    fn get(&self, pos: BlockPos) -> BlockInWorld<'a> {
        let state = *self
            .states
            .borrow_mut()
            .entry(pos)
            .or_insert_with(|| self.level.get_block_state(pos));
        BlockInWorld {
            level: self.level,
            pos,
            state,
        }
    }
}

/// A shape of block predicates that can be searched for in the world.
///
/// Vanilla parity: `BlockPattern`.
pub struct BlockPattern {
    /// Indexed `[depth][height][width]`, like vanilla's `Predicate[][][]`.
    pattern: Vec<Vec<Vec<BlockPatternPredicate>>>,
    depth: usize,
    height: usize,
    width: usize,
}

impl BlockPattern {
    /// Returns how many aisles deep this pattern is.
    #[must_use]
    pub const fn depth(&self) -> usize {
        self.depth
    }

    /// Returns how many rows tall this pattern is.
    #[must_use]
    pub const fn height(&self) -> usize {
        self.height
    }

    /// Returns how many columns wide this pattern is.
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    /// Tests this pattern at one origin and orientation.
    ///
    /// Vanilla parity: `BlockPattern.matches(LevelReader, BlockPos, Direction,
    /// Direction)`. Returns `None` when `up` is parallel to `forwards`, which
    /// vanilla rejects by throwing.
    #[must_use]
    pub fn matches<'a>(
        &self,
        level: &'a dyn LevelReader,
        origin: BlockPos,
        forwards: Direction,
        up: Direction,
    ) -> Option<BlockPatternMatch<'a>> {
        if up == forwards || up == forwards.opposite() {
            return None;
        }

        let cache = LevelCache::new(level);
        if !self.matches_in_cache(origin, forwards, up, &cache) {
            return None;
        }

        Some(self.build_match(origin, forwards, up, cache))
    }

    /// Searches for this pattern near `origin`.
    ///
    /// Vanilla parity: `BlockPattern.find`.
    #[must_use]
    pub fn find<'a>(
        &self,
        level: &'a dyn LevelReader,
        origin: BlockPos,
    ) -> Option<BlockPatternMatch<'a>> {
        let cache = LevelCache::new(level);
        let dist = self.width.max(self.height).max(self.depth) as i32;

        for test_pos in
            BlockPos::between_closed(origin, origin.offset(dist - 1, dist - 1, dist - 1))
        {
            for forwards in ALL_DIRECTIONS {
                for up in ALL_DIRECTIONS {
                    if up == forwards || up == forwards.opposite() {
                        continue;
                    }
                    if self.matches_in_cache(test_pos, forwards, up, &cache) {
                        return Some(self.build_match(test_pos, forwards, up, cache));
                    }
                }
            }
        }

        None
    }

    const fn build_match<'a>(
        &self,
        front_top_left: BlockPos,
        forwards: Direction,
        up: Direction,
        cache: LevelCache<'a>,
    ) -> BlockPatternMatch<'a> {
        BlockPatternMatch {
            front_top_left,
            forwards,
            up,
            cache,
            width: self.width,
            height: self.height,
            depth: self.depth,
        }
    }

    fn matches_in_cache(
        &self,
        origin: BlockPos,
        forwards: Direction,
        up: Direction,
        cache: &LevelCache<'_>,
    ) -> bool {
        for x in 0..self.width {
            for y in 0..self.height {
                for z in 0..self.depth {
                    let pos =
                        translate_and_rotate(origin, forwards, up, x as i32, y as i32, z as i32);
                    if !self.pattern[z][y][x](&cache.get(pos)) {
                        return false;
                    }
                }
            }
        }

        true
    }
}

/// Maps a pattern cell onto a world position.
///
/// Vanilla parity: `BlockPattern.translateAndRotate`. `right`, `down` and
/// `forward` are the cell's column, row and aisle.
#[must_use]
fn translate_and_rotate(
    origin: BlockPos,
    forwards: Direction,
    up: Direction,
    right: i32,
    down: i32,
    forward: i32,
) -> BlockPos {
    let forwards_vector = forwards.offset_vec();
    let up_vector = up.offset_vec();
    let right_vector = forwards_vector.cross(up_vector);
    origin.offset(
        up_vector.x * -down + right_vector.x * right + forwards_vector.x * forward,
        up_vector.y * -down + right_vector.y * right + forwards_vector.y * forward,
        up_vector.z * -down + right_vector.z * right + forwards_vector.z * forward,
    )
}

/// A pattern that was found in the world, with the frame to read it back.
///
/// Vanilla parity: `BlockPattern.BlockPatternMatch`.
pub struct BlockPatternMatch<'a> {
    front_top_left: BlockPos,
    forwards: Direction,
    up: Direction,
    cache: LevelCache<'a>,
    width: usize,
    height: usize,
    depth: usize,
}

impl<'a> BlockPatternMatch<'a> {
    /// Returns the position the pattern's first cell landed on.
    #[must_use]
    pub const fn front_top_left(&self) -> BlockPos {
        self.front_top_left
    }

    /// Returns the direction the pattern's aisles run in.
    #[must_use]
    pub const fn forwards(&self) -> Direction {
        self.forwards
    }

    /// Returns the direction the pattern's rows count down from.
    #[must_use]
    pub const fn up(&self) -> Direction {
        self.up
    }

    /// Returns how many columns wide the matched pattern is.
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    /// Returns how many rows tall the matched pattern is.
    #[must_use]
    pub const fn height(&self) -> usize {
        self.height
    }

    /// Returns how many aisles deep the matched pattern is.
    #[must_use]
    pub const fn depth(&self) -> usize {
        self.depth
    }

    /// Returns the block one pattern cell landed on.
    ///
    /// Vanilla parity: `BlockPatternMatch.getBlock`.
    #[must_use]
    pub fn block(&self, right: i32, down: i32, forward: i32) -> BlockInWorld<'a> {
        self.cache.get(translate_and_rotate(
            self.front_top_left,
            self.forwards,
            self.up,
            right,
            down,
            forward,
        ))
    }
}

/// Builds a [`BlockPattern`] from character aisles.
///
/// Vanilla parity: `BlockPatternBuilder`.
pub struct BlockPatternBuilder {
    /// One entry per aisle, each a grid of `[height][width]` characters.
    pattern: Vec<Vec<Vec<char>>>,
    lookup: FxHashMap<char, BlockPatternPredicate>,
    height: usize,
    width: usize,
    unknown_characters: BTreeSet<char>,
}

impl BlockPatternBuilder {
    /// Starts an empty pattern.
    ///
    /// Vanilla parity: `BlockPatternBuilder.start`. A space always matches.
    #[must_use]
    pub fn start() -> Self {
        let mut lookup: FxHashMap<char, BlockPatternPredicate> = FxHashMap::default();
        lookup.insert(' ', Arc::new(|_: &BlockInWorld<'_>| true));
        Self {
            pattern: Vec::new(),
            lookup,
            height: 0,
            width: 0,
            unknown_characters: BTreeSet::new(),
        }
    }

    /// Appends one aisle, given as its rows from top to bottom.
    ///
    /// Vanilla parity: `BlockPatternBuilder.aisle`.
    ///
    /// # Panics
    ///
    /// Panics when the aisle is empty or does not have the same dimensions as
    /// the aisles already added. Patterns are written as literals in Foton's
    /// own code, so a mismatch is a programming error rather than bad input.
    #[must_use]
    pub fn aisle(mut self, aisle: &[&str]) -> Self {
        assert!(
            !aisle.is_empty() && !aisle[0].is_empty(),
            "empty pattern for aisle"
        );

        if self.pattern.is_empty() {
            self.height = aisle.len();
            self.width = aisle[0].chars().count();
        }

        assert_eq!(
            aisle.len(),
            self.height,
            "aisle heights must all match the first aisle"
        );

        let mut rows = Vec::with_capacity(aisle.len());
        for row in aisle {
            let chars: Vec<char> = row.chars().collect();
            assert_eq!(
                chars.len(),
                self.width,
                "aisle row widths must all match the first aisle"
            );

            for &character in &chars {
                if !self.lookup.contains_key(&character) {
                    self.unknown_characters.insert(character);
                }
            }

            rows.push(chars);
        }

        self.pattern.push(rows);
        self
    }

    /// Binds `character` to a predicate.
    ///
    /// Vanilla parity: `BlockPatternBuilder.where`.
    #[must_use]
    pub fn where_char(mut self, character: char, predicate: BlockPatternPredicate) -> Self {
        self.lookup.insert(character, predicate);
        self.unknown_characters.remove(&character);
        self
    }

    /// Finishes the pattern.
    ///
    /// Vanilla parity: `BlockPatternBuilder.build`.
    ///
    /// # Panics
    ///
    /// Panics when a character used in an aisle has no predicate.
    #[must_use]
    pub fn build(self) -> BlockPattern {
        assert!(
            self.unknown_characters.is_empty(),
            "predicates for some pattern characters are missing: {:?}",
            self.unknown_characters
        );

        let never: BlockPatternPredicate = Arc::new(|_: &BlockInWorld<'_>| false);
        let pattern = self
            .pattern
            .iter()
            .map(|aisle| {
                aisle
                    .iter()
                    .map(|row| {
                        row.iter()
                            .map(|character| {
                                // `aisle` rejects characters with no predicate,
                                // so the fallback is unreachable.
                                self.lookup
                                    .get(character)
                                    .map_or_else(|| Arc::clone(&never), Arc::clone)
                            })
                            .collect()
                    })
                    .collect()
            })
            .collect();

        BlockPattern {
            depth: self.pattern.len(),
            height: self.height,
            width: self.width,
            pattern,
        }
    }
}
