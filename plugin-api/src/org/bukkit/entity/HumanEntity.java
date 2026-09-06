package org.bukkit.entity;

/** A human-controlled living entity. */
public interface HumanEntity extends LivingEntity, org.bukkit.inventory.InventoryHolder {
    /** Current hunger level, in the vanilla range 0..20. */
    int getFoodLevel();
    @Override org.bukkit.inventory.PlayerInventory getInventory();
    org.bukkit.inventory.Inventory getEnderChest();
    org.bukkit.inventory.InventoryView getOpenInventory();

    /** Opens an inventory for this human and returns the resulting view.
     *
     * <p>Returns null when the inventory could not be opened -- Bukkit's own
     * contract, and the reason plugins null-check the result before wiring
     * click handlers to it. */
    default org.bukkit.inventory.InventoryView openInventory(org.bukkit.inventory.Inventory inventory) {
        return null;
    }

    void closeInventory();
}
