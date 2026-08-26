use super::*;

#[test]
fn add_live_entity_rejects_manager_owned_unloaded_chunk() {
    let manager = WorldEntityManager::new();
    let entity = entity(1, 1, DVec3::new(1.0, 64.0, 1.0));

    assert!(matches!(
        manager.add_live_entity(entity.clone(), EntityOwnership::ManagerOwned),
        Err(AddEntityError::ChunkNotLoaded {
            entity_id: 1,
            chunk,
        }) if chunk == ChunkPos::new(0, 0)
    ));
    assert_eq!(manager.count(), 0);
    assert!(manager.get_by_id(entity.id()).is_none());
}

#[test]
fn add_live_entity_accepts_external_unloaded_chunk() {
    let manager = WorldEntityManager::new();
    let entity = entity(1, 1, DVec3::new(1.0, 64.0, 1.0));

    assert!(
        manager
            .add_live_entity(entity.clone(), EntityOwnership::External)
            .is_ok()
    );
    assert_eq!(manager.count(), 1);

    let Some(live_entity) = manager.get_by_id(entity.id()) else {
        panic!("entity in unloaded chunk should be live");
    };
    assert!(Arc::ptr_eq(&entity, &live_entity));
}

#[test]
fn add_live_entity_rejects_duplicate_uuid_without_registering_second_entity() {
    let manager = WorldEntityManager::new();
    load_chunk(&manager, ChunkPos::new(0, 0));

    let uuid = Uuid::from_u128(5);
    let first = ManagerTestEntity::shared(1, uuid, DVec3::new(1.0, 64.0, 1.0));
    let second = ManagerTestEntity::shared(2, uuid, DVec3::new(2.0, 64.0, 1.0));

    assert!(
        manager
            .add_live_entity(first.clone(), EntityOwnership::ManagerOwned)
            .is_ok()
    );
    assert!(matches!(
        manager.add_live_entity(second, EntityOwnership::ManagerOwned),
        Err(AddEntityError::DuplicateUuid {
            entity_id: 2,
            uuid: duplicate,
        }) if duplicate == uuid
    ));

    let Some(live_first) = manager.get_by_id(1) else {
        panic!("first entity should stay registered");
    };
    assert!(Arc::ptr_eq(&first, &live_first));
    assert!(manager.get_by_id(2).is_none());
    assert_eq!(manager.count(), 1);
}

#[test]
fn add_live_entity_tree_rejects_duplicate_uuid_without_partial_registration() {
    let manager = WorldEntityManager::new();
    load_chunk(&manager, ChunkPos::new(0, 0));

    let existing_uuid = Uuid::from_u128(5);
    let existing = ManagerTestEntity::shared(1, existing_uuid, DVec3::new(1.0, 64.0, 1.0));
    let result = manager.add_live_entity(Arc::clone(&existing), EntityOwnership::ManagerOwned);
    assert!(
        result.is_ok(),
        "existing entity should register before duplicate UUID test: {result:?}"
    );

    let vehicle = entity(2, 6, DVec3::new(2.0, 64.0, 2.0));
    let passenger = ManagerTestEntity::shared(3, existing_uuid, DVec3::new(2.0, 64.0, 2.0));
    EntityBase::restore_passenger_relationship(&vehicle, &passenger);

    assert!(matches!(
        manager.add_live_entity_tree(
            &[Arc::clone(&vehicle), Arc::clone(&passenger)],
            EntityOwnership::ManagerOwned,
        ),
        Err(AddEntityError::DuplicateUuid {
            entity_id: 3,
            uuid,
        }) if uuid == existing_uuid
    ));
    assert!(manager.get_by_id(2).is_none());
    assert!(manager.get_by_id(3).is_none());
    assert_eq!(manager.count(), 1);
}

#[test]
#[should_panic(expected = "entity id 1 is already registered in the world entity manager")]
fn duplicate_entity_id_is_a_loud_invariant_failure() {
    let manager = WorldEntityManager::new();
    load_chunk(&manager, ChunkPos::new(0, 0));

    assert!(
        manager
            .add_live_entity(
                entity(1, 1, DVec3::new(1.0, 64.0, 1.0)),
                EntityOwnership::ManagerOwned,
            )
            .is_ok()
    );
    let _ = manager.add_live_entity(
        entity(1, 2, DVec3::new(2.0, 64.0, 1.0)),
        EntityOwnership::ManagerOwned,
    );
}

/// A dragon's hitboxes are not live entities, so nothing in the ordinary
/// lookup path knows about them. The manager keeps them in a second map,
/// mirroring vanilla's `ServerLevel.dragonParts`, and both halves of that --
/// filling it as the dragon arrives and emptying it as the dragon goes -- have
/// to hold, or the world ends up with hittable boxes floating where a dead
/// dragon used to be.
#[test]
fn a_dragons_hitboxes_are_findable_by_id_only_while_the_dragon_is_live() {
    use crate::entity::entities::EnderDragon;
    use crate::entity::next_entity_id;
    use steel_registry::init_vanilla_registry;

    init_vanilla_registry();
    let manager = WorldEntityManager::new();
    load_chunk(&manager, ChunkPos::new(0, 0));

    let dragon = Arc::new(EnderDragon::new(
        &vanilla_entities::ENDER_DRAGON,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Weak::new(),
    ));
    let head_id = dragon.head().id();
    let dragon_id = dragon.id();

    let entity: SharedEntity = dragon;
    manager
        .add_live_entity(entity, EntityOwnership::ManagerOwned)
        .expect("dragon should go live");

    assert!(
        manager.get_by_id(head_id).is_none(),
        "a hitbox should never be a live entity"
    );
    assert!(
        manager.get_entity_or_part(head_id).is_some(),
        "the part lookup should find a live dragon's head"
    );
    assert_eq!(manager.dragon_parts().len(), 8);

    manager.remove_live_entity(dragon_id, RemovalReason::Discarded);

    assert!(
        manager.get_entity_or_part(head_id).is_none(),
        "the head hitbox outlived its dragon"
    );
    assert!(manager.dragon_parts().is_empty());
}
