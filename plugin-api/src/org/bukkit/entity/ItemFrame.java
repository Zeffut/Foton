package org.bukkit.entity;

/** An item frame attached to a block. */
public interface ItemFrame extends Hanging {
    org.bukkit.inventory.ItemStack getItem();
    default boolean setFacingDirection(org.bukkit.block.BlockFace face, boolean force) { return face != null && foton.Native.setHangingFacing(getUniqueId().toString(), face.name(), force); }
    default void setItem(org.bukkit.inventory.ItemStack item) { setItem(item, false); }
    /** Puts an item in the frame, playing the placing sound when asked. */
    default void setItem(org.bukkit.inventory.ItemStack item, boolean playSound) {
        foton.Native.setItemFrameItem(getUniqueId().toString(), foton.FotonInventory.encode(item), playSound);
    }
}
