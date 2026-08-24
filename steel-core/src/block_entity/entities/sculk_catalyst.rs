//! Sculk catalyst block entity: the thing that turns a death into sculk.
//!
//! Vanilla parity: `SculkCatalystBlockEntity` and its inner `CatalystListener`.
//!
//! The listener hears `ENTITY_DIE` within eight blocks, takes the experience the mob was
//! about to drop, and hands it to a level [`SculkSpreader`] as charge cursors. The block
//! entity ticks that spreader every tick, which is what makes the sculk creep outward over
//! the following seconds.
//!
//! Not implemented: the `KILL_MOB_NEAR_SCULK_CATALYST` advancement trigger, because Steel
//! has no advancement criteria system.

use std::sync::{Arc, Weak};

use glam::DVec3;
use rand::RngExt as _;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::NbtCompound;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, BoolProperty};
use steel_registry::particle_type::ParticleData;
use steel_registry::{
    REGISTRY, sound_events, vanilla_block_entity_types, vanilla_game_events, vanilla_particle_types,
};
use steel_utils::locks::SyncMutex;
use steel_utils::random::worldgen_random::WorldgenRandom;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use crate::behavior::blocks::SculkSpreader;
use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;
use crate::world::game_event::{
    GameEventContext, GameEventDeliveryMode, GameEventListener, SharedGameEventListener,
};
use steel_registry::game_events::GameEventRef;

/// Vanilla `SculkCatalystBlock.PULSE`.
const PULSE: &BoolProperty = &BlockStateProperties::BLOOM;
/// Vanilla `CatalystListener.PULSE_TICKS`.
const PULSE_TICKS: i32 = 8;
/// Vanilla `CatalystListener.getListenerRadius`.
const LISTENER_RADIUS: i32 = 8;

/// Vanilla `SculkCatalystBlockEntity`.
pub struct SculkCatalystBlockEntity {
    base: BlockEntityBase,
    listener: Arc<CatalystListener>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SculkCatalystBlockEntity`.
unsafe impl DowncastType for SculkCatalystBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/sculk_catalyst");
}

impl SculkCatalystBlockEntity {
    /// Creates sculk catalyst storage with a fresh level spreader.
    #[must_use]
    pub fn new(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(
                &vanilla_block_entity_types::SCULK_CATALYST,
                world,
                pos,
                state,
            ),
            listener: Arc::new(CatalystListener::new(pos, state)),
        }
    }

    /// Returns the charge still walking outward from this catalyst.
    #[must_use]
    pub fn pending_charge(&self) -> i32 {
        self.listener.spreader.lock().total_charge()
    }
}

impl BlockEntity for SculkCatalystBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla `SculkCatalystBlockEntity.serverTick`.
    fn tick(&self, world: &Arc<World>) {
        let pos = self.get_block_pos();
        let mut spreader = self.listener.spreader.lock();
        if spreader.cursors().is_empty() {
            return;
        }

        // Vanilla walks the cursors with `level.getRandom()`, the unseeded runtime source.
        // Steel's spreader is written against `WorldgenRandom` because world generation
        // shares it, so the live path seeds one per tick from the runtime source. The draw
        // sequence inside a tick is identical; only the seed is not reproducible, which is
        // what vanilla's live spreader is too.
        let mut random = WorldgenRandom::from_seed(rand::rng().random());
        spreader.update_cursors(world, &REGISTRY, pos, &mut random, true);
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let view: NbtCompoundView<'_, '_> = nbt.into();
        self.listener.spreader.lock().load(&view);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.listener.spreader.lock().save(nbt);
    }

    fn game_event_listener(&self) -> Option<SharedGameEventListener> {
        Some(Arc::clone(&self.listener) as SharedGameEventListener)
    }
}

/// Vanilla `SculkCatalystBlockEntity.CatalystListener`.
pub struct CatalystListener {
    spreader: SyncMutex<SculkSpreader>,
    pos: BlockPos,
    block_state: BlockStateId,
}

impl CatalystListener {
    const fn new(pos: BlockPos, block_state: BlockStateId) -> Self {
        Self {
            spreader: SyncMutex::new(SculkSpreader::level()),
            pos,
            block_state,
        }
    }

    /// Vanilla `CatalystListener.bloom`.
    fn bloom(&self, world: &Arc<World>) {
        let state = self.block_state.set_value(PULSE, true);
        world.set_block(self.pos, state, UpdateFlags::UPDATE_ALL);
        world.schedule_block_tick_default(self.pos, state.get_block(), PULSE_TICKS);
        world.send_particles(
            ParticleData::simple(&vanilla_particle_types::SCULK_SOUL),
            DVec3::new(
                f64::from(self.pos.x()) + 0.5,
                f64::from(self.pos.y()) + 1.15,
                f64::from(self.pos.z()) + 0.5,
            ),
            2,
            DVec3::new(0.2, 0.0, 0.2),
            0.0,
        );
        world.play_sound_at(
            &sound_events::BLOCK_SCULK_CATALYST_BLOOM,
            SoundSource::Blocks,
            DVec3::new(
                f64::from(self.pos.x()),
                f64::from(self.pos.y()),
                f64::from(self.pos.z()),
            ),
            2.0,
            0.6 + rand::rng().random::<f32>() * 0.4,
            None,
        );
    }
}

impl GameEventListener for CatalystListener {
    /// Vanilla `BlockPositionSource.getPosition`.
    fn listener_pos(&self) -> Option<DVec3> {
        Some(DVec3::new(
            f64::from(self.pos.x()) + 0.5,
            f64::from(self.pos.y()) + 0.5,
            f64::from(self.pos.z()) + 0.5,
        ))
    }

    fn listener_radius(&self) -> i32 {
        LISTENER_RADIUS
    }

    fn delivery_mode(&self) -> GameEventDeliveryMode {
        GameEventDeliveryMode::ByDistance
    }

    /// Vanilla `CatalystListener.handleGameEvent`.
    ///
    /// The mob's experience is taken here, before `dropAllDeathLoot` runs, which is why
    /// `skip_drop_experience` is enough to stop the orbs from spawning.
    fn handle_game_event(
        &self,
        world: &Arc<World>,
        event: GameEventRef,
        context: &GameEventContext<'_>,
        source_pos: DVec3,
    ) -> bool {
        if event.key != vanilla_game_events::ENTITY_DIE.key {
            return false;
        }
        let Some(source) = context.source_entity() else {
            return false;
        };
        let Some(mob) = source.as_living_entity() else {
            return false;
        };
        if mob.was_experience_consumed() {
            return true;
        }

        let killer = mob
            .last_damage_source()
            .and_then(|damage_source| damage_source.causing_entity_id);
        let experience_would_drop = mob.experience_reward(world, killer);
        if mob.should_drop_experience() && experience_would_drop > 0 {
            let charge_pos = BlockPos::containing(source_pos.x, source_pos.y + 0.5, source_pos.z);
            self.spreader
                .lock()
                .add_cursors(charge_pos, experience_would_drop);
        }

        mob.skip_drop_experience();
        self.bloom(world);
        true
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use steel_registry::{init_vanilla_registry, vanilla_blocks};

    use super::*;

    fn catalyst() -> SculkCatalystBlockEntity {
        init_vanilla_registry();
        SculkCatalystBlockEntity::new(
            Weak::new(),
            BlockPos::new(4, 12, -7),
            vanilla_blocks::SCULK_CATALYST.default_state(),
        )
    }

    /// A catalyst that ate a mob's experience and then forgot the cursors on chunk unload
    /// would have destroyed that experience for nothing. The cursors are the only place the
    /// eaten reward lives, so they have to round-trip.
    #[test]
    fn a_catalyst_carries_its_pending_charge_across_a_save_and_load() {
        let saved = catalyst();
        saved
            .listener
            .spreader
            .lock()
            .add_cursors(saved.get_block_pos(), 37);
        assert_eq!(saved.pending_charge(), 37);

        let mut written = NbtCompound::new();
        saved.save_additional(&mut written);
        let mut bytes = Vec::new();
        written.write(&mut bytes);
        let borrowed =
            read_borrowed_compound(&mut Cursor::new(bytes.as_slice())).expect("test NBT reborrows");

        let loaded = catalyst();
        loaded.load_additional(&borrowed);
        assert_eq!(loaded.pending_charge(), 37);
    }

    /// A fresh catalyst has nothing to spread, and must not resurrect cursors from an empty
    /// tag written by a vanilla world that had none either.
    #[test]
    fn a_catalyst_with_no_stored_cursors_holds_no_charge() {
        let nbt = NbtCompound::new();
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let borrowed =
            read_borrowed_compound(&mut Cursor::new(bytes.as_slice())).expect("test NBT reborrows");

        let loaded = catalyst();
        loaded.load_additional(&borrowed);
        assert_eq!(loaded.pending_charge(), 0);
    }

    /// The catalyst is the first block entity in Steel to publish a game-event listener, and
    /// the chunk only registers one for entities that return it.
    #[test]
    fn a_catalyst_publishes_a_listener_that_hears_eight_blocks_by_distance() {
        let catalyst = catalyst();
        let listener = catalyst
            .game_event_listener()
            .expect("a catalyst must publish a listener");

        assert_eq!(listener.listener_radius(), LISTENER_RADIUS);
        assert_eq!(listener.delivery_mode(), GameEventDeliveryMode::ByDistance);
        assert_eq!(
            listener.listener_pos(),
            Some(DVec3::new(4.5, 12.5, -6.5)),
            "vanilla's BlockPositionSource listens from the block center"
        );
    }
}
