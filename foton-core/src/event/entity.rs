use uuid::Uuid;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use super::Event;

/// An entity attack before damage is applied.
pub struct EntityDamageByEntityEvent { damager: Uuid, entity: Uuid, cancelled: bool }
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for EntityDamageByEntityEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_damage_by_entity");
}
impl Event for EntityDamageByEntityEvent { fn is_cancelled(&self) -> bool { self.cancelled } }
impl EntityDamageByEntityEvent {
    pub const fn new(damager: Uuid, entity: Uuid) -> Self { Self { damager, entity, cancelled: false } }
    pub const fn damager(&self) -> Uuid { self.damager }
    pub const fn entity(&self) -> Uuid { self.entity }
    pub const fn is_cancelled(&self) -> bool { self.cancelled }
    pub const fn set_cancelled(&mut self, cancelled: bool) { self.cancelled = cancelled; }
}
