//! Per-menu behavior hooks.

use foton_registry::item_stack::ItemStack;
use foton_utils::ErasedType;

use crate::inventory::menu::behavior::MenuBehavior;
use crate::{inventory::lock::ContainerLockGuard, player::Player};

use crate::inventory::click::{Click, ClickOutcome, QuickCraft};

/// Per-menu behavior that isn't shared: recompute-on-change, validity, close
/// cleanup, and the optional shift-click override.
///
/// Menu transitions requested while a hook owns mutable access to the current
/// menu are applied after that hook returns.
///
/// Concrete implementations must claim a unique
/// [`foton_utils::DowncastTypeKey`] through [`foton_utils::DowncastType`].
pub trait MenuKind: ErasedType + Send + Sync {
    /// Recompute recipe-driven slots after a click touched a real slot.
    fn slots_changed(
        &mut self,
        _behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
    }

    /// Extra cleanup on close beyond [`Menu::removed`].
    fn removed(&mut self, _behavior: &mut MenuBehavior, _player: &Player) {}

    /// Applies a rename from the client (anvil-style text input) and recomputes
    /// any result. No-op for kinds without a rename input.
    fn on_rename(&mut self, _behavior: &mut MenuBehavior, _name: String, _player: &Player) {}

    /// Runs after initial contents are built but before they're sent, so
    /// anything populated here appears in the first render.
    fn on_open(
        &mut self,
        _behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
    }

    /// Switches one container slot on or off.
    ///
    /// Vanilla parity: `ServerGamePacketListenerImpl.handleContainerSlotStateChanged`,
    /// which reaches `CrafterMenu.setSlotState`. The crafter is the only menu
    /// that has switchable slots.
    fn on_slot_state_changed(
        &mut self,
        _behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
        _player: &Player,
        _slot: usize,
        _enabled: bool,
    ) {
    }

    /// Applies the two effects a player picked in a beacon menu.
    ///
    /// Vanilla parity: `BeaconMenu.updateEffects`. The beacon is the only menu
    /// this reaches; the ids are mob-effect registry ids offset by one, so
    /// zero means "no effect".
    fn on_set_beacon_effects(
        &mut self,
        _behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
        _player: &Player,
        _primary: Option<i32>,
        _secondary: Option<i32>,
    ) {
    }

    /// Runs once per tick per viewer while open, before changes are synced.
    fn on_tick(
        &mut self,
        _behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
    }

    /// Runs for every non-drag click before default handling. Return
    /// [`ClickOutcome::Consume`] to treat the slot as a button, or
    /// [`ClickOutcome::Fallthrough`] for default handling.
    fn on_slot_clicked(
        &mut self,
        _behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
        _click: Click,
        _player: &Player,
    ) -> ClickOutcome {
        ClickOutcome::Fallthrough
    }

    /// Handles a container button press.
    ///
    /// Vanilla parity: `AbstractContainerMenu.clickMenuButton`. This is how an
    /// enchanting table's three offers arrive, and how a lectern turns a page:
    /// the click carries a button id rather than a slot. Returns whether the
    /// button was accepted, which vanilla uses to decide whether to resend the
    /// menu.
    fn on_button_click(
        &mut self,
        _behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
        _player: &Player,
        _button: i32,
    ) -> bool {
        false
    }

    /// Handles the player clicking one of a merchant's trades.
    ///
    /// Vanilla parity: `MerchantMenu.setSelectionHint`, reached from
    /// `ServerGamePacketListenerImpl.handleSelectTrade`. Vanilla follows it with
    /// `tryMoveItems`, which pulls the price out of the player's inventory into
    /// the payment slots; that half is not implemented, so the player fills the
    /// slots themselves and the trade otherwise behaves the same.
    fn on_select_trade(
        &mut self,
        _behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
        _player: &Player,
        _selected_trade: i32,
    ) {
    }

    /// Runs for each drag phase before default handling. Return
    /// [`ClickOutcome::Consume`] to cancel the drag.
    fn on_drag(
        &mut self,
        _behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
        _action: QuickCraft,
        _player: &Player,
    ) -> ClickOutcome {
        ClickOutcome::Fallthrough
    }

    /// Returns true if a drag may distribute items into `slot_index`.
    fn can_drag_to(&self, _slot_index: usize) -> bool {
        true
    }

    /// Returns true if this menu is still valid for the player.
    fn still_valid(&self, _behavior: &MenuBehavior, _player: &Player) -> bool {
        true
    }

    /// Returns true if an item may be taken from `slot_index` during pickup-all.
    fn can_take_item_for_pick_all(&self, _carried: &ItemStack, _slot_index: usize) -> bool {
        true
    }

    /// Shift-click override. Return `Some` to fully handle the quick-move, or
    /// `None` to fall back to the route table.
    fn quick_move(
        &mut self,
        _behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
        _slot_index: usize,
        _player: &Player,
    ) -> Option<ItemStack> {
        None
    }
}
