//! The plain minecart.
//!
//! Vanilla parity: `Minecart`. Everything about rolling is
//! [`super::minecart_common`], which every cart shares; what is here is the
//! one cart a player can sit in.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::entity_type::EntityTypeRef;
use foton_utils::locks::SyncMutex;
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use super::minecart_common::{self, MinecartLike, MinecartState};
use crate::behavior::InteractionResult;
use crate::entity::{Entity, EntityBase, EntityBaseLoad, EntityMovementEmission};
use crate::player::Player;
use crate::world::World;

/// Vanilla parity: `AbstractMinecart.getDefaultGravity`.
const MINECART_GRAVITY: f64 = 0.04;

/// And the same in water, where a cart barely sinks at all.
const MINECART_GRAVITY_IN_WATER: f64 = 0.005;

/// A minecart.
#[entity_behavior(class = "Minecart")]
pub struct MinecartEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    minecart: SyncMutex<MinecartState>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `MinecartEntity`.
unsafe impl DowncastType for MinecartEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/minecart");
}

impl MinecartEntity {
    /// Creates one at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates one from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self::new_with_base(
            EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
        )
    }

    fn new_with_base(base: EntityBase, entity_type: EntityTypeRef) -> Self {
        Self {
            base,
            entity_type,
            minecart: SyncMutex::new(MinecartState::default()),
        }
    }
}

impl Entity for MinecartEntity {
    fn is_minecart(&self) -> bool {
        true
    }

    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn tick(&self) {
        minecart_common::tick_minecart(self);
    }

    /// Vanilla parity: `AbstractMinecart.getDefaultGravity`.
    fn get_default_gravity(&self) -> f64 {
        if self.is_in_water() {
            MINECART_GRAVITY_IN_WATER
        } else {
            MINECART_GRAVITY
        }
    }

    /// Vanilla parity: the `blocksBuilding = true` of the constructor.
    fn blocks_building(&self) -> bool {
        true
    }

    fn is_pushable(&self) -> bool {
        true
    }

    fn is_pickable(&self) -> bool {
        !self.is_removed()
    }

    fn movement_emission(&self) -> EntityMovementEmission {
        EntityMovementEmission::Events
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    /// Vanilla parity: `Minecart.interact`. Sneaking passes, and so does a
    /// cart that already has somebody in it -- a minecart seats exactly one.
    fn interact(
        &self,
        player: &Player,
        _hand: InteractionHand,
        _location: DVec3,
    ) -> InteractionResult {
        if player.is_secondary_use_active() || self.is_vehicle() {
            return InteractionResult::Pass;
        }
        let Some(world) = self.level() else {
            return InteractionResult::Pass;
        };
        let Some(vehicle) = world.get_entity_by_id(self.id()) else {
            return InteractionResult::Pass;
        };
        if player.start_riding(&vehicle) {
            InteractionResult::Consume
        } else {
            InteractionResult::Pass
        }
    }

    fn save_additional(&self, _nbt: &mut NbtCompound) {}

    fn load_additional(&self, _nbt: BorrowedNbtCompoundView<'_, '_>) {}
}

impl MinecartLike for MinecartEntity {
    /// Vanilla parity: `Minecart.isRideable`, the one cart that overrides it.
    fn is_rideable(&self) -> bool {
        true
    }

    fn minecart_state(&self) -> &SyncMutex<MinecartState> {
        &self.minecart
    }

    /// Vanilla parity: `Minecart.activateMinecart`, which throws the rider out.
    ///
    /// Vanilla also damages the cart enough to break it. Foton has no vehicle
    /// damage yet -- nothing can break a minecart or a boat at all -- so this
    /// does the half that is reachable.
    fn activate_minecart(&self, _world: &Arc<World>, _pos: BlockPos, powered: bool) {
        if !powered {
            return;
        }
        for passenger in self.passengers() {
            passenger.stop_riding();
        }
    }
}
