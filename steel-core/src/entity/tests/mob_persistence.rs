//! What a mob has to still be after the server has been restarted.
//!
//! Every one of these types used to have no `save_additional` at all, or one
//! that skipped the shared half, so a chunk save wrote nothing but the base
//! fields: a charged creeper came back ordinary, a big slime came back tiny, a
//! baby zombie grew up, and every leash, `NoAI`, `PersistenceRequired` and
//! `CanPickUpLoot` in the world was thrown away on the next boot.
//!
//! The round trip is what proves it. A test that only checks the compound
//! coming out of `save_additional` passes with a reader that ignores it, and a
//! test that only checks the reader passes with a writer that writes nothing;
//! loading a compound and asking for it back catches both. Every value below is
//! deliberately not the default, so nothing here can pass on a mob that was
//! born that way.

use std::io::Cursor;
use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::read_compound as read_borrowed_compound;
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::{init_vanilla_registry, vanilla_entities};

use crate::entity::{ENTITIES, SharedEntity, init_entities, next_entity_id};

/// Every mob whose persistence this module covers.
///
/// The enderman, the pig and the wolf are not part of the fix; they are here as
/// the harness's own control, because they were already keeping their state and
/// a change that broke the shared layer for everyone should not look like a
/// change that only broke the mobs below.
const MOBS: &[EntityTypeRef] = &[
    &vanilla_entities::BLAZE,
    &vanilla_entities::BOGGED,
    &vanilla_entities::CAVE_SPIDER,
    &vanilla_entities::CREEPER,
    &vanilla_entities::DROWNED,
    &vanilla_entities::ELDER_GUARDIAN,
    &vanilla_entities::GIANT,
    &vanilla_entities::GUARDIAN,
    &vanilla_entities::HUSK,
    &vanilla_entities::MAGMA_CUBE,
    &vanilla_entities::PARCHED,
    &vanilla_entities::SILVERFISH,
    &vanilla_entities::SKELETON,
    &vanilla_entities::SLIME,
    &vanilla_entities::SPIDER,
    &vanilla_entities::STRAY,
    &vanilla_entities::SULFUR_CUBE,
    &vanilla_entities::WITHER_SKELETON,
    &vanilla_entities::ZOMBIE,
    &vanilla_entities::ZOMBIFIED_PIGLIN,
    // Not in the report this started from, and broken the same way: a zombie
    // villager kept the shared half but not `Zombie.IsBaby`, so a baby one grew
    // up on the next boot.
    &vanilla_entities::ZOMBIE_VILLAGER,
    // Controls.
    &vanilla_entities::ENDERMAN,
    &vanilla_entities::PIG,
    &vanilla_entities::WOLF,
];

/// Builds one mob of `entity_type` with nothing loaded into it.
fn fresh(entity_type: EntityTypeRef) -> SharedEntity {
    init_vanilla_registry();
    init_entities();
    ENTITIES
        .create(entity_type, next_entity_id(), DVec3::ZERO, Weak::new())
        .unwrap_or_else(|| panic!("{} has no entity factory", entity_type.key))
}

/// Returns what a mob of this type writes when nothing has been loaded into it.
fn default_save(entity_type: EntityTypeRef) -> NbtCompound {
    let mut nbt = NbtCompound::new();
    fresh(entity_type).save_additional(&mut nbt);
    nbt
}

/// Loads `nbt` into a fresh mob and returns what that mob saves back.
///
/// The compound is written and reborrowed rather than handed over directly, so
/// this goes through the same bytes a chunk file would.
fn round_trip(entity_type: EntityTypeRef, nbt: &NbtCompound) -> NbtCompound {
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
        .unwrap_or_else(|error| panic!("test nbt should reborrow: {error}"));

    let entity = fresh(entity_type);
    entity.load_additional((&borrowed).into());

    let mut saved = NbtCompound::new();
    entity.save_additional(&mut saved);
    saved
}

/// The flags `Mob.addAdditionalSaveData` writes for every mob, every time.
///
/// `NoAI` and `DeathLootTable` are not among them; vanilla writes those two
/// only when they are set, which
/// [`a_mob_keeps_the_two_keys_vanilla_only_writes_when_set`] covers instead.
const UNCONDITIONAL_FLAGS: [&str; 3] = ["CanPickUpLoot", "PersistenceRequired", "LeftHanded"];

/// The flags `Mob.addAdditionalSaveData` owns, which every mob inherits.
///
/// Both values are asked for rather than one plus a "was it already set"
/// control, because a mob that never reads the compound would answer one of
/// them by luck: every flag here is off at birth except an elder guardian's
/// `PersistenceRequired`, which its constructor sets.
#[test]
fn a_mob_keeps_what_the_shared_layer_owns() {
    for entity_type in MOBS {
        let key = &entity_type.key;
        for value in [1i8, 0i8] {
            let mut input = NbtCompound::new();
            for flag in UNCONDITIONAL_FLAGS {
                input.insert(flag, value);
            }
            let saved = round_trip(entity_type, &input);
            for flag in UNCONDITIONAL_FLAGS {
                // Vanilla writes `canPickUpLoot()`, not the stored flag, and a
                // sulfur cube answers that from whether it has already
                // swallowed something. The stored flag is genuinely not its
                // state, so there is nothing here to round-trip.
                if flag == "CanPickUpLoot" && entity_type.key == vanilla_entities::SULFUR_CUBE.key {
                    continue;
                }
                assert_eq!(saved.byte(flag), Some(value), "{key} lost {flag}={value}");
            }
        }
    }
}

/// Vanilla writes `NoAI` and `DeathLootTable` only when they are set, so the
/// round trip for those two is a key that appears and then does not.
#[test]
fn a_mob_keeps_the_two_keys_vanilla_only_writes_when_set() {
    let mut input = NbtCompound::new();
    input.insert("NoAI", 1i8);
    input.insert("DeathLootTable", "minecraft:entities/creeper".to_owned());

    for entity_type in MOBS {
        let key = &entity_type.key;
        let saved = round_trip(entity_type, &input);
        assert_eq!(saved.byte("NoAI"), Some(1), "{key} lost NoAI");
        assert_eq!(
            saved.string("DeathLootTable").map(ToString::to_string),
            Some("minecraft:entities/creeper".to_owned()),
            "{key} lost DeathLootTable"
        );

        let empty = round_trip(entity_type, &NbtCompound::new());
        assert_eq!(
            empty.get("NoAI"),
            None,
            "{key} writes NoAI even when it has none"
        );
        assert_eq!(
            empty.get("DeathLootTable"),
            None,
            "{key} writes DeathLootTable even when it has none"
        );
    }
}

/// A leash is held by whoever the compound names, and the knot case is the one
/// a fence post writes.
#[test]
fn a_mob_keeps_the_post_it_was_tied_to() {
    let mut input = NbtCompound::new();
    input.insert("leash", NbtTag::IntArray(vec![12, 64, -30]));

    for entity_type in MOBS {
        let key = &entity_type.key;
        assert_eq!(
            default_save(entity_type).get("leash"),
            None,
            "a fresh {key} is already tied to something"
        );
        assert_eq!(
            round_trip(entity_type, &input).get("leash"),
            Some(&NbtTag::IntArray(vec![12, 64, -30])),
            "{key} came off its fence post"
        );
    }
}

/// What each mob keeps on top of the shared layer.
///
/// The values are vanilla's own keys and types: a `Fuse` short and an
/// `ExplosionRadius` byte are not interchangeable to a `nbt=` selector, so the
/// round trip has to return the same tag, not just the same number.
fn own_state_cases() -> Vec<(EntityTypeRef, Vec<(&'static str, NbtTag)>)> {
    let baby = vec![("IsBaby", NbtTag::Byte(1))];
    let cube = vec![("Size", NbtTag::Int(3)), ("wasOnGround", NbtTag::Byte(1))];
    vec![
        (
            &vanilla_entities::CREEPER,
            vec![
                ("powered", NbtTag::Byte(1)),
                ("Fuse", NbtTag::Short(45)),
                ("ExplosionRadius", NbtTag::Byte(7)),
                ("ignited", NbtTag::Byte(1)),
            ],
        ),
        (&vanilla_entities::SLIME, cube.clone()),
        (&vanilla_entities::MAGMA_CUBE, cube),
        (
            &vanilla_entities::SULFUR_CUBE,
            vec![
                ("Size", NbtTag::Int(3)),
                ("fuse", NbtTag::Int(17)),
                ("pickup_timer", NbtTag::Int(5)),
                ("from_bucket", NbtTag::Byte(1)),
            ],
        ),
        (&vanilla_entities::ZOMBIE, baby.clone()),
        (&vanilla_entities::HUSK, baby.clone()),
        (&vanilla_entities::DROWNED, baby.clone()),
        (&vanilla_entities::ZOMBIFIED_PIGLIN, baby.clone()),
        (&vanilla_entities::ZOMBIE_VILLAGER, baby),
        (
            &vanilla_entities::BOGGED,
            vec![("sheared", NbtTag::Byte(1))],
        ),
    ]
}

#[test]
fn a_mob_keeps_the_state_that_is_its_own() {
    for (entity_type, expected) in own_state_cases() {
        let key = &entity_type.key;
        let default = default_save(entity_type);
        let mut input = NbtCompound::new();
        for (name, tag) in &expected {
            input.insert(*name, tag.clone());
        }

        let saved = round_trip(entity_type, &input);
        for (name, tag) in &expected {
            // The control, per key: every value here is off the default, so a
            // mob that never read the compound cannot answer with it.
            assert_ne!(
                default.get(name),
                Some(tag),
                "a fresh {key} already has {name} at the value this test loads"
            );
            assert_eq!(saved.get(name), Some(tag), "{key} lost {name}");
        }
    }
}

/// Vanilla stores a cube one size below what it uses, so a compound that never
/// mentioned `Size` has to come back as the smallest cube rather than as
/// nothing at all.
#[test]
fn a_cube_saved_without_a_size_comes_back_tiny() {
    for entity_type in [&vanilla_entities::SLIME, &vanilla_entities::MAGMA_CUBE] {
        assert_eq!(
            round_trip(entity_type, &NbtCompound::new()).get("Size"),
            Some(&NbtTag::Int(0)),
            "{} did not come back at vanilla's smallest size",
            entity_type.key
        );
    }
}

/// Vanilla's `getBooleanOr("IsBaby", false)`: a zombie written before the key
/// existed is an adult, not a baby.
#[test]
fn a_zombie_saved_without_the_baby_flag_comes_back_grown() {
    for entity_type in [
        &vanilla_entities::ZOMBIE,
        &vanilla_entities::HUSK,
        &vanilla_entities::DROWNED,
        &vanilla_entities::ZOMBIFIED_PIGLIN,
        &vanilla_entities::ZOMBIE_VILLAGER,
    ] {
        assert_eq!(
            round_trip(entity_type, &NbtCompound::new()).get("IsBaby"),
            Some(&NbtTag::Byte(0)),
            "{} came back a baby from a compound that never said so",
            entity_type.key
        );
    }
}

/// The creeper's fuse and blast radius have vanilla defaults of their own, and
/// a compound that omits them must not zero them.
#[test]
fn a_creeper_saved_without_a_fuse_comes_back_with_vanillas() {
    let saved = round_trip(&vanilla_entities::CREEPER, &NbtCompound::new());
    assert_eq!(saved.get("Fuse"), Some(&NbtTag::Short(30)));
    assert_eq!(saved.get("ExplosionRadius"), Some(&NbtTag::Byte(3)));
    assert_eq!(saved.get("powered"), Some(&NbtTag::Byte(0)));
    assert_eq!(saved.get("ignited"), Some(&NbtTag::Byte(0)));
}
