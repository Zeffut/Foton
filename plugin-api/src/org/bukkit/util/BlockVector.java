package org.bukkit.util;

/** Integer-coordinate vector used by block APIs. */
public class BlockVector extends Vector {
    public BlockVector(int x, int y, int z) { super(x, y, z); }
    @Override public int getBlockX() { return (int) getX(); }
    @Override public int getBlockY() { return (int) getY(); }
    @Override public int getBlockZ() { return (int) getZ(); }
}
