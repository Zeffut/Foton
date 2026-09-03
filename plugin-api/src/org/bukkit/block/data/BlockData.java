package org.bukkit.block.data;

import org.bukkit.Material;

/** What a block is, with its state. */
public interface BlockData {
    Material getMaterial();
    default Material getPlacementMaterial() { return getMaterial(); }

    /** The block state as text, the way `/setblock` writes it.
     *
     * `minecraft:oak_stairs[facing=north,half=bottom]`. Plugins parse this
     * far more than they use the typed subinterfaces, and it is the one form
     * that does not need a class per block.
     */
    String getAsString();
    default BlockData clone() { return new SimpleBlockData(getAsString()); }
    default String getAsString(boolean hideUnspecified) { return getAsString(); }
    default boolean matches(BlockData other) { return other != null && getMaterial() == other.getMaterial() && getAsString().equalsIgnoreCase(other.getAsString()); }
}
