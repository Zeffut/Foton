//! The furnace minecart.
//!
//! Vanilla parity: `MinecartFurnace`. A cart that drives itself: feed it coal
//! and it pushes along the rails in whatever direction it was fed from, for a
//! minute per piece of fuel. It is slower than an ordinary cart, which is the
//! price of not needing a powered rail.
//!
//! Everything about rolling is [`super::minecart_common`]; what is here is the
//! fuel, the push, and the slowdown the push replaces.

use std::sync::Weak;
use std::sync::atomic::{AtomicI32, Ordering};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_entity_data::FurnaceMinecartEntityData;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{REGISTRY, TaggedRegistryExt as _};
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{DowncastType, DowncastTypeKey};

use super::minecart_common::{self, MinecartLike, MinecartState};
use crate::behavior::InteractionResult;
use crate::entity::{Entity, EntityBase, EntityBaseLoad, EntityMovementEmission, EntitySyncedData};
use crate::player::Player;
use crate::world::World;

/// Vanilla parity: `AbstractMinecart.getDefaultGravity`.
const MINECART_GRAVITY: f64 = 0.04;

/// And the same in water.
const MINECART_GRAVITY_IN_WATER: f64 = 0.005;

/// How long one piece of fuel lasts.
///
/// Vanilla parity: `MinecartFurnace.FUEL_TICKS_PER_ITEM` -- three minutes.
const FUEL_TICKS_PER_ITEM: i32 = 3600;

/// How much fuel the cart will hold.
///
/// Vanilla parity: `MinecartFurnace.MAX_FUEL_TICKS`.
const MAX_FUEL_TICKS: i32 = 32000;

/// How fast a fueled cart runs, against an ordinary one.
///
/// Vanilla parity: the `* 0.5` of `MinecartFurnace.getMaxSpeed`.
const SPEED_ON_LAND: f64 = 0.5;

/// And in water, where it loses less.
///
/// Vanilla parity: the `* 0.75` of the same method.
const SPEED_IN_WATER: f64 = 0.75;

/// The drag a pushing cart feels, against the `0.98` of a coasting one.
///
/// Vanilla parity: the `multiply(0.8, 0.0, 0.8)` of `applyNaturalSlowdown`.
const PUSHING_DRAG: f64 = 0.8;

/// And the drag with no fuel left.
const COASTING_DRAG: f64 = 0.98;

/// How much of its push a cart keeps under water.
///
/// Vanilla parity: the `scale(0.1)` of `applyNaturalSlowdown`.
const UNDERWATER_PUSH: f64 = 0.1;

/// Below this, the push counts as nothing.
///
/// Vanilla parity: the `1.0E-7` of `applyNaturalSlowdown`.
const PUSH_EPSILON: f64 = 1.0e-7;

/// Vanilla parity: the `epsilonPushCheck` of `calculateNewPushAlong`.
const PUSH_REALIGN_EPSILON: f64 = 1.0e-4;

/// Vanilla parity: the `epsilonMovementCheck` of the same method.
const MOVEMENT_REALIGN_EPSILON: f64 = 0.001;

/// A furnace minecart.
#[entity_behavior(class = "MinecartFurnace")]
pub struct FurnaceMinecartEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    minecart: SyncMutex<MinecartState>,
    entity_data: SyncMutex<FurnaceMinecartEntityData>,
    /// Ticks of fuel left.
    fuel: AtomicI32,
    /// The direction the cart drives itself in, horizontal only.
    push: SyncMutex<DVec3>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `FurnaceMinecartEntity`.
unsafe impl DowncastType for FurnaceMinecartEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/furnace_minecart");
}

impl FurnaceMinecartEntity {
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
            entity_data: SyncMutex::new(FurnaceMinecartEntityData::new()),
            fuel: AtomicI32::new(0),
            push: SyncMutex::new(DVec3::ZERO),
        }
    }

    /// Returns whether the furnace is burning.
    #[must_use]
    pub fn has_fuel(&self) -> bool {
        self.fuel.load(Ordering::Relaxed) > 0
    }

    /// Returns the ticks of fuel left.
    #[must_use]
    pub fn fuel(&self) -> i32 {
        self.fuel.load(Ordering::Relaxed)
    }

    /// Feeds the cart, reporting whether the item was taken.
    ///
    /// Vanilla parity: `MinecartFurnace.addFuel`. The push points away from
    /// whoever fed it, which is how a player chooses the direction: you stand
    /// behind the cart and feed it forwards.
    pub fn add_fuel(&self, from: DVec3, item: &ItemStack) -> bool {
        if !REGISTRY
            .items
            .is_in_tag(item.item(), &ItemTag::FURNACE_MINECART_FUEL)
        {
            return false;
        }
        let fuel = self.fuel.load(Ordering::Relaxed);
        if fuel + FUEL_TICKS_PER_ITEM > MAX_FUEL_TICKS {
            return false;
        }

        self.fuel
            .store(fuel + FUEL_TICKS_PER_ITEM, Ordering::Relaxed);
        let away = self.position() - from;
        *self.push.lock() = DVec3::new(away.x, 0.0, away.z);
        true
    }

    /// Keeps the push pointing the way the cart is actually going.
    ///
    /// Vanilla parity: `MinecartFurnace.calculateNewPushAlong`. A rail corner
    /// turns the cart, and without this the push would keep shoving it at the
    /// wall it just turned away from.
    fn realign_push(&self, movement: DVec3) -> DVec3 {
        let push = *self.push.lock();
        let push_xz = push.x.mul_add(push.x, push.z * push.z);
        let movement_xz = movement.x.mul_add(movement.x, movement.z * movement.z);
        if push_xz <= PUSH_REALIGN_EPSILON || movement_xz <= MOVEMENT_REALIGN_EPSILON {
            return push;
        }

        // The projection of the push onto the direction of travel, scaled back
        // to the push's own length.
        let projected = movement * (push.dot(movement) / movement.length_squared());
        if projected.length_squared() <= 0.0 {
            return push;
        }
        projected.normalize() * push.length()
    }
}

impl Entity for FurnaceMinecartEntity {
    fn is_minecart(&self) -> bool {
        true
    }

    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `MinecartFurnace.tick`.
    fn tick(&self) {
        minecart_common::tick_minecart(self);

        let fuel = self.fuel.load(Ordering::Relaxed);
        if fuel > 0 {
            self.fuel.store(fuel - 1, Ordering::Relaxed);
        }
        if self.fuel.load(Ordering::Relaxed) <= 0 {
            *self.push.lock() = DVec3::ZERO;
        }

        // The synced flag is what lights the furnace texture on the client.
        let burning = self.has_fuel();
        let mut data = self.entity_data.lock();
        if *data.minecart_furnace.id_fuel.get() != burning {
            data.minecart_furnace.id_fuel.set(burning);
        }
    }

    fn get_default_gravity(&self) -> f64 {
        if self.is_in_water() {
            MINECART_GRAVITY_IN_WATER
        } else {
            MINECART_GRAVITY
        }
    }

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

    /// Vanilla parity: `MinecartFurnace.interact`, which answers `SUCCESS`
    /// whether or not the item was fuel -- there is nothing else to do with a
    /// furnace cart, so a failed feed is not a fall-through.
    fn interact(
        &self,
        player: &Player,
        hand: InteractionHand,
        _location: DVec3,
    ) -> InteractionResult {
        let fed = {
            let inventory = player.inventory.lock();
            let held = inventory.get_item_in_hand(hand);
            self.add_fuel(player.position(), held)
        };
        if fed && !player.has_infinite_materials() {
            player.inventory.lock().get_item_in_hand_mut(hand).shrink(1);
        }
        InteractionResult::Success
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let push = *self.push.lock();
        nbt.insert("PushX", push.x);
        nbt.insert("PushZ", push.z);
        nbt.insert("Fuel", self.fuel.load(Ordering::Relaxed) as i16);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        let x = nbt.double("PushX").unwrap_or(0.0);
        let z = nbt.double("PushZ").unwrap_or(0.0);
        *self.push.lock() = DVec3::new(x, 0.0, z);
        if let Some(fuel) = nbt.short("Fuel") {
            self.fuel.store(i32::from(fuel), Ordering::Relaxed);
        }
    }
}

impl MinecartLike for FurnaceMinecartEntity {
    fn minecart_state(&self) -> &SyncMutex<MinecartState> {
        &self.minecart
    }

    /// Vanilla parity: `MinecartFurnace.getMaxSpeed`, which is half an ordinary
    /// cart's on land and three quarters of it in water.
    fn max_speed_factor(&self) -> f64 {
        if self.is_in_water() {
            SPEED_IN_WATER
        } else {
            SPEED_ON_LAND
        }
    }

    /// Vanilla parity: `MinecartFurnace.applyNaturalSlowdown`. A pushing cart
    /// sheds speed faster than a coasting one and then adds the push back, so
    /// it settles at a steady pace rather than accelerating forever.
    fn apply_natural_slowdown(&self, movement: DVec3) -> DVec3 {
        let push = *self.push.lock();
        if push.length_squared() <= PUSH_EPSILON {
            return DVec3::new(movement.x * COASTING_DRAG, 0.0, movement.z * COASTING_DRAG);
        }

        let realigned = self.realign_push(movement);
        *self.push.lock() = realigned;
        let mut pushed =
            DVec3::new(movement.x * PUSHING_DRAG, 0.0, movement.z * PUSHING_DRAG) + realigned;
        if self.is_in_water() {
            pushed *= UNDERWATER_PUSH;
        }
        pushed
    }
}
