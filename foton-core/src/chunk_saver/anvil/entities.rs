//! Block entities and entities from vanilla chunk NBT.
//!
//! Foton persists both as typed base fields plus an NBT blob that the type's
//! own loader reads, which is the same shape vanilla's compound has once the
//! base keys are taken out. So the conversion is mostly a relabeling: nothing
//! about a chest or an item display is interpreted here.

use std::io::Cursor;
use std::str::FromStr as _;

use foton_registry::{REGISTRY, RegistryExt as _, blocks::block_state_ext::BlockStateExt as _};
use foton_utils::version::WORLD_VERSION;
use foton_utils::{BlockStateId, ChunkPos, Identifier};
use simdnbt::borrow::read_compound as read_borrowed_compound;
use simdnbt::owned::{NbtCompound, NbtList};
use uuid::Uuid;

use super::convert::{ConvertError, Issues, Shape};
use crate::chunk_saver::nesting::MAX_NESTING_DEPTH;
use crate::chunk_saver::{ChunkStorage, Nested, PersistentBlockEntity, PersistentEntity};
use crate::entity::nbt_load::read_entity_nbt;

/// Keys that locate a block entity; Foton stores them as fields beside the blob.
const BLOCK_ENTITY_METADATA: [&str; 4] = ["id", "x", "y", "z"];

/// Reads the `block_entities` list.
///
/// A block entity is kept only if Foton knows its type and the block under it
/// accepts that type, which is the same test the chunk loader applies. Anything
/// else would load as nothing, so it is counted here instead.
pub(super) fn convert_block_entities(
    list: Option<&[NbtCompound]>,
    chunk: ChunkPos,
    shape: Shape,
    state_at: &dyn Fn(usize, i32, usize) -> Option<BlockStateId>,
    issues: &mut Issues,
) -> Vec<PersistentBlockEntity> {
    let mut converted = Vec::new();
    for tag in list.unwrap_or(&[]) {
        let name = tag
            .string("id")
            .map(|id| id.to_str().into_owned())
            .unwrap_or_default();
        let Some(entity_type) = Identifier::from_str(&name)
            .ok()
            .and_then(|key| REGISTRY.block_entity_types.by_key(&key))
        else {
            Issues::tally(&mut issues.unknown_block_entities, &name);
            continue;
        };
        let (Some(x), Some(y), Some(z)) = (tag.int("x"), tag.int("y"), tag.int("z")) else {
            Issues::tally(&mut issues.unknown_block_entities, &name);
            continue;
        };
        let local_x = u8::try_from(x - chunk.0.x * 16).ok().filter(|x| *x < 16);
        let local_z = u8::try_from(z - chunk.0.y * 16).ok().filter(|z| *z < 16);
        let in_range = y >= shape.min_y && y < shape.min_y + shape.height;
        let (Some(local_x), Some(local_z), true, Ok(stored_y)) =
            (local_x, local_z, in_range, i16::try_from(y))
        else {
            Issues::tally(&mut issues.misplaced_block_entities, &name);
            continue;
        };
        let accepted = state_at(usize::from(local_x), y, usize::from(local_z))
            .is_some_and(|state| entity_type.is_valid(state.get_block()));
        if !accepted {
            Issues::tally(&mut issues.misplaced_block_entities, &name);
            continue;
        }

        let mut data = tag.clone();
        for key in BLOCK_ENTITY_METADATA {
            let _ = data.remove(key);
        }
        let mut nbt_data = Vec::new();
        data.write(&mut nbt_data);
        converted.push(PersistentBlockEntity {
            x: local_x,
            y: stored_y,
            z: local_z,
            entity_type: Some(entity_type.key.clone()),
            nbt_data,
        });
    }
    converted
}

/// Reads one chunk of a vanilla `entities/` region.
///
/// # Errors
/// [`ConvertError::DataVersion`] if the entity chunk was written by another version.
pub(super) fn convert_entities(
    root: &NbtCompound,
    issues: &mut Issues,
) -> Result<Vec<PersistentEntity>, ConvertError> {
    let version = root.int("DataVersion");
    if version != Some(WORLD_VERSION) {
        return Err(ConvertError::DataVersion(version));
    }
    Ok(root
        .list("Entities")
        .and_then(NbtList::compounds)
        .unwrap_or(&[])
        .iter()
        .filter_map(|tag| convert_entity(tag, 0, issues))
        .collect())
}

/// Converts one entity compound and, recursively, its passengers.
///
/// The base keys go through Foton's own `read_entity_nbt`, the reader `/summon`
/// and structure templates use, so an entity imported here reads exactly as one
/// summoned with the same NBT. The remainder is what the type's loader gets.
fn convert_entity(tag: &NbtCompound, depth: u32, issues: &mut Issues) -> Option<PersistentEntity> {
    if depth > MAX_NESTING_DEPTH {
        issues.malformed_entities += 1;
        return None;
    }
    let name = tag
        .string("id")
        .map(|id| id.to_str().into_owned())
        .unwrap_or_default();

    let mut encoded = Vec::new();
    tag.write(&mut encoded);
    let Ok(borrowed) = read_borrowed_compound(&mut Cursor::new(encoded.as_slice())) else {
        issues.malformed_entities += 1;
        return None;
    };
    let view = simdnbt::borrow::NbtCompound::from(&borrowed);
    let Some(loaded) = read_entity_nbt(&view) else {
        Issues::tally(&mut issues.unknown_entities, &name);
        return None;
    };
    let position = tag
        .list("Pos")
        .and_then(NbtList::doubles)
        .filter(|position| position.len() == 3 && position.iter().all(|c| c.is_finite()));
    let Some(position) = position else {
        issues.malformed_entities += 1;
        return None;
    };

    let mut remainder = loaded.remainder;
    let _ = remainder.remove("Passengers");
    let mut nbt_data = Vec::new();
    remainder.write(&mut nbt_data);

    let passengers = loaded
        .passengers
        .iter()
        .filter_map(|passenger| convert_entity(passenger, depth + 1, issues))
        .map(Nested)
        .collect();
    let save_data = loaded.save_data;
    Some(PersistentEntity {
        entity_type: loaded.entity_type.key.clone(),
        uuid: *loaded.uuid.unwrap_or_else(Uuid::new_v4).as_bytes(),
        pos: [position[0], position[1], position[2]],
        motion: [loaded.velocity.x, loaded.velocity.y, loaded.velocity.z],
        rotation: [loaded.rotation.0, loaded.rotation.1],
        fall_distance: loaded.fall_distance,
        remaining_fire_ticks: loaded.fire_freeze.remaining_fire_ticks(),
        ticks_frozen: loaded.fire_freeze.ticks_frozen(),
        is_in_powder_snow: loaded.fire_freeze.is_in_powder_snow(),
        was_in_powder_snow: loaded.fire_freeze.was_in_powder_snow(),
        has_visual_fire: loaded.fire_freeze.has_visual_fire(),
        on_ground: loaded.on_ground,
        no_gravity: save_data.no_gravity,
        invulnerable: save_data.invulnerable,
        air_supply: save_data.air_supply,
        portal_cooldown: save_data.portal_cooldown,
        custom_name_nbt: ChunkStorage::custom_name_to_persistent(save_data.custom_name.as_ref()),
        custom_name_visible: save_data.custom_name_visible,
        silent: save_data.silent,
        glowing: save_data.glowing,
        tags: save_data.tags.into_iter().collect(),
        custom_data_nbt: ChunkStorage::compound_to_persistent(&save_data.custom_data),
        bukkit_values_nbt: ChunkStorage::compound_to_persistent(&save_data.bukkit_values),
        nbt_data,
        passengers,
    })
}
