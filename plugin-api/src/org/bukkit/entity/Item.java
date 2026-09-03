package org.bukkit.entity;

/** An item entity lying in a world. */
public interface Item extends Entity {
    org.bukkit.inventory.ItemStack getItemStack();
    void setItemStack(org.bukkit.inventory.ItemStack item);
    default void setUnlimitedLifetime(boolean unlimited) { }
    default boolean isUnlimitedLifetime() { return false; }
    default int getTicksLived() { return 0; }
    default void setTicksLived(int value) { }
}
