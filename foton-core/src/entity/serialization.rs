//! One entity as a self-contained blob, for plugins that keep an entity
//! outside the world -- a stabled horse in a database -- and bring it back.
//!
//! The format is Paper's `UnsafeValues.serializeEntity`: vanilla's entity
//! compound (`id` included, passengers left out), written as a named root
//! compound and gzipped the way `NbtIo.writeCompressed` does. Foton performs no
//! data fixing, so the compound carries no `DataVersion`; a blob written by a
//! Paper server of the same Minecraft version still reads back.

use std::io::{Cursor, Read as _, Write as _};
use std::sync::Arc;

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use glam::DVec3;
use simdnbt::borrow::read_compound as read_borrowed_compound;
use simdnbt::owned::{Nbt, NbtCompound, NbtList, read as read_owned};
use uuid::Uuid;

use super::nbt_load::read_entity_nbt;
use super::{ENTITIES, Entity, EntityLoadRequest, SharedEntity};
use crate::world::World;

/// The entity's save data, compressed; `None` for an entity vanilla never
/// saves (a player, a lightning bolt, anything already discarded for good).
#[must_use]
pub fn serialize_entity(entity: &dyn Entity) -> Option<Vec<u8>> {
    let mut compound = entity.nbt_for_passenger_save()?;
    let _ = compound.remove("Passengers");
    let mut raw = Vec::new();
    Nbt::new("".into(), compound).write(&mut raw);
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&raw).ok()?;
    encoder.finish().ok()
}

/// Builds the entity a blob describes, in `world` but not added to it; the
/// caller decides where and whether it spawns.
///
/// Vanilla parity: `EntityType.create(CompoundTag, Level, LOAD)`, which reads
/// no passengers. Without `preserve_uuid` the entity gets a fresh UUID, so a
/// copy can stand beside the original it was taken from.
#[must_use]
pub fn deserialize_entity(
    bytes: &[u8],
    world: &Arc<World>,
    preserve_uuid: bool,
) -> Option<SharedEntity> {
    let mut raw = Vec::new();
    GzDecoder::new(bytes).read_to_end(&mut raw).ok()?;
    let Nbt::Some(root) = read_owned(&mut Cursor::new(raw.as_slice())).ok()? else {
        return None;
    };
    let compound: NbtCompound = root.as_compound();
    let mut unnamed = Vec::new();
    compound.write(&mut unnamed);
    let borrowed = read_borrowed_compound(&mut Cursor::new(unnamed.as_slice())).ok()?;
    let view = (&borrowed).into();
    let loaded = read_entity_nbt(&view)?;
    if !ENTITIES.has_load_factory(loaded.entity_type) {
        return None;
    }
    let position = compound
        .list("Pos")
        .and_then(NbtList::doubles)
        .filter(|values| values.len() >= 3)
        .map_or(DVec3::ZERO, |values| {
            DVec3::new(values[0], values[1], values[2])
        });

    let mut remainder_bytes = Vec::new();
    loaded.remainder.write(&mut remainder_bytes);
    let remainder = read_borrowed_compound(&mut Cursor::new(remainder_bytes.as_slice())).ok()?;
    let uuid = if preserve_uuid {
        loaded.uuid.unwrap_or_else(Uuid::new_v4)
    } else {
        Uuid::new_v4()
    };
    Some(ENTITIES.create_and_load_or_raw(
        EntityLoadRequest {
            entity_type: loaded.entity_type,
            position,
            uuid,
            velocity: loaded.velocity,
            rotation: loaded.rotation,
            fall_distance: loaded.fall_distance,
            fire_freeze: loaded.fire_freeze,
            on_ground: loaded.on_ground,
            save_data: loaded.save_data,
            world: Arc::downgrade(world),
        },
        &remainder,
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use foton_registry::{init_vanilla_registry, vanilla_entities};
    use glam::DVec3;

    use super::{deserialize_entity, serialize_entity};
    use crate::entity::entities::PigEntity;
    use crate::entity::{Entity, Mob as _, init_entities};
    use crate::test_support::test_world;

    #[test]
    fn an_entity_comes_back_with_its_state_and_a_new_uuid() {
        init_vanilla_registry();
        init_entities();
        let world = test_world();
        let pig = PigEntity::new(
            &vanilla_entities::PIG,
            1,
            DVec3::new(1.0, 64.0, 2.0),
            Arc::downgrade(world),
        );
        pig.set_no_ai(true);
        assert!(pig.add_tag("stabled".to_owned()));

        let Some(bytes) = serialize_entity(&pig) else {
            panic!("a pig is saveable");
        };
        let Some(copy) = deserialize_entity(&bytes, world, false) else {
            panic!("the blob should read back");
        };
        assert_eq!(copy.entity_type().key, vanilla_entities::PIG.key);
        assert_ne!(copy.uuid(), pig.uuid());
        assert_eq!(copy.position(), pig.position());
        assert_eq!(copy.tags(), vec!["stabled".to_owned()]);
        assert!(copy.as_mob().is_some_and(|mob| mob.is_no_ai()));

        let Some(same) = deserialize_entity(&bytes, world, true) else {
            panic!("the blob should read back twice");
        };
        assert_eq!(same.uuid(), pig.uuid());
    }
}
