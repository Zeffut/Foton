package org.bukkit.entity;

import org.bukkit.Color;
import org.bukkit.util.Transformation;

/** A display entity: a block, item or text rendered by the client, with no
 * collision, animated by interpolating between the values it is sent. */
public interface Display extends Entity {
    Transformation getTransformation();
    void setTransformation(Transformation transformation);

    int getInterpolationDuration();
    void setInterpolationDuration(int duration);

    /** Ticks a position or rotation change is spread over, 0 to 59. */
    int getTeleportDuration();
    void setTeleportDuration(int duration);

    float getViewRange();
    void setViewRange(float range);
    float getShadowRadius();
    void setShadowRadius(float radius);
    float getShadowStrength();
    void setShadowStrength(float strength);
    float getDisplayWidth();
    void setDisplayWidth(float width);
    float getDisplayHeight();
    void setDisplayHeight(float height);

    /** Ticks before an interpolation starts; setting it (re)starts one. */
    int getInterpolationDelay();
    void setInterpolationDelay(int ticks);

    Billboard getBillboard();
    void setBillboard(Billboard billboard);

    /** The glow outline colour, or null for the team colour. */
    Color getGlowColorOverride();
    void setGlowColorOverride(Color color);

    /** The fixed light level, or null for the light where the display stands. */
    Brightness getBrightness();
    void setBrightness(Brightness brightness);

    /** How a display turns to face the viewer; ordinals are vanilla's ids. */
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

    /** A block and sky light level, each 0 to 15. */
    class Brightness {
        private final int blockLight;
        private final int skyLight;

        public Brightness(int blockLight, int skyLight) {
            if (blockLight < 0 || blockLight > 15) throw new IllegalArgumentException("Block brightness out of range: " + blockLight);
            if (skyLight < 0 || skyLight > 15) throw new IllegalArgumentException("Sky brightness out of range: " + skyLight);
            this.blockLight = blockLight;
            this.skyLight = skyLight;
        }

        public int getBlockLight() { return blockLight; }
        public int getSkyLight() { return skyLight; }

        @Override public int hashCode() { return 47 * (47 * 7 + blockLight) + skyLight; }
        @Override public boolean equals(Object other) {
            return other instanceof Brightness brightness && brightness.blockLight == blockLight && brightness.skyLight == skyLight;
        }
        @Override public String toString() { return "Brightness{blockLight=" + blockLight + ", skyLight=" + skyLight + '}'; }
    }
}
