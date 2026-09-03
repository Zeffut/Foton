package org.bukkit.block;

/** A jukebox block state backed by its vanilla block entity. */
public interface Jukebox extends TileState {
    boolean isPlaying();
    default org.bukkit.inventory.ItemStack getRecord() { return null; }
    default void setRecord(org.bukkit.inventory.ItemStack record) { }
}
