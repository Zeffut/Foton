use super::Event;
use foton_utils::BlockPos;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use glam::DVec3;
use uuid::Uuid;

/// Fired when a living entity would die but has a death-protection item.
#[derive(Debug)]
pub struct EntityResurrectEvent {
    entity: Uuid,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for EntityResurrectEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_resurrect");
}
impl Event for EntityResurrectEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl EntityResurrectEvent {
    pub const fn new(entity: Uuid) -> Self {
        Self {
            entity,
            cancelled: false,
        }
    }
    pub const fn entity_id(&self) -> Uuid {
        self.entity
    }
    pub fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}
/// Fired when a living entity reaches its vanilla death processing step.
pub struct EntityDeathEvent {
    entity: Uuid,
}
unsafe impl DowncastType for EntityDeathEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_death");
}
impl Event for EntityDeathEvent {}
impl EntityDeathEvent {
    pub const fn new(entity: Uuid) -> Self {
        Self { entity }
    }
    pub const fn entity_id(&self) -> Uuid {
        self.entity
    }
}

/// Fired before a projectile created by a player is inserted into a world.
pub struct ProjectileLaunchEvent {
    shooter: Uuid,
    projectile: Uuid,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for ProjectileLaunchEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/projectile_launch");
}
impl Event for ProjectileLaunchEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl ProjectileLaunchEvent {
    pub const fn new(shooter: Uuid, projectile: Uuid) -> Self {
        Self {
            shooter,
            projectile,
            cancelled: false,
        }
    }
    pub const fn shooter(&self) -> Uuid {
        self.shooter
    }
    pub const fn projectile(&self) -> Uuid {
        self.projectile
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

/// Fired when a mob changes its selected target.
pub struct EntityTargetEvent {
    entity: Uuid,
    target: Option<Uuid>,
    cancelled: bool,
}
unsafe impl DowncastType for EntityTargetEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_target");
}
impl Event for EntityTargetEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl EntityTargetEvent {
    pub const fn new(entity: Uuid, target: Option<Uuid>) -> Self {
        Self {
            entity,
            target,
            cancelled: false,
        }
    }
    pub const fn entity_id(&self) -> Uuid {
        self.entity
    }
    pub const fn target_id(&self) -> Option<Uuid> {
        self.target
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

/// An entity is about to change a block, such as a mob breaking a block.
pub struct EntityChangeBlockEvent {
    entity: Uuid,
    world: String,
    block: BlockPos,
    to: String,
    cancelled: bool,
}
unsafe impl DowncastType for EntityChangeBlockEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_change_block");
}
impl Event for EntityChangeBlockEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl EntityChangeBlockEvent {
    pub fn new(entity: Uuid, world: String, block: BlockPos, to: String) -> Self {
        Self {
            entity,
            world,
            block,
            to,
            cancelled: false,
        }
    }
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn block(&self) -> BlockPos {
        self.block
    }
    pub fn to(&self) -> &str {
        &self.to
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

/// A lightning strike before its consequences are applied.
pub struct LightningStrikeEvent {
    entity: std::sync::Arc<str>,
    world: String,
    cause: String,
    cancelled: bool,
}
unsafe impl DowncastType for LightningStrikeEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/lightning_strike");
}
impl Event for LightningStrikeEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl LightningStrikeEvent {
    pub fn new(
        entity: impl Into<std::sync::Arc<str>>,
        world: impl Into<String>,
        cause: impl Into<String>,
    ) -> Self {
        Self {
            entity: entity.into(),
            world: world.into(),
            cause: cause.into(),
            cancelled: false,
        }
    }
    pub fn entity(&self) -> &str {
        &self.entity
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub fn cause(&self) -> &str {
        &self.cause
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, v: bool) {
        self.cancelled = v;
    }
}

#[cfg(test)]
mod lightning_strike_tests {
    use super::*;
    #[test]
    fn lightning_strike_event_can_be_cancelled() {
        let mut event = LightningStrikeEvent::new("bolt", "minecraft:overworld", "WEATHER");
        assert_eq!(event.entity(), "bolt");
        assert_eq!(event.world(), "minecraft:overworld");
        assert_eq!(event.cause(), "WEATHER");
        assert!(!event.is_cancelled());
        event.set_cancelled(true);
        assert!(event.is_cancelled());
    }
}

/// An entity explosion before its blocks are destroyed.
pub struct EntityExplodeEvent {
    entity: Option<Uuid>,
    world: String,
    blocks: Vec<BlockPos>,
    yield_factor: f32,
    explosion_result: String,
    cancelled: bool,
}

/// An entity is about to be replaced by another entity.
pub struct EntityTransformEvent {
    entity: Uuid,
    transformed: Uuid,
    cancelled: bool,
    reason: crate::entity::conversion::ConversionReason,
}
unsafe impl DowncastType for EntityTransformEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_transform");
}
impl Event for EntityTransformEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl EntityTransformEvent {
    pub const fn new(
        entity: Uuid,
        transformed: Uuid,
        reason: crate::entity::conversion::ConversionReason,
    ) -> Self {
        Self {
            entity,
            transformed,
            cancelled: false,
            reason,
        }
    }
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    pub const fn transformed(&self) -> Uuid {
        self.transformed
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
    pub const fn reason(&self) -> crate::entity::conversion::ConversionReason {
        self.reason
    }
}

/// A block-originated explosion before its blocks are destroyed.
pub struct BlockExplodeEvent {
    world: String,
    source: BlockPos,
    blocks: Vec<BlockPos>,
    cancelled: bool,
}
unsafe impl DowncastType for BlockExplodeEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/block_explode");
}
impl Event for BlockExplodeEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl BlockExplodeEvent {
    pub fn new(world: String, source: BlockPos, blocks: Vec<BlockPos>) -> Self {
        Self {
            world,
            source,
            blocks,
            cancelled: false,
        }
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn source(&self) -> BlockPos {
        self.source
    }
    pub fn blocks(&self) -> &[BlockPos] {
        &self.blocks
    }
    pub fn blocks_mut(&mut self) -> &mut Vec<BlockPos> {
        &mut self.blocks
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for EntityExplodeEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_explode");
}
impl Event for EntityExplodeEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl EntityExplodeEvent {
    pub fn new(
        entity: Option<Uuid>,
        world: String,
        blocks: Vec<BlockPos>,
        yield_factor: f32,
        explosion_result: impl Into<String>,
    ) -> Self {
        Self {
            entity,
            world,
            blocks,
            yield_factor,
            explosion_result: explosion_result.into(),
            cancelled: false,
        }
    }
    pub const fn entity(&self) -> Option<Uuid> {
        self.entity
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub fn blocks(&self) -> &[BlockPos] {
        &self.blocks
    }
    pub fn blocks_mut(&mut self) -> &mut Vec<BlockPos> {
        &mut self.blocks
    }
    pub const fn yield_factor(&self) -> f32 {
        self.yield_factor
    }
    pub fn explosion_result(&self) -> &str {
        &self.explosion_result
    }
    pub const fn set_yield_factor(&mut self, value: f32) {
        self.yield_factor = value;
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

/// An entity attack before damage is applied.
pub struct EntityDamageByEntityEvent {
    damager: Uuid,
    entity: Uuid,
    cause: String,
    cancelled: bool,
}

/// Fired immediately before an entity receives attack knockback.
pub struct EntityPushedByEntityAttackEvent {
    entity: Uuid,
    pushed_by: Uuid,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete event type.
unsafe impl DowncastType for EntityPushedByEntityAttackEvent {
    const TYPE_KEY: DowncastTypeKey =
        DowncastTypeKey::new("foton:event/entity_pushed_by_entity_attack");
}
impl Event for EntityPushedByEntityAttackEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl EntityPushedByEntityAttackEvent {
    pub const fn new(entity: Uuid, pushed_by: Uuid) -> Self {
        Self {
            entity,
            pushed_by,
            cancelled: false,
        }
    }
    pub const fn entity_id(&self) -> Uuid {
        self.entity
    }
    pub const fn pushed_by(&self) -> Uuid {
        self.pushed_by
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// A hanging entity is about to be removed by a non-entity cause.
pub struct HangingBreakEvent {
    entity: Uuid,
    cause: String,
    remover: Option<Uuid>,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for HangingBreakEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/hanging_break");
}
impl Event for HangingBreakEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl HangingBreakEvent {
    pub fn new(entity: Uuid, cause: impl Into<String>) -> Self {
        Self {
            entity,
            cause: cause.into(),
            remover: None,
            cancelled: false,
        }
    }
    pub fn new_with_remover(entity: Uuid, cause: impl Into<String>, remover: Uuid) -> Self {
        Self {
            entity,
            cause: cause.into(),
            remover: Some(remover),
            cancelled: false,
        }
    }
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    pub fn cause(&self) -> &str {
        &self.cause
    }
    pub const fn remover(&self) -> Option<Uuid> {
        self.remover
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}
/// A hanging entity is about to be placed by a player.
pub struct HangingPlaceEvent {
    entity: Uuid,
    player: Uuid,
    world: String,
    block: foton_utils::BlockPos,
    face: String,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for HangingPlaceEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/hanging_place");
}
impl Event for HangingPlaceEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl HangingPlaceEvent {
    pub fn new(
        entity: Uuid,
        player: Uuid,
        world: impl Into<String>,
        block: foton_utils::BlockPos,
        face: impl Into<String>,
    ) -> Self {
        Self {
            entity,
            player,
            world: world.into(),
            block,
            face: face.into(),
            cancelled: false,
        }
    }
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    pub const fn player(&self) -> Uuid {
        self.player
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn block(&self) -> foton_utils::BlockPos {
        self.block
    }
    pub fn face(&self) -> &str {
        &self.face
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
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

/// A creature spawn is proposed before the entity is inserted into a world.
pub struct PreCreatureSpawnEvent {
    world: String,
    x: f64,
    y: f64,
    z: f64,
    entity_type: String,
    reason: String,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PreCreatureSpawnEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/pre_creature_spawn");
}
impl Event for PreCreatureSpawnEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl PreCreatureSpawnEvent {
    pub fn new(world: String, x: f64, y: f64, z: f64, entity_type: String, reason: String) -> Self {
        Self {
            world,
            x,
            y,
            z,
            entity_type,
            reason,
            cancelled: false,
        }
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn position(&self) -> (f64, f64, f64) {
        (self.x, self.y, self.z)
    }
    pub fn entity_type(&self) -> &str {
        &self.entity_type
    }
    pub fn reason(&self) -> &str {
        &self.reason
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

/// Fired before a non-player entity travels through a portal.
pub struct EntityPortalEvent {
    entity: Uuid,
    from_world: String,
    from_position: DVec3,
    to_world: String,
    to_position: DVec3,
    portal_type: String,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for EntityPortalEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_portal");
}
impl Event for EntityPortalEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl EntityPortalEvent {
    pub fn new(
        entity: Uuid,
        from_world: String,
        from_position: DVec3,
        to_world: String,
        to_position: DVec3,
        portal_type: String,
    ) -> Self {
        Self {
            entity,
            from_world,
            from_position,
            to_world,
            to_position,
            portal_type,
            cancelled: false,
        }
    }
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    pub fn from_world(&self) -> &str {
        &self.from_world
    }
    pub const fn from_position(&self) -> DVec3 {
        self.from_position
    }
    pub fn to_world(&self) -> &str {
        &self.to_world
    }
    pub fn set_destination(&mut self, world: String, position: DVec3) {
        self.to_world = world;
        self.to_position = position;
    }
    pub const fn to_position(&self) -> DVec3 {
        self.to_position
    }
    pub fn portal_type(&self) -> &str {
        &self.portal_type
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
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

/// An item entity is about to be inserted into a world.
pub struct ItemSpawnEvent {
    entity: Uuid,
    world: String,
    x: f64,
    y: f64,
    z: f64,
    item: foton_registry::item_stack::ItemStack,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for ItemSpawnEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/item_spawn");
}
impl Event for ItemSpawnEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl ItemSpawnEvent {
    pub fn new(
        entity: Uuid,
        world: String,
        x: f64,
        y: f64,
        z: f64,
        item: foton_registry::item_stack::ItemStack,
    ) -> Self {
        Self {
            entity,
            world,
            x,
            y,
            z,
            item,
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
    pub fn item(&self) -> &foton_registry::item_stack::ItemStack {
        &self.item
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// Experience awarded when a thrown experience bottle hits.
pub struct ExpBottleEvent {
    entity: Uuid,
    experience: i32,
    cancelled: bool,
}
unsafe impl DowncastType for ExpBottleEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/exp_bottle");
}
impl Event for ExpBottleEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl ExpBottleEvent {
    pub const fn new(entity: Uuid, experience: i32) -> Self {
        Self {
            entity,
            experience,
            cancelled: false,
        }
    }
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    pub const fn experience(&self) -> i32 {
        self.experience
    }
    pub fn set_experience(&mut self, v: i32) {
        self.experience = v.max(0)
    }
    pub const fn set_cancelled(&mut self, v: bool) {
        self.cancelled = v
    }
}

#[cfg(test)]
mod exp_bottle_tests {
    use super::*;
    #[test]
    fn exp_bottle_event_mutates_and_cancels() {
        let id = Uuid::nil();
        let mut event = ExpBottleEvent::new(id, 7);
        assert_eq!(event.experience(), 7);
        event.set_experience(3);
        assert_eq!(event.experience(), 3);
        event.set_cancelled(true);
        assert!(event.is_cancelled());
    }
}

/// Fired before an entity starts riding a vehicle.
pub struct EntityMountEvent {
    entity: Uuid,
    vehicle: Uuid,
    cancelled: bool,
}
unsafe impl DowncastType for EntityMountEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_mount");
}
impl Event for EntityMountEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl EntityMountEvent {
    pub const fn new(entity: Uuid, vehicle: Uuid) -> Self {
        Self {
            entity,
            vehicle,
            cancelled: false,
        }
    }
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    pub const fn vehicle(&self) -> Uuid {
        self.vehicle
    }
    pub const fn set_cancelled(&mut self, v: bool) {
        self.cancelled = v
    }
}

#[cfg(test)]
mod entity_mount_tests {
    use super::*;
    #[test]
    fn entity_mount_event_can_be_cancelled() {
        let mut event = EntityMountEvent::new(Uuid::nil(), Uuid::from_u128(1));
        assert!(!event.is_cancelled());
        event.set_cancelled(true);
        assert!(event.is_cancelled());
    }
}
