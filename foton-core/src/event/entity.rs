use super::Event;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use uuid::Uuid;

/// An entity attack before damage is applied.
pub struct EntityDamageByEntityEvent {
    damager: Uuid,
    entity: Uuid,
    cause: String,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for EntityDamageByEntityEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_damage_by_entity");
}
impl Event for EntityDamageByEntityEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl EntityDamageByEntityEvent {
    pub fn new(damager: Uuid, entity: Uuid, cause: String) -> Self {
        Self {
            damager,
            entity,
            cause,
            cancelled: false,
        }
    }
    pub const fn damager(&self) -> Uuid {
        self.damager
    }
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    pub fn cause(&self) -> &str {
        &self.cause
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// A living entity attempting to pick up an item entity.
pub struct EntityPickupItemEvent {
    entity: Uuid,
    item: Uuid,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for EntityPickupItemEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_pickup_item");
}
impl Event for EntityPickupItemEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl EntityPickupItemEvent {
    pub const fn new(entity: Uuid, item: Uuid) -> Self {
        Self {
            entity,
            item,
            cancelled: false,
        }
    }
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    pub const fn item(&self) -> Uuid {
        self.item
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// A living entity is about to be inserted into a world by a spawn system.
pub struct CreatureSpawnEvent {
    entity: Uuid,
    world: String,
    x: f64,
    y: f64,
    z: f64,
    reason: String,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for CreatureSpawnEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/creature_spawn");
}
impl Event for CreatureSpawnEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl CreatureSpawnEvent {
    pub fn new(entity: Uuid, world: String, x: f64, y: f64, z: f64, reason: String) -> Self {
        Self {
            entity,
            world,
            x,
            y,
            z,
            reason,
            cancelled: false,
        }
    }
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn position(&self) -> (f64, f64, f64) {
        (self.x, self.y, self.z)
    }
    pub fn reason(&self) -> &str {
        &self.reason
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// A living entity is about to regain health.
pub struct EntityRegainHealthEvent {
    entity: Uuid,
    amount: f32,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for EntityRegainHealthEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_regain_health");
}
impl Event for EntityRegainHealthEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl EntityRegainHealthEvent {
    pub const fn new(entity: Uuid, amount: f32) -> Self {
        Self {
            entity,
            amount,
            cancelled: false,
        }
    }
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    pub const fn amount(&self) -> f32 {
        self.amount
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// An entity detached from a world during unload.
pub struct EntityRemoveFromWorldEvent {
    entity: Uuid,
}
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for EntityRemoveFromWorldEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_remove_from_world");
}
impl Event for EntityRemoveFromWorldEvent {}
impl EntityRemoveFromWorldEvent {
    pub const fn new(entity: Uuid) -> Self {
        Self { entity }
    }
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
}
