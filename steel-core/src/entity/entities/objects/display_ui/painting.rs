//! The painting.

use std::str::FromStr as _;
use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_macros::entity_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::painting_variant::PaintingVariantRef;
use steel_registry::vanilla_entity_data::PaintingEntityData;
use steel_registry::vanilla_game_rules::ENTITY_DROPS;
use steel_registry::vanilla_painting_variant_tags::PaintingVariantTag;
use steel_registry::{
    REGISTRY, RegistryExt as _, RegistryReference, TaggedRegistryExt as _, sound_events,
    vanilla_blocks, vanilla_items,
};
use steel_utils::locks::SyncMutex;
use steel_utils::{
    BlockPos, BlockStateId, Direction, Downcast as _, DowncastType, DowncastTypeKey, Identifier,
    WorldAabb, axis::Axis,
};

use crate::entity::block_attached::drop_would_be_wasted;
use crate::entity::damage::DamageSource;
use crate::entity::{
    BlockAttached, Entity, EntityBase, EntityBaseLoad, EntityBaseState, EntitySyncedData,
    ItemFrame, SharedEntity,
};
use crate::physics::{WorldCollisionProvider, has_block_collision};
use crate::world::World;

/// A painting hanging on a wall.
///
/// Vanilla parity: `Painting`. The variant carries the size, so every change
/// of variant or facing resizes the entity. Not implemented: the periodic
/// survival check of `BlockAttachedEntity.tick` that pops a painting off a wall
/// someone mined -- Steel has no block-attached tick pass, so a painting
/// outlives its wall.
#[entity_behavior(class = "Painting")]
pub struct PaintingEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<PaintingEntityData>,
    block_pos: SyncMutex<BlockPos>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `PaintingEntity`.
unsafe impl DowncastType for PaintingEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/painting");
}

/// How far the canvas sits from the middle of its block, toward the wall.
///
/// Vanilla parity: the `shiftToBlockWall` local of
/// `Painting.calculateBoundingBox`.
const SHIFT_TO_BLOCK_WALL: f64 = 0.46875;

/// Vanilla parity: `Painting.DEPTH`.
const DEPTH: f64 = 0.0625;

/// Vanilla parity: the `deflate(1.0E-7)` of `HangingEntity.calculateSupportBox`.
///
/// Without it the support region would spill into the block past each edge,
/// and a painting would demand a wall one block wider than it is.
const SUPPORT_BOX_EPSILON: f64 = 1.0e-7;

impl PaintingEntity {
    /// Creates a fresh painting from the generic entity factory path.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_attached(
            entity_type,
            id,
            BlockPos::new(
                position.x.floor() as i32,
                position.y.floor() as i32,
                position.z.floor() as i32,
            ),
            Direction::South,
            world,
        )
    }

    /// Creates a fresh painting attached to `block_pos`, carrying the default
    /// variant.
    ///
    /// Vanilla parity: `Painting(Level, BlockPos)` plus the
    /// `VariantUtils.getAny` seed of `Painting.defineSynchedData`. The
    /// generated synced data already defaults to the first registry entry, so
    /// the seed is inherited rather than repeated here.
    #[must_use]
    pub fn new_attached(
        entity_type: EntityTypeRef,
        id: i32,
        block_pos: BlockPos,
        direction: Direction,
        world: Weak<World>,
    ) -> Self {
        let entity_data = PaintingEntityData::new();
        let variant = entity_data.painting_variant.get().value();
        let bounding_box = Self::calculate_bounding_box(block_pos, direction, variant);
        let entity = Self {
            base: EntityBase::new_with_state(
                id,
                EntityBaseState::new_with_bounding_box(
                    bounding_box.center(),
                    entity_type.dimensions,
                    bounding_box,
                )
                .with_rotation(Self::rotation_for_direction(direction)),
                world,
            ),
            entity_type,
            entity_data: SyncMutex::new(entity_data),
            block_pos: SyncMutex::new(block_pos),
        };
        entity
            .entity_data
            .lock()
            .hanging_entity
            .direction
            .set(direction);
        entity
    }

    /// Creates a painting from persistent entity data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        let position = load.position;
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(PaintingEntityData::new()),
            block_pos: SyncMutex::new(BlockPos::new(
                position.x.floor() as i32,
                position.y.floor() as i32,
                position.z.floor() as i32,
            )),
        }
    }

    /// Builds a painting for `block_pos`, wearing the biggest picture that fits.
    ///
    /// Vanilla parity: `Painting.create`. Returns `None` when nothing in the
    /// `placeable` tag survives on that wall -- that is what makes a painting
    /// item do nothing against a lone block instead of hanging a canvas in
    /// mid-air.
    #[must_use]
    pub fn create(
        entity_type: EntityTypeRef,
        id: i32,
        world: &Arc<World>,
        block_pos: BlockPos,
        direction: Direction,
    ) -> Option<Self> {
        let candidate =
            Self::new_attached(entity_type, id, block_pos, direction, Arc::downgrade(world));

        let mut fitting: Vec<PaintingVariantRef> = REGISTRY
            .painting_variants
            .iter_tag(&PaintingVariantTag::PLACEABLE)
            .filter(|variant| {
                candidate.set_variant(variant);
                candidate.survives()
            })
            .collect();

        // Vanilla keeps only the joint-largest by area and rolls between them,
        // so a wide wall never gets a postage stamp.
        let largest_area = fitting.iter().map(|variant| variant_area(variant)).max()?;
        fitting.retain(|variant| variant_area(variant) >= largest_area);
        let selected = *fitting.get(rand::random_range(0..fitting.len()))?;

        candidate.set_variant(selected);
        Some(candidate)
    }

    /// Returns the picture this painting wears.
    #[must_use]
    pub fn variant(&self) -> PaintingVariantRef {
        self.entity_data.lock().painting_variant.get().value()
    }

    /// Hangs a different picture, resizing the entity to match.
    ///
    /// Vanilla parity: `Painting.setVariant` together with the
    /// `recalculateBoundingBox` its `onSyncedDataUpdated` fires -- a different
    /// variant is a different size, so the box cannot stay.
    pub fn set_variant(&self, variant: PaintingVariantRef) {
        self.entity_data
            .lock()
            .painting_variant
            .set(RegistryReference::new(variant));
        self.recalculate_position();
    }

    /// Returns the direction the canvas faces.
    #[must_use]
    pub fn direction(&self) -> Direction {
        *self.entity_data.lock().hanging_entity.direction.get()
    }

    /// Returns whether the painting can stay where it is.
    ///
    /// Vanilla parity: `HangingEntity.survives`. Steel's item frame has no
    /// such check at all today, and this change deliberately leaves it that
    /// way; the painting needs one because its size is chosen by whether it
    /// fits.
    ///
    /// Deviation: vanilla's `hasLevelCollision` also refuses a pop box that
    /// crosses the world border. Steel's `WorldCollisionProvider` only reports
    /// border shapes for a moving entity, so only the block half is tested.
    #[must_use]
    pub fn survives(&self) -> bool {
        let Some(world) = self.level() else {
            return false;
        };
        let pop_box = self.bounding_box();
        if has_block_collision(&WorldCollisionProvider::new(&world), pop_box) {
            return false;
        }
        self.is_supported(&world, pop_box) && self.can_coexist(&world, pop_box)
    }

    /// Vanilla parity: the support half of `HangingEntity.survives`, walked
    /// over `calculateSupportBox`.
    fn is_supported(&self, world: &World, pop_box: WorldAabb) -> bool {
        let support = pop_box
            .translate(self.direction().offset_vec().as_dvec3() * -0.5)
            .deflate(SUPPORT_BOX_EPSILON);
        let min = BlockPos::new(
            support.min_x().floor() as i32,
            support.min_y().floor() as i32,
            support.min_z().floor() as i32,
        );
        let max = BlockPos::new(
            support.max_x().floor() as i32,
            support.max_y().floor() as i32,
            support.max_z().floor() as i32,
        );
        BlockPos::between_closed(min, max).all(|pos| {
            let state = world.get_block_state(pos);
            state.is_solid() || is_diode(state)
        })
    }

    /// Vanilla parity: `HangingEntity.canCoexist(false)`.
    ///
    /// Deviation: vanilla tests `EntityTypeTest.forClass(HangingEntity.class)`.
    /// Steel has no `HangingEntity` supertype, so the concrete hanging
    /// entities are recognized one by one in [`hanging_entity_direction`].
    fn can_coexist(&self, world: &World, pop_box: WorldAabb) -> bool {
        let own_id = self.base.id();
        let own_direction = self.direction();
        let own_type = self.entity_type;
        !world.has_entity_in_aabb_matching(&pop_box, |entity| {
            let Some(direction) = hanging_entity_direction(entity) else {
                return false;
            };
            entity.id() != own_id
                && (entity.entity_type() == own_type || direction == own_direction)
        })
    }

    /// Vanilla parity: `HangingEntity.setDirection`.
    fn set_direction(&self, direction: Direction) {
        self.entity_data
            .lock()
            .hanging_entity
            .direction
            .set(direction);
        self.base
            .set_rotation(Self::rotation_for_direction(direction));
        self.recalculate_position();
    }

    /// Vanilla parity: `HangingEntity.recalculateBoundingBox`.
    fn recalculate_position(&self) {
        let block_pos = *self.block_pos.lock();
        let (direction, variant) = {
            let entity_data = self.entity_data.lock();
            (
                *entity_data.hanging_entity.direction.get(),
                entity_data.painting_variant.get().value(),
            )
        };
        let bounding_box = Self::calculate_bounding_box(block_pos, direction, variant);
        if let Err(error) = self.base.try_set_position(bounding_box.center()) {
            panic!(
                "failed to commit painting {} position recalculation: {error}",
                self.base.id()
            );
        }
        self.base.set_bounding_box(bounding_box);
    }

    fn rotation_for_direction(direction: Direction) -> (f32, f32) {
        (f32::from(direction_2d_data_value(direction)) * 90.0, 0.0)
    }

    /// Vanilla parity: `Painting.calculateBoundingBox`.
    ///
    /// The canvas hangs off the middle of the block, then slides half a block
    /// left and up for every even dimension: an even-sized painting straddles
    /// the seam between blocks, an odd-sized one is centered on one.
    fn calculate_bounding_box(
        block_pos: BlockPos,
        direction: Direction,
        variant: PaintingVariantRef,
    ) -> WorldAabb {
        let value = variant.value();
        let attached_to_wall = block_pos.0.as_dvec3() + DVec3::splat(0.5)
            - direction.offset_vec().as_dvec3() * SHIFT_TO_BLOCK_WALL;
        let left = direction.rotate_y_counter_clockwise();
        let position = attached_to_wall
            + left.offset_vec().as_dvec3() * offset_for_painting_size(value.width)
            + DVec3::Y * offset_for_painting_size(value.height);

        let width = f64::from(value.width);
        let x_size = if direction.axis() == Axis::X {
            DEPTH
        } else {
            width
        };
        let z_size = if direction.axis() == Axis::Z {
            DEPTH
        } else {
            width
        };
        WorldAabb::of_size(position, x_size, f64::from(value.height), z_size)
    }
}

impl Entity for PaintingEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    /// Vanilla parity: `Painting.getAddEntityPacket`, which sends the 3-D
    /// facing even though the save file keeps the 2-D one.
    fn spawn_data(&self) -> i32 {
        self.direction().get_3d_data_value()
    }

    /// Vanilla parity: `BlockAttachedEntity.thunderHit`, an empty override.
    /// Lightning passes straight through anything hung on a block.
    fn thunder_hit(&self, _world: &World, _bolt: &dyn Entity) {}

    fn hurt(&self, world: &World, source: &DamageSource, _amount: f32) -> bool {
        self.hurt_block_attached(world, source)
    }

    /// Vanilla parity: `Painting.trackingPosition`, the lower corner of the
    /// block rather than the middle of the canvas.
    fn spawn_position(&self) -> DVec3 {
        let block_pos = *self.block_pos.lock();
        DVec3::new(
            f64::from(block_pos.x()),
            f64::from(block_pos.y()),
            f64::from(block_pos.z()),
        )
    }

    fn is_pickable(&self) -> bool {
        true
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `Painting.addAdditionalSaveData`.
    ///
    /// The facing is the 2-D legacy value, not the 3-D one the item frame
    /// writes: `Painting` stores it through `Direction.LEGACY_ID_CODEC_2D`.
    fn save_additional(&self, nbt: &mut NbtCompound) {
        nbt.insert("facing", direction_2d_data_value(self.direction()) as i8);

        let block_pos = *self.block_pos.lock();
        nbt.insert(
            "block_pos",
            NbtTag::IntArray(vec![block_pos.x(), block_pos.y(), block_pos.z()]),
        );

        nbt.insert("variant", self.variant().key.to_string());
    }

    /// Vanilla parity: `Painting.readAdditionalSaveData`.
    ///
    /// The facing is read before `block_pos` but applied after it, because
    /// applying it recalculates a box anchored on that block.
    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        let facing = nbt.byte("facing").map_or(Direction::South, |value| {
            direction_from_2d_data_value(i32::from(value))
        });

        if let Some(block_pos) = nbt.int_array("block_pos")
            && block_pos.len() == 3
        {
            *self.block_pos.lock() = BlockPos::new(block_pos[0], block_pos[1], block_pos[2]);
        }

        self.set_direction(facing);

        if let Some(variant) = nbt.string("variant")
            && let Ok(key) = Identifier::from_str(variant.to_str().as_ref())
            && let Some(variant) = REGISTRY.painting_variants.by_key(&key)
        {
            self.set_variant(variant);
        }

        self.recalculate_position();
    }
}

/// Vanilla parity: `PaintingVariant.area`, which Steel's registry value does
/// not carry.
const fn variant_area(variant: PaintingVariantRef) -> i32 {
    variant.value().width * variant.value().height
}

/// Vanilla parity: `Painting.offsetForPaintingSize`.
const fn offset_for_painting_size(size: i32) -> f64 {
    if size % 2 == 0 { 0.5 } else { 0.0 }
}

/// Vanilla parity: `DiodeBlock.isDiode`. `RepeaterBlock` and `ComparatorBlock`
/// are the only two `DiodeBlock` subclasses, so the class test becomes a pair
/// of block comparisons.
fn is_diode(state: BlockStateId) -> bool {
    let block = state.get_block();
    block == &vanilla_blocks::REPEATER || block == &vanilla_blocks::COMPARATOR
}

/// The facing of `entity` when it is one of Steel's hanging entities.
///
/// Vanilla parity: the `EntityTypeTest.forClass(HangingEntity.class)` of
/// `HangingEntity.canCoexist`. Steel has no `HangingEntity` supertype, so the
/// painting is matched by its concrete type and both item frames through the
/// `ItemFrame` capability trait -- which is what stops a glow frame being hung
/// inside a plain one.
fn hanging_entity_direction(entity: &dyn Entity) -> Option<Direction> {
    if let Some(painting) = entity.downcast_ref::<PaintingEntity>() {
        return Some(painting.direction());
    }
    entity.as_item_frame().map(ItemFrame::direction)
}

/// Vanilla parity: `Direction.get2DDataValue`.
const fn direction_2d_data_value(direction: Direction) -> u8 {
    match direction {
        Direction::South | Direction::Down | Direction::Up => 0,
        Direction::West => 1,
        Direction::North => 2,
        Direction::East => 3,
    }
}

/// Vanilla parity: `Direction.from2DDataValue`, wrap included.
const fn direction_from_2d_data_value(value: i32) -> Direction {
    let remainder = value % 4;
    let index = if remainder < 0 { -remainder } else { remainder };
    match index {
        1 => Direction::West,
        2 => Direction::North,
        3 => Direction::East,
        _ => Direction::South,
    }
}

impl BlockAttached for PaintingEntity {
    /// Vanilla parity: `Painting.dropItem`. Every painting drops the same
    /// plain item -- the variant is rolled again when it is hung back up.
    fn drop_item(&self, world: &World, caused_by: Option<&SharedEntity>) {
        if !world.get_game_rule(&ENTITY_DROPS) {
            return;
        }
        self.play_sound(&sound_events::ENTITY_PAINTING_BREAK, 1.0, 1.0);
        if drop_would_be_wasted(caused_by) {
            return;
        }
        self.spawn_at_location(ItemStack::new(&vanilla_items::PAINTING), 0.0);
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_entities, vanilla_painting_variants};
    use steel_utils::ChunkPos;
    use steel_utils::types::UpdateFlags;

    use steel_registry::vanilla_damage_types;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::entity::entities::ItemEntity;
    use crate::entity::next_entity_id;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    /// The block a test painting is anchored to.
    fn anchor() -> BlockPos {
        BlockPos::new(4, 70, 9)
    }

    /// The box a painting of `variant` occupies on the wall it faces.
    fn canvas_box(direction: Direction, variant: PaintingVariantRef) -> WorldAabb {
        let painting = PaintingEntity::new_attached(
            &vanilla_entities::PAINTING,
            1,
            anchor(),
            direction,
            Weak::new(),
        );
        painting.set_variant(variant);
        painting.bounding_box()
    }

    /// The two corners, so a failure prints coordinates instead of a struct.
    fn corners(aabb: WorldAabb) -> ([f64; 3], [f64; 3]) {
        (
            [aabb.min_x(), aabb.min_y(), aabb.min_z()],
            [aabb.max_x(), aabb.max_y(), aabb.max_z()],
        )
    }

    /// A one-by-one canvas lies flat on the face it looks at, a sixteenth of a
    /// block thick, covering exactly the block it is anchored to. Every one of
    /// the four walls has to come out the same way round.
    #[test]
    fn a_painting_lies_flat_against_the_wall_it_faces() {
        let kebab = &vanilla_painting_variants::KEBAB;

        assert_eq!(
            corners(canvas_box(Direction::South, kebab)),
            ([4.0, 70.0, 9.0], [5.0, 71.0, 9.0625])
        );
        assert_eq!(
            corners(canvas_box(Direction::North, kebab)),
            ([4.0, 70.0, 9.9375], [5.0, 71.0, 10.0])
        );
        assert_eq!(
            corners(canvas_box(Direction::East, kebab)),
            ([4.0, 70.0, 9.0], [4.0625, 71.0, 10.0])
        );
        assert_eq!(
            corners(canvas_box(Direction::West, kebab)),
            ([4.9375, 70.0, 9.0], [5.0, 71.0, 10.0])
        );
    }

    /// An even dimension slides the canvas half a block so it straddles the
    /// seam between two blocks; an odd one leaves it centered on the anchor.
    /// Sliding the wrong way is invisible on a square wall and glaring on a
    /// wide one, so the direction is pinned here too.
    #[test]
    fn an_even_sided_painting_straddles_the_block_seam() {
        assert_eq!(
            corners(canvas_box(
                Direction::South,
                &vanilla_painting_variants::POOL
            )),
            ([4.0, 70.0, 9.0], [6.0, 71.0, 9.0625]),
            "two wide, one tall: grows east of the anchor when facing south"
        );
        assert_eq!(
            corners(canvas_box(
                Direction::South,
                &vanilla_painting_variants::GRAHAM
            )),
            ([4.0, 70.0, 9.0], [5.0, 72.0, 9.0625]),
            "one wide, two tall: grows up, not sideways"
        );
        assert_eq!(
            corners(canvas_box(
                Direction::South,
                &vanilla_painting_variants::WITHER
            )),
            ([4.0, 70.0, 9.0], [6.0, 72.0, 9.0625]),
            "two by two: grows both ways"
        );
        assert_eq!(
            corners(canvas_box(
                Direction::North,
                &vanilla_painting_variants::POOL
            )),
            ([3.0, 70.0, 9.9375], [5.0, 71.0, 10.0]),
            "facing north the counter-clockwise side is west, so it grows west"
        );
    }

    /// Given a wall, a painting takes the biggest picture that fits on it --
    /// two blocks of wall means a two-block canvas, never a one-block one.
    #[test]
    fn a_painting_picks_the_largest_variant_that_fits() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("painting_largest_fit");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        // Two blocks of wall side by side, one block tall, with air in front.
        for x in 8..=9 {
            let _ = world.set_block(
                BlockPos::new(x, 70, 7),
                vanilla_blocks::STONE.default_state(),
                UpdateFlags::UPDATE_ALL,
            );
        }

        let painting = PaintingEntity::create(
            &vanilla_entities::PAINTING,
            1,
            &world,
            BlockPos::new(8, 70, 8),
            Direction::South,
        )
        .expect("a wall two wide holds a painting");

        let variant = painting.variant().value();
        assert_eq!((variant.width, variant.height), (2, 1));
    }

    /// A wall too small for anything in the placeable tag hangs nothing.
    #[test]
    fn a_painting_with_nothing_to_hang_on_is_not_created() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("painting_no_wall");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        assert!(
            PaintingEntity::create(
                &vanilla_entities::PAINTING,
                1,
                &world,
                BlockPos::new(8, 70, 8),
                Direction::South,
            )
            .is_none()
        );
    }

    /// A painting saves the 2-D facing while the item frame beside it saves
    /// the 3-D one. Writing the wrong table is silent -- the painting just
    /// comes back on a different wall -- so the value is pinned.
    #[test]
    fn a_saved_painting_stores_the_two_dimensional_facing() {
        let painting = PaintingEntity::new_attached(
            &vanilla_entities::PAINTING,
            1,
            anchor(),
            Direction::West,
            Weak::new(),
        );

        let mut nbt = NbtCompound::new();
        painting.save_additional(&mut nbt);

        assert_eq!(nbt.byte("facing"), Some(1), "west is 1 in 2-D, 4 in 3-D");
        assert_eq!(nbt.int_array("block_pos"), Some([4, 70, 9].as_slice()));
    }

    /// A painting is not a living entity, so the ordinary damage path refuses
    /// it and it would be indestructible without `BlockAttachedEntity`.
    #[test]
    fn a_punched_painting_comes_off_the_wall_and_drops_its_item() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("painting_breaks_and_drops");
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(anchor()));

        let painting = Arc::new(PaintingEntity::new_attached(
            &vanilla_entities::PAINTING,
            next_entity_id(),
            anchor(),
            Direction::West,
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(painting.clone())
            .expect("the painting's chunk is loaded");

        assert!(painting.hurt(
            &world,
            &DamageSource::environment(&vanilla_damage_types::GENERIC),
            1.0
        ));

        assert!(painting.is_removed());
        let everywhere = WorldAabb::new(-32.0, 0.0, -32.0, 32.0, 128.0, 32.0);
        assert!(
            world
                .get_entities_in_aabb(&everywhere)
                .iter()
                .any(|entity| {
                    entity
                        .as_ref()
                        .downcast_ref::<ItemEntity>()
                        .is_some_and(|item| item.get_item().is(&vanilla_items::PAINTING))
                }),
            "breaking a painting leaves the painting item behind"
        );
    }
}
