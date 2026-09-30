package org.bukkit.entity;

import org.bukkit.block.data.BlockData;

/** A display entity whose rendered content is block data. */
public interface BlockDisplay extends Display {
    BlockData getBlock();

    void setBlock(BlockData block);
}
