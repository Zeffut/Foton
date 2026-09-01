package org.bukkit;

/** Sixteen by sixteen columns of a world. */
public interface Chunk {
    int getX();

    int getZ();

    World getWorld();

    org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer();

    default org.bukkit.block.Block getBlock(int x, int y, int z) {
        return getWorld().getBlockAt(getX() * 16 + x, y, getZ() * 16 + z);
    }
}
