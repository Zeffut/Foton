package org.bukkit.util;

/** Mutable axis-aligned entity bounds. */
public final class BoundingBox {
    private double minX, minY, minZ, maxX, maxY, maxZ;

    public BoundingBox(double minX, double minY, double minZ, double maxX, double maxY, double maxZ) {
        this.minX = Math.min(minX, maxX);
        this.minY = Math.min(minY, maxY);
        this.minZ = Math.min(minZ, maxZ);
        this.maxX = Math.max(minX, maxX);
        this.maxY = Math.max(minY, maxY);
        this.maxZ = Math.max(minZ, maxZ);
    }
    public double getMinX() { return minX; }
    public double getMinY() { return minY; }
    public double getMinZ() { return minZ; }
    public double getMaxX() { return maxX; }
    public double getMaxY() { return maxY; }
    public double getMaxZ() { return maxZ; }
    public double getWidthX() { return maxX - minX; }
    public double getHeight() { return maxY - minY; }

    /** Expands equally towards both sides of each axis. Negative values shrink. */
    public BoundingBox expand(double x, double y, double z) {
        return expand(x, y, z, x, y, z);
    }

    /** Expands only towards the side indicated by each signed component. */
    public BoundingBox expandDirectional(double dirX, double dirY, double dirZ) {
        return expand(
            dirX < 0.0D ? -dirX : 0.0D,
            dirY < 0.0D ? -dirY : 0.0D,
            dirZ < 0.0D ? -dirZ : 0.0D,
            dirX > 0.0D ? dirX : 0.0D,
            dirY > 0.0D ? dirY : 0.0D,
            dirZ > 0.0D ? dirZ : 0.0D);
    }

    private BoundingBox expand(
            double negativeX, double negativeY, double negativeZ,
            double positiveX, double positiveY, double positiveZ) {
        double newMinX = minX - negativeX;
        double newMinY = minY - negativeY;
        double newMinZ = minZ - negativeZ;
        double newMaxX = maxX + positiveX;
        double newMaxY = maxY + positiveY;
        double newMaxZ = maxZ + positiveZ;

        double[] x = limitShrink(newMinX, newMaxX, (minX + maxX) * 0.5D);
        double[] y = limitShrink(newMinY, newMaxY, (minY + maxY) * 0.5D);
        double[] z = limitShrink(newMinZ, newMaxZ, (minZ + maxZ) * 0.5D);
        minX = x[0]; maxX = x[1];
        minY = y[0]; maxY = y[1];
        minZ = z[0]; maxZ = z[1];
        return this;
    }

    private static double[] limitShrink(double min, double max, double center) {
        if (min <= max) return new double[] {min, max};
        if (max >= center) return new double[] {max, max};
        if (min <= center) return new double[] {min, min};
        return new double[] {center, center};
    }

    /** Borders that only touch are not overlapping, matching Bukkit AABBs. */
    public boolean overlaps(BoundingBox other) {
        if (other == null) return false;
        return minX < other.maxX && maxX > other.minX
            && minY < other.maxY && maxY > other.minY
            && minZ < other.maxZ && maxZ > other.minZ;
    }
}
