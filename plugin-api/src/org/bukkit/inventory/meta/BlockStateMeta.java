package org.bukkit.inventory.meta;

import org.bukkit.block.BlockState;

/** Item metadata carrying a block-state snapshot. */
public interface BlockStateMeta extends ItemMeta {
    BlockState getBlockState();
    void setBlockState(BlockState state);
}
