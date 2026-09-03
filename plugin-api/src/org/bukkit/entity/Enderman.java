package org.bukkit.entity;

import org.bukkit.block.data.BlockData;

/** Enderman entity with its carried block state. */
public interface Enderman extends Monster {
    BlockData getCarriedBlock();
    void setCarriedBlock(BlockData block);
}
