//! The dragon's hitboxes.
//!
//! Vanilla parity: `EnderDragonPart`. The dragon is not one box. It is eight,
//! and every hit a player lands arrives addressed to one of them rather than to
//! the dragon: the head takes full damage, everything else a quarter. A part is
//! a real `Entity` -- it needs a position, a bounding box and a network ID,
//! because the attack and interact packets carry an entity ID and the server
//! range-checks against the box that ID names.
//!
//! Three things make a part unlike every other entity in the tree:
//!
//! * **It is never spawned.** No part is added to the world entity manager, so
//!   nothing ticks it, saves it, or tracks it. Vanilla is the same: `ChunkMap`
//!   skips `EnderDragonPart` in `addEntity`, and `getAddEntityPacket` throws.
//!   The client builds all eight itself from the dragon's spawn packet.
//! * **Its ID is not its own.** The client assigns `dragonId + 1 ..= + 8` in
//!   `EnderDragon.recreateFromPacket`, so the server has to hand out exactly
//!   that block. See [`reserve_entity_ids`](crate::entity::reserve_entity_ids).
//! * **It cannot be hurt.** [`Entity::hurt`] forwards to the dragon, which is
//!   the whole point of the part existing.
//!
//! **Gaps**: vanilla overrides `getPickResult` to return the dragon's, and `is`
//! so that a part counts as its parent in identity tests. Steel implements
//! neither hook yet -- there is no `ServerboundPickItemFromEntityPacket`
//! handler, and no `Entity::is` -- so there is nothing to override.

use std::sync::Weak;

use glam::DVec3;
use steel_registry::entity_type::{EntityDimensions, EntityTypeRef};
use steel_utils::{Downcast as _, DowncastType, DowncastTypeKey};

use super::EnderDragon;
use crate::entity::damage::DamageSource;
use crate::entity::{Entity, EntityBase};
use crate::world::World;

/// Vanilla `EntityDimensions.defaultEyeHeight`.
const EYE_HEIGHT_FACTOR: f32 = 0.85;

/// Hitboxes a dragon has.
///
/// Vanilla parity: the length of `EnderDragon.subEntities`.
pub const PART_COUNT: usize = 8;

/// One of the dragon's eight hitboxes.
pub struct EnderDragonPart {
    base: EntityBase,
    entity_type: EntityTypeRef,
    /// Network ID of the dragon this hitbox belongs to.
    ///
    /// Vanilla holds `parentMob` directly. Steel cannot: the dragon's `Arc`
    /// does not exist yet while its constructor is building the parts, so the
    /// link is stored as an ID and resolved through the world the same way a
    /// projectile resolves its owner.
    parent_id: i32,
    /// Which of [`EnderDragon`]'s parts this is.
    index: DragonPartIndex,
}

/// Which hitbox a part is.
///
/// Vanilla distinguishes them by `name` for the renderer and by reference
/// identity in `EnderDragon.hurt`, where only `part != this.head` matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DragonPartIndex {
    /// Vanilla parity: `EnderDragon.head`.
    Head,
    /// Vanilla parity: `EnderDragon.neck`.
    Neck,
    /// Vanilla parity: `EnderDragon.body`.
    Body,
    /// Vanilla parity: `EnderDragon.tail1`.
    Tail1,
    /// Vanilla parity: `EnderDragon.tail2`.
    Tail2,
    /// Vanilla parity: `EnderDragon.tail3`.
    Tail3,
    /// Vanilla parity: `EnderDragon.wing1`.
    Wing1,
    /// Vanilla parity: `EnderDragon.wing2`.
    Wing2,
}

impl DragonPartIndex {
    /// The eight parts in the order vanilla builds them.
    ///
    /// Vanilla parity: the `subEntities` array. The order is load-bearing --
    /// the client derives each part's ID from its index in it.
    pub const ORDER: [Self; PART_COUNT] = [
        Self::Head,
        Self::Neck,
        Self::Body,
        Self::Tail1,
        Self::Tail2,
        Self::Tail3,
        Self::Wing1,
        Self::Wing2,
    ];

    /// Returns the part's slot in [`Self::ORDER`].
    #[must_use]
    pub const fn slot(self) -> usize {
        match self {
            Self::Head => 0,
            Self::Neck => 1,
            Self::Body => 2,
            Self::Tail1 => 3,
            Self::Tail2 => 4,
            Self::Tail3 => 5,
            Self::Wing1 => 6,
            Self::Wing2 => 7,
        }
    }

    /// Returns the part's vanilla name.
    ///
    /// Vanilla parity: the `name` each part is constructed with. The three tail
    /// segments share one name, and so do the two wings.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Head => "head",
            Self::Neck => "neck",
            Self::Body => "body",
            Self::Tail1 | Self::Tail2 | Self::Tail3 => "tail",
            Self::Wing1 | Self::Wing2 => "wing",
        }
    }

    /// Returns the part's width and height.
    ///
    /// Vanilla parity: the two floats each `new EnderDragonPart` is given.
    #[must_use]
    pub const fn size(self) -> (f32, f32) {
        match self {
            Self::Head => (1.0, 1.0),
            Self::Neck => (3.0, 3.0),
            Self::Body => (5.0, 3.0),
            Self::Tail1 | Self::Tail2 | Self::Tail3 => (2.0, 2.0),
            Self::Wing1 | Self::Wing2 => (4.0, 2.0),
        }
    }

    /// Returns the part's bounding-box dimensions.
    ///
    /// Vanilla parity: `EntityDimensions.scalable(w, h)`.
    #[must_use]
    pub fn dimensions(self) -> EntityDimensions {
        let (width, height) = self.size();
        EntityDimensions::new(width, height, height * EYE_HEIGHT_FACTOR)
    }
}

// SAFETY: This key is owned by Steel and uniquely identifies `EnderDragonPart`.
unsafe impl DowncastType for EnderDragonPart {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/ender_dragon_part");
}

impl EnderDragonPart {
    /// Builds one hitbox for a dragon.
    ///
    /// `id` must come from the block the dragon reserved, and `parent_id` must
    /// be the dragon's own ID; the client reconstructs the relationship from
    /// nothing but the arithmetic between them.
    #[must_use]
    pub fn new(
        entity_type: EntityTypeRef,
        id: i32,
        parent_id: i32,
        index: DragonPartIndex,
        position: DVec3,
        world: Weak<World>,
    ) -> Self {
        Self {
            base: EntityBase::new(id, position, index.dimensions(), world),
            entity_type,
            parent_id,
            index,
        }
    }

    /// Returns which hitbox this is.
    #[must_use]
    pub const fn index(&self) -> DragonPartIndex {
        self.index
    }

    /// Returns the network ID of the dragon this hitbox belongs to.
    #[must_use]
    pub const fn parent_id(&self) -> i32 {
        self.parent_id
    }

    /// Moves the hitbox.
    ///
    /// Vanilla parity: the `part.setPos` of `EnderDragon.tickPart`. A part is
    /// not registered with the world entity manager, so this is a local move --
    /// there is no spatial index entry to keep in step.
    pub fn set_part_position(&self, position: DVec3) {
        self.base.set_position_local(position);
    }

    /// Returns a height `progress` of the way up the hitbox.
    ///
    /// Vanilla parity: `Entity.getY(double)`. The dragon's phases aim from
    /// `head.getY(0.5)` -- the middle of the head, not its feet.
    #[must_use]
    pub fn y_at(&self, progress: f64) -> f64 {
        let (_, height) = self.index.size();
        self.position().y + f64::from(height) * progress
    }

    /// Runs `action` against the dragon this hitbox belongs to.
    ///
    /// Returns `None` when the dragon is no longer live, which is the state a
    /// part is left in for the rest of the tick that removed its parent.
    pub fn with_parent<R>(
        &self,
        world: &World,
        action: impl FnOnce(&EnderDragon) -> R,
    ) -> Option<R> {
        let parent = world.get_entity_by_id(self.parent_id)?;
        let dragon = parent.downcast_ref::<EnderDragon>()?;
        Some(action(dragon))
    }
}

impl Entity for EnderDragonPart {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    /// Vanilla parity: `super(parentMob.getType(), parentMob.level())` -- a
    /// part reports the dragon's own entity type.
    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    /// Vanilla parity: `EnderDragonPart.isPickable` returns true, which is what
    /// makes the hitbox the thing a player's crosshair and attack packet find.
    fn is_pickable(&self) -> bool {
        true
    }

    /// Vanilla parity: `EnderDragonPart.hurtServer`. This is the routing the
    /// whole part mechanism exists for -- the hit arrives on the hitbox and is
    /// resolved against the dragon behind it.
    fn hurt(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        if self.is_invulnerable_to_base(source) {
            return false;
        }

        self.with_parent(world, |dragon| {
            dragon.hurt_part(world, self.index, source, amount)
        })
        .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use steel_registry::vanilla_entities;

    #[test]
    fn the_head_is_the_smallest_hitbox_and_the_body_the_widest() {
        assert_eq!(DragonPartIndex::Head.size(), (1.0, 1.0));
        assert_eq!(DragonPartIndex::Body.size(), (5.0, 3.0));
    }

    #[test]
    fn part_slots_match_the_order_the_client_derives_ids_from() {
        for (slot, part) in DragonPartIndex::ORDER.into_iter().enumerate() {
            assert_eq!(part.slot(), slot);
        }
    }

    #[test]
    fn a_hitbox_reports_the_dragons_entity_type_rather_than_one_of_its_own() {
        let part = EnderDragonPart::new(
            &vanilla_entities::ENDER_DRAGON,
            42,
            41,
            DragonPartIndex::Head,
            DVec3::new(8.5, 64.0, 8.5),
            Weak::new(),
        );

        assert_eq!(part.entity_type(), &vanilla_entities::ENDER_DRAGON);
        assert_eq!(part.parent_id(), 41);
    }

    #[test]
    fn a_hitbox_bounding_box_is_centered_on_its_position_and_sized_from_its_part() {
        let part = EnderDragonPart::new(
            &vanilla_entities::ENDER_DRAGON,
            42,
            41,
            DragonPartIndex::Body,
            DVec3::new(8.5, 64.0, 8.5),
            Weak::new(),
        );

        let aabb = part.bounding_box();
        assert!((aabb.max_x() - aabb.min_x() - 5.0).abs() < 1.0e-9);
        assert!((aabb.max_y() - aabb.min_y() - 3.0).abs() < 1.0e-9);
        assert!((aabb.min_y() - 64.0).abs() < 1.0e-9);
    }
}
