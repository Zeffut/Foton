package org.bukkit.entity;

import org.bukkit.util.Transformation;

/** A renderable display entity. */
public interface Display extends Entity {
    final class Brightness {
        private final int blockLight;
        private final int skyLight;
        public Brightness(int blockLight, int skyLight) { this.blockLight = blockLight; this.skyLight = skyLight; }
        public int getBlockLight() { return blockLight; }
        public int getSkyLight() { return skyLight; }
    }


    /** How a display entity turns to face the viewer.
     *
     * <p>{@code CENTER} is the one plugins name explicitly -- it is what makes
     * a floating label readable from every angle, and it is not the default,
     * so a hologram library that could not reference it would have no way to
     * ask for the behavior its users expect. */
    enum Billboard {
        /** Fixed in world space; no rotation towards the viewer. */
        FIXED,
        /** Rotates about the vertical axis only. */
        VERTICAL,
        /** Rotates about the horizontal axis only. */
        HORIZONTAL,
        /** Rotates about both axes, always facing the viewer. */
        CENTER
    }

    default Billboard getBillboard() { return Billboard.FIXED; }

    default void setBillboard(Billboard billboard) { }

    default Brightness getBrightness() { return null; }

    default void setBrightness(Brightness brightness) { }

    default float getViewRange() { return 1.0f; }

    default void setViewRange(float range) { }

    default Transformation getTransformation() {
        return new Transformation(new org.joml.Vector3f(), new org.joml.Quaternionf(), new org.joml.Vector3f(1, 1, 1), new org.joml.Quaternionf());
    }
    default void setTransformation(Transformation transformation) { }
}
