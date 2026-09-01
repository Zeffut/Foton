use uuid::Uuid;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use super::Event;

/// An entity attack before damage is applied.
pub struct EntityDamageByEntityEvent { damager: Uuid, entity: Uuid, cause: String, cancelled: bool }
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for EntityDamageByEntityEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_damage_by_entity");
}
impl Event for EntityDamageByEntityEvent { fn is_cancelled(&self) -> bool { self.cancelled } }
impl EntityDamageByEntityEvent {
    pub fn new(damager: Uuid, entity: Uuid, cause: String) -> Self { Self { damager, entity, cause, cancelled: false } }
    pub const fn damager(&self) -> Uuid { self.damager }
    pub const fn entity(&self) -> Uuid { self.entity }
    pub fn cause(&self) -> &str { &self.cause }
    pub const fn is_cancelled(&self) -> bool { self.cancelled }
    pub const fn set_cancelled(&mut self, cancelled: bool) { self.cancelled = cancelled; }
}


/// A living entity attempting to pick up an item entity.
pub struct EntityPickupItemEvent { entity: Uuid, item: Uuid, cancelled: bool }
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for EntityPickupItemEvent { const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_pickup_item"); }
impl Event for EntityPickupItemEvent { fn is_cancelled(&self) -> bool { self.cancelled } }
impl EntityPickupItemEvent {
    pub const fn new(entity: Uuid, item: Uuid) -> Self { Self { entity, item, cancelled: false } }
    pub const fn entity(&self) -> Uuid { self.entity }
    pub const fn item(&self) -> Uuid { self.item }
    pub const fn is_cancelled(&self) -> bool { self.cancelled }
    pub const fn set_cancelled(&mut self, cancelled: bool) { self.cancelled = cancelled; }
}


/// An entity detached from a world during unload.
pub struct EntityRemoveFromWorldEvent { entity: Uuid }
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for EntityRemoveFromWorldEvent { const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_remove_from_world"); }
impl Event for EntityRemoveFromWorldEvent {}
impl EntityRemoveFromWorldEvent { pub const fn new(entity: Uuid) -> Self { Self { entity } } pub const fn entity(&self) -> Uuid { self.entity } }
