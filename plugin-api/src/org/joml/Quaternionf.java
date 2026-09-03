package org.joml;

/** Minimal mutable JOML-compatible quaternion used by the Bukkit bridge. */
public class Quaternionf {
    public float x, y, z, w;
    public Quaternionf() { this(0, 0, 0, 1); }
    public Quaternionf(float x, float y, float z, float w) { this.x = x; this.y = y; this.z = z; this.w = w; }
    public float x() { return x; }
    public float y() { return y; }
    public float z() { return z; }
    public float w() { return w; }
}
