use super::*;
use crate::entity::entities::CowEntity;
use crate::entity::{AgeableMob as _, Animal as _, PluginSpawnReason, SharedEntity};
use crate::event::CreatureSpawnEvent;
use foton_utils::WorldAabb;

/// A calf a listener vetoes never reaches the world, and the courtship still
/// ends: left in love, the parents would ask the listener again every tick.
#[test]
fn a_vetoed_calf_is_not_born_and_the_parents_stop_courting() {
    let world = fresh_test_world("creature_spawn_gates");
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");
    runtime.block_on(async {
        let storage_root = test_storage_root("creature-spawn-gates");
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
        init_entities();
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        let mother = cow(&world, 8.5);
        let father = cow(&world, 9.5);
        let allow = Arc::new(AtomicBool::new(false));
        let reasons = Arc::new(SyncMutex::new(Vec::new()));
        let allow_in_event = Arc::clone(&allow);
        let reasons_in_event = Arc::clone(&reasons);
        let event_world = Arc::downgrade(&world);
        server.events.on::<CreatureSpawnEvent, _>(
            Identifier::new_static("test", "calf"),
            move |event| {
                let world = event_world.upgrade().expect("world outlives event");
                let pending = world
                    .get_entity_by_uuid(&event.entity())
                    .expect("the calf is queryable while listeners run");
                assert!(
                    world.get_entity_by_id(pending.id()).is_none(),
                    "the event precedes insertion"
                );
                reasons_in_event.lock().push(event.reason());
                event.set_cancelled(!allow_in_event.load(Ordering::Relaxed));
            },
        );

        let bounds = WorldAabb::new(0.0, 60.0, 0.0, 16.0, 70.0, 16.0);
        let cows = || {
            world
                .get_entities_in_aabb_matching(&bounds, |entity| {
                    entity.entity_type() == &vanilla_entities::COW
                })
                .len()
        };
        mother.set_in_love(None);
        father.set_in_love(None);
        mother.spawn_child_from_breeding(&world, father.as_ref());
        assert_eq!(cows(), 2, "a vetoed calf is not added");
        assert!(!mother.is_in_love() && !father.is_in_love());
        assert!(
            mother.get_age() > 0 && father.get_age() > 0,
            "the parents still wait out a breeding cooldown"
        );

        allow.store(true, Ordering::Relaxed);
        mother.set_age(0);
        father.set_age(0);
        mother.set_in_love(None);
        father.set_in_love(None);
        mother.spawn_child_from_breeding(&world, father.as_ref());
        assert_eq!(cows(), 3, "an allowed calf joins the world");
        assert_eq!(
            *reasons.lock(),
            [PluginSpawnReason::Breeding, PluginSpawnReason::Breeding]
        );

        drop(server);
        fs::remove_dir_all(&storage_root)
            .await
            .expect("test storage cleanup");
    });
}

fn cow(world: &Arc<World>, x: f64) -> Arc<CowEntity> {
    let cow = Arc::new(CowEntity::new(
        &vanilla_entities::COW,
        next_entity_id(),
        DVec3::new(x, 64.0, 8.5),
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&cow) as SharedEntity)
        .expect("the test chunk is loaded");
    cow
}
