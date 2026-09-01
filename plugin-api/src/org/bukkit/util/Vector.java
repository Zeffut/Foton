package org.bukkit.util;

/** Three doubles that mean a direction or an offset.
 *
 * Mutable, and its arithmetic returns `this`, because Bukkit's is and plugins
 * chain off that. A copy is what `clone` is for.
 */
public class Vector implements Cloneable {
    protected double x;
    protected double y;
    protected double z;

    public Vector() {
        this(0, 0, 0);
    }

    public Vector(double x, double y, double z) {
        this.x = x;
        this.y = y;
        this.z = z;
    }

    public Vector(int x, int y, int z) {
        this((double) x, (double) y, (double) z);
    }

    public double getX() {
        return x;
    }

    public double getY() {
        return y;
    }

    public double getZ() {
        return z;
    }

    public int getBlockX() {
        return (int) Math.floor(x);
    }

    public int getBlockY() {
        return (int) Math.floor(y);
    }

    public int getBlockZ() {
        return (int) Math.floor(z);
    }

    public Vector setX(double value) {
        this.x = value;
        return this;
    }

    public Vector setY(double value) {
        this.y = value;
        return this;
    }

    public Vector setZ(double value) {
        this.z = value;
        return this;
    }

    public Vector add(Vector other) {
        x += other.x;
        y += other.y;
        z += other.z;
        return this;
    }

    public Vector subtract(Vector other) {
        x -= other.x;
        y -= other.y;
        z -= other.z;
        return this;
    }

    public Vector multiply(double factor) {
        x *= factor;
        y *= factor;
        z *= factor;
        return this;
    }

    public Vector multiply(int factor) {
        return multiply((double) factor);
    }

    public org.bukkit.Location toLocation(org.bukkit.World world) {
        return new org.bukkit.Location(world, x, y, z);
    }

    public org.bukkit.Location toLocation(org.bukkit.World world, float yaw, float pitch) {
        return new org.bukkit.Location(world, x, y, z, yaw, pitch);
    }

    public double length() {
        return Math.sqrt(lengthSquared());
    }

    public double lengthSquared() {
        return x * x + y * y + z * z;
    }

    public double distance(Vector other) {
        return Math.sqrt(distanceSquared(other));
    }

    public double distanceSquared(Vector other) {
        double dx = x - other.x;
        double dy = y - other.y;
        double dz = z - other.z;
        return dx * dx + dy * dy + dz * dz;
    }

    /** Scales to length one. A zero vector stays zero rather than becoming NaN. */
    public Vector normalize() {
        double length = length();
        if (length > 0) {
            multiply(1 / length);
        }
        return this;
    }

    public Vector zero() {
        return setX(0).setY(0).setZ(0);
    }

    @Override
    public Vector clone() {
        try {
            return (Vector) super.clone();
        } catch (CloneNotSupportedException impossible) {
            throw new AssertionError(impossible);
        }
    }

    @Override
    public boolean equals(Object other) {
        if (!(other instanceof Vector)) {
            return false;
        }
        Vector vector = (Vector) other;
        return Double.compare(x, vector.x) == 0
            && Double.compare(y, vector.y) == 0
            && Double.compare(z, vector.z) == 0;
    }

    @Override
    public int hashCode() {
        return java.util.Objects.hash(x, y, z);
    }

    @Override
    public String toString() {
        return "Vector{" + x + ", " + y + ", " + z + "}";
    }
}
