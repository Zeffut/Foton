package org.bukkit.inventory;

/** Inventory contract shared by modern armored, saddled mounts. */
public interface ArmoredSaddledMountInventory extends Inventory {
    ItemStack getSaddle();
    void setSaddle(ItemStack item);
}
