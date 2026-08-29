//! What a vibration listener is attached to.

use std::sync::{Arc, Weak};

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::game_events::GameEventRef;
use foton_registry::position_source::{
    BlockPositionSource, EntityPositionSource, PositionSource as ParticlePositionSource,
};
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::vanilla_game_event_tags::GameEventTag;
use foton_registry::{REGISTRY, TaggedRegistryExt as _, vanilla_position_source_types};
use foton_utils::{BlockPos, Identifier};
use glam::DVec3;

use crate::entity::Entity;
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// Where a vibration listener sits, and where its particles fly to.
///
/// Vanilla parity: `PositionSource`, the level-side interface with `getPosition`. Foton's
/// [`foton_registry::position_source::PositionSource`] is only that interface's network
/// payload, so the two halves are separate types here.
#[derive(Clone)]
pub enum VibrationPositionSource {
    /// Vanilla `BlockPositionSource`.
    Block(BlockPos),
    /// Vanilla `EntityPositionSource`.
    Entity {
        /// The world the entity is tracked in.
        world: Weak<World>,
        /// The entity's network id.
        entity_id: i32,
        /// How far above the entity's feet the listener sits.
        y_offset: f32,
    },
}

impl VibrationPositionSource {
    /// Vanilla `PositionSource.getPosition`.
    #[must_use]
    pub fn resolve(&self) -> Option<DVec3> {
        match self {
            Self::Block(pos) => Some(DVec3::new(
                f64::from(pos.x()) + 0.5,
                f64::from(pos.y()) + 0.5,
                f64::from(pos.z()) + 0.5,
            )),
            Self::Entity {
                world,
                entity_id,
                y_offset,
            } => {
                let entity = world.upgrade()?.get_entity_by_id(*entity_id)?;
                Some(entity.position() + DVec3::new(0.0, f64::from(*y_offset), 0.0))
            }
        }
    }

    /// Returns the payload the vibration particle carries to the client.
    #[must_use]
    pub fn to_particle_source(&self) -> ParticlePositionSource {
        match self {
            Self::Block(pos) => ParticlePositionSource::new(
                &vanilla_position_source_types::BLOCK,
                BlockPositionSource::new(*pos),
            ),
            Self::Entity {
                entity_id,
                y_offset,
                ..
            } => ParticlePositionSource::new(
                &vanilla_position_source_types::ENTITY,
                EntityPositionSource::new(*entity_id, *y_offset),
            ),
        }
    }
}

/// Vanilla `VibrationSystem.User`.
///
/// One implementation per thing that hears vibrations: the two sculk sensors, the shrieker,
/// the warden and the allay.
pub trait VibrationUser: Send + Sync {
    /// Vanilla `VibrationSystem.User.getListenerRadius`.
    fn listener_radius(&self) -> i32;

    /// Vanilla `VibrationSystem.User.getPositionSource`.
    fn position_source(&self) -> VibrationPositionSource;

    /// Vanilla `VibrationSystem.User.canReceiveVibration`.
    fn can_receive_vibration(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        event: GameEventRef,
        context: &GameEventContext<'_>,
    ) -> bool;

    /// Vanilla `VibrationSystem.User.onReceiveVibration`.
    fn on_receive_vibration(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        event: GameEventRef,
        source_entity: Option<&dyn Entity>,
        projectile_owner: Option<&dyn Entity>,
        receiving_distance: f32,
    );

    /// Vanilla `VibrationSystem.User.getListenableEvents`.
    fn listenable_events(&self) -> Identifier {
        GameEventTag::VIBRATIONS
    }

    /// Vanilla `VibrationSystem.User.canTriggerAvoidVibration`.
    fn can_trigger_avoid_vibration(&self) -> bool {
        false
    }

    /// Vanilla `VibrationSystem.User.requiresAdjacentChunksToBeTicking`.
    fn requires_adjacent_chunks_to_be_ticking(&self) -> bool {
        false
    }

    /// Vanilla `VibrationSystem.User.calculateTravelTimeInTicks`.
    ///
    /// This is the delay that makes a sensor feel like it is hearing something rather than
    /// touching it: one tick per block between the source and the listener.
    fn calculate_travel_time_in_ticks(&self, distance_to_destination: f32) -> i32 {
        distance_to_destination.floor() as i32
    }

    /// Vanilla `VibrationSystem.User.isValidVibration`.
    ///
    /// Not implemented: the `AVOID_VIBRATION` advancement trigger a sneaking player earns
    /// here, because Foton has no advancement criteria system.
    fn is_valid_vibration(&self, event: GameEventRef, context: &GameEventContext<'_>) -> bool {
        if !REGISTRY
            .game_events
            .is_in_tag(event, &self.listenable_events())
        {
            return false;
        }

        if let Some(source_entity) = context.source_entity() {
            if source_entity.is_spectator() {
                return false;
            }
            if source_entity.is_stepping_carefully()
                && REGISTRY
                    .game_events
                    .is_in_tag(event, &GameEventTag::IGNORE_VIBRATIONS_SNEAKING)
            {
                return false;
            }
            if source_entity.dampens_vibrations() {
                return false;
            }
        }

        context.affected_state().is_none_or(|state| {
            !REGISTRY
                .blocks
                .is_in_tag(state.get_block(), &BlockTag::DAMPENS_VIBRATIONS)
        })
    }

    /// Vanilla `VibrationSystem.User.onDataChanged`.
    fn on_data_changed(&self) {}
}
