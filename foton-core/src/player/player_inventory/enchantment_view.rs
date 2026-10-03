//! Live Bukkit enchantment view access outside a synchronous prepare callback.

use foton_registry::item_stack::ItemStack;
use foton_utils::{BlockPos, Downcast as _};

use crate::{event::EnchantmentViewState, inventory::menu::kinds::EnchantmentKind, player::Player};

impl Player {
    /// Reads the title atomically with the identity check, so a reopened table cannot leak through.
    #[must_use]
    pub fn enchantment_title(&self, instance: u64) -> Option<String> {
        let open = self.open_menu.lock();
        let menu = open.menu.as_ref()?;
        if menu.behavior().instance_id() != instance
            || menu.kind().downcast_ref::<EnchantmentKind>().is_none()
        {
            return None;
        }
        open.title.clone()
    }

    /// Reads the table location and full-width view values from the active menu.
    #[must_use]
    pub fn enchantment_view(&self) -> Option<(u64, BlockPos, String, EnchantmentViewState)> {
        let open = self.open_menu.lock();
        let menu = open.menu.as_ref()?;
        let kind = menu.kind().downcast_ref::<EnchantmentKind>()?;
        let (position, world) = kind.table();
        Some((
            menu.behavior().instance_id(),
            position,
            world,
            kind.view_state(),
        ))
    }

    /// Updates an active enchanting view without rerolling or consuming items.
    pub fn set_enchantment_view(&self, instance: u64, state: EnchantmentViewState) -> bool {
        let mut open = self.open_menu.lock();
        let Some(menu) = open.menu.as_mut() else {
            return false;
        };
        if menu.behavior().instance_id() != instance || !menu.set_enchantment_view_state(state) {
            return false;
        }
        menu.behavior_mut().broadcast_changes(&self.connection);
        true
    }

    /// Reads a slot only while the handle still belongs to the active table.
    #[must_use]
    pub fn enchantment_item(&self, instance: u64, index: usize) -> Option<ItemStack> {
        let open = self.open_menu.lock();
        let menu = open.menu.as_ref()?;
        if index >= 2
            || menu.behavior().instance_id() != instance
            || menu.kind().downcast_ref::<EnchantmentKind>().is_none()
        {
            return None;
        }
        let guard = menu.behavior().lock_all_containers();
        Some(menu.behavior().slots().get(index)?.get_item(&guard).clone())
    }

    /// Writes and prepares the exact table named by a plugin's view handle.
    pub fn set_enchantment_item(&self, instance: u64, index: usize, stack: ItemStack) -> bool {
        let Ok(mut menu) = self.take_open_menu_for_callback(None) else {
            return false;
        };
        let changed = menu.behavior().instance_id() == instance
            && menu.set_enchantment_item(index, stack, self);
        self.finish_open_menu_callback(menu);
        if changed {
            self.broadcast_inventory_changes();
        }
        changed
    }

    /// Closes only the table to which the plugin view belongs.
    pub fn close_enchantment_view(&self, instance: u64) -> bool {
        let Ok(menu) = self.take_open_menu_for_callback(None) else {
            return false;
        };
        let matches = menu.behavior().instance_id() == instance
            && menu.kind().downcast_ref::<EnchantmentKind>().is_some();
        if matches {
            self.close_container();
        }
        self.finish_open_menu_callback(menu);
        matches
    }
}
