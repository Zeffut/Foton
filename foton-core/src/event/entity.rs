use std::sync::Arc;

use foton_registry::item_stack::ItemStack;
use foton_utils::BlockPos;
use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use glam::DVec3;
use uuid::Uuid;

use super::Event;
use crate::entity::conversion::ConversionReason;

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
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(entity: Uuid) -> Self {
        Self {
            entity,
            cancelled: false,
        }
    }
    /// Which entity this is about.
    #[must_use]
    pub const fn entity_id(&self) -> Uuid {
        self.entity
    }
    /// Stops this from happening, or lets it happen again.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}
/// Fired when a living entity reaches its vanilla death processing step.
pub struct EntityDeathEvent {
    entity: Uuid,
}
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for EntityDeathEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_death");
}
impl Event for EntityDeathEvent {}
impl EntityDeathEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(entity: Uuid) -> Self {
        Self { entity }
    }
    /// Which entity this is about.
    #[must_use]
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
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(shooter: Uuid, projectile: Uuid) -> Self {
        Self {
            shooter,
            projectile,
            cancelled: false,
        }
    }
    /// Who threw it.
    #[must_use]
    pub const fn shooter(&self) -> Uuid {
        self.shooter
    }
    /// What was thrown.
    #[must_use]
    pub const fn projectile(&self) -> Uuid {
        self.projectile
    }
    /// Stops this from happening, or lets it happen again.
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
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for EntityTargetEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_target");
}
impl Event for EntityTargetEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl EntityTargetEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(entity: Uuid, target: Option<Uuid>) -> Self {
        Self {
            entity,
            target,
            cancelled: false,
        }
    }
    /// Which entity this is about.
    #[must_use]
    pub const fn entity_id(&self) -> Uuid {
        self.entity
    }
    /// Who the mob is now after, or nobody if it lost interest.
    #[must_use]
    pub const fn target_id(&self) -> Option<Uuid> {
        self.target
    }
    /// Stops this from happening, or lets it happen again.
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
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for EntityChangeBlockEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_change_block");
}
impl Event for EntityChangeBlockEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl EntityChangeBlockEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(entity: Uuid, world: String, block: BlockPos, to: String) -> Self {
        Self {
            entity,
            world,
            block,
            to,
            cancelled: false,
        }
    }
    /// Which entity this is about.
    #[must_use]
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    /// Which world this happened in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }
    /// Which block this is about.
    #[must_use]
    pub const fn block(&self) -> BlockPos {
        self.block
    }
    /// What the block is turning into.
    #[must_use]
    pub fn to(&self) -> &str {
        &self.to
    }
    /// Stops this from happening, or lets it happen again.
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

/// A lightning strike before its consequences are applied.
pub struct LightningStrikeEvent {
    entity: Arc<str>,
    world: String,
    cause: String,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for LightningStrikeEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/lightning_strike");
}
impl Event for LightningStrikeEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl LightningStrikeEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    pub fn new(
        entity: impl Into<Arc<str>>,
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
    /// Which entity this is about.
    #[must_use]
    pub fn entity(&self) -> &str {
        &self.entity
    }
    /// Which world this happened in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }
    /// What brought this about.
    #[must_use]
    pub fn cause(&self) -> &str {
        &self.cause
    }
    /// Whether a listener has stopped this from happening.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Stops this from happening, or lets it happen again.
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
    reason: ConversionReason,
}
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for EntityTransformEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_transform");
}
impl Event for EntityTransformEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl EntityTransformEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(entity: Uuid, transformed: Uuid, reason: ConversionReason) -> Self {
        Self {
            entity,
            transformed,
            cancelled: false,
            reason,
        }
    }
    /// Which entity this is about.
    #[must_use]
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    /// What it became. The entity it was is already gone.
    #[must_use]
    pub const fn transformed(&self) -> Uuid {
        self.transformed
    }
    /// Whether a listener has stopped this from happening.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Stops this from happening, or lets it happen again.
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
    /// What brought this about.
    #[must_use]
    pub const fn reason(&self) -> ConversionReason {
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
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for BlockExplodeEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/block_explode");
}
impl Event for BlockExplodeEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl BlockExplodeEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(world: String, source: BlockPos, blocks: Vec<BlockPos>) -> Self {
        Self {
            world,
            source,
            blocks,
            cancelled: false,
        }
    }
    /// Which world this happened in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }
    /// Which block went off.
    #[must_use]
    pub const fn source(&self) -> BlockPos {
        self.source
    }
    /// Every block this will affect.
    #[must_use]
    pub fn blocks(&self) -> &[BlockPos] {
        &self.blocks
    }
    /// Every block this will affect, so a listener can take some out.
    pub const fn blocks_mut(&mut self) -> &mut Vec<BlockPos> {
        &mut self.blocks
    }
    /// Whether a listener has stopped this from happening.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Stops this from happening, or lets it happen again.
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
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
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
    /// Which entity this is about.
    #[must_use]
    pub const fn entity(&self) -> Option<Uuid> {
        self.entity
    }
    /// Which world this happened in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }
    /// Every block this will affect.
    #[must_use]
    pub fn blocks(&self) -> &[BlockPos] {
        &self.blocks
    }
    /// Every block this will affect, so a listener can take some out.
    pub const fn blocks_mut(&mut self) -> &mut Vec<BlockPos> {
        &mut self.blocks
    }
    /// What fraction of the broken blocks will actually drop.
    #[must_use]
    pub const fn yield_factor(&self) -> f32 {
        self.yield_factor
    }
    /// What this does to blocks, as Bukkit names it: `KEEP`, `BLOCK` or `DESTROY`.
    #[must_use]
    pub fn explosion_result(&self) -> &str {
        &self.explosion_result
    }
    /// Changes what fraction of the blocks drop.
    pub const fn set_yield_factor(&mut self, value: f32) {
        self.yield_factor = value;
    }
    /// Whether a listener has stopped this from happening.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Stops this from happening, or lets it happen again.
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
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(entity: Uuid, pushed_by: Uuid) -> Self {
        Self {
            entity,
            pushed_by,
            cancelled: false,
        }
    }
    /// Which entity this is about.
    #[must_use]
    pub const fn entity_id(&self) -> Uuid {
        self.entity
    }
    /// Who pushed them.
    #[must_use]
    pub const fn pushed_by(&self) -> Uuid {
        self.pushed_by
    }
    /// Whether a listener has stopped this from happening.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Stops this from happening, or lets it happen again.
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
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    pub fn new(entity: Uuid, cause: impl Into<String>) -> Self {
        Self {
            entity,
            cause: cause.into(),
            remover: None,
            cancelled: false,
        }
    }
    /// Called by Foton when it was a player who took it down, rather than something.
    pub fn new_with_remover(entity: Uuid, cause: impl Into<String>, remover: Uuid) -> Self {
        Self {
            entity,
            cause: cause.into(),
            remover: Some(remover),
            cancelled: false,
        }
    }
    /// Which entity this is about.
    #[must_use]
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    /// What brought this about.
    #[must_use]
    pub fn cause(&self) -> &str {
        &self.cause
    }
    /// Who took it down, when somebody did.
    #[must_use]
    pub const fn remover(&self) -> Option<Uuid> {
        self.remover
    }
    /// Whether a listener has stopped this from happening.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Stops this from happening, or lets it happen again.
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
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
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
    /// Which entity this is about.
    #[must_use]
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    /// Who did it.
    #[must_use]
    pub const fn player(&self) -> Uuid {
        self.player
    }
    /// Which world this happened in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }
    /// Which block this is about.
    #[must_use]
    pub const fn block(&self) -> foton_utils::BlockPos {
        self.block
    }
    /// Which side of the block it hangs on.
    #[must_use]
    pub fn face(&self) -> &str {
        &self.face
    }
    /// Whether a listener has stopped this from happening.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Stops this from happening, or lets it happen again.
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
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(damager: Uuid, entity: Uuid, cause: String) -> Self {
        Self {
            damager,
            entity,
            cause,
            cancelled: false,
        }
    }
    /// Who dealt the damage.
    #[must_use]
    pub const fn damager(&self) -> Uuid {
        self.damager
    }
    /// Which entity this is about.
    #[must_use]
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    /// What brought this about.
    #[must_use]
    pub fn cause(&self) -> &str {
        &self.cause
    }
    /// Whether a listener has stopped this from happening.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Stops this from happening, or lets it happen again.
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
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(entity: Uuid, item: Uuid) -> Self {
        Self {
            entity,
            item,
            cancelled: false,
        }
    }
    /// Which entity this is about.
    #[must_use]
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    /// The item involved.
    #[must_use]
    pub const fn item(&self) -> Uuid {
        self.item
    }
    /// Whether a listener has stopped this from happening.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Stops this from happening, or lets it happen again.
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
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(
        world: String,
        x: f64,
        y: f64,
        z: f64,
        entity_type: String,
        reason: String,
    ) -> Self {
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
    /// Which world this happened in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }
    /// Where it happened.
    #[must_use]
    pub const fn position(&self) -> (f64, f64, f64) {
        (self.x, self.y, self.z)
    }
    /// What is about to spawn.
    #[must_use]
    pub fn entity_type(&self) -> &str {
        &self.entity_type
    }
    /// What brought this about.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
    /// Whether a listener has stopped this from happening.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Stops this from happening, or lets it happen again.
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
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(
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
    /// Which entity this is about.
    #[must_use]
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    /// Which world they came from.
    #[must_use]
    #[expect(
        clippy::wrong_self_convention,
        reason = "`from_world` is where they came from, not a constructor -- it pairs with `to_world` and renaming either would make the pair unreadable"
    )]
    pub fn from_world(&self) -> &str {
        &self.from_world
    }
    /// Where they came from.
    #[must_use]
    #[expect(
        clippy::wrong_self_convention,
        reason = "`from_world` is where they came from, not a constructor -- it pairs with `to_world` and renaming either would make the pair unreadable"
    )]
    pub const fn from_position(&self) -> DVec3 {
        self.from_position
    }
    /// Which world they are going to.
    #[must_use]
    pub fn to_world(&self) -> &str {
        &self.to_world
    }
    /// Sends them somewhere other than where the portal leads.
    pub fn set_destination(&mut self, world: String, position: DVec3) {
        self.to_world = world;
        self.to_position = position;
    }
    /// Where they are going.
    #[must_use]
    pub const fn to_position(&self) -> DVec3 {
        self.to_position
    }
    /// Which kind of portal it is.
    #[must_use]
    pub fn portal_type(&self) -> &str {
        &self.portal_type
    }
    /// Whether a listener has stopped this from happening.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Stops this from happening, or lets it happen again.
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
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(entity: Uuid, world: String, x: f64, y: f64, z: f64, reason: String) -> Self {
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
    /// Which entity this is about.
    #[must_use]
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    /// Which world this happened in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }
    /// Where it happened.
    #[must_use]
    pub const fn position(&self) -> (f64, f64, f64) {
        (self.x, self.y, self.z)
    }
    /// What brought this about.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
    /// Whether a listener has stopped this from happening.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Stops this from happening, or lets it happen again.
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
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(entity: Uuid, amount: f32) -> Self {
        Self {
            entity,
            amount,
            cancelled: false,
        }
    }
    /// Which entity this is about.
    #[must_use]
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    /// How much health is coming back.
    #[must_use]
    pub const fn amount(&self) -> f32 {
        self.amount
    }
    /// Whether a listener has stopped this from happening.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Stops this from happening, or lets it happen again.
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
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(entity: Uuid) -> Self {
        Self { entity }
    }
    /// Which entity this is about.
    #[must_use]
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
    item: ItemStack,
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
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(entity: Uuid, world: String, x: f64, y: f64, z: f64, item: ItemStack) -> Self {
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
    /// Which entity this is about.
    #[must_use]
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    /// Which world this happened in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }
    /// Where it happened.
    #[must_use]
    pub const fn position(&self) -> (f64, f64, f64) {
        (self.x, self.y, self.z)
    }
    /// The item involved.
    #[must_use]
    pub const fn item(&self) -> &ItemStack {
        &self.item
    }
    /// Whether a listener has stopped this from happening.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Stops this from happening, or lets it happen again.
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
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for ExpBottleEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/exp_bottle");
}
impl Event for ExpBottleEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl ExpBottleEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(entity: Uuid, experience: i32) -> Self {
        Self {
            entity,
            experience,
            cancelled: false,
        }
    }
    /// Which entity this is about.
    #[must_use]
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    /// How much experience the bottle is worth.
    #[must_use]
    pub const fn experience(&self) -> i32 {
        self.experience
    }
    /// Changes what the bottle is worth.
    pub fn set_experience(&mut self, v: i32) {
        self.experience = v.max(0);
    }
    /// Stops this from happening, or lets it happen again.
    pub const fn set_cancelled(&mut self, v: bool) {
        self.cancelled = v;
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
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for EntityMountEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/entity_mount");
}
impl Event for EntityMountEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl EntityMountEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(entity: Uuid, vehicle: Uuid) -> Self {
        Self {
            entity,
            vehicle,
            cancelled: false,
        }
    }
    /// Which entity this is about.
    #[must_use]
    pub const fn entity(&self) -> Uuid {
        self.entity
    }
    /// What is being ridden.
    #[must_use]
    pub const fn vehicle(&self) -> Uuid {
        self.vehicle
    }
    /// Stops this from happening, or lets it happen again.
    pub const fn set_cancelled(&mut self, v: bool) {
        self.cancelled = v;
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
