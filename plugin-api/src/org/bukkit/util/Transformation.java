package org.bukkit.util;

import java.util.Objects;
import org.joml.AxisAngle4f;
import org.joml.Quaternionf;
import org.joml.Vector3f;

/** The four components of a display entity affine transformation. */
public class Transformation {
    private final Vector3f translation;
    private final Quaternionf leftRotation;
    private final Vector3f scale;
    private final Quaternionf rightRotation;

    public Transformation(Vector3f translation, Quaternionf leftRotation, Vector3f scale, Quaternionf rightRotation) {
        this.translation = Objects.requireNonNull(translation, "translation");
        this.leftRotation = Objects.requireNonNull(leftRotation, "leftRotation");
        this.scale = Objects.requireNonNull(scale, "scale");
        this.rightRotation = Objects.requireNonNull(rightRotation, "rightRotation");
    }
    public Transformation(Vector3f translation, AxisAngle4f leftRotation, Vector3f scale, AxisAngle4f rightRotation) {
        this(translation, quaternion(leftRotation), scale, quaternion(rightRotation));
    }
    private static Quaternionf quaternion(AxisAngle4f value) {
        float half = value.angle / 2.0f, sine = (float) Math.sin(half);
        return new Quaternionf(value.x * sine, value.y * sine, value.z * sine, (float) Math.cos(half));
    }
    public Vector3f getTranslation() { return translation; }
    public Quaternionf getLeftRotation() { return leftRotation; }
    public Vector3f getScale() { return scale; }
    public Quaternionf getRightRotation() { return rightRotation; }
    @Override public boolean equals(Object other) {
        if (!(other instanceof Transformation value)) return false;
        return translation.x == value.translation.x && translation.y == value.translation.y && translation.z == value.translation.z
            && scale.x == value.scale.x && scale.y == value.scale.y && scale.z == value.scale.z
            && leftRotation.x == value.leftRotation.x && leftRotation.y == value.leftRotation.y && leftRotation.z == value.leftRotation.z && leftRotation.w == value.leftRotation.w
            && rightRotation.x == value.rightRotation.x && rightRotation.y == value.rightRotation.y && rightRotation.z == value.rightRotation.z && rightRotation.w == value.rightRotation.w;
    }
    @Override public int hashCode() { return Objects.hash(translation.x, translation.y, translation.z, scale.x, scale.y, scale.z, leftRotation.x, leftRotation.y, leftRotation.z, leftRotation.w, rightRotation.x, rightRotation.y, rightRotation.z, rightRotation.w); }
}
