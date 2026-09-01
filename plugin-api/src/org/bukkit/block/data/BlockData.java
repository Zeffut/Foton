package org.bukkit.block.data;

import org.bukkit.Material;

/** What a block is, with its state. */
public interface BlockData {
    Material getMaterial();

    /** The block state as text, the way `/setblock` writes it.
     *
     * `minecraft:oak_stairs[facing=north,half=bottom]`. Plugins parse this
     * far more than they use the typed subinterfaces, and it is the one form
     * that does not need a class per block.
     */
    String getAsString();
}
