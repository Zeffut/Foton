package org.bukkit.inventory;

/** Common saddle and armor inventory shared by horse-family entities. */
public interface AbstractHorseInventory extends ArmoredSaddledMountInventory {
    ItemStack getSaddle();
    void setSaddle(ItemStack item);
    ItemStack getArmor();
    void setArmor(ItemStack item);
}
