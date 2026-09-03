package io.papermc.paper.math;

import org.bukkit.util.Vector;

/** Immutable integer block coordinates used by Paper command resolvers. */
public final class BlockPosition implements Position {
    private final int x;
    private final int y;
    private final int z;
    public BlockPosition(int x, int y, int z) { this.x = x; this.y = y; this.z = z; }
    public int blockX() { return x; }
    public double x() { return x; }
    public int blockY() { return y; }
    public double y() { return y; }
    public int blockZ() { return z; }
    public double z() { return z; }
    public Vector toVector() { return new Vector(x, y, z); }
}
