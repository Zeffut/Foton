package org.bukkit.util;

import java.util.LinkedHashMap;
import java.util.Map;
import org.bukkit.Location;
import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;
import org.bukkit.configuration.serialization.ConfigurationSerializable;

/** A mutable axis-aligned box. Every operation that changes it returns the
 * box itself, so calls chain, and {@link #clone} is how to keep a copy. */
public class BoundingBox implements Cloneable, ConfigurationSerializable {
    private double minX, minY, minZ, maxX, maxY, maxZ;

    /** The empty box at the origin. */
    public BoundingBox() { resize(0, 0, 0, 0, 0, 0); }

    /** A box between two corners, in either order. */
    public BoundingBox(double x1, double y1, double z1, double x2, double y2, double z2) {
        resize(x1, y1, z1, x2, y2, z2);
    }

    public static BoundingBox of(Vector corner1, Vector corner2) {
        require(corner1, "corner1"); require(corner2, "corner2");
        return new BoundingBox(corner1.getX(), corner1.getY(), corner1.getZ(), corner2.getX(), corner2.getY(), corner2.getZ());
    }

    public static BoundingBox of(Location corner1, Location corner2) {
        require(corner1, "corner1"); require(corner2, "corner2");
        if (corner1.getWorld() != null && corner2.getWorld() != null && !corner1.getWorld().equals(corner2.getWorld()))
            throw new IllegalArgumentException("Locations from different worlds");
        return new BoundingBox(corner1.getX(), corner1.getY(), corner1.getZ(), corner2.getX(), corner2.getY(), corner2.getZ());
    }

    /** The box spanning both blocks, each block counted whole. */
    public static BoundingBox of(Block corner1, Block corner2) {
        require(corner1, "corner1"); require(corner2, "corner2");
        int x1 = Math.min(corner1.getX(), corner2.getX()), y1 = Math.min(corner1.getY(), corner2.getY()), z1 = Math.min(corner1.getZ(), corner2.getZ());
        int x2 = Math.max(corner1.getX(), corner2.getX()) + 1, y2 = Math.max(corner1.getY(), corner2.getY()) + 1, z2 = Math.max(corner1.getZ(), corner2.getZ()) + 1;
        return new BoundingBox(x1, y1, z1, x2, y2, z2);
    }

    /** The unit box of the block's position. */
    public static BoundingBox of(Block block) {
        require(block, "block");
        return new BoundingBox(block.getX(), block.getY(), block.getZ(), block.getX() + 1, block.getY() + 1, block.getZ() + 1);
    }

    public static BoundingBox of(Vector center, double x, double y, double z) {
        require(center, "center");
        return new BoundingBox(center.getX() - x, center.getY() - y, center.getZ() - z, center.getX() + x, center.getY() + y, center.getZ() + z);
    }

    public static BoundingBox of(Location center, double x, double y, double z) {
        require(center, "center");
        return new BoundingBox(center.getX() - x, center.getY() - y, center.getZ() - z, center.getX() + x, center.getY() + y, center.getZ() + z);
    }

    private static void require(Object value, String name) {
        if (value == null) throw new IllegalArgumentException(name + " cannot be null");
    }

    public BoundingBox resize(double x1, double y1, double z1, double x2, double y2, double z2) {
        checkFinite(x1, y1, z1); checkFinite(x2, y2, z2);
        minX = Math.min(x1, x2); minY = Math.min(y1, y2); minZ = Math.min(z1, z2);
        maxX = Math.max(x1, x2); maxY = Math.max(y1, y2); maxZ = Math.max(z1, z2);
        return this;
    }

    private static void checkFinite(double x, double y, double z) {
        if (!Double.isFinite(x) || !Double.isFinite(y) || !Double.isFinite(z))
            throw new IllegalArgumentException("Coordinates must be finite: " + x + ", " + y + ", " + z);
    }

    public double getMinX() { return minX; }
    public double getMinY() { return minY; }
    public double getMinZ() { return minZ; }
    public Vector getMin() { return new Vector(minX, minY, minZ); }
    public double getMaxX() { return maxX; }
    public double getMaxY() { return maxY; }
    public double getMaxZ() { return maxZ; }
    public Vector getMax() { return new Vector(maxX, maxY, maxZ); }
    public double getWidthX() { return maxX - minX; }
    public double getWidthZ() { return maxZ - minZ; }
    public double getHeight() { return maxY - minY; }
    public double getVolume() { return getHeight() * getWidthX() * getWidthZ(); }
    public double getCenterX() { return minX + getWidthX() * 0.5; }
    public double getCenterY() { return minY + getHeight() * 0.5; }
    public double getCenterZ() { return minZ + getWidthZ() * 0.5; }
    public Vector getCenter() { return new Vector(getCenterX(), getCenterY(), getCenterZ()); }

    public BoundingBox copy(BoundingBox other) {
        require(other, "other");
        return resize(other.minX, other.minY, other.minZ, other.maxX, other.maxY, other.maxZ);
    }

    /** Grows each face by its own amount; a negative amount shrinks it, and
     * a face shrunk past its opposite meets it at their common midpoint. */
    public BoundingBox expand(double negativeX, double negativeY, double negativeZ, double positiveX, double positiveY, double positiveZ) {
        if (negativeX == 0 && negativeY == 0 && negativeZ == 0 && positiveX == 0 && positiveY == 0 && positiveZ == 0) return this;
        double newMinX = minX - negativeX, newMinY = minY - negativeY, newMinZ = minZ - negativeZ;
        double newMaxX = maxX + positiveX, newMaxY = maxY + positiveY, newMaxZ = maxZ + positiveZ;
        if (newMinX > newMaxX) {
            double centerX = getCenterX();
            if (newMaxX >= centerX) newMinX = newMaxX;
            else if (newMinX <= centerX) newMaxX = newMinX;
            else { newMinX = centerX; newMaxX = centerX; }
        }
        if (newMinY > newMaxY) {
            double centerY = getCenterY();
            if (newMaxY >= centerY) newMinY = newMaxY;
            else if (newMinY <= centerY) newMaxY = newMinY;
            else { newMinY = centerY; newMaxY = centerY; }
        }
        if (newMinZ > newMaxZ) {
            double centerZ = getCenterZ();
            if (newMaxZ >= centerZ) newMinZ = newMaxZ;
            else if (newMinZ <= centerZ) newMaxZ = newMinZ;
            else { newMinZ = centerZ; newMaxZ = centerZ; }
        }
        return resize(newMinX, newMinY, newMinZ, newMaxX, newMaxY, newMaxZ);
    }

    /** Grows both faces of each axis by that axis's amount. */
    public BoundingBox expand(double x, double y, double z) { return expand(x, y, z, x, y, z); }
    public BoundingBox expand(Vector expansion) {
        require(expansion, "expansion");
        return expand(expansion.getX(), expansion.getY(), expansion.getZ());
    }
    public BoundingBox expand(double expansion) { return expand(expansion, expansion, expansion, expansion, expansion, expansion); }

    /** Grows the faces the direction points at, each by the direction's
     * component times {@code expansion}. */
    public BoundingBox expand(double dirX, double dirY, double dirZ, double expansion) {
        if (expansion == 0) return this;
        if (dirX == 0 && dirY == 0 && dirZ == 0) return this;
        double negativeX = dirX < 0 ? -dirX * expansion : 0, negativeY = dirY < 0 ? -dirY * expansion : 0, negativeZ = dirZ < 0 ? -dirZ * expansion : 0;
        double positiveX = dirX > 0 ? dirX * expansion : 0, positiveY = dirY > 0 ? dirY * expansion : 0, positiveZ = dirZ > 0 ? dirZ * expansion : 0;
        return expand(negativeX, negativeY, negativeZ, positiveX, positiveY, positiveZ);
    }
    public BoundingBox expand(Vector direction, double expansion) {
        require(direction, "direction");
        return expand(direction.getX(), direction.getY(), direction.getZ(), expansion);
    }
    public BoundingBox expand(BlockFace blockFace, double expansion) {
        require(blockFace, "blockFace");
        if (blockFace == BlockFace.SELF) return this;
        return expand(blockFace.getModX(), blockFace.getModY(), blockFace.getModZ(), expansion);
    }

    /** Grows the box the way the given movement would sweep it. */
    public BoundingBox expandDirectional(double dirX, double dirY, double dirZ) { return expand(dirX, dirY, dirZ, 1.0); }
    public BoundingBox expandDirectional(Vector direction) {
        require(direction, "direction");
        return expand(direction.getX(), direction.getY(), direction.getZ(), 1.0);
    }

    public BoundingBox union(double posX, double posY, double posZ) {
        return resize(Math.min(minX, posX), Math.min(minY, posY), Math.min(minZ, posZ),
            Math.max(maxX, posX), Math.max(maxY, posY), Math.max(maxZ, posZ));
    }
    public BoundingBox union(Vector position) {
        require(position, "position");
        return union(position.getX(), position.getY(), position.getZ());
    }
    public BoundingBox union(Location position) {
        require(position, "position");
        return union(position.getX(), position.getY(), position.getZ());
    }
    public BoundingBox union(BoundingBox other) {
        require(other, "other");
        if (contains(other)) return this;
        return resize(Math.min(minX, other.minX), Math.min(minY, other.minY), Math.min(minZ, other.minZ),
            Math.max(maxX, other.maxX), Math.max(maxY, other.maxY), Math.max(maxZ, other.maxZ));
    }

    /** Shrinks the box to its overlap with {@code other}, which must overlap it. */
    public BoundingBox intersection(BoundingBox other) {
        require(other, "other");
        if (!overlaps(other)) throw new IllegalArgumentException("The bounding boxes do not overlap");
        return resize(Math.max(minX, other.minX), Math.max(minY, other.minY), Math.max(minZ, other.minZ),
            Math.min(maxX, other.maxX), Math.min(maxY, other.maxY), Math.min(maxZ, other.maxZ));
    }

    public BoundingBox shift(double shiftX, double shiftY, double shiftZ) {
        if (shiftX == 0 && shiftY == 0 && shiftZ == 0) return this;
        return resize(minX + shiftX, minY + shiftY, minZ + shiftZ, maxX + shiftX, maxY + shiftY, maxZ + shiftZ);
    }
    public BoundingBox shift(Vector shift) {
        require(shift, "shift");
        return shift(shift.getX(), shift.getY(), shift.getZ());
    }
    public BoundingBox shift(Location shift) {
        require(shift, "shift");
        return shift(shift.getX(), shift.getY(), shift.getZ());
    }

    private boolean overlaps(double minX, double minY, double minZ, double maxX, double maxY, double maxZ) {
        return this.minX < maxX && this.maxX > minX
            && this.minY < maxY && this.maxY > minY
            && this.minZ < maxZ && this.maxZ > minZ;
    }
    /** Whether the boxes share volume; touching faces do not count. */
    public boolean overlaps(BoundingBox other) {
        require(other, "other");
        return overlaps(other.minX, other.minY, other.minZ, other.maxX, other.maxY, other.maxZ);
    }
    public boolean overlaps(Vector min, Vector max) {
        require(min, "min"); require(max, "max");
        double x1 = min.getX(), y1 = min.getY(), z1 = min.getZ(), x2 = max.getX(), y2 = max.getY(), z2 = max.getZ();
        return overlaps(Math.min(x1, x2), Math.min(y1, y2), Math.min(z1, z2), Math.max(x1, x2), Math.max(y1, y2), Math.max(z1, z2));
    }

    /** Whether the point is inside: the minimum faces count, the maximum ones do not. */
    public boolean contains(double x, double y, double z) {
        return x >= minX && x < maxX && y >= minY && y < maxY && z >= minZ && z < maxZ;
    }
    public boolean contains(Vector position) {
        require(position, "position");
        return contains(position.getX(), position.getY(), position.getZ());
    }
    private boolean contains(double minX, double minY, double minZ, double maxX, double maxY, double maxZ) {
        return this.minX <= minX && this.maxX >= maxX
            && this.minY <= minY && this.maxY >= maxY
            && this.minZ <= minZ && this.maxZ >= maxZ;
    }
    public boolean contains(BoundingBox other) {
        require(other, "other");
        return contains(other.minX, other.minY, other.minZ, other.maxX, other.maxY, other.maxZ);
    }
    public boolean contains(Vector min, Vector max) {
        require(min, "min"); require(max, "max");
        double x1 = min.getX(), y1 = min.getY(), z1 = min.getZ(), x2 = max.getX(), y2 = max.getY(), z2 = max.getZ();
        return contains(Math.min(x1, x2), Math.min(y1, y2), Math.min(z1, z2), Math.max(x1, x2), Math.max(y1, y2), Math.max(z1, z2));
    }

    /** Where a ray from {@code start} along {@code direction} first enters
     * the box -- or leaves it, when it starts inside -- within
     * {@code maxDistance}; null when it does not reach it. */
    public RayTraceResult rayTrace(Vector start, Vector direction, double maxDistance) {
        require(start, "start"); require(direction, "direction");
        checkFinite(start.getX(), start.getY(), start.getZ());
        checkFinite(direction.getX(), direction.getY(), direction.getZ());
        if (direction.lengthSquared() <= 0) throw new IllegalArgumentException("Direction's magnitude is 0!");
        if (maxDistance < 0.0) return null;

        double startX = start.getX(), startY = start.getY(), startZ = start.getZ();
        Vector dir = new Vector(zero(direction.getX()), zero(direction.getY()), zero(direction.getZ())).normalize();
        double dirX = dir.getX(), dirY = dir.getY(), dirZ = dir.getZ();
        double divX = 1.0 / dirX, divY = 1.0 / dirY, divZ = 1.0 / dirZ;

        double tMin, tMax;
        BlockFace hitBlockFaceMin, hitBlockFaceMax;
        if (dirX >= 0.0) {
            tMin = (minX - startX) * divX; tMax = (maxX - startX) * divX;
            hitBlockFaceMin = BlockFace.WEST; hitBlockFaceMax = BlockFace.EAST;
        } else {
            tMin = (maxX - startX) * divX; tMax = (minX - startX) * divX;
            hitBlockFaceMin = BlockFace.EAST; hitBlockFaceMax = BlockFace.WEST;
        }

        double tyMin, tyMax;
        BlockFace hitBlockFaceYMin, hitBlockFaceYMax;
        if (dirY >= 0.0) {
            tyMin = (minY - startY) * divY; tyMax = (maxY - startY) * divY;
            hitBlockFaceYMin = BlockFace.DOWN; hitBlockFaceYMax = BlockFace.UP;
        } else {
            tyMin = (maxY - startY) * divY; tyMax = (minY - startY) * divY;
            hitBlockFaceYMin = BlockFace.UP; hitBlockFaceYMax = BlockFace.DOWN;
        }
        if (tMin > tyMax || tMax < tyMin) return null;
        if (tyMin > tMin) { tMin = tyMin; hitBlockFaceMin = hitBlockFaceYMin; }
        if (tyMax < tMax) { tMax = tyMax; hitBlockFaceMax = hitBlockFaceYMax; }

        double tzMin, tzMax;
        BlockFace hitBlockFaceZMin, hitBlockFaceZMax;
        if (dirZ >= 0.0) {
            tzMin = (minZ - startZ) * divZ; tzMax = (maxZ - startZ) * divZ;
            hitBlockFaceZMin = BlockFace.NORTH; hitBlockFaceZMax = BlockFace.SOUTH;
        } else {
            tzMin = (maxZ - startZ) * divZ; tzMax = (minZ - startZ) * divZ;
            hitBlockFaceZMin = BlockFace.SOUTH; hitBlockFaceZMax = BlockFace.NORTH;
        }
        if (tMin > tzMax || tMax < tzMin) return null;
        if (tzMin > tMin) { tMin = tzMin; hitBlockFaceMin = hitBlockFaceZMin; }
        if (tzMax < tMax) { tMax = tzMax; hitBlockFaceMax = hitBlockFaceZMax; }

        if (tMax < 0.0) return null;
        if (tMin > maxDistance) return null;

        double t;
        BlockFace hitBlockFace;
        if (tMin < 0.0) { t = tMax; hitBlockFace = hitBlockFaceMax; }
        else { t = tMin; hitBlockFace = hitBlockFaceMin; }
        Vector hitPosition = dir.multiply(t).add(start);
        return new RayTraceResult(hitPosition, hitBlockFace);
    }

    /** Negative zero as zero, so a flat axis divides to +infinity. */
    private static double zero(double value) { return value == -0.0 ? 0.0 : value; }

    @Override public int hashCode() {
        final int prime = 31;
        int result = 1;
        long temp;
        temp = Double.doubleToLongBits(maxX); result = prime * result + (int) (temp ^ (temp >>> 32));
        temp = Double.doubleToLongBits(maxY); result = prime * result + (int) (temp ^ (temp >>> 32));
        temp = Double.doubleToLongBits(maxZ); result = prime * result + (int) (temp ^ (temp >>> 32));
        temp = Double.doubleToLongBits(minX); result = prime * result + (int) (temp ^ (temp >>> 32));
        temp = Double.doubleToLongBits(minY); result = prime * result + (int) (temp ^ (temp >>> 32));
        temp = Double.doubleToLongBits(minZ); result = prime * result + (int) (temp ^ (temp >>> 32));
        return result;
    }

    @Override public boolean equals(Object obj) {
        if (this == obj) return true;
        if (!(obj instanceof BoundingBox other)) return false;
        return Double.doubleToLongBits(maxX) == Double.doubleToLongBits(other.maxX)
            && Double.doubleToLongBits(maxY) == Double.doubleToLongBits(other.maxY)
            && Double.doubleToLongBits(maxZ) == Double.doubleToLongBits(other.maxZ)
            && Double.doubleToLongBits(minX) == Double.doubleToLongBits(other.minX)
            && Double.doubleToLongBits(minY) == Double.doubleToLongBits(other.minY)
            && Double.doubleToLongBits(minZ) == Double.doubleToLongBits(other.minZ);
    }

    @Override public String toString() {
        return "BoundingBox [minX=" + minX + ", minY=" + minY + ", minZ=" + minZ
            + ", maxX=" + maxX + ", maxY=" + maxY + ", maxZ=" + maxZ + "]";
    }

    @Override public BoundingBox clone() {
        try { return (BoundingBox) super.clone(); }
        catch (CloneNotSupportedException error) { throw new Error(error); }
    }

    @Override public Map<String, Object> serialize() {
        Map<String, Object> result = new LinkedHashMap<>();
        result.put("minX", minX); result.put("minY", minY); result.put("minZ", minZ);
        result.put("maxX", maxX); result.put("maxY", maxY); result.put("maxZ", maxZ);
        return result;
    }

    public static BoundingBox deserialize(Map<String, Object> args) {
        double minX = number(args, "minX"), minY = number(args, "minY"), minZ = number(args, "minZ");
        double maxX = number(args, "maxX"), maxY = number(args, "maxY"), maxZ = number(args, "maxZ");
        return new BoundingBox(minX, minY, minZ, maxX, maxY, maxZ);
    }

    private static double number(Map<String, Object> args, String key) {
        Object value = args.get(key);
        return value instanceof Number number ? number.doubleValue() : 0.0;
    }
}
