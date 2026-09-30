//! Events about entities arriving in the world and leaving their vehicles.

use foton_utils::BlockPos;
use foton_utils::ChunkPos;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use foton_utils::types::InteractionHand;
use uuid::Uuid;

use super::Event;

/// A player placed an entity with an item: a boat, a minecart, an armor
/// stand, an end crystal.
///
/// Fired once the entity is in the world, so a listener can read everything
/// about it; cancelling takes it back out and leaves the item unspent.
pub struct EntityPlaceEvent {
    entity: Uuid,
    player: Uuid,
    world: String,
    block: BlockPos,
    face: &'static str,
    hand: InteractionHand,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for EntityPlaceEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_place");
}

impl Event for EntityPlaceEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl EntityPlaceEvent {
    /// Creates the event for `entity`, placed by `player` against `block`.
    ///
    /// `face` is the Bukkit `BlockFace` name of the side that was used.
    #[must_use]
    pub const fn new(
        entity: Uuid,
        player: Uuid,
        world: String,
        block: BlockPos,
        face: &'static str,
        hand: InteractionHand,
    ) -> Self {
        Self {
            entity,
            player,
            world,
            block,
            face,
            hand,
            cancelled: false,
        }
    }

    /// What was placed.
    #[must_use]
    pub const fn entity(&self) -> Uuid {
        self.entity
    }

    /// Who placed it.
    #[must_use]
    pub const fn player(&self) -> Uuid {
        self.player
    }

    /// The world it was placed in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }

    /// The block it was placed against.
    #[must_use]
    pub const fn block(&self) -> BlockPos {
        self.block
    }

    /// The side of that block, as a Bukkit `BlockFace` name.
    #[must_use]
    pub const fn face(&self) -> &'static str {
        self.face
    }

    /// The hand that held the item.
    #[must_use]
    pub const fn hand(&self) -> InteractionHand {
        self.hand
    }

    /// Takes the entity back out, or lets it stay.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// An entity is getting off what it rides.
///
/// Cancellable only when the dismount is a choice: an entity leaving because
/// it or its vehicle is being removed is reported, but cannot be kept on.
pub struct EntityDismountEvent {
    entity: Uuid,
    vehicle: Uuid,
    cancellable: bool,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for EntityDismountEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_dismount");
}

impl Event for EntityDismountEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl EntityDismountEvent {
    /// Creates the event for `entity` leaving `vehicle`.
    #[must_use]
    pub const fn new(entity: Uuid, vehicle: Uuid, cancellable: bool) -> Self {
        Self {
            entity,
            vehicle,
            cancellable,
            cancelled: false,
        }
    }

    /// The rider.
    #[must_use]
    pub const fn entity(&self) -> Uuid {
        self.entity
    }

    /// What it was riding.
    #[must_use]
    pub const fn vehicle(&self) -> Uuid {
        self.vehicle
    }

    /// Whether a listener may keep the rider on.
    #[must_use]
    pub const fn cancellable(&self) -> bool {
        self.cancellable
    }

    /// Keeps the rider on, when that is allowed at all.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled && self.cancellable;
    }
}

/// Entities saved in a chunk were loaded back into the world.
pub struct EntitiesLoadEvent {
    world: String,
    chunk: ChunkPos,
    entities: Vec<Uuid>,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for EntitiesLoadEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entities_load");
}

impl Event for EntitiesLoadEvent {}

impl EntitiesLoadEvent {
    /// Creates the event for the entities of one chunk.
    #[must_use]
    pub const fn new(world: String, chunk: ChunkPos, entities: Vec<Uuid>) -> Self {
        Self {
            world,
            chunk,
            entities,
        }
    }

    /// The world the chunk is in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }

    /// The chunk.
    #[must_use]
    pub const fn chunk(&self) -> ChunkPos {
        self.chunk
    }

    /// What came back.
    #[must_use]
    pub fn entities(&self) -> &[Uuid] {
        &self.entities
    }
}

/// The entities of a chunk are about to be unloaded with it.
///
/// Fired while they are still in the world, which is the last moment a
/// listener can read them.
pub struct EntitiesUnloadEvent {
    world: String,
    chunk: ChunkPos,
    entities: Vec<Uuid>,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for EntitiesUnloadEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entities_unload");
}

impl Event for EntitiesUnloadEvent {}

impl EntitiesUnloadEvent {
    /// Creates the event for the entities of one chunk.
    #[must_use]
    pub const fn new(world: String, chunk: ChunkPos, entities: Vec<Uuid>) -> Self {
        Self {
            world,
            chunk,
            entities,
        }
    }

    /// The world the chunk is in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }

    /// The chunk.
    #[must_use]
    pub const fn chunk(&self) -> ChunkPos {
        self.chunk
    }

    /// What is leaving.
    #[must_use]
    pub fn entities(&self) -> &[Uuid] {
        &self.entities
    }
}
