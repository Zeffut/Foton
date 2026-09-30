package org.bukkit.entity;

import org.bukkit.inventory.ItemStack;

/** A display entity rendering an item, through one of its model contexts. */
public interface ItemDisplay extends Display {
    ItemStack getItemStack();

    void setItemStack(ItemStack item);

    ItemDisplayTransform getItemDisplayTransform();

    void setItemDisplayTransform(ItemDisplayTransform display);

    /** The item model context; each name is vanilla's, upper-cased. */
    enum ItemDisplayTransform {
        NONE,
        THIRDPERSON_LEFTHAND,
        THIRDPERSON_RIGHTHAND,
        FIRSTPERSON_LEFTHAND,
        FIRSTPERSON_RIGHTHAND,
        HEAD,
        GUI,
        GROUND,
        FIXED,
        ON_SHELF
    }
}
