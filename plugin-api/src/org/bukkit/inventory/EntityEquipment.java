package org.bukkit.inventory;

/** Equipment slots exposed for a living entity. */
public interface EntityEquipment {
    ItemStack[] getArmorContents();
    ItemStack getItemInMainHand();
    ItemStack getItemInOffHand();
}
