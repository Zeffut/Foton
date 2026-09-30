package org.bukkit.entity;

import org.bukkit.inventory.ItemStack;

/** A fireball whose rendered item is exposed by the server. */
public interface SizedFireball extends Fireball {
    ItemStack getDisplayItem();
    void setDisplayItem(ItemStack item);
}
