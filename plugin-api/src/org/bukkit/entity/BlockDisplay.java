package org.bukkit.entity;

import org.bukkit.block.data.BlockData;

/** A display entity whose rendered content is block data. */
public interface BlockDisplay extends Display {
    default void setBlock(BlockData data) { }
    default BlockData getBlock() { return null; }
    default void setBrightness(Brightness brightness) { }
    default void setViewRange(float range) { }
    default void setShadowRadius(float radius) { }
}
