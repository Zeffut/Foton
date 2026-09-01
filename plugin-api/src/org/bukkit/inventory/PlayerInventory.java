package org.bukkit.inventory;

/** A player's own inventory, with the slots that have names. */
public interface PlayerInventory extends Inventory {
    ItemStack getItemInMainHand();

    void setItemInMainHand(ItemStack item);

    ItemStack getItemInHand();

    void setItemInHand(ItemStack item);

    ItemStack getItemInOffHand();

    void setItemInOffHand(ItemStack item);

    ItemStack getHelmet();

    ItemStack getChestplate();

    ItemStack getLeggings();

    ItemStack getBoots();

    default ItemStack[] getArmorContents() {
        return new ItemStack[] { getBoots(), getLeggings(), getChestplate(), getHelmet() };
    }

    int getHeldItemSlot();
}
