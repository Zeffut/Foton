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


    default Transformation getTransformation() {
        return new Transformation(new org.joml.Vector3f(), new org.joml.Quaternionf(), new org.joml.Vector3f(1, 1, 1), new org.joml.Quaternionf());
    }
    default void setTransformation(Transformation transformation) { }
}
