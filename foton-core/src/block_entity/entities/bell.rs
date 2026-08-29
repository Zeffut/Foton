//! Bell block entity.
//!
//! Vanilla parity: `BellBlockEntity`. A rung bell does three things a block
//! state cannot hold: it swings for fifty ticks, it tells everything within
//! thirty-two blocks that it was rung, and -- if any of those were raiders --
//! it resonates for two seconds and then lights every raider within forty-eight
//! blocks up for three.
//!
//! The swing itself is the client's, driven by the block event this sends; the
//! server keeps the counter only because the resonance is measured from it.

use std::sync::{Arc, Weak};

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::vanilla_entity_type_tags::EntityTypeTag;
use foton_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_block_entity_types, vanilla_mob_effects,
};
use foton_utils::locks::SyncMutex;
use foton_utils::{BlockPos, BlockStateId, Direction, DowncastType, DowncastTypeKey, WorldAabb};
use simdnbt::borrow::BaseNbtCompound as BorrowedNbtCompound;
use simdnbt::owned::NbtCompound;

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::{LivingEntity, Mob, MobEffectInstance, SharedEntity};
use crate::world::World;

/// The block event a rung bell broadcasts.
///
/// Vanilla parity: `BellBlock.EVENT_BELL_RING`, the `1` of `BellBlockEntity`.
pub const EVENT_BELL_RING: i32 = 1;

/// How long the bell swings for.
///
/// Vanilla parity: `BellBlockEntity.DURATION`.
const DURATION: i32 = 50;

/// How long a revealed raider glows.
///
/// Vanilla parity: `BellBlockEntity.GLOW_DURATION`.
const GLOW_DURATION: i32 = 60;

/// How long a cached sweep of nearby entities is reused for.
///
/// Vanilla parity: `BellBlockEntity.MIN_TICKS_BETWEEN_SEARCHES`.
const MIN_TICKS_BETWEEN_SEARCHES: i64 = 60;

/// How long the bell resonates before it gives the raiders away.
///
/// Vanilla parity: `BellBlockEntity.MAX_RESONATION_TICKS`.
const MAX_RESONATION_TICKS: i32 = 40;

/// How far into the swing the resonance may start.
///
/// Vanilla parity: `BellBlockEntity.TICKS_BEFORE_RESONATION`.
const TICKS_BEFORE_RESONATION: i32 = 5;

/// How far out the sweep of nearby entities reaches.
///
/// Vanilla parity: `BellBlockEntity.SEARCH_RADIUS`.
const SEARCH_RADIUS: f64 = 48.0;

/// How far a mob can be and still be told the bell rang.
///
/// Vanilla parity: `BellBlockEntity.HEAR_BELL_RADIUS`.
const HEAR_BELL_RADIUS: f64 = 32.0;

/// How far a raider can be and still be given away by the resonance.
///
/// Vanilla parity: `BellBlockEntity.HIGHLIGHT_RAIDERS_RADIUS`.
const HIGHLIGHT_RAIDERS_RADIUS: f64 = 48.0;

/// Volume the resonance is played at.
///
/// Vanilla parity: the `1.0F` of `SoundEvents.BELL_RESONATE`.
const RESONATE_VOLUME: f32 = 1.0;

/// What a rung bell is in the middle of.
#[derive(Debug)]
struct BellState {
    /// Vanilla parity: `BellBlockEntity.lastRingTimestamp`.
    last_ring_timestamp: i64,
    /// Vanilla parity: `BellBlockEntity.ticks`.
    ticks: i32,
    /// Vanilla parity: `BellBlockEntity.shaking`.
    shaking: bool,
    /// Vanilla parity: `BellBlockEntity.nearbyEntities`, by entity id.
    ///
    /// Vanilla caches the mobs themselves. A block entity that held them would
    /// keep dead mobs alive for a minute at a time and reach into the entity
    /// list from a chunk, so Foton caches ids and resolves them again, the way
    /// [`crate::entity::raider::Raider`] resolves its raid. `None` is vanilla's
    /// null: no sweep has been taken yet.
    nearby_entities: Option<Vec<i32>>,
    /// Vanilla parity: `BellBlockEntity.resonating`.
    resonating: bool,
    /// Vanilla parity: `BellBlockEntity.resonationTicks`.
    resonation_ticks: i32,
}

/// A bell, and what it is in the middle of.
///
/// Vanilla parity: `net.minecraft.world.level.block.entity.BellBlockEntity`.
///
/// Vanilla also stores the direction it was struck from; that is only read by
/// the client's swing animation, which learns it from the block event instead,
/// so nothing on the server would ever read it back.
pub struct BellBlockEntity {
    base: BlockEntityBase,
    bell: SyncMutex<BellState>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `BellBlockEntity`.
unsafe impl DowncastType for BellBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:block_entity/bell");
}

impl BellBlockEntity {
    /// Creates a bell that has never been rung.
    #[must_use]
    pub fn new(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(&vanilla_block_entity_types::BELL, world, pos, state),
            bell: SyncMutex::new(BellState {
                last_ring_timestamp: 0,
                ticks: 0,
                shaking: false,
                nearby_entities: None,
                resonating: false,
                resonation_ticks: 0,
            }),
        }
    }

    /// Starts the swing and tells the world about it.
    ///
    /// Vanilla parity: `BellBlockEntity.onHit`. The block event it broadcasts
    /// comes back through [`BlockEntity::trigger_event`], which is what
    /// actually takes the sweep -- so a bell rung on a server nobody is
    /// watching still finds its raiders.
    pub fn on_hit(&self, world: &Arc<World>, click_direction: Direction) {
        {
            let mut bell = self.bell.lock();
            if bell.shaking {
                bell.ticks = 0;
            } else {
                bell.shaking = true;
            }
        }
        world.block_event(
            self.get_block_pos(),
            self.get_block_state().get_block(),
            EVENT_BELL_RING,
            click_direction.get_3d_data_value(),
        );
    }

    /// Refreshes the cached sweep and tells everything close enough it rang.
    ///
    /// Vanilla parity: the server half of `BellBlockEntity.updateEntities`.
    fn update_entities(&self, world: &Arc<World>) {
        let pos = self.get_block_pos();
        let game_time = world.game_time();

        let rescan = {
            let bell = self.bell.lock();
            game_time > bell.last_ring_timestamp + MIN_TICKS_BETWEEN_SEARCHES
                || bell.nearby_entities.is_none()
        };
        if rescan {
            let (x, y, z) = (f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()));
            let search_box =
                WorldAabb::new(x, y, z, x + 1.0, y + 1.0, z + 1.0).inflate(SEARCH_RADIUS);
            let found: Vec<i32> = world
                .get_entities_in_aabb_matching(&search_box, |entity| {
                    entity.as_living_entity().is_some()
                })
                .iter()
                .map(|entity| entity.id())
                .collect();
            let mut bell = self.bell.lock();
            bell.last_ring_timestamp = game_time;
            bell.nearby_entities = Some(found);
        }

        for entity in self.nearby_living(world) {
            let Some(living) = entity.as_living_entity() else {
                continue;
            };
            if !Self::is_close_enough(pos, living, HEAR_BELL_RADIUS) {
                continue;
            }
            let Some(brain) = entity.as_mob().and_then(Mob::brain) else {
                continue;
            };
            brain.set_memory(memory_module_types::HEARD_BELL_TIME, game_time);
        }
    }

    /// Resolves the cached sweep back to the mobs that are still there.
    ///
    /// Vanilla parity: the `entity.isAlive() && !entity.isRemoved()` half of
    /// every walk over `nearbyEntities`; an id that no longer resolves is a mob
    /// that has since been removed.
    fn nearby_living(&self, world: &Arc<World>) -> Vec<SharedEntity> {
        let ids = self.bell.lock().nearby_entities.clone().unwrap_or_default();
        ids.into_iter()
            .filter_map(|id| world.get_entity_by_id(id))
            .filter(|entity| {
                !entity.is_removed()
                    && entity
                        .as_living_entity()
                        .is_some_and(LivingEntity::is_alive)
            })
            .collect()
    }

    /// Vanilla parity: the `blockPos.closerToCenterThan(entity.position(), r)`
    /// every one of the sweeps measures with.
    fn is_close_enough(pos: BlockPos, living: &dyn LivingEntity, radius: f64) -> bool {
        let (x, y, z) = pos.get_center();
        glam::DVec3::new(x, y, z).distance_squared(living.position()) < radius * radius
    }

    /// Vanilla parity: `BellBlockEntity.areRaidersNearby`.
    fn are_raiders_nearby(&self, world: &Arc<World>) -> bool {
        let pos = self.get_block_pos();
        self.nearby_living(world).iter().any(|entity| {
            Self::is_raider(entity)
                && entity
                    .as_living_entity()
                    .is_some_and(|living| Self::is_close_enough(pos, living, HEAR_BELL_RADIUS))
        })
    }

    /// Vanilla parity: `BellBlockEntity.makeRaidersGlow`, whose
    /// `isRaiderWithinRange` measures forty-eight rather than the thirty-two
    /// that started the resonance.
    fn make_raiders_glow(&self, world: &Arc<World>) {
        let pos = self.get_block_pos();
        for entity in self.nearby_living(world) {
            if !Self::is_raider(&entity) {
                continue;
            }
            let Some(living) = entity.as_living_entity() else {
                continue;
            };
            if !Self::is_close_enough(pos, living, HIGHLIGHT_RAIDERS_RADIUS) {
                continue;
            }
            living.add_mob_effect(MobEffectInstance::with_duration(
                vanilla_mob_effects::GLOWING,
                GLOW_DURATION,
                0,
            ));
        }
    }

    /// Vanilla parity: the `entity.is(EntityTypeTags.RAIDERS)` of both sweeps.
    fn is_raider(entity: &SharedEntity) -> bool {
        REGISTRY
            .entity_types
            .is_in_tag(entity.entity_type(), &EntityTypeTag::RAIDERS)
    }
}

impl BlockEntity for BellBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn trigger_event(&self, param_a: i32, _param_b: i32) -> bool {
        if param_a != EVENT_BELL_RING {
            return false;
        }
        let Some(world) = self.get_level() else {
            return false;
        };
        self.update_entities(&world);
        let mut bell = self.bell.lock();
        bell.resonation_ticks = 0;
        bell.ticks = 0;
        bell.shaking = true;
        true
    }

    /// Vanilla saves nothing for a bell: the swing and the resonance are both
    /// shorter than the chunk save interval, and `BellBlockEntity` overrides
    /// neither `loadAdditional` nor `saveAdditional`.
    fn load_additional(&self, _nbt: &BorrowedNbtCompound<'_>) {}

    fn save_additional(&self, _nbt: &mut NbtCompound) {}

    /// Vanilla parity: the server half of `BellBlockEntity.tick`, whose
    /// resonation-end action is `makeRaidersGlow`. The client half throws
    /// particles instead, which is the client's own business.
    ///
    /// Vanilla runs the raider sweep and the resonance counter in one pass;
    /// the sweep has to resolve entity ids through the world, so it runs here
    /// with the bell's own lock released and the counter is advanced after it.
    /// That keeps the vanilla ordering, where the tick that starts the
    /// resonance is also its first tick -- and so the last one on which the
    /// sweep can run.
    fn tick(&self, world: &Arc<World>) {
        let check_raiders = {
            let mut bell = self.bell.lock();
            if bell.shaking {
                bell.ticks += 1;
            }
            if bell.ticks >= DURATION {
                bell.shaking = false;
                bell.ticks = 0;
            }
            bell.ticks >= TICKS_BEFORE_RESONATION && bell.resonation_ticks == 0
        };

        let raiders_nearby = check_raiders && self.are_raiders_nearby(world);

        let resonation_over = {
            let mut bell = self.bell.lock();
            if raiders_nearby {
                bell.resonating = true;
            }
            if !bell.resonating {
                false
            } else if bell.resonation_ticks < MAX_RESONATION_TICKS {
                bell.resonation_ticks += 1;
                false
            } else {
                bell.resonating = false;
                true
            }
        };

        if raiders_nearby {
            world.play_block_sound(
                &sound_events::BLOCK_BELL_RESONATE,
                self.get_block_pos(),
                RESONATE_VOLUME,
                1.0,
                None,
            );
        }
        if resonation_over {
            self.make_raiders_glow(world);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use foton_registry::{
        init_vanilla_registry, vanilla_blocks, vanilla_entities, vanilla_mob_effects,
    };
    use foton_utils::types::UpdateFlags;
    use foton_utils::{BlockPos, ChunkPos, Downcast as _};
    use glam::DVec3;

    use super::{BellBlockEntity, GLOW_DURATION, MAX_RESONATION_TICKS, TICKS_BEFORE_RESONATION};
    use crate::behavior::blocks::BellBlock;
    use crate::behavior::init_behaviors;
    use crate::block_entity::{SharedBlockEntity, init_block_entities};
    use crate::entity::{ENTITIES, SharedEntity, init_entities, next_entity_id};
    use crate::test_support::{fresh_test_world, insert_entity_ticking_chunk};
    use crate::world::World;

    const BELL: BlockPos = BlockPos::new(8, 64, 8);

    /// The tick the reveal lands on, counting from the ring: five before the
    /// resonance may start, then its full forty.
    const TICKS_TO_RESONATE: i32 = TICKS_BEFORE_RESONATION + MAX_RESONATION_TICKS;

    /// Ten blocks out: inside the thirty-two that sets the bell resonating.
    const NEAR_RAIDER: DVec3 = DVec3::new(18.5, 64.0, 8.5);

    /// Forty blocks out: outside the thirty-two that sets it resonating, inside
    /// the forty-eight the resonance reaches.
    const FAR_RAIDER: DVec3 = DVec3::new(48.5, 64.0, 8.5);

    fn bell_world(key: &'static str) -> Arc<World> {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
        init_entities();
        let world = fresh_test_world(key);
        for chunk_x in -1..=4 {
            for chunk_z in -1..=4 {
                insert_entity_ticking_chunk(&world, ChunkPos::new(chunk_x, chunk_z));
            }
        }
        assert!(world.set_block(
            BELL,
            vanilla_blocks::BELL.default_state(),
            UpdateFlags::UPDATE_ALL,
        ));
        world
    }

    /// The bell's own block entity, which placing the block should have
    /// attached -- a bell without one cannot ring at all.
    fn bell_entity(world: &Arc<World>) -> SharedBlockEntity {
        let block_entity = world
            .get_block_entity(BELL)
            .expect("placing a bell attaches its block entity");
        assert!(
            block_entity.downcast_ref::<BellBlockEntity>().is_some(),
            "the attached block entity is a bell"
        );
        block_entity
    }

    fn spawn_pillager(world: &Arc<World>, position: DVec3) -> SharedEntity {
        let entity = ENTITIES
            .create(
                &vanilla_entities::PILLAGER,
                next_entity_id(),
                position,
                Arc::downgrade(world),
            )
            .expect("the pillager factory is registered");
        world
            .try_add_entity(SharedEntity::clone(&entity))
            .expect("the chunk is entity-ticking, so the pillager attaches");
        entity
    }

    fn is_glowing(entity: &SharedEntity) -> bool {
        entity
            .as_living_entity()
            .expect("a pillager is a living entity")
            .mob_effect(vanilla_mob_effects::GLOWING)
            .is_some()
    }

    /// Rings the bell the way a player does and then runs the bell for `ticks`.
    fn ring_and_run(world: &Arc<World>, bell: &SharedBlockEntity, ticks: i32) {
        assert!(
            BellBlock::attempt_to_ring(world, BELL, None, None),
            "there is a bell block entity at the bell, so the ring lands"
        );
        // The ring only queues a block event; running the queue is what calls
        // `trigger_event`, and that is what takes the sweep.
        world.run_block_events();
        for _ in 0..ticks {
            bell.tick(world);
        }
    }

    /// The headline. Every link is on this path: the queued block event coming
    /// back through the block behavior, the sweep it takes, the two-second
    /// resonance, and the Glowing effect the raider is left wearing.
    #[test]
    fn a_bell_rung_over_a_raider_lights_it_up() {
        let world = bell_world("bell_reveals_raider");
        let bell = bell_entity(&world);
        let pillager = spawn_pillager(&world, NEAR_RAIDER);

        assert!(
            !is_glowing(&pillager),
            "nothing has rung yet, so the pillager is not lit"
        );
        ring_and_run(&world, &bell, TICKS_TO_RESONATE);

        let glow = pillager
            .as_living_entity()
            .expect("a pillager is a living entity")
            .mob_effect(vanilla_mob_effects::GLOWING)
            .expect("the resonance ran out over a raider, so it should be glowing");
        assert!(
            glow.duration() <= GLOW_DURATION,
            "the bell hands out the three-second effect, not a longer one"
        );
    }

    /// The resonance only starts if a raider is within thirty-two blocks. One
    /// at forty is inside the forty-eight-block highlight but outside that
    /// trigger, so on its own it is never given away -- which is what makes the
    /// next test's pass mean something.
    #[test]
    fn a_raider_too_far_to_set_the_bell_resonating_is_not_given_away() {
        let world = bell_world("bell_far_raider_alone");
        let bell = bell_entity(&world);
        let distant = spawn_pillager(&world, FAR_RAIDER);

        ring_and_run(&world, &bell, TICKS_TO_RESONATE);

        assert!(
            !is_glowing(&distant),
            "no raider was within thirty-two blocks, so the bell never resonated"
        );
    }

    /// Once something close enough has set the bell off, the reveal reaches
    /// further than the trigger did: the same distant raider left alone above is
    /// lit by a companion standing at the bell.
    #[test]
    fn a_resonating_bell_reaches_further_than_what_set_it_off() {
        let world = bell_world("bell_far_raider_with_company");
        let bell = bell_entity(&world);
        let near = spawn_pillager(&world, NEAR_RAIDER);
        let distant = spawn_pillager(&world, FAR_RAIDER);

        ring_and_run(&world, &bell, TICKS_TO_RESONATE);

        assert!(is_glowing(&near), "the raider that set it off is lit");
        assert!(
            is_glowing(&distant),
            "forty blocks is outside the trigger but inside the highlight"
        );
    }

    /// The reveal is not instant. Vanilla makes the village wait two seconds
    /// between the ring and the glow, and that window is the raiders' chance to
    /// get out of range.
    #[test]
    fn the_reveal_waits_out_the_resonance() {
        let world = bell_world("bell_resonance_takes_time");
        let bell = bell_entity(&world);
        let pillager = spawn_pillager(&world, NEAR_RAIDER);

        ring_and_run(&world, &bell, TICKS_TO_RESONATE - 1);
        assert!(
            !is_glowing(&pillager),
            "one tick short of the full resonance, nothing has been given away"
        );

        bell.tick(&world);
        assert!(
            is_glowing(&pillager),
            "the next tick is the one that reveals"
        );
    }

    /// `attempt_to_ring` only queues the block event; the sweep happens when it
    /// comes back. A bell whose event never ran would swing on the client and
    /// do nothing at all on the server.
    #[test]
    fn a_ring_whose_block_event_never_runs_reveals_nobody() {
        let world = bell_world("bell_event_never_runs");
        let bell = bell_entity(&world);
        let pillager = spawn_pillager(&world, NEAR_RAIDER);

        assert!(BellBlock::attempt_to_ring(&world, BELL, None, None));
        for _ in 0..=TICKS_TO_RESONATE {
            bell.tick(&world);
        }

        assert!(
            !is_glowing(&pillager),
            "without the block event the bell never started swinging"
        );
    }
}
