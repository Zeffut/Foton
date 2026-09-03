package org.bukkit.entity;

import org.bukkit.Material;
import org.bukkit.block.data.BlockData;

/** A block entity falling through the world. */
public interface FallingBlock extends Entity {
    Material getMaterial();
    BlockData getBlockData();
    void setBlockData(BlockData data);
    boolean getDropItem();
    void setDropItem(boolean drop);
    boolean getHurtEntities();
    void setHurtEntities(boolean hurt);
}
