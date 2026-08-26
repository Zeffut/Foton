//! One vibration, from the moment it is scheduled to the moment it arrives.

use std::sync::Arc;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as NbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_registry::game_events::GameEventRef;
use steel_registry::{REGISTRY, RegistryExt as _};
use steel_utils::{Identifier, UuidExt as _};
use uuid::Uuid;

use crate::entity::{Entity, SharedEntity};
use crate::world::World;

/// Vanilla `VibrationInfo`.
///
/// Deviation: vanilla's record keeps a nullable hard reference to the source entity beside
/// its UUID so a vibration still names its source after the entity left the level. Steel
/// keeps the same pair -- a [`SharedEntity`] and the UUID -- because the entity handle is
/// reference counted and the UUID is what survives the save.
#[derive(Clone)]
pub struct VibrationInfo {
    game_event: GameEventRef,
    distance: f32,
    pos: DVec3,
    uuid: Option<Uuid>,
    projectile_owner_uuid: Option<Uuid>,
    entity: Option<SharedEntity>,
}

impl VibrationInfo {
    /// Vanilla `VibrationInfo(Holder<GameEvent>, float, Vec3, Entity)`.
    ///
    /// The world is only used to promote the borrowed source entity to a handle the
    /// vibration can carry until it arrives, which is what vanilla's direct field does.
    #[must_use]
    pub fn new(
        game_event: GameEventRef,
        distance: f32,
        pos: DVec3,
        source_entity: Option<&dyn Entity>,
        world: &Arc<World>,
    ) -> Self {
        Self {
            game_event,
            distance,
            pos,
            uuid: source_entity.map(Entity::uuid),
            projectile_owner_uuid: source_entity
                .and_then(|entity| entity.as_projectile()?.get_owner())
                .map(|owner| owner.uuid()),
            entity: source_entity.and_then(|entity| world.get_entity_by_id(entity.id())),
        }
    }

    /// Returns the game event that produced this vibration.
    #[must_use]
    pub const fn game_event(&self) -> GameEventRef {
        self.game_event
    }

    /// Returns how far the vibration has to travel, in blocks.
    #[must_use]
    pub const fn distance(&self) -> f32 {
        self.distance
    }

    /// Returns where the vibration started.
    #[must_use]
    pub const fn pos(&self) -> DVec3 {
        self.pos
    }

    /// Vanilla `VibrationInfo.getEntity`.
    #[must_use]
    pub fn get_entity(&self, world: &Arc<World>) -> Option<SharedEntity> {
        self.entity
            .clone()
            .or_else(|| world.get_entity_by_uuid(&self.uuid?))
    }

    /// Vanilla `VibrationInfo.getProjectileOwner`.
    #[must_use]
    pub fn get_projectile_owner(&self, world: &Arc<World>) -> Option<SharedEntity> {
        self.get_entity(world)
            .and_then(|entity| entity.as_projectile()?.get_owner())
            .or_else(|| world.get_entity_by_uuid(&self.projectile_owner_uuid?))
    }

    /// Writes vanilla's `VibrationInfo.CODEC` shape.
    pub fn save(&self, nbt: &mut NbtCompound) {
        nbt.insert("game_event", self.game_event.key.to_string());
        nbt.insert("distance", self.distance);
        nbt.insert(
            "pos",
            NbtTag::List(NbtList::Double(vec![self.pos.x, self.pos.y, self.pos.z])),
        );
        if let Some(uuid) = self.uuid {
            nbt.insert("source", NbtTag::IntArray(uuid.to_int_array().to_vec()));
        }
        if let Some(uuid) = self.projectile_owner_uuid {
            nbt.insert(
                "projectile_owner",
                NbtTag::IntArray(uuid.to_int_array().to_vec()),
            );
        }
    }

    /// Reads vanilla's `VibrationInfo.CODEC` shape.
    ///
    /// The entity handle is deliberately absent: a reloaded vibration only has the UUID,
    /// exactly as in vanilla, and resolves its entity through the level.
    #[must_use]
    pub fn load(nbt: &NbtCompoundView<'_, '_>) -> Option<Self> {
        let game_event = nbt
            .string("game_event")
            .and_then(|key| key.to_str().parse::<Identifier>().ok())
            .and_then(|key| REGISTRY.game_events.by_key(&key))?;
        let distance = nbt.float("distance")?;
        if distance < 0.0 {
            return None;
        }
        let pos = nbt.list("pos").and_then(|list| list.doubles())?;
        let [x, y, z] = pos[..] else { return None };

        Some(Self {
            game_event,
            distance,
            pos: DVec3::new(x, y, z),
            uuid: nbt
                .int_array("source")
                .and_then(|values| Uuid::from_int_array(&values)),
            projectile_owner_uuid: nbt
                .int_array("projectile_owner")
                .and_then(|values| Uuid::from_int_array(&values)),
            entity: None,
        })
    }
}
