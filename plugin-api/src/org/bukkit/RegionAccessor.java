package org.bukkit;

/** Provides block access to a world or world-generation region. */
public interface RegionAccessor {
    org.bukkit.block.Block getBlockAt(int x, int y, int z);
}
