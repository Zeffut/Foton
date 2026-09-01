package org.bukkit;

import org.bukkit.util.Vector;

/** A point in a world, with a facing.
 *
 * The most-called type in the whole corpus after Bukkit itself: thirty-odd of
 * the fifty-nine plugins surveyed read at least one of its getters.
 *
 * Mutable, and its setters return `this`, because Bukkit's do. It holds its
 * world by reference and Bukkit holds it weakly; the difference does not show
 * for a plugin, and a Foton world handle is a name rather than a live object
 * so there is nothing here to keep alive.
 */
public class Location implements Cloneable {
    private World world;
    private double x;
    private double y;
    private double z;
    private float yaw;
    private float pitch;

    public Location(World world, double x, double y, double z) {
        this(world, x, y, z, 0, 0);
    }

    public Location(World world, double x, double y, double z, float yaw, float pitch) {
        this.world = world;
        this.x = x;
        this.y = y;
        this.z = z;
        this.yaw = yaw;
        this.pitch = pitch;
    }

    public World getWorld() {
        return world;
    }

    public void setWorld(World value) {
        this.world = value;
    }

    public boolean isWorldLoaded() {
        return world != null;
    }

    public double getX() {
        return x;
    }

    public Location setX(double value) {
        this.x = value;
        return this;
    }

    public double getY() {
        return y;
    }

    public Location setY(double value) {
        this.y = value;
        return this;
    }

    public double getZ() {
        return z;
    }

    public Location setZ(double value) {
        this.z = value;
        return this;
    }

    /** The block containing this point. Floor, not truncation: -0.5 is in
     * block -1, and truncating would put it in block 0 and quietly move
     * everything on the negative side of the world by one. */
    public int getBlockX() {
        return (int) Math.floor(x);
    }

    public int getBlockY() {
        return (int) Math.floor(y);
    }

    public int getBlockZ() {
        return (int) Math.floor(z);
    }

    public float getYaw() {
        return yaw;
    }

    public Location setYaw(float value) {
        this.yaw = value;
        return this;
    }

    public float getPitch() {
        return pitch;
    }

    public Location setPitch(float value) {
        this.pitch = value;
        return this;
    }

    public Location add(double dx, double dy, double dz) {
        x += dx;
        y += dy;
        z += dz;
        return this;
    }

    public Location add(Location other) {
        return add(other.x, other.y, other.z);
    }

    public Location add(Vector other) {
        return add(other.getX(), other.getY(), other.getZ());
    }

    public Location subtract(double dx, double dy, double dz) {
        return add(-dx, -dy, -dz);
    }

    public Location subtract(Location other) {
        return add(-other.x, -other.y, -other.z);
    }

    public Location subtract(Vector other) {
        return add(-other.getX(), -other.getY(), -other.getZ());
    }

    /** The distance to another point. Points in different worlds have none,
     * and Bukkit throws rather than answering a number that would be wrong. */
    public double distance(Location other) {
        return Math.sqrt(distanceSquared(other));
    }

    public double distanceSquared(Location other) {
        if (other == null) {
            throw new IllegalArgumentException("cannot measure to a null location");
        }
        if (world != other.world
            && (world == null || other.world == null || !world.equals(other.world))) {
            throw new IllegalArgumentException("cannot measure between different worlds");
        }
        double dx = x - other.x;
        double dy = y - other.y;
        double dz = z - other.z;
        return dx * dx + dy * dy + dz * dz;
    }

    public Vector toVector() {
        return new Vector(x, y, z);
    }

    public org.bukkit.block.Block getBlock() {
        return world == null ? null : world.getBlockAt(getBlockX(), getBlockY(), getBlockZ());
    }

    public Chunk getChunk() {
        return world == null ? null : world.getChunkAt(this);
    }

    @Override
    public Location clone() {
        try {
            return (Location) super.clone();
        } catch (CloneNotSupportedException impossible) {
            throw new AssertionError(impossible);
        }
    }

    @Override
    public boolean equals(Object other) {
        if (!(other instanceof Location)) {
            return false;
        }
        Location location = (Location) other;
        return java.util.Objects.equals(world, location.world)
            && Double.compare(x, location.x) == 0
            && Double.compare(y, location.y) == 0
            && Double.compare(z, location.z) == 0
            && Float.compare(yaw, location.yaw) == 0
            && Float.compare(pitch, location.pitch) == 0;
    }

    @Override
    public int hashCode() {
        return java.util.Objects.hash(world, x, y, z, yaw, pitch);
    }

    @Override
    public String toString() {
        return "Location{world=" + (world == null ? "null" : world.getName())
            + ", x=" + x + ", y=" + y + ", z=" + z + ", yaw=" + yaw + ", pitch=" + pitch + "}";
    }
}
