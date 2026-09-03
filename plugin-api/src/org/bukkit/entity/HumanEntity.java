package org.bukkit.entity;

/** A human-controlled living entity. */
public interface HumanEntity extends LivingEntity, org.bukkit.inventory.InventoryHolder {
    /** Current hunger level, in the vanilla range 0..20. */
    int getFoodLevel();
    @Override org.bukkit.inventory.PlayerInventory getInventory();
    org.bukkit.inventory.Inventory getEnderChest();
    org.bukkit.inventory.InventoryView getOpenInventory();
    void closeInventory();
}
