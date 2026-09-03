package org.joml;

/** Minimal mutable JOML-compatible vector used by the Bukkit bridge. */
public class Vector3f {
    public float x, y, z;
    public Vector3f() { this(0, 0, 0); }
    public Vector3f(float x, float y, float z) { this.x = x; this.y = y; this.z = z; }
    public float x() { return x; }
    public float y() { return y; }
    public float z() { return z; }
    public Vector3f set(float x, float y, float z) { this.x = x; this.y = y; this.z = z; return this; }
}
