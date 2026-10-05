use super::*;
use crate::block_entity::entities::{BEEHIVE_MIN_OCCUPATION_TICKS_NECTAR, BeehiveBlockEntity};
use crate::block_entity::{BlockEntity as _, init_block_entities};
use crate::entity::entities::BeeEntity;
use crate::entity::{EntitySpawnReason, PluginSpawnReason, init_entities, next_entity_id};
use crate::event::CreatureSpawnEvent;
use crate::world::game_event::{GameEventContext, GameEventListener};
use foton_registry::game_events::GameEventRef;
use foton_registry::packets::play::C_SOUND;
use foton_registry::{sound_events, vanilla_game_events};
use foton_utils::{SectionPos, WorldAabb};
use std::sync::atomic::AtomicUsize;

struct ExitEvents(Arc<AtomicUsize>);

impl GameEventListener for ExitEvents {
    fn listener_pos(&self) -> Option<DVec3> {
        Some(DVec3::new(8.5, 64.5, 8.5))
    }
    fn listener_radius(&self) -> i32 {
        16
    }
    fn handle_game_event(
        &self,
        _world: &Arc<World>,
        event: GameEventRef,
        _context: &GameEventContext<'_>,
        _source_pos: DVec3,
    ) -> bool {
        if event == &vanilla_game_events::BLOCK_CHANGE {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
        true
    }
}

#[test]
fn beehive_success_fires_before_insertion_and_inserts_once() {
    release(false);
}

#[test]
fn beehive_cancellation_retains_occupant_and_suppresses_release_effects() {
    release(true);
}

#[expect(
    clippy::too_many_lines,
    reason = "Keeps release ordering and cancellation side effects in one scenario"
)]
fn release(cancel_first: bool) {
    let world = fresh_test_world("beehive_spawn_event");
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");
    runtime.block_on(async {
        let storage_root = test_storage_root(if cancel_first {
            "beehive-spawn-cancelled"
        } else {
            "beehive-spawn-success"
        });
        fs::create_dir_all(&storage_root)
            .await
            .expect("test storage directory");
        let server = test_server(
            Arc::clone(&world),
            PermissionSubjectIndex::new(),
            &storage_root,
        )
        .await
        .expect("test server");
        server.attach_worlds();
        init_block_entities();
        init_entities();
        let pos = BlockPos::new(8, 64, 8);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
        let state = vanilla_blocks::BEEHIVE.default_state();
        assert!(world.set_block(pos, state, UpdateFlags::UPDATE_ALL));
        let block_entity = world.get_block_entity(pos).expect("placed hive");
        let hive = block_entity
            .downcast_ref::<BeehiveBlockEntity>()
            .expect("hive block entity");
        let (player, packets) =
            test_player_with_packets(&server, Arc::clone(&world), "BeeObserver", next_entity_id());
        player.base().set_position_local(DVec3::new(8.5, 64.5, 8.5));
        assert!(world.add_player(Arc::clone(&player), ResetReason::InitialJoin));
        world.player_area_map.on_player_view_change(
            player.id(),
            &[ChunkPos::from_block_pos(pos)],
            &[],
        );
        let exit_sounds = || {
            packets
                .lock()
                .iter()
                .filter(|packet| {
                    let mut cursor = Cursor::new(packet.encoded_data.as_slice());
                    let _ = VarInt::read(&mut cursor).expect("encoded packet VarInt");
                    VarInt::read(&mut cursor).expect("encoded packet VarInt").0 == C_SOUND
                        && VarInt::read(&mut cursor).expect("encoded packet VarInt").0
                            == sound_events::BLOCK_BEEHIVE_EXIT.packet_holder_id()
                })
                .count()
        };
        let bee = BeeEntity::new(
            &vanilla_entities::BEE,
            next_entity_id(),
            DVec3::new(8.5, 64.5, 8.5),
            Arc::downgrade(&world),
        );
        bee.set_has_nectar(true);
        hive.add_occupant(&bee);

        let game_events = Arc::new(AtomicUsize::new(0));
        world.register_game_event_listener(
            SectionPos::from_block_pos(pos),
            Arc::new(ExitEvents(Arc::clone(&game_events))),
        );
        let cancelled = Arc::new(AtomicBool::new(cancel_first));
        let attempts = Arc::new(SyncMutex::new(Vec::new()));
        let attempts_in_event = Arc::clone(&attempts);
        let cancel_in_event = Arc::clone(&cancelled);
        let event_world = Arc::downgrade(&world);
        server.events.on::<CreatureSpawnEvent, _>(
            Identifier::new_static("test", "beehive"),
            move |event| {
                let world = event_world.upgrade().expect("world outlives event");
                let pending = world
                    .get_entity_by_uuid(&event.entity())
                    .expect("pending bee is queryable");
                assert!(
                    world.get_entity_by_id(pending.id()).is_none(),
                    "event precedes insertion"
                );
                assert_eq!(pending.base().spawn_reason(), Some(EntitySpawnReason::Load));
                assert_eq!(event.reason(), PluginSpawnReason::Beehive);
                assert_eq!(
                    pending.base().plugin_spawn_reason(),
                    Some(PluginSpawnReason::Beehive)
                );
                assert_eq!(
                    world.get_block_state(pos),
                    state,
                    "event precedes honey delivery"
                );
                attempts_in_event.lock().push(event.entity());
                event.set_cancelled(cancel_in_event.load(Ordering::Relaxed));
            },
        );
        for _ in 0..=BEEHIVE_MIN_OCCUPATION_TICKS_NECTAR + 1 {
            hive.tick(&world);
        }
        assert_eq!(
            attempts.lock().len(),
            1,
            "release fires exactly one spawn event"
        );
        let bounds = WorldAabb::new(4.0, 60.0, 4.0, 13.0, 69.0, 13.0);
        if cancel_first {
            assert_eq!(hive.occupant_count(), 1);
            assert_eq!(
                world.get_block_state(pos),
                state,
                "cancellation preserves honey"
            );
            assert!(
                world
                    .get_entities_in_aabb_matching(&bounds, |entity| entity.entity_type()
                        == &vanilla_entities::BEE)
                    .is_empty()
            );
            assert_eq!(exit_sounds(), 0);
            assert!(
                world.get_entity_by_uuid(&attempts.lock()[0]).is_none(),
                "pending scope closes"
            );
            assert_eq!(game_events.load(Ordering::Relaxed), 0);
            cancelled.store(false, Ordering::Relaxed);
            hive.tick(&world);
            assert_eq!(attempts.lock().len(), 2, "retained occupant can retry");
        }
        assert_eq!(hive.occupant_count(), 0);
        let released = world.get_entities_in_aabb_matching(&bounds, |entity| {
            entity.entity_type() == &vanilla_entities::BEE
        });
        assert_eq!(released.len(), 1);
        assert_eq!(
            released[0].uuid(),
            *attempts.lock().last().expect("observed spawn")
        );
        assert_eq!(
            released[0].base().spawn_reason(),
            Some(EntitySpawnReason::Load)
        );
        assert_eq!(
            released[0].base().plugin_spawn_reason(),
            Some(PluginSpawnReason::Beehive)
        );
        assert!(
            !released[0]
                .downcast_ref::<BeeEntity>()
                .expect("released bee")
                .has_nectar()
        );
        assert!(
            world
                .get_block_state(pos)
                .get_value(&BlockStateProperties::LEVEL_HONEY)
                > 0
        );
        assert_eq!(game_events.load(Ordering::Relaxed), 1);
        assert_eq!(exit_sounds(), 1);
        drop(server);
        fs::remove_dir_all(&storage_root)
            .await
            .expect("test storage cleanup");
    });
}
