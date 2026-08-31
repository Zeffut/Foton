//! End gateway destination calculation.

use std::sync::Arc;

use foton_protocol::packets::game::RelativeMovement;
use foton_registry::vanilla_entities;
use foton_utils::{BlockPos, ChunkPos, Downcast as _, SectionPos};
use glam::DVec3;

use crate::{
    block_entity::entities::EndGatewayBlockEntity,
    entity::Entity,
    portal::{PortalTicketTarget, TeleportPostTransition, TeleportTransition},
    world::World,
};

const GATEWAY_HEIGHT_ABOVE_SURFACE: i32 = 10;
const EXIT_PORTAL_SEARCH_DISTANCE: f64 = 1024.0;
const EXIT_PORTAL_SEARCH_STEP: f64 = 16.0;
const EXIT_PORTAL_SEARCH_LIMIT: i32 = 16;
const EXIT_POSITION_SEARCH_RADIUS: i32 = 5;
const VALID_TELEPORT_SEARCH_RADIUS: i32 = 16;
const GENERATED_ISLAND_Y: i32 = 75;

/// Initial chunk preparation needed before resolving an End gateway transition.
pub(crate) enum EndGatewayChunkPreparation {
    /// Requested chunks are enough to calculate the transition after they load.
    Ready(Vec<ChunkPos>),
    /// Requested chunks only cover vanilla's tentative outer-island search.
    SearchPath(Vec<ChunkPos>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GatewayExitState {
    Stored { exit: BlockPos, exact: bool },
    Missing { exact: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GatewayTeleportAnchor {
    Existing(BlockPos),
    NeedsIsland(BlockPos),
}

impl GatewayTeleportAnchor {
    const fn pos(self) -> BlockPos {
        match self {
            Self::Existing(pos) | Self::NeedsIsland(pos) => pos,
        }
    }
}

/// Returns the first chunks that must be ready before an End gateway transition can be resolved.
#[must_use]
pub(crate) fn initial_chunks(
    world: &World,
    portal_pos: BlockPos,
    source_is_end: bool,
) -> Option<EndGatewayChunkPreparation> {
    let Some(state) = gateway_exit_state(world, portal_pos) else {
        // The block is there and the player walked into it, so the block
        // entity that holds the exit should be there too. If it is not, the
        // gateway is inert and nothing else in the chain can say so.
        log::warn!(
            "end gateway at {portal_pos:?} has no gateway block entity, so it leads nowhere"
        );
        return None;
    };
    match state {
        GatewayExitState::Stored { exit, exact: true } => Some(EndGatewayChunkPreparation::Ready(
            chunks_for_block_square(exit, 0),
        )),
        GatewayExitState::Stored { exit, exact: false } => Some(EndGatewayChunkPreparation::Ready(
            chunks_for_block_square(exit.offset(0, 2, 0), EXIT_POSITION_SEARCH_RADIUS),
        )),
        GatewayExitState::Missing { .. } if source_is_end => Some(
            EndGatewayChunkPreparation::SearchPath(exit_search_candidate_chunks(portal_pos)),
        ),
        // Vanilla only searches for an outer island from inside the End. A
        // gateway anywhere else with no stored exit has nowhere to send anyone.
        GatewayExitState::Missing { .. } => {
            log::warn!(
                "end gateway at {portal_pos:?} stores no exit and is not in the End, \
                 so there is nowhere to search"
            );
            None
        }
    }
}

/// Returns final chunks needed after the tentative outer-island search chunks are ready.
#[must_use]
pub(crate) fn final_chunks_after_search(
    world: &World,
    portal_pos: BlockPos,
    source_is_end: bool,
) -> Option<Vec<ChunkPos>> {
    let Some(state) = gateway_exit_state(world, portal_pos) else {
        log::warn!("end gateway at {portal_pos:?} lost its block entity mid-search");
        return None;
    };
    match state {
        GatewayExitState::Stored { exit, exact: true } => Some(chunks_for_block_square(exit, 0)),
        GatewayExitState::Stored { exit, exact: false } => Some(chunks_for_block_square(
            exit.offset(0, 2, 0),
            EXIT_POSITION_SEARCH_RADIUS,
        )),
        GatewayExitState::Missing { .. } if source_is_end => {
            // The search walks chunk by chunk toward the outer islands and
            // gives up the moment one of them is not loaded, which is the
            // likeliest way a gateway the dragon opened ends up inert.
            let Some(anchor) = find_teleport_anchor(world, portal_pos) else {
                log::warn!(
                    "end gateway at {portal_pos:?} could not read its way out to the \
                     outer islands; a chunk along the search line is missing"
                );
                return None;
            };
            Some(chunks_for_block_square(
                anchor.pos(),
                VALID_TELEPORT_SEARCH_RADIUS,
            ))
        }
        GatewayExitState::Missing { .. } => {
            log::warn!(
                "end gateway at {portal_pos:?} stores no exit and is not in the End, \
                 so the search has nowhere to go"
            );
            None
        }
    }
}

/// Calculates vanilla's End gateway transition after the required chunks are available.
#[must_use]
pub(crate) fn calculate_transition(
    world: &Arc<World>,
    entity: &dyn Entity,
    portal_pos: BlockPos,
    source_is_end: bool,
) -> Option<TeleportTransition> {
    let Some(state) = gateway_exit_state(world, portal_pos) else {
        log::warn!("end gateway at {portal_pos:?} has no block entity to read an exit from");
        return None;
    };
    let (exit, exact) = match state {
        GatewayExitState::Stored { exit, exact } => (exit, exact),
        // The gateways the dragon's death leaves behind take this path every
        // time: they store no exit, so the outer island has to be found first.
        GatewayExitState::Missing { exact } if source_is_end => {
            let Some(found) = find_or_create_valid_teleport_pos(world, portal_pos) else {
                log::warn!(
                    "end gateway at {portal_pos:?} found no outer-island landing spot to aim at"
                );
                return None;
            };
            let exit = found.above_n(GATEWAY_HEIGHT_ABOVE_SURFACE);
            if !world.create_end_gateway_portal(exit, portal_pos, false) {
                log::error!("Unable to create End gateway portal at {}", world.key);
                return None;
            }
            if !set_gateway_exit_position(world, portal_pos, exit, exact) {
                log::warn!("end gateway at {portal_pos:?} could not record its exit at {exit:?}");
                return None;
            }
            (exit, exact)
        }
        GatewayExitState::Missing { .. } => {
            log::warn!(
                "end gateway at {portal_pos:?} stores no exit and is not in the End, \
                 so there is nowhere to search"
            );
            return None;
        }
    };

    let destination = if exact {
        exit
    } else {
        find_exit_position(world, exit)
    };
    Some(gateway_transition(world, entity, destination))
}

fn gateway_exit_state(world: &World, portal_pos: BlockPos) -> Option<GatewayExitState> {
    let block_entity = world.get_block_entity(portal_pos)?;
    let gateway = block_entity.downcast_ref::<EndGatewayBlockEntity>()?;
    Some(match gateway.exit_portal() {
        Some(exit) => GatewayExitState::Stored {
            exit,
            exact: gateway.exact_teleport(),
        },
        None => GatewayExitState::Missing {
            exact: gateway.exact_teleport(),
        },
    })
}

fn set_gateway_exit_position(
    world: &World,
    portal_pos: BlockPos,
    exit: BlockPos,
    exact: bool,
) -> bool {
    let Some(block_entity) = world.get_block_entity(portal_pos) else {
        return false;
    };
    let Some(gateway) = block_entity.downcast_ref::<EndGatewayBlockEntity>() else {
        return false;
    };
    gateway.set_exit_position(exit, exact);
    true
}

fn find_exit_position(world: &World, exit_portal: BlockPos) -> BlockPos {
    world
        .find_end_gateway_tallest_block(
            exit_portal.offset(0, 2, 0),
            EXIT_POSITION_SEARCH_RADIUS,
            false,
        )
        .above()
}

fn find_or_create_valid_teleport_pos(
    world: &Arc<World>,
    gateway_pos: BlockPos,
) -> Option<BlockPos> {
    let anchor = find_teleport_anchor(world, gateway_pos)?;
    if let GatewayTeleportAnchor::NeedsIsland(pos) = anchor
        && !world.create_end_island(pos)
    {
        log::error!("Unable to create End island at {}", world.key);
        return None;
    }

    Some(world.find_end_gateway_tallest_block(anchor.pos(), VALID_TELEPORT_SEARCH_RADIUS, true))
}

fn find_teleport_anchor(world: &World, gateway_pos: BlockPos) -> Option<GatewayTeleportAnchor> {
    let tentative = find_exit_portal_xz_pos_tentative(world, gateway_pos)?;
    let chunk = chunk_for_xz_vec(tentative);
    if let Some(pos) = world.find_end_gateway_valid_spawn_in_chunk(chunk) {
        return Some(GatewayTeleportAnchor::Existing(pos));
    }

    Some(GatewayTeleportAnchor::NeedsIsland(BlockPos::new(
        (tentative.x + 0.5).floor() as i32,
        GENERATED_ISLAND_Y,
        (tentative.z + 0.5).floor() as i32,
    )))
}

fn find_exit_portal_xz_pos_tentative(world: &World, gateway_pos: BlockPos) -> Option<DVec3> {
    let direction = xz_direction(gateway_pos);
    let mut tentative = direction * EXIT_PORTAL_SEARCH_DISTANCE;

    let mut remaining = EXIT_PORTAL_SEARCH_LIMIT;
    while !is_chunk_empty(world, tentative)? && remaining > 0 {
        remaining -= 1;
        tentative -= direction * EXIT_PORTAL_SEARCH_STEP;
    }

    let mut remaining = EXIT_PORTAL_SEARCH_LIMIT;
    while is_chunk_empty(world, tentative)? && remaining > 0 {
        remaining -= 1;
        tentative += direction * EXIT_PORTAL_SEARCH_STEP;
    }

    Some(tentative)
}

fn is_chunk_empty(world: &World, xz_pos: DVec3) -> Option<bool> {
    world.is_end_gateway_chunk_empty(chunk_for_xz_vec(xz_pos))
}

fn gateway_transition(
    world: &Arc<World>,
    entity: &dyn Entity,
    destination: BlockPos,
) -> TeleportTransition {
    let is_ender_pearl = entity.entity_type() == &vanilla_entities::ENDER_PEARL;
    TeleportTransition {
        target_world: world.clone(),
        position: block_bottom_center(destination),
        rotation: (0.0, 0.0),
        velocity: DVec3::ZERO,
        relatives: if is_ender_pearl {
            RelativeMovement::NONE
        } else {
            RelativeMovement::DELTA.union(RelativeMovement::ROTATION)
        },
        portal_cooldown: entity.dimension_changing_delay(),
        as_passenger: false,
        post_transition: TeleportPostTransition::place_portal_ticket(
            PortalTicketTarget::Destination,
        ),
    }
}

fn exit_search_candidate_chunks(gateway_pos: BlockPos) -> Vec<ChunkPos> {
    let direction = xz_direction(gateway_pos);
    let start = direction * EXIT_PORTAL_SEARCH_DISTANCE;
    let mut chunks = Vec::with_capacity((EXIT_PORTAL_SEARCH_LIMIT * 2 + 1) as usize);
    for step in -EXIT_PORTAL_SEARCH_LIMIT..=EXIT_PORTAL_SEARCH_LIMIT {
        chunks.push(chunk_for_xz_vec(
            start + direction * (f64::from(step) * EXIT_PORTAL_SEARCH_STEP),
        ));
    }
    chunks
}

fn chunks_for_block_square(center: BlockPos, block_radius: i32) -> Vec<ChunkPos> {
    let min_chunk_x = SectionPos::block_to_section_coord(center.x() - block_radius);
    let max_chunk_x = SectionPos::block_to_section_coord(center.x() + block_radius);
    let min_chunk_z = SectionPos::block_to_section_coord(center.z() - block_radius);
    let max_chunk_z = SectionPos::block_to_section_coord(center.z() + block_radius);
    let mut chunks = Vec::with_capacity(
        ((max_chunk_x - min_chunk_x + 1) * (max_chunk_z - min_chunk_z + 1)) as usize,
    );

    for chunk_z in min_chunk_z..=max_chunk_z {
        for chunk_x in min_chunk_x..=max_chunk_x {
            chunks.push(ChunkPos::new(chunk_x, chunk_z));
        }
    }
    chunks
}

fn chunk_for_xz_vec(pos: DVec3) -> ChunkPos {
    ChunkPos::new((pos.x / 16.0).floor() as i32, (pos.z / 16.0).floor() as i32)
}

fn xz_direction(pos: BlockPos) -> DVec3 {
    let vector = DVec3::new(f64::from(pos.x()), 0.0, f64::from(pos.z()));
    let length = vector.length();
    if length < 1.0E-4 {
        DVec3::ZERO
    } else {
        vector / length
    }
}

fn block_bottom_center(pos: BlockPos) -> DVec3 {
    let (x, y, z) = pos.get_bottom_center();
    DVec3::new(x, y, z)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{
        EXIT_PORTAL_SEARCH_LIMIT, EndGatewayChunkPreparation, calculate_transition,
        chunks_for_block_square, exit_search_candidate_chunks, final_chunks_after_search,
        initial_chunks, xz_direction,
    };
    use crate::behavior::init_behaviors;
    use crate::block_entity::entities::EndGatewayBlockEntity;
    use crate::entity::{entities::PigEntity, next_entity_id};
    use crate::test_support::{fresh_end_test_world, insert_ready_full_chunk};
    use crate::world::World;
    use foton_registry::{init_vanilla_registry, vanilla_blocks, vanilla_entities};
    use foton_utils::types::UpdateFlags;
    use foton_utils::{BlockPos, ChunkPos, Downcast as _};
    use glam::DVec3;

    /// Puts a gateway with no stored exit where the dragon fight puts them.
    ///
    /// This is the case that matters. `EnderDragonFight.spawnNewGateway` places
    /// `END_GATEWAY_DELAYED`, whose configuration carries no exit at all, so
    /// every gateway the fight opens has to find the outer islands from
    /// nothing. A gateway with a stored exit skips the entire path below.
    fn dragon_gateway(world: &Arc<World>, pos: BlockPos) {
        insert_ready_full_chunk(world, ChunkPos::from_block_pos(pos));
        let state = vanilla_blocks::END_GATEWAY.default_state();
        world.set_block(pos, state, UpdateFlags::UPDATE_ALL);
        world.set_block_entity(Arc::new(EndGatewayBlockEntity::new(
            Arc::downgrade(world),
            pos,
            state,
        )));
    }

    /// Serves exactly the chunks the teleport job would have loaded.
    ///
    /// The job is two chunk requests wrapped around `calculate_transition` and
    /// nothing else, so a world holding those chunks exercises everything the
    /// job would have reached -- without needing a `Server` to schedule them.
    fn load_what_the_job_loads(world: &Arc<World>, gateway: BlockPos) {
        let Some(EndGatewayChunkPreparation::SearchPath(search)) =
            initial_chunks(world, gateway, true)
        else {
            panic!("a gateway with no exit, inside the End, has to ask for a search path");
        };
        // The job's two requests overlap, and the chunk map refuses the same
        // chunk twice, so this keeps track of what it has already served.
        let mut loaded = vec![ChunkPos::from_block_pos(gateway)];
        let serve = |chunks: Vec<ChunkPos>, loaded: &mut Vec<ChunkPos>| {
            for chunk in chunks {
                if !loaded.contains(&chunk) {
                    loaded.push(chunk);
                    insert_ready_full_chunk(world, chunk);
                }
            }
        };
        serve(search, &mut loaded);
        let final_chunks = final_chunks_after_search(world, gateway, true)
            .expect("the search path is loaded, so the anchor has to resolve");
        serve(final_chunks, &mut loaded);
    }

    /// A gateway the dragon opened resolves somewhere to send you.
    ///
    /// Everything before this is covered elsewhere: the block triggers, the
    /// portal processor fires, the world change is queued. All the server adds
    /// is the two chunk loads this test performs by hand -- so this is the
    /// last stretch, and it was broken.
    ///
    /// `create_end_gateway_portal` writes a three-by-three-by-five box, most of
    /// it air, into a spot on the outer island that is already air. `set_block`
    /// answers `false` for an unchanged write, the same as vanilla's
    /// `LevelChunk.setBlockState` returning null -- but vanilla's
    /// `Feature.setBlock` returns `void` and never reads it. Foton read it, and
    /// gave up on the second block of the box. Every gateway the dragon opened
    /// was inert, every time, and nothing was logged.
    #[test]
    fn a_gateway_with_no_stored_exit_still_resolves_a_destination() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_end_test_world("gateway_destination_from_scratch");

        // Ninety-six blocks out on the ring, which is where the fight puts them.
        let gateway = BlockPos::new(96, 75, 0);
        dragon_gateway(&world, gateway);
        load_what_the_job_loads(&world, gateway);

        let traveler = PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            DVec3::new(96.5, 75.0, 0.5),
            Arc::downgrade(&world),
        );

        let transition = calculate_transition(&world, &traveler, gateway, true).expect(
            "a gateway the dragon opened resolved nowhere to send anyone, which is \
             exactly what an inert portal looks like from inside the game",
        );
        assert!(
            transition.position.x > 512.0,
            "the outer islands are a thousand blocks out along the gateway's bearing, \
             so a destination near the arena means the search never left home"
        );

        // Vanilla writes the exit down, so the next traveler takes the stored
        // path instead of searching the whole way again.
        let stored = world.get_block_entity(gateway).and_then(|entity| {
            entity
                .downcast_ref::<EndGatewayBlockEntity>()
                .and_then(EndGatewayBlockEntity::exit_portal)
        });
        assert!(
            stored.is_some(),
            "the gateway found its exit and did not record it, so every later \
             traveler pays for the whole search again"
        );
    }

    #[test]
    fn zero_gateway_position_has_zero_search_direction() {
        assert_eq!(xz_direction(BlockPos::ZERO), DVec3::ZERO);
    }

    #[test]
    fn exit_search_candidates_cover_vanilla_probe_range() {
        let chunks = exit_search_candidate_chunks(BlockPos::new(1, 70, 0));

        assert_eq!(chunks.len(), (EXIT_PORTAL_SEARCH_LIMIT * 2 + 1) as usize);
        assert!(chunks.contains(&ChunkPos::new(48, 0)));
        assert!(chunks.contains(&ChunkPos::new(64, 0)));
        assert!(chunks.contains(&ChunkPos::new(80, 0)));
    }

    #[test]
    fn block_square_chunks_cover_radius_across_chunk_edges() {
        let chunks = chunks_for_block_square(BlockPos::new(16, 70, 16), 5);

        assert_eq!(
            chunks,
            vec![
                ChunkPos::new(0, 0),
                ChunkPos::new(1, 0),
                ChunkPos::new(0, 1),
                ChunkPos::new(1, 1),
            ]
        );
    }
}
