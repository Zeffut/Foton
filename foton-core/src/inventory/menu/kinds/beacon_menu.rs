//! Beacon menu.
//!
//! Vanilla parity: `BeaconMenu`. One slot that takes a single ingot or gem,
//! three numbers the client reads to draw the effect buttons, and the player
//! inventory. Picking the effects is not a click: it arrives as its own
//! packet, which is why nothing here handles buttons.

use std::sync::Arc;

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{REGISTRY, TaggedRegistryExt as _, vanilla_blocks, vanilla_menu_types};
use foton_utils::locks::IntoShared as _;
use std::array;

use crate::block_entity::SharedBlockEntity;
use crate::block_entity::entities::{
    BEACON_DATA_SLOTS, BeaconBlockEntity, BeaconDataSlots, effect_from_holder_id,
};
use crate::inventory::container::SimpleContainer;
use crate::inventory::menu::builder::{DataSlot, SectionKind};
use crate::inventory::prelude::*;
use crate::player::player_inventory::PlayerInventory;
use crate::world::LevelReader as _;
use foton_utils::Downcast as _;

/// Builds the beacon menu.
#[must_use]
pub fn beacon(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    data: Arc<BeaconDataSlots>,
    block_entity: SharedBlockEntity,
) -> Menu {
    let payment: ContainerRef = SimpleContainer::new(1).into_shared().into();

    let mut builder = MenuBuilder::new(&vanilla_menu_types::BEACON, container_id);
    // Vanilla parity: `BeaconMenu.PaymentSlot`, which takes one item and only
    // from the payment tag.
    let payment_slot = builder.section_with(
        payment.clone(),
        1,
        SectionKind::restricted(|_slot, stack| {
            REGISTRY
                .items
                .is_in_tag(stack.item(), &ItemTag::BEACON_PAYMENT_ITEMS)
        }),
    );
    let data_slots: [DataSlot; BEACON_DATA_SLOTS] = array::from_fn(|_| builder.data_slot(0));
    let player = builder.player_inventory(&inventory);

    builder.route(payment_slot, player.all(), FillDirection::Backward);
    builder.route(player.all(), payment_slot, FillDirection::Forward);
    // Vanilla parity: `BeaconMenu.removed`, which hands the payment back
    // rather than keeping it -- a beacon has no storage.
    builder.drain(payment_slot);

    builder.build(BeaconKind {
        payment,
        data,
        data_slots,
        block_entity,
    })
}

/// Per-menu beacon state.
pub struct BeaconKind {
    /// The one-slot payment container.
    payment: ContainerRef,
    /// Levels and effects, shared with the block entity.
    data: Arc<BeaconDataSlots>,
    /// Handles to the three synced values.
    data_slots: [DataSlot; BEACON_DATA_SLOTS],
    /// The beacon itself, which owns the chosen effects.
    block_entity: SharedBlockEntity,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl foton_utils::DowncastType for BeaconKind {
    const TYPE_KEY: foton_utils::DowncastTypeKey =
        foton_utils::DowncastTypeKey::new("foton:menu/beacon");
}

impl MenuKind for BeaconKind {
    /// Vanilla parity: `BeaconMenu.stillValid`, which checks the block is
    /// still a beacon and the player still in reach.
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        let pos = self.block_entity.get_block_pos();
        let world = player.get_world();
        world.get_block_state(pos).get_block() == &vanilla_blocks::BEACON
            && player.is_within_block_interaction_range_with_buffer(pos, 4.0)
    }

    /// Vanilla parity: `BeaconMenu.updateEffects`.
    ///
    /// The payment is only taken once the effects are accepted, so a request
    /// the beacon refuses -- too small a pyramid, an effect it does not offer
    /// -- costs the player nothing.
    fn on_set_beacon_effects(
        &mut self,
        _behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        _player: &Player,
        primary: Option<i32>,
        secondary: Option<i32>,
    ) {
        let paid = guard
            .get(self.payment.container_id())
            .is_some_and(|container| !container.get_item(0).is_empty());
        if !paid {
            return;
        }

        let Some(beacon) = self.block_entity.downcast_ref::<BeaconBlockEntity>() else {
            return;
        };
        if !beacon.set_effects(
            primary.and_then(effect_from_holder_id),
            secondary.and_then(effect_from_holder_id),
        ) {
            return;
        }

        if let Some(container) = guard.get_mut(self.payment.container_id()) {
            container.remove_item(0, 1);
            container.set_changed();
        }
    }

    /// Pushes the pyramid size and the chosen effects into the synced data
    /// slots, which is what draws the buttons the client offers.
    fn on_tick(
        &mut self,
        behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        let values = self.data.snapshot();
        for (slot, value) in self.data_slots.iter().zip(values) {
            slot.set(behavior, value);
        }
    }
}
