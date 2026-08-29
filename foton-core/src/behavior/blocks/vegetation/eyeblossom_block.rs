use std::sync::Arc;

use foton_macros::block_behavior;
use foton_registry::particle_type::{ParticleData, TrailParticleOption};
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::{sound_events, vanilla_blocks, vanilla_game_events, vanilla_particle_types};
use foton_utils::color::RgbColor;
use foton_utils::types::UpdateFlags;
use foton_utils::{BlockPos, BlockStateId};
use glam::DVec3;

use foton_registry::vanilla_mob_effects;

use crate::behavior::block::BlockBehavior;
use crate::behavior::context::BlockPlaceContext;
use crate::entity::MobEffectInstance;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader, World};

/// Vanilla `EyeblossomBlock.getBeeInteractionEffect`'s duration.
const BEE_POISON_DURATION_TICKS: i32 = 25;

/// Vanilla parity: `EyeblossomBlock.EYEBLOSSOM_XZ_RANGE`.
const NEIGHBOUR_XZ_RANGE: i32 = 3;

/// Vanilla parity: `EyeblossomBlock.EYEBLOSSOM_Y_RANGE`.
const NEIGHBOUR_Y_RANGE: i32 = 2;

/// Vanilla parity: the `distance * 5.0` and `distance * 10.0` bounds of the
/// delay `tryChangingState` gives each flower it wakes. A patch of eyeblossoms
/// turns as a ripple rather than all at once because of these two numbers.
const NEIGHBOUR_DELAY_MIN_PER_BLOCK: f64 = 5.0;
const NEIGHBOUR_DELAY_MAX_PER_BLOCK: f64 = 10.0;

use super::{BlockRef, default_surviving_state, survives_on_tag};

#[derive(Clone, Copy, PartialEq, Eq)]
/// Vanilla open/closed eyeblossom type from `classes.json`.
pub enum EyeblossomType {
    /// Emits open-eyeblossom effects and transforms closed at daytime.
    Open,
    /// Emits closed-eyeblossom effects and transforms open at nighttime.
    Closed,
}

impl EyeblossomType {
    /// Vanilla parity: `EyeblossomBlock.Type.open`, the flag the whole enum
    /// exists to carry.
    const fn is_open(self) -> bool {
        matches!(self, Self::Open)
    }

    /// Vanilla parity: `EyeblossomBlock.Type.transform`.
    const fn transform(self) -> Self {
        match self {
            Self::Open => Self::Closed,
            Self::Closed => Self::Open,
        }
    }

    /// Vanilla parity: `EyeblossomBlock.Type.block`.
    const fn block(self) -> BlockRef {
        match self {
            Self::Open => &vanilla_blocks::OPEN_EYEBLOSSOM,
            Self::Closed => &vanilla_blocks::CLOSED_EYEBLOSSOM,
        }
    }

    /// Vanilla parity: `EyeblossomBlock.Type.longSwitchSound`, played when the
    /// flower turns on its own random tick.
    const fn long_switch_sound(self) -> SoundEventRef {
        match self {
            Self::Open => &sound_events::BLOCK_EYEBLOSSOM_OPEN_LONG,
            Self::Closed => &sound_events::BLOCK_EYEBLOSSOM_CLOSE_LONG,
        }
    }

    /// Vanilla parity: `EyeblossomBlock.Type.shortSwitchSound`, played when the
    /// flower turns because a neighbour woke it.
    const fn short_switch_sound(self) -> SoundEventRef {
        match self {
            Self::Open => &sound_events::BLOCK_EYEBLOSSOM_OPEN,
            Self::Closed => &sound_events::BLOCK_EYEBLOSSOM_CLOSE,
        }
    }

    /// Vanilla parity: `EyeblossomBlock.Type.particleColor`.
    const fn particle_color(self) -> i32 {
        match self {
            Self::Open => 16_545_810,
            Self::Closed => 6_250_335,
        }
    }

    /// Vanilla parity: `EyeblossomBlock.Type.spawnTransformParticle`.
    fn spawn_transform_particle(self, world: &Arc<World>, pos: BlockPos) {
        let start = DVec3::new(
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()) + 0.5,
            f64::from(pos.z()) + 0.5,
        );
        let lifetime = 0.5 + rand::random::<f64>();
        let velocity = DVec3::new(
            rand::random::<f64>() - 0.5,
            rand::random::<f64>() + 1.0,
            rand::random::<f64>() - 0.5,
        );
        let target = start + velocity * lifetime;

        world.send_particles(
            ParticleData::new(
                &vanilla_particle_types::TRAIL,
                TrailParticleOption::new(
                    target,
                    RgbColor::new(self.particle_color()),
                    (20.0 * lifetime) as i32,
                ),
            ),
            start,
            1,
            DVec3::ZERO,
            0.0,
        );
    }
}

/// Vanilla parity: `EyeblossomBlock`.
///
/// The flower turns with the overworld clock: the `gameplay/eyeblossom_open`
/// environment attribute says which way it should be facing, and a flower that
/// disagrees swaps itself for the other block and wakes every eyeblossom within
/// three blocks so a patch turns as a ripple.
///
/// Foton gap: the ambient idle sound of `animateTick` is a `playLocalSound` and
/// is the client's own business, and `entityInside` -- the poison a bee walks
/// into -- waits on Foton dispatching that hook.
#[block_behavior]
pub struct EyeblossomBlock {
    block: BlockRef,
    #[json_arg(r#enum = "EyeblossomType", json = "type")]
    eyeblossom_type: EyeblossomType,
}

impl EyeblossomBlock {
    /// Creates a new eyeblossom behavior.
    #[must_use]
    pub const fn new(block: BlockRef, eyeblossom_type: EyeblossomType) -> Self {
        Self {
            block,
            eyeblossom_type,
        }
    }

    /// Turns the flower if the world disagrees with which way it is facing.
    ///
    /// Vanilla parity: `EyeblossomBlock.tryChangingState`. Returns whether it
    /// turned, which is what decides whether the caller makes a sound.
    fn try_changing_state(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) -> bool {
        let open = self.eyeblossom_type.is_open();
        if world.eyeblossom_open(open) == open {
            return false;
        }

        let new_type = self.eyeblossom_type.transform();
        world.set_block(
            pos,
            new_type.block().default_state(),
            UpdateFlags::UPDATE_ALL,
        );
        world.game_event(
            &vanilla_game_events::BLOCK_CHANGE,
            pos,
            &GameEventContext::new(None, Some(state)),
        );
        new_type.spawn_transform_particle(world, pos);
        self.wake_neighbours(state, world, pos);
        true
    }

    /// Queues the turn of every flower still facing the old way nearby.
    ///
    /// Vanilla parity: the `BlockPos.betweenClosed(..).forEach(..)` tail of
    /// `tryChangingState`. The delay grows with the distance, which is what
    /// makes a patch of eyeblossoms turn outwards from whichever one went
    /// first. The flower that just turned no longer matches `state`, so it does
    /// not wake itself.
    fn wake_neighbours(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        for x in -NEIGHBOUR_XZ_RANGE..=NEIGHBOUR_XZ_RANGE {
            for y in -NEIGHBOUR_Y_RANGE..=NEIGHBOUR_Y_RANGE {
                for z in -NEIGHBOUR_XZ_RANGE..=NEIGHBOUR_XZ_RANGE {
                    let nearby = pos.offset(x, y, z);
                    if world.get_block_state(nearby) != state {
                        continue;
                    }
                    let distance = f64::from(x * x + y * y + z * z).sqrt();
                    let delay = rand::random_range(
                        (distance * NEIGHBOUR_DELAY_MIN_PER_BLOCK) as i32
                            ..=(distance * NEIGHBOUR_DELAY_MAX_PER_BLOCK) as i32,
                    );
                    world.schedule_block_tick_default(nearby, self.block, delay);
                }
            }
        }
    }
}

impl BlockBehavior for EyeblossomBlock {
    fn can_survive(&self, _state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        survives_on_tag(world, pos, &BlockTag::SUPPORTS_VEGETATION)
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        default_surviving_state(self.block, self, context)
    }

    /// Vanilla parity: `EyeblossomBlock.getBeeInteractionEffect`, which is the
    /// same poison whether a bee walks into the flower or is fed one.
    fn bee_interaction_effect(&self) -> Option<MobEffectInstance> {
        Some(MobEffectInstance::with_duration(
            vanilla_mob_effects::POISON,
            BEE_POISON_DURATION_TICKS,
            0,
        ))
    }

    /// Vanilla parity: `EyeblossomBlock.randomTick`, the flower noticing the
    /// hour for itself. It gets the drawn-out sound.
    fn random_tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        if self.try_changing_state(state, world, pos) {
            let sound = self.eyeblossom_type.transform().long_switch_sound();
            world.play_block_sound(sound, pos, 1.0, 1.0, None);
        }
    }

    /// Vanilla parity: `EyeblossomBlock.tick`, the flower turning because a
    /// neighbour woke it. It gets the short sound.
    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        if self.try_changing_state(state, world, pos) {
            let sound = self.eyeblossom_type.transform().short_switch_sound();
            world.play_block_sound(sound, pos, 1.0, 1.0, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::{init_vanilla_registry, vanilla_blocks};
    use foton_utils::BlockPos;

    use crate::test_support::TestLevel;

    use super::*;

    fn level_with_support(support: BlockRef) -> TestLevel {
        TestLevel::default().with_block(BlockPos::new(0, 63, 0), support.default_state())
    }

    #[test]
    fn eyeblossom_requires_vegetation_support() {
        init_vanilla_registry();
        let behavior =
            EyeblossomBlock::new(&vanilla_blocks::CLOSED_EYEBLOSSOM, EyeblossomType::Closed);
        let pos = BlockPos::new(0, 64, 0);
        let state = vanilla_blocks::CLOSED_EYEBLOSSOM.default_state();

        assert!(behavior.can_survive(state, &level_with_support(&vanilla_blocks::DIRT), pos));
        assert!(!behavior.can_survive(state, &level_with_support(&vanilla_blocks::AIR), pos));
    }

    mod turning {
        use std::sync::Arc;

        use foton_registry::blocks::block_state_ext::BlockStateExt as _;
        use foton_registry::{init_vanilla_registry, vanilla_world_clocks};
        use foton_utils::ChunkPos;

        use super::super::*;
        use crate::behavior::init_behaviors;
        use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

        /// The overworld tick a flower should be closed at, from the `day`
        /// timeline's `gameplay/eyeblossom_open` track: false from 23401 round
        /// to 12600.
        const DAYTIME: i64 = 6000;
        /// And the stretch in between, where it should be open.
        const NIGHT: i64 = 18000;

        fn world_at(key: &'static str, time_of_day: i64) -> Arc<World> {
            init_vanilla_registry();
            init_behaviors();
            let world = fresh_test_world(key);
            insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
            world
                .set_clock_total_ticks(&vanilla_world_clocks::OVERWORLD, time_of_day)
                .expect("the overworld clock exists in a test world");
            world
        }

        fn plant(world: &Arc<World>, pos: BlockPos, block: BlockRef) {
            assert!(world.set_block(
                pos.below(),
                vanilla_blocks::DIRT.default_state(),
                UpdateFlags::UPDATE_ALL,
            ));
            assert!(world.set_block(pos, block.default_state(), UpdateFlags::UPDATE_ALL));
        }

        /// Vanilla parity: `tryChangingState` reading
        /// `EnvironmentAttributes.EYEBLOSSOM_OPEN` and disagreeing with itself.
        /// This is the whole block: both ticks were literal no-ops, so an
        /// eyeblossom stayed whichever way it was placed for ever.
        #[test]
        fn a_closed_eyeblossom_opens_at_night() {
            let world = world_at("eyeblossom_opens", NIGHT);
            let behavior =
                EyeblossomBlock::new(&vanilla_blocks::CLOSED_EYEBLOSSOM, EyeblossomType::Closed);
            let pos = BlockPos::new(8, 64, 8);
            plant(&world, pos, &vanilla_blocks::CLOSED_EYEBLOSSOM);

            behavior.random_tick(world.get_block_state(pos), &world, pos);

            assert_eq!(
                world.get_block_state(pos).get_block(),
                &vanilla_blocks::OPEN_EYEBLOSSOM
            );
        }

        /// And the other way round, which is the arm that reads the timeline's
        /// `false` keyframe rather than its `true` one.
        #[test]
        fn an_open_eyeblossom_closes_by_day() {
            let world = world_at("eyeblossom_closes", DAYTIME);
            let behavior =
                EyeblossomBlock::new(&vanilla_blocks::OPEN_EYEBLOSSOM, EyeblossomType::Open);
            let pos = BlockPos::new(8, 64, 8);
            plant(&world, pos, &vanilla_blocks::OPEN_EYEBLOSSOM);

            behavior.random_tick(world.get_block_state(pos), &world, pos);

            assert_eq!(
                world.get_block_state(pos).get_block(),
                &vanilla_blocks::CLOSED_EYEBLOSSOM
            );
        }

        /// A flower already facing the way the hour wants stays put, and says
        /// so -- which is what stops the sound and the particle.
        #[test]
        fn an_eyeblossom_that_already_agrees_with_the_hour_stays_put() {
            let world = world_at("eyeblossom_agrees", NIGHT);
            let behavior =
                EyeblossomBlock::new(&vanilla_blocks::OPEN_EYEBLOSSOM, EyeblossomType::Open);
            let pos = BlockPos::new(8, 64, 8);
            plant(&world, pos, &vanilla_blocks::OPEN_EYEBLOSSOM);

            assert!(!behavior.try_changing_state(world.get_block_state(pos), &world, pos));
            assert_eq!(
                world.get_block_state(pos).get_block(),
                &vanilla_blocks::OPEN_EYEBLOSSOM
            );
        }

        /// Vanilla parity: the neighbour sweep at the end of `tryChangingState`,
        /// which is what turns a patch as a ripple instead of leaving the rest
        /// of it waiting on random ticks of its own. The flower six blocks away
        /// is out of the seven-wide box and must be left alone.
        #[test]
        fn a_turning_eyeblossom_wakes_the_patch_around_it() {
            let world = world_at("eyeblossom_patch", NIGHT);
            let behavior =
                EyeblossomBlock::new(&vanilla_blocks::CLOSED_EYEBLOSSOM, EyeblossomType::Closed);
            let pos = BlockPos::new(8, 64, 8);
            let near = BlockPos::new(10, 64, 8);
            let far = BlockPos::new(8, 64, 14);
            for spot in [pos, near, far] {
                plant(&world, spot, &vanilla_blocks::CLOSED_EYEBLOSSOM);
            }

            behavior.random_tick(world.get_block_state(pos), &world, pos);

            assert!(
                world.has_scheduled_block_tick(near, &vanilla_blocks::CLOSED_EYEBLOSSOM),
                "the flower two blocks away was woken"
            );
            assert!(
                !world.has_scheduled_block_tick(far, &vanilla_blocks::CLOSED_EYEBLOSSOM),
                "the flower six blocks away is outside the box vanilla sweeps"
            );
            assert_eq!(
                world.get_block_state(near).get_block(),
                &vanilla_blocks::CLOSED_EYEBLOSSOM,
                "waking is a scheduled tick, not an immediate turn"
            );
        }
    }
}
