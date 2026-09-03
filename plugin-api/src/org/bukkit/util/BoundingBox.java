package org.bukkit.util;

/** Immutable axis-aligned entity bounds. */
public final class BoundingBox {
    private final double minX, minY, minZ, maxX, maxY, maxZ;
    public BoundingBox(double minX, double minY, double minZ, double maxX, double maxY, double maxZ) {
        this.minX = minX; this.minY = minY; this.minZ = minZ;
        this.maxX = maxX; this.maxY = maxY; this.maxZ = maxZ;
    }
    public double getMinX() { return minX; }
    public double getMinY() { return minY; }
    public double getMinZ() { return minZ; }
    public double getMaxX() { return maxX; }
    public double getMaxY() { return maxY; }
    public double getMaxZ() { return maxZ; }
    public double getWidthX() { return maxX - minX; }
    public double getHeight() { return maxY - minY; }
}
