package org.joml;

/** Axis-angle value accepted by Bukkit's Transformation constructor. */
public class AxisAngle4f {
    public float angle, x, y, z;
    public AxisAngle4f(float angle, float x, float y, float z) { this.angle = angle; this.x = x; this.y = y; this.z = z; }
}
