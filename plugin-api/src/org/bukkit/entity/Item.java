package org.bukkit.entity;

/** An item entity lying in a world. */
public interface Item extends Entity {
    org.bukkit.inventory.ItemStack getItemStack();
    void setItemStack(org.bukkit.inventory.ItemStack item);
    default void setUnlimitedLifetime(boolean unlimited) { }
    default boolean isUnlimitedLifetime() { return false; }
    default int getTicksLived() { return 0; }
    /** Who dropped or threw the item, or null. */
    default java.util.UUID getThrower() { return foton.Native.parse(foton.Native.itemThrower(getUniqueId().toString())); }
    /** The only player allowed to pick the item up, or null for anyone. */
    default java.util.UUID getOwner() { return foton.Native.parse(foton.Native.itemOwner(getUniqueId().toString())); }
    default void setOwner(java.util.UUID owner) { foton.Native.setItemOwner(getUniqueId().toString(), owner == null ? null : owner.toString()); }
    /** Ticks before the item can be picked up; 32767 means never. */
    default int getPickupDelay() { return foton.Native.itemPickupDelay(getUniqueId().toString()); }
    default void setPickupDelay(int delay) { foton.Native.setItemPickupDelay(getUniqueId().toString(), delay); }
    default void setTicksLived(int value) { }
}
